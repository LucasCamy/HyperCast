"""Render a single-node SFU deployment without embedding credentials.

Outputs Kubernetes JSON (also valid YAML) for review/apply. Host-network ports
must be checked on the selected node before applying. Never deploy on PVE itself.
"""
import argparse
import ipaddress
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IMAGE = re.compile(r"ghcr\.io/lucascamy/hypercast-sfu@sha256:[0-9a-f]{64}\Z")


def render(image, node, node_ip, internet=False, cap=3, pull_secret=None, public_ip=None):
    if not IMAGE.fullmatch(image):
        raise ValueError("Use the published SFU image pinned by SHA256 digest")
    if not re.fullmatch(r"[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?", node):
        raise ValueError("Invalid Kubernetes node name")
    address = ipaddress.IPv4Address(node_ip)
    if address.is_loopback or address.is_unspecified or address.is_multicast:
        raise ValueError("A reachable node IPv4 address is required")
    if cap not in (1, 2, 3):
        raise ValueError("Publisher limit must be 1..3")
    if pull_secret is not None and not re.fullmatch(r"[a-z0-9][a-z0-9.-]{0,251}", pull_secret):
        raise ValueError("Invalid image pull secret name")
    if public_ip is not None and not ipaddress.IPv4Address(public_ip).is_global:
        raise ValueError("The advertised public IPv4 must be globally routable")
    config = (ROOT / "livekit.yaml").read_text(encoding="utf-8")
    config = config.replace("node_ip: 192.168.15.19", f"node_ip: {public_ip or address}")
    config = config.replace("use_external_ip: false", f"use_external_ip: {'true' if internet and not public_ip else 'false'}")
    config = config.replace("  packet_buffer_size_video:",
                            f"  advertise_internal_ip: true\n  ips:\n    includes: [\"{address}/32\"]\n  packet_buffer_size_video:")
    labels = {"app.kubernetes.io/name": "hypercast-sfu"}
    metadata = {"name": "hypercast-sfu", "namespace": "hypercast", "labels": labels}
    container = {
        "name": "sfu", "image": image, "imagePullPolicy": "IfNotPresent",
        "args": ["--config", "/etc/hypercast/livekit.yaml"],
        "env": [
            {"name": "LIVEKIT_KEYS", "valueFrom": {"secretKeyRef": {"name": "hypercast-livekit", "key": "keys"}}},
            {"name": "HYPERCAST_MAX_PUBLISHERS", "value": str(cap)},
        ],
        "securityContext": {"runAsNonRoot": True, "runAsUser": 65532, "runAsGroup": 65532,
                            "readOnlyRootFilesystem": True, "allowPrivilegeEscalation": False,
                            "capabilities": {"drop": ["ALL"]}, "seccompProfile": {"type": "RuntimeDefault"}},
        "resources": {"requests": {"cpu": "500m", "memory": "256Mi"},
                      "limits": {"cpu": "3", "memory": "2Gi"}},
        "ports": [{"name": "signal", "containerPort": 8100}, {"name": "ice-tcp", "containerPort": 8101}]
                 + [{"name": f"rtp-{p}", "containerPort": p, "protocol": "UDP"} for p in range(8102, 8106)],
        "volumeMounts": [{"name": "config", "mountPath": "/etc/hypercast", "readOnly": True}],
        "startupProbe": {"httpGet": {"path": "/", "port": 8100}, "periodSeconds": 2, "failureThreshold": 30},
        "readinessProbe": {"httpGet": {"path": "/", "port": 8100}, "periodSeconds": 5},
        "livenessProbe": {"httpGet": {"path": "/", "port": 8100}, "periodSeconds": 10, "failureThreshold": 6},
    }
    return {"apiVersion": "v1", "kind": "List", "items": [
        {"apiVersion": "v1", "kind": "Namespace", "metadata": {"name": "hypercast"}},
        {"apiVersion": "v1", "kind": "ConfigMap", "metadata": metadata, "data": {"livekit.yaml": config}},
        {"apiVersion": "apps/v1", "kind": "Deployment", "metadata": metadata, "spec": {
            "replicas": 1, "strategy": {"type": "Recreate"}, "revisionHistoryLimit": 3,
            "selector": {"matchLabels": labels}, "template": {"metadata": {"labels": labels}, "spec": {
                "hostNetwork": True, "dnsPolicy": "ClusterFirstWithHostNet",
                "nodeSelector": {"kubernetes.io/hostname": node}, "automountServiceAccountToken": False,
                "terminationGracePeriodSeconds": 300,
                **({"imagePullSecrets": [{"name": pull_secret}]} if pull_secret else {}),
                "containers": [container], "volumes": [{"name": "config", "configMap": {"name": "hypercast-sfu"}}],
            }}}},
        {"apiVersion": "v1", "kind": "Service", "metadata": metadata, "spec": {
            "type": "ClusterIP", "selector": labels,
            "ports": [{"name": "signal", "port": 8100, "targetPort": 8100}],
        }},
    ]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True)
    parser.add_argument("--node", required=True)
    parser.add_argument("--node-ip", required=True)
    parser.add_argument("--internet", action="store_true", help="Enable STUN external-IP discovery; does not bypass CGNAT")
    parser.add_argument("--public-ip", help="Explicit public IPv4 mapping, when STUN self-validation fails behind NAT")
    parser.add_argument("--max-publishers", type=int, default=3)
    parser.add_argument("--image-pull-secret", help="Use keel for private GHCR pulls; omit for node-local imported image")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    document = render(args.image, args.node, args.node_ip, args.internet, args.max_publishers, args.image_pull_secret, args.public_ip)
    # Refuse accidental replacement of a reviewed manifest.
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(document, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
