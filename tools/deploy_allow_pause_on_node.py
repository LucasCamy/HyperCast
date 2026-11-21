"""Root-only, drained-room switch of the SFU to never pause single-layer video.

Run on the existing k3s node from /etc/hypercast, next to deploy_turn_on_node.py.
Edits only `congestion_control.allow_pause` in the ConfigMap and the TURN config
Secret, writes a private rollback snapshot, restarts the SFU and restores the
previous configuration if the rollout fails. No credentials are printed.
"""
import base64
import json
import os
from pathlib import Path
import runpy
import subprocess

ROOT = Path('/etc/hypercast')


def k(*args, payload=None):
    return subprocess.check_output(['k3s', 'kubectl', '-n', 'hypercast', *args], input=payload, timeout=150)


def pause_config(text):
    """Return `text` with allow_pause disabled; reject unexpected layouts."""
    disabled = '    allow_pause: false\n'
    if text.count(disabled) == 1 and '    allow_pause: true\n' not in text:
        return text
    if text.count('  congestion_control:\n') != 1 or text.count('    allow_pause: true\n') != 1:
        raise RuntimeError('Unexpected congestion_control configuration')
    return text.replace('    allow_pause: true\n', disabled)


def main():
    if os.geteuid() != 0:
        raise RuntimeError('Run as root on the Kubernetes node')
    runpy.run_path(str(ROOT / 'deploy_turn_on_node.py'))['assert_empty']()
    original_secret = json.loads(k('get', 'secret', 'hypercast-sfu-turn-config', '-o', 'json'))
    original_config = json.loads(k('get', 'configmap', 'hypercast-sfu', '-o', 'json'))
    secret_text = base64.b64decode(original_secret['data']['livekit.yaml']).decode()
    config_text = original_config['data']['livekit.yaml']
    next_secret, next_config = pause_config(secret_text), pause_config(config_text)
    if (next_secret, next_config) == (secret_text, config_text):
        print('allow_pause already disabled; nothing to do')
        return
    backup = ROOT / 'allow-pause-rollback.json'
    if backup.exists():
        raise RuntimeError('allow_pause rollback snapshot already exists; inspect before another rollout')
    with backup.open('x') as stream:
        os.chmod(backup, 0o600)
        json.dump({'secret': original_secret, 'configmap': original_config}, stream)

    def patch(resource, name, obj):
        k('patch', resource, name, '--type=merge', '--patch-file=/dev/stdin', payload=json.dumps(obj).encode())

    def restart():
        k('rollout', 'restart', 'deployment/hypercast-sfu')
        k('rollout', 'status', 'deployment/hypercast-sfu', '--timeout=120s')

    try:
        patch('secret', 'hypercast-sfu-turn-config',
              {'data': {'livekit.yaml': base64.b64encode(next_secret.encode()).decode()}})
        patch('configmap', 'hypercast-sfu', {'data': {'livekit.yaml': next_config}})
        restart()
        print('allow_pause disabled; SFU healthy')
    except Exception:
        patch('secret', 'hypercast-sfu-turn-config', {'data': original_secret['data']})
        patch('configmap', 'hypercast-sfu', {'data': original_config['data']})
        restart()
        raise RuntimeError('allow_pause rollout failed; original configuration restored') from None


if __name__ == '__main__':
    main()
