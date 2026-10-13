import unittest
from tools.render_k8s import render

IMAGE = "ghcr.io/lucascamy/hypercast-sfu@sha256:" + "a" * 64


class DeploymentTests(unittest.TestCase):
    def test_single_host_network_node_and_no_secrets(self):
        result = render(IMAGE, "k3s-node", "192.168.15.19", internet=True)
        pod = result["items"][2]["spec"]["template"]["spec"]
        self.assertTrue(pod["hostNetwork"])
        self.assertEqual(pod["nodeSelector"], {"kubernetes.io/hostname": "k3s-node"})
        self.assertFalse(pod["automountServiceAccountToken"])
        self.assertEqual(pod["containers"][0]["env"][0]["valueFrom"]["secretKeyRef"]["key"], "keys")
        self.assertEqual(result["items"][3]["spec"]["type"], "ClusterIP")
        self.assertIn("use_external_ip: true", result["items"][1]["data"]["livekit.yaml"])

    def test_fail_closed_for_unpinned_images_and_invalid_node(self):
        for image, node, ip, cap in [
            ("livekit/livekit-server:latest", "k3s-node", "192.168.15.19", 3),
            (IMAGE, "node\nother", "192.168.15.19", 3),
            (IMAGE, "k3s-node", "0.0.0.0", 3),
            (IMAGE, "k3s-node", "192.168.15.19", 4),
        ]:
            with self.assertRaises(ValueError):
                render(image, node, ip, cap=cap)

    def test_explicit_public_mapping_without_stun_and_private_ip_rejected(self):
        result = render(IMAGE, "vm-200", "192.168.15.19", public_ip="8.8.8.8")
        config = result["items"][1]["data"]["livekit.yaml"]
        self.assertIn("node_ip: 8.8.8.8", config)
        self.assertIn("use_external_ip: false", config)
        self.assertIn('includes: ["192.168.15.19/32"]', config)
        with self.assertRaises(ValueError):
            render(IMAGE, "vm-200", "192.168.15.19", public_ip="100.64.0.1")


if __name__ == "__main__":
    unittest.main()
