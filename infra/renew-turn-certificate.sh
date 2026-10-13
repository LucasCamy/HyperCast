#!/bin/sh
set -eu
if [ "${RENEWED_LINEAGE:-}" = "/etc/letsencrypt/live/hypercast-turn" ]; then
    /usr/bin/python3 /etc/hypercast/deploy_turn_on_node.py --renew-certificate
fi
