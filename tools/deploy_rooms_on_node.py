"""Run as root: update only the existing access workload, with rollback."""
import json
import os
from pathlib import Path
import subprocess
import sys
sys.path.insert(0,'/etc/hypercast')
from deploy_turn_on_node import assert_empty, kubectl

ROOT=Path('/etc/hypercast')

def main():
    # Explicit operator authorization is required to interrupt active signaling.
    if "--drain-approved" not in sys.argv[1:]: assert_empty()
    ref='docker.io/library/hypercast-auth:rooms'
    subprocess.run(['k3s','ctr','images','import',str(ROOT/'hypercast-auth-rooms.tar')],check=True,stdout=subprocess.DEVNULL,timeout=120)
    rows=subprocess.check_output(['k3s','ctr','images','list'],timeout=20).decode().splitlines()
    digest=next(row.split()[2] for row in rows if row.split()[0]==ref)
    if not digest.startswith('sha256:') or len(digest)!=71: raise RuntimeError('Invalid image digest')
    target='ghcr.io/lucascamy/hypercast-auth@'+digest
    subprocess.run(['k3s','ctr','images','tag','--force',ref,target],check=True,stdout=subprocess.DEVNULL,timeout=20)
    original=json.loads(kubectl('-n','hypercast','get','deployment','hypercast-access','-o','json'))
    backup=ROOT/('rooms-rollback-'+digest[7:19]+'.json')
    with backup.open('x') as stream:
        os.chmod(backup,0o600);json.dump(original,stream)
    previous=next(c['image'] for c in original['spec']['template']['spec']['containers'] if c['name']=='auth')
    try:
        kubectl('-n','hypercast','set','image','deployment/hypercast-access','auth='+target)
        kubectl('-n','hypercast','rollout','status','deployment/hypercast-access','--timeout=120s')
    except Exception:
        kubectl('-n','hypercast','set','image','deployment/hypercast-access','auth='+previous)
        kubectl('-n','hypercast','rollout','status','deployment/hypercast-access','--timeout=120s')
        raise RuntimeError('Room API rollout failed; restored original access image') from None
    print('Room API healthy; SFU and media ports unchanged')
    print(target)

if __name__=='__main__':main()
