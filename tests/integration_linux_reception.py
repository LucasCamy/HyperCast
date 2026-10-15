"""Linux receiver engine: two synthetic streams, rapid focus, receive-only resets.
The ignored Rust workload uses the same commands as the GUI, without a desktop.
"""
import json,os,subprocess,time,urllib.request,uuid
from pathlib import Path
from tools import room_token as rt
ROOT=Path(__file__).resolve().parent.parent

def main():
    artifacts=[json.loads(line) for line in (ROOT/'.tools/linux-reception-build.jsonl').read_text(encoding='utf-8').splitlines() if line.startswith('{')]
    paths=[a['executable'] for a in artifacts if a.get('reason')=='compiler-artifact' and a.get('target',{}).get('name')=='hypercast' and a.get('executable')]
    assert len(paths)==1 and paths[0].startswith('/home/lucas/hypercast-target/debug/deps/hypercast-'),'Unexpected Linux test artifact'
    binary=paths[0];key,secret=rt.load_env(ROOT/'.tools/homelab.env');room='receive-linux-'+uuid.uuid4().hex[:12];now=int(time.time())
    def token(identity,pub):
        grant={'room':room,'roomJoin':True,'canPublish':pub,'canSubscribe':True,'canPublishData':False}
        if pub:grant['canPublishSources']=['screen_share','screen_share_audio']
        return rt.sign({'iss':key,'sub':identity,'nbf':now-5,'exp':now+180,'video':grant},secret)
    base={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_','GH_','GITHUB_'))};children=[];logs=[]
    def launch(name,args,jwt):
        log=(ROOT/f'.tools/receive-linux-{name}.log').open('wb');logs.append(log)
        env={**base,'HYPERCAST_ROOM_TOKEN':jwt,'HYPERCAST_URL':'wss://hypercast.cloudlane.com.br','HYPERCAST_AUDIO_TEST_SILENT_OUTPUT':'1','WSLENV':'HYPERCAST_ROOM_TOKEN/u:HYPERCAST_URL/u:HYPERCAST_AUDIO_TEST_SILENT_OUTPUT/u'}
        child=subprocess.Popen(['wsl','-d','Ubuntu','--',*args],env=env,stdout=log,stderr=subprocess.STDOUT);children.append(child);return child
    try:
        for slot in [1,2]:
            launch(str(slot),['/mnt/f/HyperCast/dist/HyperCast-0.7.2-linux-x86_64.AppImage','--appimage-extract-and-run','--linux-publish-test-url','wss://hypercast.cloudlane.com.br','--seconds','55'],token(f'hc-screen-{slot}',True))
        viewer=launch('test',[binary,'receiver_switch_and_refresh_cycles','--ignored','--nocapture'],token('hc-member-'+uuid.uuid4().hex,False))
        assert viewer.wait(timeout=65)==0,'Linux receive-only restart test failed; inspect scoped log'
        print('PASS: Linux receiver rapid switching, three receive-only restarts, fresh selected frames and decoder teardown')
    finally:
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for log in logs:log.close()
        admin=rt.sign({'iss':key,'sub':'operator','nbf':now-5,'exp':now+180,'video':{'room':room,'roomAdmin':True,'roomCreate':True}},secret)
        request=urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom',data=json.dumps({'room':room}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+admin,'User-Agent':'HyperCast-Operator/0.7'})
        try:
            with urllib.request.urlopen(request,timeout=10):pass
        except urllib.error.HTTPError as error:
            if error.code!=404:raise
if __name__=='__main__':main()
