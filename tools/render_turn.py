"""Coturn workload for the single-node Cloudlane SFU. Secrets supplied separately."""
import argparse
import json
from pathlib import Path

IMAGE = 'ghcr.io/coturn/coturn@sha256:c29135c08565810d37053f384584392b7c24b69570fc81a721b878d6fe63e04e'

def render():
    labels = {'app.kubernetes.io/name': 'hypercast-turn'}
    return {'apiVersion': 'apps/v1', 'kind': 'Deployment',
        'metadata': {'name': 'hypercast-turn', 'namespace': 'hypercast', 'labels': labels},
        'spec': {'replicas': 1, 'strategy': {'type': 'Recreate'}, 'revisionHistoryLimit': 3,
            'selector': {'matchLabels': labels}, 'template': {'metadata': {'labels': labels}, 'spec': {
                'nodeSelector': {'kubernetes.io/hostname': 'vm-200'}, 'hostNetwork': True,
                'dnsPolicy': 'ClusterFirstWithHostNet', 'automountServiceAccountToken': False,
                'securityContext': {'fsGroup': 65532},
                # Upstream binary has file capabilities. Copy its bytes into an
                # emptyDir without xattrs to run on 5349 with all capabilities dropped.
                'initContainers': [{'name': 'unprivileged-binary', 'image': IMAGE,
                    'command': ['/bin/cp', '/usr/bin/turnserver', '/opt/turn/turnserver'],
                    'securityContext': {'runAsNonRoot': True, 'runAsUser': 65532, 'runAsGroup': 65532,
                        'allowPrivilegeEscalation': False, 'readOnlyRootFilesystem': True,
                        'capabilities': {'drop': ['ALL']}, 'seccompProfile': {'type': 'RuntimeDefault'}},
                    'volumeMounts': [{'name': 'binary', 'mountPath': '/opt/turn'}]}],
                'containers': [{'name': 'turn', 'image': IMAGE, 'imagePullPolicy': 'IfNotPresent',
                    'command': ['/opt/turn/turnserver'], 'args': ['-c', '/etc/turn-config/turnserver.conf'],
                    'securityContext': {'runAsNonRoot': True, 'runAsUser': 65532, 'runAsGroup': 65532,
                        'allowPrivilegeEscalation': False, 'readOnlyRootFilesystem': True,
                        'capabilities': {'drop': ['ALL']}, 'seccompProfile': {'type': 'RuntimeDefault'}},
                    'resources': {'requests': {'cpu': '100m', 'memory': '64Mi'},
                        'limits': {'cpu': '1', 'memory': '256Mi'}},
                    'ports': [{'name': 'turn-tls', 'containerPort': 5349}],
                    'startupProbe': {'tcpSocket': {'port': 5349}, 'periodSeconds': 2, 'failureThreshold': 30},
                    'readinessProbe': {'tcpSocket': {'port': 5349}, 'periodSeconds': 5},
                    'livenessProbe': {'tcpSocket': {'port': 5349}, 'periodSeconds': 10},
                    'volumeMounts': [{'name': 'config', 'mountPath': '/etc/turn-config', 'readOnly': True},
                        {'name': 'binary', 'mountPath': '/opt/turn', 'readOnly': True},
                        {'name': 'tls', 'mountPath': '/etc/turn-tls', 'readOnly': True},
                        {'name': 'tmp', 'mountPath': '/tmp'}, {'name': 'db', 'mountPath': '/var/lib/coturn'}]}],
                'volumes': [{'name': 'config', 'secret': {'secretName': 'hypercast-turn-config', 'defaultMode': 288}},
                    {'name': 'binary', 'emptyDir': {'medium': 'Memory', 'sizeLimit': '16Mi'}},
                    {'name': 'tls', 'secret': {'secretName': 'hypercast-turn-tls', 'defaultMode': 288}},
                    {'name': 'tmp', 'emptyDir': {'medium': 'Memory', 'sizeLimit': '16Mi'}},
                    {'name': 'db', 'emptyDir': {'medium': 'Memory', 'sizeLimit': '16Mi'}}]}}}}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(render(), indent=2), encoding='utf-8')
