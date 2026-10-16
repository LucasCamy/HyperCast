"""Cut only this test client's signaling TCP proxy; never restart production.

Unique room, ephemeral JWT, one synthetic application window. Socket contents
(including JWT) are forwarded without recording or inspecting them.
"""
import asyncio
import os
from pathlib import Path
import re
import subprocess
import threading
import time
import uuid
from tools import room_token as rt
from tests.integration_desktop import owned_window

ROOT=Path(__file__).resolve().parent.parent

def main():
    exhausted=os.environ.get('HYPERCAST_TEST_RECONNECT_EXHAUST')=='1'
    key,secret=rt.load_env(ROOT/'.tools/homelab.env');now=int(time.time());room='reconnect-'+uuid.uuid4().hex[:12]
    token=rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+300,
        'video':{'room':room,'roomJoin':True,'canPublish':True,'canSubscribe':True,'canPublishData':False,
                 'canPublishSources':['screen_share','screen_share_audio']}},secret)
    loop=asyncio.new_event_loop();writers=set();blocked=False;refused=False;ready=threading.Event();address=[]
    async def serve(reader,writer):
        nonlocal refused
        other=None
        try:
            # Refuse the first resume, exercising a full SDK restart next.
            if blocked and (exhausted or not refused):
                refused=True
                return
            while blocked: await asyncio.sleep(0.05)
            upstream,other=await asyncio.open_connection('192.168.15.19',8100)
            writers.update((writer,other))
            async def pipe(src,dst):
                while data:=await src.read(65536):dst.write(data);await dst.drain()
            tasks=[asyncio.create_task(pipe(reader,other)),asyncio.create_task(pipe(upstream,writer))]
            await asyncio.wait(tasks,return_when=asyncio.FIRST_COMPLETED)
            for task in tasks:task.cancel()
            await asyncio.gather(*tasks,return_exceptions=True)
        except (ConnectionError,OSError):pass
        finally:
            for w in (writer,other):
                if w:writers.discard(w);w.close()
    async def start():
        server=await asyncio.start_server(serve,'127.0.0.1',0)
        address.append(server.sockets[0].getsockname()[1]);ready.set()
    def proxy():
        asyncio.set_event_loop(loop);loop.run_until_complete(start());loop.run_forever()
        pending=asyncio.all_tasks(loop)
        for task in pending:task.cancel()
        loop.run_until_complete(asyncio.gather(*pending,return_exceptions=True));loop.close()
    proxy_thread=threading.Thread(target=proxy,daemon=True);proxy_thread.start();assert ready.wait(5)
    app=child=viewer=None
    viewer_output=None
    try:
        app=subprocess.Popen([str(ROOT/'.tools/ffmpeg/bin/ffplay.exe'),'-window_title','HyperCast reconnect test',
            '-x','640','-y','360','-volume','10','-f','lavfi','-i',
            'testsrc2=size=640x360:rate=60[out0];sine=frequency=997:sample_rate=48000[out1]'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        time.sleep(2);owned_window(app.pid)
        env={k:v for k,v in os.environ.items() if not k.startswith('LIVEKIT_') and not k.startswith('HYPERCAST_SMOKE_')}
        env.update(HYPERCAST_URL=f'ws://127.0.0.1:{address[0]}',HYPERCAST_ROOM_TOKEN=token,HYPERCAST_AUTO_CONNECT='1',
            HYPERCAST_SMOKE_SECONDS='27',HYPERCAST_SMOKE_SHARE='1',HYPERCAST_SMOKE_SOURCE_SWITCH=os.environ.get('HYPERCAST_TEST_SOURCE_SWITCH','1'),
            HYPERCAST_SMOKE_WINDOW_PID=str(app.pid),HYPERCAST_SYSTEM_AUDIO='1')
        exe=os.environ.get('HYPERCAST_TEST_EXE',str(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe'))
        viewer_token=rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+300,
            'video':{'room':room,'roomJoin':True,'canPublish':False,'canSubscribe':True,'canPublishData':False}},secret)
        viewer_env={k:v for k,v in env.items() if not k.startswith('HYPERCAST_SMOKE_')}
        viewer_env.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_ROOM_TOKEN=viewer_token,
            HYPERCAST_SMOKE_SECONDS='30',HYPERCAST_AUDIO_TEST_SILENT_OUTPUT='1')
        viewer_output=(ROOT/'.tools/signal-reconnect-viewer.log').open('wb')
        viewer=subprocess.Popen([exe,'--gui'],cwd=ROOT,env=viewer_env,stdout=viewer_output,stderr=subprocess.STDOUT)
        with (ROOT/'.tools/signal-reconnect-child.log').open('wb') as out:
            child=subprocess.Popen([exe,'--gui'],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
            time.sleep(12)
            def cut():
                nonlocal blocked
                blocked=True
                for w in tuple(writers):w.transport.abort()
            loop.call_soon_threadsafe(cut)
            time.sleep(4)
            def resume():
                nonlocal blocked
                blocked=False
            if not exhausted: loop.call_soon_threadsafe(resume)
            result=child.wait(timeout=35)
            assert result==0,f'Publisher exited with {result} (0x{result & 0xffffffff:08x})'
        assert viewer.wait(timeout=15)==0
        viewer_output.close()
        diagnostics=Path(os.environ['LOCALAPPDATA'])/'HyperCast/diagnostics'
        text=next(diagnostics.glob(f'gui-*-{child.pid}.log')).read_text(encoding='utf-8',errors='replace')
        assert 'Reconnecting; publication retained' in text,'Did not exercise SDK reconnection'
        attempts=list(map(int,re.findall(r'connection: Reconnect attempt (\d+)/3',text)))
        assert attempts and max(attempts)<=3, 'Retry budget exceeded or no progress events'
        if exhausted:
            assert attempts==[1,2,3],f'Expected exactly three attempts, got {attempts}'
            attempt_times=list(map(int,re.findall(r'(\d+) connection: Reconnect attempt \d/3',text)))
            assert all(b-a>=1900 for a,b in zip(attempt_times,attempt_times[1:])), 'Retries were not spaced'
            assert 'Reconnected; requesting keyframe' not in text, 'Unexpected recovery'
            assert 'SDK disconnected' in text, 'Did not close after retry exhaustion'
            assert 'session_end: Conexão encerrada após tentativa 3/3' in text, 'Failure status lost during teardown'
            assert not re.search(r'unpublish_error:|audio_unpublish_error:',text), 'Duplicate teardown failed'
            print('PASS: persistent outage closes after exactly three attempts')
            return
        assert 'Reconnected; requesting keyframe' in text,'SDK did not reconnect'
        limits=list(map(int,re.findall(r'bandwidth: video_limit_bps=(\d+)',text)))
        rates=list(map(int,re.findall(r'applied_bps=(\d+)',text)))
        if limits:assert rates and max(rates)<=max(limits),'Encoder lost video budget during recovery'
        after=text.split('Reconnected; requesting keyframe',1)[1]
        before=text.split('Reconnecting; publication retained',1)[0]
        before_count=max(map(int,re.findall(r'media: sent=(\d+)',before)),default=0)
        after_count=max(map(int,re.findall(r'media: sent=(\d+)',after)),default=0)
        assert after_count>before_count+100,'Video did not resume after reconnect'
        assert not re.search(r'capture_error:|publication_error:|audio_error:|unpublish_error:|audio_unpublish_error:',text),'Pipeline failed during reconnect'
        assert 'Reconnecting; publication retained' not in after,'Repeated reconnect loop'
        viewer_text=next(diagnostics.glob(f'gui-*-{viewer.pid}.log')).read_text(encoding='utf-8',errors='replace')
        assert 'Retained screen during participant interruption' in viewer_text,'Viewer discarded screen during restart'
        assert 'Resumed retained screen' in viewer_text,'Retained screen did not resume'
        resumed_at=int(re.search(r'(\d+) connection: Reconnected',text)[1])
        snapshots=[(int(m[1]),int(m[2])) for m in re.finditer(r'(\d+) media: sent=\d+ received=(\d+)',viewer_text)]
        audio=[(int(m[1]),int(m[2])) for m in re.finditer(r'(\d+) audio_stats: sent=\d+ received=(\d+)',viewer_text)]
        for label,values in [('video',snapshots),('audio',audio)]:
            before=max((n for t,n in values if t<resumed_at),default=0)
            after=max((n for t,n in values if t>resumed_at),default=0)
            assert after>before+100,f'{label} was not received after full reconnect'
        print('PASS: real signaling interruption, SDK reconnect, resumed GPU video and audio at remote viewer, clean stop')
    finally:
        for c in (child,viewer,app):
            if c and c.poll() is None:c.terminate();c.wait(timeout=10)
        if viewer_output:viewer_output.close()
        loop.call_soon_threadsafe(loop.stop);proxy_thread.join(timeout=5)

if __name__=='__main__':main()
