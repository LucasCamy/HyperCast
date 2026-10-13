"""Real SFU: >3 members may join; only 3 pending/active screens may publish.

No pixels captured. Requires deployed invitation API and compiled native client.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time
import uuid
from tools.room_invite import request

ROOT=Path(__file__).resolve().parent.parent

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--env-file',type=Path,default=ROOT/'.tools/homelab.env')
    args=parser.parse_args()
    room='members-'+uuid.uuid4().hex[:10]
    invite=request('/api/admin/invites',{'room':room,'role':'publisher','days':1,'max_uses':12},args.env_file)
    import urllib.request
    tokens=[]
    for i in range(4):
        body={'invite':invite['invite'],'name':f'Member{i}','request_id':uuid.uuid4().hex}
        req=urllib.request.Request('https://hypercast.cloudlane.com.br/api/join',data=json.dumps(body).encode(),
            headers={'Content-Type':'application/json','User-Agent':'HyperCast-Operator/0.1'})
        with urllib.request.urlopen(req,timeout=10) as response: tokens.append(json.load(response)['token'])
    exe=ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe'
    url='wss://hypercast.cloudlane.com.br'
    children=[]; files=[]
    def run(token,arguments,label):
        path=ROOT/'.tools'/f'{room}-{label}.log'
        stream=path.open('wb'); files.append(stream)
        child=subprocess.Popen([str(exe),*arguments],env={**os.environ,'HYPERCAST_ROOM_TOKEN':token},
            stdout=stream,stderr=subprocess.STDOUT,cwd=ROOT)
        children.append(child); return child,path
    try:
        waiting,_=run(tokens[3],['--connect-url',url,'--seconds','25'],'waiting')
        holders=[]
        for i in range(3):
            argv=['--admission-hold-url',url,'--seconds','22']
            if i==0: argv+=['--admission-release-after','8']
            holders.append(run(tokens[i],argv,f'hold{i}'))
        deadline=time.monotonic()+15
        while not all('publication accepted' in p.read_text(errors='replace') for _,p in holders):
            assert time.monotonic()<deadline,'Publishers did not reserve quota'
            time.sleep(.1)
        blocked=subprocess.run([str(exe),'--admission-probe-url',url,'--reject-first'],
            env={**os.environ,'HYPERCAST_ROOM_TOKEN':tokens[3]},capture_output=True,text=True,timeout=15)
        assert blocked.returncode==0,blocked.stdout+blocked.stderr
        assert 'rejected forbidden publisher' in blocked.stdout
        print('PASS: fourth member entered but a fourth pending screen was rejected')
        deadline=time.monotonic()+15
        while 'unpublished, remaining connected' not in holders[0][1].read_text(errors='replace'):
            assert time.monotonic()<deadline,'First publisher did not stop'
            time.sleep(.1)
        time.sleep(1)
        admitted=subprocess.run([str(exe),'--admission-probe-url',url],
            env={**os.environ,'HYPERCAST_ROOM_TOKEN':tokens[3]},capture_output=True,text=True,timeout=15)
        assert admitted.returncode==0,admitted.stdout+admitted.stderr
        assert 'rejected duplicate screen' in admitted.stdout
        print('PASS: stopping a screen freed quota while the member remained connected')
    finally:
        for child in children:
            if child.poll() is None: child.terminate(); child.wait(timeout=10)
        for stream in files: stream.close()
        request('/api/admin/revoke',{'invitation_id':invite['invitation_id']},args.env_file)

if __name__=='__main__': main()
