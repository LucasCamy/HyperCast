"""Scoped access key, ephemeral room, owned GUI. Never prints tokens or codes."""
import json
import hashlib
import os
from pathlib import Path
import re
import select
import socket
import socketserver
import subprocess
import threading
import time
import urllib.error
import urllib.request
import uuid
from tools import room_token as rt
from tests.integration_desktop import screenshot

ROOT=Path(__file__).resolve().parent.parent
URL='https://hypercast.cloudlane.com.br'

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs):return None

def post(path,body,token):
    request=urllib.request.Request(URL+path,data=json.dumps(body).encode(),headers={
        'Authorization':'Bearer '+token,'Content-Type':'application/json','User-Agent':'HyperCast-Catalog-Test/0.8'})
    with urllib.request.build_opener(NoRedirect).open(request,timeout=15) as response:
        return json.loads(response.read(65536))

def main():
    key,secret=rt.load_env(ROOT/'.tools/homelab.env');now=int(time.time())
    admin=rt.sign({'iss':key,'sub':'catalog-test','aud':'hypercast-invites','scope':'invites:write',
        'nbf':now-5,'exp':now+90},secret)
    access=post('/api/admin/access-keys',{'label':'catalog-'+uuid.uuid4().hex[:8],'days':1},admin)
    session='';room=None
    try:
        device=uuid.uuid4().hex
        session=post('/api/login',{'device_id':device},access['access_key'])['token']
        title='Jogos e amigos · teste '+uuid.uuid4().hex[:6]
        room=post('/api/rooms',{'room_name':title,'request_id':uuid.uuid4().hex,'persistent':True},session)
        assert re.fullmatch('[A-Z]{8}',room['invite']) and room['persistent'] and room['expires_at']==0
        listing=post('/api/rooms/list',{},session)['rooms']
        assert listing[0]['room_name']==title and listing[0]['invite']==room['invite']
        duplicate=post('/api/rooms',{'room_name':title,'request_id':uuid.uuid4().hex,'persistent':True},session)
        assert duplicate['room']==room['room']
        try:post('/api/join',{'invite':room['invite'],'name':'Tester','request_id':device},'')
        except urllib.error.HTTPError as error:assert error.code==401
        else:raise AssertionError('Short room code allowed unauthenticated access')
        joined=post('/api/join',{'invite':room['invite'].lower(),'name':'Tester','request_id':device},session)
        assert joined['room_name']==title
        direct=post('/api/rooms/join',{'room':room['room'],'name':'Tester','request_id':device},session)
        assert direct['room_name']==title
        if os.environ.get('HYPERCAST_CATALOG_UI')=='1':gui(access['access_key'],title,room['invite'])
        print('PASS: permanent room, eight-letter code, catalog, duplicate recovery, authenticated join and friendly name')
    finally:
        if room and session:
            post('/api/rooms/keep',{'room':room['room'],'persistent':False},session)
            operator=rt.sign({'iss':key,'sub':'catalog-cleanup','nbf':int(time.time())-5,'exp':int(time.time())+60,
                'video':{'room':room['room'],'roomCreate':True,'roomAdmin':True}},secret)
            post('/twirp/livekit.RoomService/DeleteRoom',{'room':room['room']},operator)
        admin=rt.sign({'iss':key,'sub':'catalog-test','aud':'hypercast-invites','scope':'invites:write',
            'nbf':int(time.time())-5,'exp':int(time.time())+60},secret)
        post('/api/admin/revoke-access',{'access_id':access['access_id']},admin)

def gui(access_key,title,legacy_code):
    class Proxy(socketserver.BaseRequestHandler):
        def handle(self):
            try:
                with socket.create_connection(('192.168.15.19',8100),timeout=5) as upstream:
                    while True:
                        ready,_,_=select.select([self.request,upstream],[],[],1)
                        for source in ready:
                            data=source.recv(65536)
                            if not data:return
                            (upstream if source is self.request else self.request).sendall(data)
            except (OSError,ConnectionError):pass
    class Server(socketserver.ThreadingTCPServer):daemon_threads=True
    server=Server(('127.0.0.1',0),Proxy)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    exe=Path(os.environ.get('HYPERCAST_WINDOWS_EXE',str(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe')))
    env={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_'))}
    env.update(HYPERCAST_URL=f'ws://127.0.0.1:{server.server_address[1]}',HYPERCAST_NAME='CatalogTester',HYPERCAST_SMOKE_SECONDS='85')
    # Seed only a new loopback-origin credential using the same DPAPI binding
    # as the app. No keyboard injection, plaintext file or real server key change.
    origin=f'http://127.0.0.1:{server.server_address[1]}'
    credential=Path(os.environ['LOCALAPPDATA'])/'HyperCast'/('server-access-'+hashlib.sha256(origin.encode()).hexdigest()+'.bin')
    setup={**env,'HC_TEST_CODE':access_key,'HC_TEST_ORIGIN':origin,'HC_TEST_PATH':str(credential)}
    command="Add-Type -AssemblyName System.Security; $bytes=[Text.Encoding]::UTF8.GetBytes($env:HC_TEST_CODE); $salt=[Text.Encoding]::UTF8.GetBytes($env:HC_TEST_ORIGIN); $sealed=[Security.Cryptography.ProtectedData]::Protect($bytes,$salt,[Security.Cryptography.DataProtectionScope]::CurrentUser); $file=[IO.File]::Open($env:HC_TEST_PATH,[IO.FileMode]::CreateNew); try{$file.Write($sealed,0,$sealed.Length)}finally{$file.Dispose()}"
    subprocess.run(['powershell','-NoProfile','-Command',command],env=setup,check=True,capture_output=True)
    with (ROOT/'.tools/catalog-gui.log').open('wb') as log:
        app=subprocess.Popen([str(exe),'--gui'],env=env,stdout=log,stderr=subprocess.STDOUT)
        def control(name='',*args,check=True):
            local=env.copy()
            result=subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/control_room.ps1'),
                '-TargetPid',str(app.pid),'-ExpectedExecutable',str(exe),'-Name',name,*args],env=local,capture_output=True)
            if check:assert result.returncode==0,'Owned GUI control action failed: '+name+' '+result.stderr.decode(errors='replace')[-400:]
            return result.returncode==0
        def wait(name,timeout=12):
            deadline=time.monotonic()+timeout
            while time.monotonic()<deadline:
                assert app.poll() is None,'GUI unexpectedly exited'
                if control(name,'-Read',check=False):return
                time.sleep(.3)
            raise AssertionError('Owned GUI control unavailable: '+name)
        try:
            wait('Escolher sala')
            control('Escolher sala')
            wait(title)
            assert not control('Copiar código','-Read',check=False)
            assert not control(legacy_code,'-Read',check=False)
            screenshot(app.pid,ROOT/'.tools/catalog-list.png')
            control('Manter esta sala','-Toggle')
            wait('Temporária · 24 h · 0 pessoa(s)')
            control('Manter esta sala','-Toggle')
            wait('Permanente · 0 pessoa(s)')
            control('Entrar')
            wait('Conectado',20)
            wait(title)
            assert not control('Copiar convite','-Read',check=False)
            screenshot(app.pid,ROOT/'.tools/catalog-connected.png')
            control('Sair')
            wait('Escolher sala')
            control('Escolher sala')
            wait(title)
            print('PASS: GUI room picker without invitation controls, direct entry, leave and rediscovery')
        finally:
            if app.poll() is None:app.terminate()
            app.wait(timeout=10);server.shutdown();server.server_close();credential.unlink(missing_ok=True)

if __name__=='__main__':main()
