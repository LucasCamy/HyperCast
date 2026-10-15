"""Root-only Cloudlane WAN address reconciliation. Never prints credentials.

Run --apply on vm-200; default is read-only. Two HTTPS IP sources must agree.
Only the existing TURN A record and SFU advertised IPv4 are changed.
"""
import argparse
import base64
import ipaddress
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.request

ROOT = Path('/etc/hypercast')
HOST = 'turn.hypercast.cloudlane.com.br'
ZONE = 'cloudlane.com.br'
ANNOTATION = 'hypercast.cloudlane.com.br/public-ip'


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def fetch(url, *, token=None, method='GET', payload=None):
    headers = {'User-Agent': 'HyperCast-Network-Sync/1', 'Cache-Control': 'no-cache'}
    if token:
        if not url.startswith('https://api.cloudflare.com/client/v4/'):
            raise RuntimeError('Invalid credential destination')
        headers['Authorization'] = 'Bearer ' + token
    if payload is not None:
        headers['Content-Type'] = 'application/json'
    request = urllib.request.Request(url, method=method, headers=headers,
        data=None if payload is None else json.dumps(payload).encode())
    try:
        with urllib.request.build_opener(NoRedirect).open(request, timeout=8) as response:
            body = response.read(262145)
            if len(body) > 262144:
                raise RuntimeError('Network response exceeded limit')
            return body
    except urllib.error.HTTPError as error:
        raise RuntimeError(f'Network API refused operation (HTTP {error.code})') from None
    except (urllib.error.URLError, OSError):
        raise RuntimeError('Network API unreachable; configuration preserved') from None


def public_ipv4():
    addresses = [ipaddress.IPv4Address(fetch(url).decode('ascii').strip())
        for url in ('https://api.ipify.org', 'https://checkip.amazonaws.com')]
    if addresses[0] != addresses[1] or not addresses[0].is_global:
        raise RuntimeError('Public IPv4 sources disagree or are not globally routable')
    return str(addresses[0])


def replace_address(text, address):
    if not ipaddress.IPv4Address(address).is_global:
        raise ValueError('A global IPv4 is required')
    matches = list(re.finditer(r'(?m)^(  node_ip: )([^\r\n]+)$', text))
    if len(matches) != 1 or '  use_external_ip: false' not in text:
        raise RuntimeError('Unexpected SFU address configuration; manual review required')
    match = matches[0]
    old = str(ipaddress.IPv4Address(match.group(2).strip()))
    return old, text[:match.start(2)] + address + text[match.end(2):]


def k(*args, payload=None, raw=False):
    result = subprocess.run(['k3s', 'kubectl', '-n', 'hypercast', *args],
        input=None if payload is None else json.dumps(payload).encode(),
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=100)
    if result.returncode:
        raise RuntimeError('Kubernetes operation failed (details suppressed to protect secrets)')
    return result.stdout if raw else (json.loads(result.stdout) if result.stdout.strip() else {})


def cloudflare(token, path, **kwargs):
    data = json.loads(fetch('https://api.cloudflare.com/client/v4/' + path, token=token, **kwargs))
    if data.get('success') is not True:
        raise RuntimeError('Cloudflare did not confirm operation')
    return data['result']


def dns_record(token):
    zones = cloudflare(token, 'zones?name=' + ZONE)
    if len(zones) != 1 or zones[0]['name'] != ZONE or not re.fullmatch('[a-f0-9]{32}', zones[0]['id']):
        raise RuntimeError('Expected one Cloudflare zone')
    prefix = f"zones/{zones[0]['id']}/dns_records"
    records = cloudflare(token, prefix + '?type=A&name=' + HOST)
    if len(records) != 1 or records[0]['name'] != HOST or records[0]['type'] != 'A' or not re.fullmatch('[a-f0-9]{32}', records[0]['id']):
        raise RuntimeError('Expected one existing TURN A record')
    return prefix + '/' + records[0]['id'], records[0]


def reconcile(apply):
    address = public_ipv4()
    secret = k('get', 'secret', 'hypercast-sfu-turn-config', '-o', 'json')
    text = base64.b64decode(secret['data']['livekit.yaml']).decode()
    old, updated = replace_address(text, address)
    deployment = k('get', 'deployment', 'hypercast-sfu', '-o', 'json')
    volumes = deployment['spec']['template']['spec']['volumes']
    if not any(v.get('secret', {}).get('secretName') == 'hypercast-sfu-turn-config' for v in volumes):
        raise RuntimeError('SFU does not mount expected secret')
    ini = Path('/etc/letsencrypt/cloudflare-hypercast.ini').read_text()
    values = re.findall(r'(?m)^\s*dns_cloudflare_api_token\s*=\s*(\S+)\s*$', ini)
    if len(values) != 1:
        raise RuntimeError('Cloudflare token unavailable in protected certificate credentials')
    token = values[0]
    path, record = dns_record(token)
    dns_changed = record['content'] != address or record.get('proxied') is not False or record.get('ttl') != 60
    annotations = deployment['spec']['template']['metadata'].get('annotations', {})
    restart = old != address or annotations.get(ANNOTATION) != address or deployment['spec']['template']['spec'].get('terminationGracePeriodSeconds') != 30
    print(f'WAN {address}; SFU {old}; TURN DNS {record["content"]}; DNS update={dns_changed}; SFU rollout={restart}')
    if not apply:
        return
    ROOT.mkdir(mode=0o700, exist_ok=True)
    if old != address:
        # Retain an operator-only rollback snapshot without logging the secret.
        backup = ROOT / f'network-before-{secret["metadata"]["resourceVersion"]}.json'
        if not backup.exists():
            with backup.open('x') as stream:
                os.chmod(backup, 0o600)
                json.dump(secret, stream)
    if dns_changed:
        result = cloudflare(token, path, method='PATCH', payload={'content': address, 'proxied': False, 'ttl': 60})
        if result.get('content') != address or result.get('proxied') is not False:
            raise RuntimeError('DNS update was not confirmed; SFU configuration preserved')
        print('TURN DNS synchronized (DNS-only, TTL 60)')
    if old != address:
        # resourceVersion prevents overwriting an operator's concurrent edit.
        k('patch', 'secret', 'hypercast-sfu-turn-config', '--type=merge', '--patch-file=/dev/stdin', payload={
            'metadata': {'resourceVersion': secret['metadata']['resourceVersion']},
            'data': {'livekit.yaml': base64.b64encode(updated.encode()).decode()}}, raw=True)
    if restart:
        # Annotation is also a retry marker if a previous run stopped after the
        # Secret update. A changed WAN already invalidates external sessions.
        k('patch', 'deployment', 'hypercast-sfu', '--type=merge', '--patch-file=/dev/stdin', payload={
            'spec': {'template': {'metadata': {'annotations': {ANNOTATION: address}},
                'spec': {'terminationGracePeriodSeconds': 30}}}}, raw=True)
        k('rollout', 'status', 'deployment/hypercast-sfu', '--timeout=90s', raw=True)
        print('SFU address synchronized and rollout requested')
    else:
        print('Public network addressing already synchronized')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise RuntimeError('Run as root on vm-200')
    import fcntl
    with open('/run/hypercast-network-sync.lock', 'w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        reconcile(args.apply)


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Library errors may contain request or Secret data. Only explicitly
        # authored operational messages are safe for the journal.
        print(str(error) if type(error) is RuntimeError else 'Network synchronization failed; inspect configuration privately')
        raise SystemExit(1)
