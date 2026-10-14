"""Owned desktop UI creates a real isolated production room and joins a friend."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time
import uuid
from tools import room_invite as invites, room_token as rt
from tests.integration_app_update import invoke
from tests.integration_desktop import screenshot,owned_window
from tests.integration_room_creation import cleanup,post
ROOT=Path(__file__).resolve().parent.parent

def main():
    env_file=ROOT/'.tools/homelab.env';key,secret=rt.load_env(env_file)
    credential=Path(os.environ['LOCALAPPDATA'])/'HyperCast'/('server-access-'+hashlib.sha256(b'https://hypercast.cloudlane.com.br').hexdigest()+'.bin')
    assert not credential.exists(),'Refusing to overwrite an existing user creator credential'
    actor=invites.request('/api/admin/creators',{'label':'GUI-QA-'+uuid.uuid4().hex[:8],'days':1},env_file)
    exe=Path(os.environ.get('HYPERCAST_TEST_EXE',str(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe')))
    title='Sala de testes '+uuid.uuid4().hex[:6]
    input_path=ROOT/'.tools'/('room-ui-'+uuid.uuid4().hex+'.json')
    with input_path.open('x',encoding='utf-8') as stream:json.dump({'title':title,'name':'QA_GUI','code':actor['creator_code']},stream)
    env={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_','GH_','GITHUB_'))}
    env.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_AUTO_CONNECT='0',HYPERCAST_SMOKE_SECONDS='60')
    children=[];files=[];room=None;invite=None
    try:
        out=(ROOT/'.tools/room-ui-owner.log').open('wb');files.append(out)
        child=subprocess.Popen([str(exe),'--gui'],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT);children.append(child)
        deadline=time.monotonic()+15
        while True:
            assert child.poll() is None and time.monotonic()<deadline
            try:owned_window(child.pid);break
            except AssertionError:time.sleep(.5)
        time.sleep(2)
        subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/fill_room_creation.ps1'),
            '-TargetPid',str(child.pid),'-ExpectedExecutable',str(exe),'-InputFile',str(input_path),'-Mode','login'],check=True,timeout=15)
        invoke(child,exe,'Entrar no servidor')
        deadline=time.monotonic()+20
        while not invoke(child,exe,'Criar sala',False):
            assert child.poll() is None and time.monotonic()<deadline,'Server login failed';time.sleep(.5)
        subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/fill_room_creation.ps1'),
            '-TargetPid',str(child.pid),'-ExpectedExecutable',str(exe),'-InputFile',str(input_path)],check=True,timeout=15)
        time.sleep(.5);screenshot(child.pid,ROOT/'.tools/room-create-modal.png')
        invoke(child,exe,'Criar e entrar')
        deadline=time.monotonic()+25
        while not invoke(child,exe,'Copiar convite',False):
            assert child.poll() is None and time.monotonic()<deadline,'Creation/automatic join failed';time.sleep(.5)
        screenshot(child.pid,ROOT/'.tools/room-created.png')
        assert credential.exists(),'Creator code was not DPAPI-protected'
        assert actor['creator_code'].encode() not in credential.read_bytes()
        # Read only the value just copied by the owned app, never print it.
        clip=subprocess.run(['powershell','-NoProfile','-Command','Get-Clipboard -Raw'],capture_output=True,timeout=10)
        invite=clip.stdout.decode().strip();assert re.fullmatch(r'[A-Za-z0-9_-]{32,64}',invite)
        device=uuid.uuid4().hex
        session=post('/api/login',{'device_id':device},actor['access_key'])['token']
        member=post('/api/join',{'invite':invite,'name':'QA_Friend','request_id':device},session)
        room=member['room'];friend_env={**env,'HYPERCAST_AUTO_CONNECT':'1','HYPERCAST_ROOM_TOKEN':member['token'],'HYPERCAST_SMOKE_SECONDS':'8'}
        out=(ROOT/'.tools/room-ui-friend.log').open('wb');files.append(out)
        friend=subprocess.Popen([str(exe),'--gui'],cwd=ROOT,env=friend_env,stdout=out,stderr=subprocess.STDOUT);children.append(friend)
        assert friend.wait(timeout=20)==0
        diagnostics=Path(os.environ['LOCALAPPDATA'])/'HyperCast/diagnostics'
        assert 'Room connected' in next(diagnostics.glob(f'gui-*-{friend.pid}.log')).read_text(encoding='utf-8')
        out=(ROOT/'.tools/room-ui-remembered.log').open('wb');files.append(out)
        remembered=subprocess.Popen([str(exe),'--gui'],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT);children.append(remembered)
        deadline=time.monotonic()+20
        while not invoke(remembered,exe,'Trocar acesso ao servidor',False):
            assert remembered.poll() is None and time.monotonic()<deadline,'Remembered login failed';time.sleep(.5)
        assert not credential.exists(),'Logout did not remove remembered key'
        print('PASS: login, remembered access/logout, create-room modal, automatic room join, friendly name, copied invitation, second member, DPAPI storage')
    finally:
        if children and children[0].poll() is None:screenshot(children[0].pid,ROOT/'.tools/room-ui-final.png')
        for child in children:
            if child.poll() is None:child.terminate();child.wait(timeout=10)
        for stream in files:stream.close()
        input_path.unlink(missing_ok=True)
        if credential.exists():credential.unlink()
        invites.request('/api/admin/revoke-creator',{'creator_id':actor['creator_id']},env_file)
        if room:cleanup(room,key,secret)
        if invite:
            subprocess.run(['powershell','-NoProfile','-Command','if ((Get-Clipboard -Raw).Trim() -eq $env:HYPERCAST_QA_INVITE) { Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Clipboard]::Clear() }'],env={**os.environ,'HYPERCAST_QA_INVITE':invite},stdout=subprocess.DEVNULL,timeout=10)

if __name__=='__main__':main()
