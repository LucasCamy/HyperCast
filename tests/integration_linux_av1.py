"""An owned generated FFplay window on Windows -> WGC/NVENC AV1 -> Linux decoder.
No desktop recording; only generated test content is shared in a unique room.
"""
import json,os,re,subprocess,time,urllib.request,uuid
from pathlib import Path
from tools import room_token as rt
ROOT=Path(__file__).resolve().parent.parent

def main():
    key,secret=rt.load_env(ROOT/'.tools/homelab.env');room='linux-av1-'+uuid.uuid4().hex[:10];now=int(time.time())
    def token(pub):
        grant={'room':room,'roomJoin':True,'canPublish':pub,'canSubscribe':True,'canPublishData':False}
        if pub:grant['canPublishSources']=['screen_share','screen_share_audio']
        return rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+180,'video':grant},secret)
    base={k:v for k,v in os.environ.items() if not k.startswith(('LIVEKIT_','HYPERCAST_','GH_','GITHUB_'))};children=[];logs=[]
    binary=os.environ.get('HYPERCAST_LINUX_EXE','/home/lucas/hypercast-target/release/hypercast')
    try:
        app=subprocess.Popen([str(ROOT/'.tools/ffmpeg/bin/ffplay.exe'),'-window_title','HyperCast generated AV1 test','-x','640','-y','360','-f','lavfi','-i','testsrc2=size=640x360:rate=60'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);children.append(app);time.sleep(2)
        file=(ROOT/'.tools/linux-av1-receiver.log').open('wb');logs.append(file)
        args=[binary,*(['--appimage-extract-and-run'] if binary.endswith('.AppImage') else []),'--receive-url','wss://hypercast.cloudlane.com.br','--seconds','15']
        receiver=subprocess.Popen(['wsl','-d','Ubuntu','--',*args],env={**base,'HYPERCAST_ROOM_TOKEN':token(False),'WSLENV':'HYPERCAST_ROOM_TOKEN/u'},stdout=file,stderr=subprocess.STDOUT);children.append(receiver)
        file=(ROOT/'.tools/linux-av1-publisher.log').open('wb');logs.append(file)
        windows=os.environ.get('HYPERCAST_WINDOWS_EXE',str(ROOT/'dist/HyperCast-0.6.5-windows-x64/hypercast.exe'))
        env={**base,'HYPERCAST_URL':'wss://hypercast.cloudlane.com.br','HYPERCAST_ROOM_TOKEN':token(True),'HYPERCAST_AUTO_CONNECT':'1','HYPERCAST_SMOKE_SECONDS':'12','HYPERCAST_SMOKE_SHARE':'1','HYPERCAST_SMOKE_WINDOW_PID':str(app.pid),'HYPERCAST_CODEC':'av1'}
        publisher=subprocess.Popen([windows,'--gui'],env=env,stdout=file,stderr=subprocess.STDOUT);children.append(publisher)
        code=receiver.wait(timeout=40);assert code==0,f'Linux AV1 receive failed with exit {code}; inspect scoped local log'
        assert publisher.wait(timeout=15)==0,'Windows AV1 publish failed; inspect scoped local log'
        for file in logs:file.close()
        output=(ROOT/'.tools/linux-av1-receiver.log').read_text(encoding='utf-8',errors='replace')
        assert int(re.search(r'Receiver total: (\d+) decoded frames',output)[1])>100,'AV1 frames were not decoded'
        print('PASS: Windows WGC/NVENC AV1 -> SFU -> Linux decoded frames')
    finally:
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for file in logs:file.close()
        admin=rt.sign({'iss':key,'sub':'operator','nbf':now-5,'exp':now+180,'video':{'room':room,'roomAdmin':True,'roomCreate':True}},secret)
        request=urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom',data=json.dumps({'room':room}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+admin,'User-Agent':'HyperCast-Operator/0.7'})
        try:
            with urllib.request.urlopen(request,timeout=10):pass
        except urllib.error.HTTPError as error:
            if error.code!=404:raise
if __name__=='__main__':main()
