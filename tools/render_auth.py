"""Native-client invitation API and HTTP gateway for the existing Cloudlane Tunnel."""
import argparse
import json
from pathlib import Path
import re

CADDY='docker.io/library/caddy@sha256:d8542f48d34a9cf4e4c11a478865229840e87e4c96ea3f439101f31a5d35f75f'

def render(image):
    if not re.fullmatch(r'ghcr.io/lucascamy/hypercast-auth@sha256:[0-9a-f]{64}',image):
        raise ValueError('Pin the invitation image by digest')
    labels={'app.kubernetes.io/name':'hypercast-access'}
    security={'runAsNonRoot':True,'runAsUser':65532,'runAsGroup':65532,
        'allowPrivilegeEscalation':False,'readOnlyRootFilesystem':True,
        'capabilities':{'drop':['ALL']},'seccompProfile':{'type':'RuntimeDefault'}}
    mounts=[{'name':'binary','mountPath':'/opt/gateway','readOnly':True},
        {'name':'gateway-config','mountPath':'/etc/caddy','readOnly':True},
        {'name':'tmp','mountPath':'/tmp'}]
    containers=[{'name':'auth','image':image,'imagePullPolicy':'IfNotPresent',
        'securityContext':security,'env':[{'name':'LIVEKIT_KEYS','valueFrom':{'secretKeyRef':{'name':'hypercast-livekit','key':'keys'}}}],
        'volumeMounts':[{'name':'data','mountPath':'/data'}],
        'resources':{'requests':{'cpu':'100m','memory':'64Mi'},'limits':{'cpu':'500m','memory':'128Mi'}},
        'readinessProbe':{'httpGet':{'host':'127.0.0.1','path':'/health','port':8106},'periodSeconds':5},
        'livenessProbe':{'httpGet':{'host':'127.0.0.1','path':'/health','port':8106},'periodSeconds':10}},
        {'name':'gateway','image':CADDY,'command':['/opt/gateway/caddy'],
            'args':['run','--config','/etc/caddy/Caddyfile','--adapter','caddyfile'],
            'securityContext':security,'env':[{'name':'XDG_DATA_HOME','value':'/tmp/data'},
                {'name':'XDG_CONFIG_HOME','value':'/tmp/config'}], 'volumeMounts':mounts,
            'resources':{'requests':{'cpu':'50m','memory':'32Mi'},'limits':{'cpu':'500m','memory':'128Mi'}},
            'readinessProbe':{'httpGet':{'path':'/','port':8100},'periodSeconds':5},
            'livenessProbe':{'httpGet':{'path':'/','port':8100},'periodSeconds':10,'failureThreshold':6}}]
    return {'apiVersion':'v1','kind':'List','items':[
        {'apiVersion':'v1','kind':'PersistentVolumeClaim','metadata':{'namespace':'hypercast','name':'hypercast-admission'},
            'spec':{'accessModes':['ReadWriteOnce'],'storageClassName':'local-path','resources':{'requests':{'storage':'128Mi'}}}},
        {'apiVersion':'v1','kind':'ConfigMap','metadata':{'namespace':'hypercast','name':'hypercast-gateway'},
            'data':{'Caddyfile':Path('infra/Caddyfile').read_text(encoding='utf-8')}},
        {'apiVersion':'apps/v1','kind':'Deployment','metadata':{'namespace':'hypercast','name':'hypercast-access'},
            'spec':{'replicas':1,'strategy':{'type':'Recreate'},'selector':{'matchLabels':labels},
                'template':{'metadata':{'labels':labels},'spec':{'hostNetwork':True,'dnsPolicy':'ClusterFirstWithHostNet',
                    'nodeSelector':{'kubernetes.io/hostname':'vm-200'},'automountServiceAccountToken':False,
                    'securityContext':{'fsGroup':65532},
                    'initContainers':[{'name':'unprivileged-gateway','image':CADDY,'securityContext':security,
                        'command':['/bin/cp','/usr/bin/caddy','/opt/gateway/caddy'],
                        'volumeMounts':[{'name':'binary','mountPath':'/opt/gateway'}]}],
                    'containers':containers,'volumes':[
                        {'name':'data','persistentVolumeClaim':{'claimName':'hypercast-admission'}},
                        {'name':'gateway-config','configMap':{'name':'hypercast-gateway'}},
                        {'name':'binary','emptyDir':{'medium':'Memory','sizeLimit':'128Mi'}},
                        {'name':'tmp','emptyDir':{'medium':'Memory','sizeLimit':'16Mi'}}]}}}}]}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image',required=True); parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    args.output.write_text(json.dumps(render(args.image),indent=2),encoding='utf-8')
