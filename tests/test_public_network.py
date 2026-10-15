import unittest
from unittest.mock import patch
import base64
from pathlib import Path
import tempfile
from tools import sync_public_network as sync


class PublicNetworkTests(unittest.TestCase):
    def test_rollout_retries_after_secret_was_already_updated(self):
        address = '1.1.1.1'
        calls = []
        secret = {'metadata': {'resourceVersion': '10'}, 'data': {'livekit.yaml':
            base64.b64encode(b'rtc:\n  node_ip: 1.1.1.1\n  use_external_ip: false\n').decode()}}
        deployment = {'spec': {'template': {'metadata': {'annotations': {}}, 'spec': {
            'terminationGracePeriodSeconds': 30, 'volumes': [{'secret': {'secretName': 'hypercast-sfu-turn-config'}}]}}}}
        def k(*args, **kwargs):
            calls.append((args, kwargs))
            if args[:2] == ('get', 'secret'): return secret
            if args[:2] == ('get', 'deployment'): return deployment
            return b''
        with tempfile.TemporaryDirectory() as folder, patch.object(sync, 'ROOT', Path(folder)), \
            patch.object(sync, 'public_ipv4', return_value=address), patch.object(sync, 'k', side_effect=k), \
            patch.object(sync.Path, 'read_text', return_value='dns_cloudflare_api_token = test-token'), \
            patch.object(sync, 'dns_record', return_value=('record', {'content': address, 'proxied': False, 'ttl': 60})), \
            patch.object(sync, 'cloudflare') as cf, patch('builtins.print'):
            sync.reconcile(True)
            self.assertFalse(cf.called)
            self.assertFalse(any(args[:2] == ('patch', 'secret') for args, _ in calls))
            self.assertTrue(any(args[:2] == ('patch', 'deployment') and kw.get('raw') for args, kw in calls))
            deployment['spec']['template']['metadata']['annotations'][sync.ANNOTATION] = address
            calls.clear()
            sync.reconcile(True)
            self.assertFalse(any(args[0] in ('patch', 'rollout') for args, _ in calls))

    def test_changes_only_public_candidate_and_preserves_turn_secret(self):
        config = 'rtc:\n  node_ip: 8.8.8.8\n  use_external_ip: false\n  ips:\n    includes: ["192.168.15.19/32"]\n  turn_servers:\n    - host: turn.example\n      secret: protected-value\n'
        old, updated = sync.replace_address(config, '1.1.1.1')
        self.assertEqual(old, '8.8.8.8')
        self.assertEqual(updated, config.replace('node_ip: 8.8.8.8', 'node_ip: 1.1.1.1'))
        for bad in ['127.0.0.1', '192.168.15.19', '100.64.0.1', '0.0.0.0']:
            with self.assertRaises(ValueError):
                sync.replace_address(config, bad)

    def test_refuses_disagreement_and_unexpected_config(self):
        with patch.object(sync, 'fetch', side_effect=[b'8.8.8.8', b'1.1.1.1']):
            with self.assertRaises(RuntimeError):
                sync.public_ipv4()
        with patch.object(sync, 'fetch', side_effect=[b'1.1.1.1', b'1.1.1.1']):
            self.assertEqual(sync.public_ipv4(), '1.1.1.1')
        for text in ['rtc:\n', '  node_ip: 8.8.8.8\n  use_external_ip: true',
            '  node_ip: 8.8.8.8\n  node_ip: 1.1.1.1\n  use_external_ip: false']:
            with self.assertRaises(RuntimeError):
                sync.replace_address(text, '1.1.1.1')


if __name__ == '__main__':
    unittest.main()
