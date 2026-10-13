"""Run as root on vm-200. Keep TURN credentials and private key on the node.

Usage: deploy_turn_on_node.py --template PATH [--activate-sfu]
Renewal: deploy_turn_on_node.py --renew-certificate
"""
import argparse
import base64
import hashlib
import hmac
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import urllib.request

ROOT = Path('/etc/hypercast')
CERT = Path('/etc/letsencrypt/live/hypercast-turn')

def kubectl(*args, payload=None):
    result = subprocess.run(['k3s', 'kubectl', *args], input=payload,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False, timeout=45)
    if result.returncode:
        # Never echo kubectl validation output: manifests may contain secrets.
        raise RuntimeError('Kubernetes operation failed: ' + ' '.join(args[:3]))
    return result.stdout

def secret(name, data, kind='Opaque'):
    obj = {'apiVersion': 'v1', 'kind': 'Secret', 'type': kind,
        'metadata': {'name': name, 'namespace': 'hypercast'},
        'data': {k: base64.b64encode(v).decode() for k,v in data.items()}}
    kubectl('apply', '--server-side', '--field-manager=hypercast-turn', '-f', '-',
        payload=json.dumps(obj).encode())

def sync_certificate():
    secret('hypercast-turn-tls', {'tls.crt': (CERT/'fullchain.pem').read_bytes(),
        'tls.key': (CERT/'privkey.pem').read_bytes()}, 'kubernetes.io/tls')

def assert_empty():
    obj = json.loads(kubectl('-n', 'hypercast', 'get', 'secret', 'hypercast-livekit', '-o', 'json'))
    key, api_secret = base64.b64decode(obj['data']['keys']).decode().strip().split(':', 1)
    now = int(time.time())
    def b64(data): return base64.urlsafe_b64encode(data).rstrip(b'=')
    claims = {'iss': key.strip(), 'sub': 'hypercast-operator', 'nbf': now-5,
        'exp': now+60, 'video': {'roomList': True}}
    msg = b64(b'{"alg":"HS256","typ":"JWT"}') + b'.' + b64(json.dumps(claims).encode())
    token = msg + b'.' + b64(hmac.new(api_secret.strip().encode(), msg, hashlib.sha256).digest())
    req = urllib.request.Request('http://127.0.0.1:8100/twirp/livekit.RoomService/ListRooms',
        data=b'{}', headers={'Authorization': 'Bearer '+token.decode(), 'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=5) as response: rooms = json.load(response)
    if any(room.get('num_participants', 0) for room in rooms.get('rooms', [])):
        raise RuntimeError('Active participants: drain rooms before SFU activation')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--template', type=Path)
    parser.add_argument('--activate-sfu', action='store_true')
    parser.add_argument('--renew-certificate', action='store_true')
    args = parser.parse_args()
    if os.geteuid() != 0: raise RuntimeError('Run as root on the Kubernetes node')
    ROOT.mkdir(mode=0o700, exist_ok=True)
    if args.renew_certificate:
        sync_certificate()
        kubectl('-n', 'hypercast', 'rollout', 'restart', 'deployment/hypercast-turn')
        print('TURN certificate synchronized; TURN workload restarting')
        return
    if not args.template: parser.error('--template is required')
    path = ROOT/'turn.secret'
    if not path.exists():
        with path.open('x', encoding='utf-8') as stream:
            os.chmod(path, 0o600)
            stream.write(secrets.token_hex(32))
    turn_secret = path.read_text(encoding='utf-8').strip()
    if len(turn_secret) < 64: raise RuntimeError('TURN secret is too short')
    template = args.template.read_text(encoding='utf-8')
    if template.count('@TURN_SECRET@') != 1: raise RuntimeError('Invalid TURN template')
    secret('hypercast-turn-config', {'turnserver.conf': template.replace('@TURN_SECRET@',turn_secret).encode()})
    sync_certificate()
    if args.activate_sfu:
        assert_empty()
        deployment = json.loads(kubectl('-n','hypercast','get','deployment','hypercast-sfu','-o','json'))
        listener = next(p['containerPort'] for c in deployment['spec']['template']['spec']['containers']
            if c['name']=='sfu' for p in c['ports'] if p['name']=='signal')
        config = json.loads(kubectl('-n','hypercast','get','configmap','hypercast-sfu','-o','json'))['data']['livekit.yaml']
        if 'turn_servers:' in config: raise RuntimeError('TURN already configured in source ConfigMap')
        extra = ('rtc:\n  turn_servers:\n    - host: turn.hypercast.cloudlane.com.br\n'
            '      port: 5349\n      protocol: tls\n      secret: '+turn_secret+'\n')
        if config.count('rtc:\n') != 1: raise RuntimeError('Unexpected SFU config structure')
        config = config.replace('rtc:\n', extra)
        config = config.replace('port: 8100\n',f'port: {listener}\n',1)
        secret('hypercast-sfu-turn-config', {'livekit.yaml': config.encode()})
        backup = ROOT/'sfu-before-turn.json'
        if not backup.exists():
            with backup.open('x', encoding='utf-8') as stream:
                os.chmod(backup,0o600); json.dump(deployment,stream)
        volumes = deployment['spec']['template']['spec']['volumes']
        volume = next(v for v in volumes if v['name']=='config')
        volume.pop('configMap', None)
        volume['secret'] = {'secretName':'hypercast-sfu-turn-config'}
        patch = {'spec':{'template':{'spec':{'volumes':volumes}}}}
        kubectl('-n','hypercast','patch','deployment','hypercast-sfu','--type=merge','--patch-file=/dev/stdin',
            payload=json.dumps(patch).encode())
        print('SFU switched to secret-backed configuration with external TURN')
    print('TURN secrets prepared; no credential output')

if __name__=='__main__': main()
