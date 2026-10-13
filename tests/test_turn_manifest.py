import unittest
from tools.render_turn import render

class TurnManifestTests(unittest.TestCase):
    def test_relay_has_no_cluster_credentials_or_privileged_capabilities(self):
        pod = render()['spec']['template']['spec']
        self.assertFalse(pod['automountServiceAccountToken'])
        for container in pod['containers'] + pod['initContainers']:
            security = container['securityContext']
            self.assertTrue(security['runAsNonRoot'])
            self.assertFalse(security['allowPrivilegeEscalation'])
            self.assertEqual(security['capabilities'], {'drop': ['ALL']})
            self.assertIn('@sha256:', container['image'])
        self.assertEqual(pod['containers'][0]['ports'][0]['containerPort'], 5349)
        self.assertEqual(pod['nodeSelector']['kubernetes.io/hostname'], 'vm-200')
        self.assertTrue(all('hostPath' not in volume for volume in pod['volumes']))
