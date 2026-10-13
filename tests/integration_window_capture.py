"""Synthetic application window -> WGC/NVENC/SFU -> GPU video and process audio.

Only generated FFplay media, no recording. Tests resizing, same video SID across
window/monitor switches, fullscreen entry/exit, and close-without-desktop-fallback.
"""
import ctypes
import json
import os
from pathlib import Path
import re
import subprocess
import time
import urllib.request
import uuid
from tools import room_token as rt
from tests.integration_desktop import owned_window, screenshot

ROOT=Path(__file__).resolve().parent.parent

def main():
    key,secret=rt.load_env(ROOT/'.tools/homelab.env')
    room='window-'+uuid.uuid4().hex[:12];now=int(time.time())
    def token(pub):
        grant={'room':room,'roomJoin':True,'canPublish':pub,'canSubscribe':True,'canPublishData':False}
        if pub:grant['canPublishSources']=['screen_share','screen_share_audio']
        return rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+300,'video':grant},secret)
    admin=rt.sign({'iss':key,'sub':'operator','nbf':now-5,'exp':now+90,'video':{'room':room,'roomAdmin':True}},secret)
    base={k:v for k,v in os.environ.items() if not k.startswith('LIVEKIT_') and not k.startswith('HYPERCAST_SMOKE_')}
    base.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_AUTO_CONNECT='1',HYPERCAST_RELAY_ONLY='0')
    exe=os.environ.get('HYPERCAST_TEST_EXE',str(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe'))
    children=[];files=[];sids=set();counts=[]
    try:
        app=subprocess.Popen([str(ROOT/'.tools/ffmpeg/bin/ffplay.exe'),'-window_title','HyperCast synthetic window',
            '-x','640','-y','360','-volume','10','-f','lavfi','-i',
            'testsrc2=size=640x360:rate=60[out0];sine=frequency=997:sample_rate=48000[out1]'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        children.append(app);time.sleep(2);user,app_hwnd=owned_window(app.pid)
        user.SetWindowPos.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_uint]
        for pub,label,duration in [(False,'viewer',23),(True,'publisher',21)]:
            file=(ROOT/'.tools'/f'window-{label}.log').open('wb');files.append(file)
            env={**base,'HYPERCAST_ROOM_TOKEN':token(pub),'HYPERCAST_SMOKE_SECONDS':str(duration)}
            if pub:env.update(HYPERCAST_SMOKE_SHARE='1',HYPERCAST_SMOKE_SOURCE_SWITCH='1',HYPERCAST_SMOKE_WINDOW_PID=str(app.pid),HYPERCAST_SYSTEM_AUDIO='1')
            else:env.update(HYPERCAST_SMOKE_FULLSCREEN='1',HYPERCAST_AUDIO_TEST_SILENT_OUTPUT='1')
            children.append(subprocess.Popen([exe,'--gui'],cwd=ROOT,env=env,stdout=file,stderr=subprocess.STDOUT));time.sleep(1)
        started=time.monotonic();resized=False;closed=False;shot=False;fullscreen_seen=False
        while children[-1].poll() is None:
            elapsed=time.monotonic()-started
            if elapsed>11 and not resized:
                assert user.SetWindowPos(app_hwnd,None,0,0,854,510,6);resized=True
            if elapsed>15 and not closed:app.terminate();app.wait(timeout=10);closed=True
            viewer_user,viewer_hwnd=owned_window(children[1].pid)
            rect=ctypes.wintypes.RECT();viewer_user.GetWindowRect(viewer_hwnd,ctypes.byref(rect))
            fullscreen_seen|=rect.left==0 and rect.top==0 and rect.right>=1900
            if elapsed>10 and not shot:screenshot(children[1].pid,ROOT/'.tools/layout-fullscreen.png');shot=True
            req=urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/ListParticipants',
                data=json.dumps({'room':room}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+admin,'User-Agent':'HyperCast-Operator/0.4'})
            with urllib.request.urlopen(req,timeout=5) as response:members=json.load(response).get('participants',[])
            tracks=[t for p in members for t in p.get('tracks',[]) if t.get('type') in ('VIDEO',1)]
            counts.append((elapsed,len(tracks)))
            sids.update(t['sid'] for t in tracks)
            assert elapsed<35,'GUI timeout';time.sleep(.4)
        for child in children[1:]:assert child.wait(timeout=15)==0
        for file in files:file.close()
        diagnostics=Path(os.environ['LOCALAPPDATA'])/'HyperCast/diagnostics'
        logs=[next(diagnostics.glob(f'gui-*-{c.pid}.log')).read_text(errors='replace') for c in children[1:]]
        viewer,publisher=logs
        assert publisher.count(' source_switch:')==2,'Missing monitor/window switches'
        assert len(sids)==1,f'Switch changed video publication: {len(sids)} SIDs'
        assert 'Application audio published:' in publisher and publisher.count('audio_source_switch:')==2
        assert 'Decoded non-silent system audio' in viewer,'Application audio missing'
        assert max(map(int,re.findall(r'received=(\d+)',viewer)),default=0)>200
        assert 'foi fechada' in publisher,'Closing source did not terminate capture'
        assert any(t>17 and n==0 for t,n in counts),'Closing window retained a video track'
        assert fullscreen_seen,'Fullscreen not entered'
        assert 'source_error:' not in publisher and 'publication_error:' not in publisher
        assert 'capture_error:' not in publisher.split('foi fechada')[0].rsplit('\n',1)[0],'Unexpected capture failure'
        print('PASS: WGC GPU capture, process audio, resize, same video SID across two switches, fullscreen, closed-window privacy')
    finally:
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for file in files:file.close()

if __name__=='__main__':main()
