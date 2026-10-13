"""Run as root after deploying access workload. No participants may be connected."""
import json
import sys
from pathlib import Path
sys.path.insert(0,'/etc/hypercast')
from deploy_turn_on_node import assert_empty, kubectl, secret

def main():
    assert_empty()
    obj=json.loads(kubectl('-n','hypercast','get','secret','hypercast-sfu-turn-config','-o','json'))
    import base64
    config=base64.b64decode(obj['data']['livekit.yaml']).decode()
    if not config.startswith('port: 8100\n'): raise RuntimeError('SFU port is already changed or unexpected')
    config=config.replace('port: 8100\n','port: 8107\n',1)
    secret('hypercast-sfu-turn-config',{'livekit.yaml':config.encode()})
    obj=json.loads(kubectl('-n','hypercast','get','deployment','hypercast-sfu','-o','json'))
    spec=obj['spec']['template']['spec']
    for container in spec['containers']:
        if container['name']!='sfu': continue
        for probe in ('startupProbe','readinessProbe','livenessProbe'):
            container[probe]['httpGet']['port']=8107
        for port in container['ports']:
            if port['name']=='signal': port['containerPort']=8107
    kubectl('-n','hypercast','patch','deployment','hypercast-sfu','--type=merge','--patch-file=/dev/stdin',
        payload=json.dumps({'spec':{'template':{'spec':spec}}}).encode())
    kubectl('-n','hypercast','patch','service','hypercast-sfu','--type=merge','--patch-file=/dev/stdin',
        payload=json.dumps({'spec':{'ports':[{'name':'signal','port':8100,'targetPort':8107}]}}).encode())
    print('SFU moved to internal 8107; existing tunnel continues using gateway 8100')

if __name__=='__main__': main()
