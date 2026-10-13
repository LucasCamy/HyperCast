"""Root-only, drained-room deployment of already imported HyperCast audio images.

Run on the existing k3s node. Preserves TURN credentials/ports and writes a
private rollback snapshot. No credentials are printed or accepted as CLI args.
"""
import base64
import json
import os
from pathlib import Path
import runpy
import subprocess

ROOT = Path('/etc/hypercast')

def k(*args, payload=None):
    return subprocess.check_output(['k3s','kubectl','-n','hypercast',*args], input=payload, timeout=150)

def audio_config(text):
    if 'mime: audio/opus' not in text:
        if text.count('  enabled_codecs:\n') != 1: raise RuntimeError('Unexpected codec configuration')
        text = text.replace('  enabled_codecs:\n','  enabled_codecs:\n    - mime: audio/opus\n')
    if 'subscription_limit_audio:' not in text:
        if text.count('limit:\n') != 1: raise RuntimeError('Unexpected subscription configuration')
        text = text.replace('limit:\n','limit:\n  subscription_limit_audio: 3\n')
    return text

def main():
    if os.geteuid() != 0: raise RuntimeError('Run as root on the Kubernetes node')
    runpy.run_path(str(ROOT/'deploy_turn_on_node.py'))['assert_empty']()
    names = {'hypercast-sfu':'sfu','hypercast-access':'auth'}
    targets = {}
    for deployment, container in names.items():
        ref = 'docker.io/library/'+('hypercast-sfu' if container=='sfu' else 'hypercast-auth')+':audio'
        rows = subprocess.check_output(['k3s','ctr','images','list'],timeout=20).decode().splitlines()
        digest = next(row.split()[2] for row in rows if row.split()[0]==ref)
        if not digest.startswith('sha256:') or len(digest)!=71: raise RuntimeError('Invalid imported digest')
        target = 'ghcr.io/lucascamy/'+('hypercast-sfu' if container=='sfu' else 'hypercast-auth')+'@'+digest
        subprocess.run(['k3s','ctr','images','tag','--force',ref,target],check=True,stdout=subprocess.DEVNULL,timeout=20)
        targets[deployment] = target
    original_secret = json.loads(k('get','secret','hypercast-sfu-turn-config','-o','json'))
    original_config = json.loads(k('get','configmap','hypercast-sfu','-o','json'))
    deployments = {name:json.loads(k('get','deployment',name,'-o','json')) for name in names}
    backup = ROOT/'audio-rollback.json'
    if backup.exists(): raise RuntimeError('Audio rollback snapshot already exists; inspect before another rollout')
    with backup.open('x') as stream:
        os.chmod(backup,0o600)
        json.dump({'secret':original_secret,'configmap':original_config,'deployments':deployments},stream)
    def patch(resource,name,obj):
        k('patch',resource,name,'--type=merge','--patch-file=/dev/stdin',payload=json.dumps(obj).encode())
    try:
        text = base64.b64decode(original_secret['data']['livekit.yaml']).decode()
        patch('secret','hypercast-sfu-turn-config',{'data':{'livekit.yaml':base64.b64encode(audio_config(text).encode()).decode()}})
        patch('configmap','hypercast-sfu',{'data':{'livekit.yaml':audio_config(original_config['data']['livekit.yaml'])}})
        for deployment,container in names.items():
            k('set','image','deployment/'+deployment,container+'='+targets[deployment])
            k('rollout','status','deployment/'+deployment,'--timeout=120s')
        print('Audio enabled; SFU and invitation API healthy')
        for deployment,target in targets.items(): print(deployment+' '+target)
    except Exception:
        patch('secret','hypercast-sfu-turn-config',{'data':original_secret['data']})
        patch('configmap','hypercast-sfu',{'data':original_config['data']})
        for deployment,container in names.items():
            image = next(c['image'] for c in deployments[deployment]['spec']['template']['spec']['containers'] if c['name']==container)
            k('set','image','deployment/'+deployment,container+'='+image)
            k('rollout','status','deployment/'+deployment,'--timeout=120s')
        raise RuntimeError('Audio rollout failed; original configuration and images restored') from None

if __name__ == '__main__': main()
