"""Real GUI -> stable GitHub asset -> SHA-256 -> Inno upgrade -> GUI restart.

Operator test only. Refuses an existing registered installation. Builds a private
older-version prototype from the same source (no updater bypass in shipping code),
restores both manifests, and upgrades only .tools/update-install-test. No room join.
Requires a published stable release and the previous local 0.4.2 installer.
"""
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import winreg
from tests.integration_desktop import screenshot

ROOT=Path(__file__).resolve().parent.parent
FIXTURE=ROOT/'.tools/update-install-test'
KEY=r'Software\Microsoft\Windows\CurrentVersion\Uninstall\{34FA3F08-63D0-4A64-9E61-F79E7078B39D}_is1'

def registration():
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,KEY) as key:
            return {name:winreg.QueryValueEx(key,name)[0] for name in ('InstallLocation','DisplayVersion')}
    except FileNotFoundError:return None

def invoke(child,exe,name,required=True):
    result=subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tests/invoke_update_control.ps1'),
        '-TargetPid',str(child.pid),'-ExpectedExecutable',str(exe),'-Name',name],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=12,creationflags=subprocess.CREATE_NO_WINDOW)
    if result.returncode:
        (ROOT/'.tools/updater-control-error.log').write_bytes(result.stdout)
        if required:
            print(result.stdout.decode(errors='replace'))
            raise subprocess.CalledProcessError(result.returncode,result.args)
        return False
    return True

def main():
    assert registration() is None,'Existing user installation: refusing to replace its registration'
    assert not FIXTURE.exists(),'Fixture directory exists: inspect before retrying; no recursive deletion'
    assert FIXTURE.resolve().is_relative_to((ROOT/'.tools').resolve())
    baseline=Path(os.environ.get('HYPERCAST_UPDATE_BASELINE_SETUP',str(ROOT/'dist/HyperCast-0.4.2-windows-x64-setup.exe')))
    assert baseline.is_file(),'Baseline installer unavailable'
    version=re.search(r'^version = "([0-9.]+)"', (ROOT/'Cargo.toml').read_text(),re.M)[1]
    shipped=ROOT/f'dist/HyperCast-{version}-windows-x64/hypercast.exe';assert shipped.is_file()
    expected_hash=hashlib.sha256(shipped.read_bytes()).digest()
    originals={p:p.read_bytes() for p in (ROOT/'Cargo.toml',ROOT/'Cargo.lock')}
    prototype=ROOT/'.tools/updater-prototype.exe'
    try:
        manifest=originals[ROOT/'Cargo.toml'].decode().replace('\r\n','\n').replace(f'version = "{version}"','version = "0.4.99"',1)
        lock=originals[ROOT/'Cargo.lock'].decode().replace('\r\n','\n').replace(f'name = "hypercast"\nversion = "{version}"','name = "hypercast"\nversion = "0.4.99"')
        (ROOT/'Cargo.toml').write_text(manifest,encoding='utf-8');(ROOT/'Cargo.lock').write_text(lock,encoding='utf-8')
        with (ROOT/'.tools/updater-prototype-build.log').open('wb') as out:
            subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ROOT/'tools/cargo-native.ps1'),
                'build','--locked','--features','desktop'],cwd=ROOT,stdout=out,stderr=subprocess.STDOUT,check=True,timeout=180)
        shutil.copy2(ROOT/'target/x86_64-pc-windows-msvc/debug/hypercast.exe',prototype)
    finally:
        for path,data in originals.items():path.write_bytes(data)
    env={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_','GH_','GITHUB_','CLOUDFLARE_'))}
    env.update(HYPERCAST_AUTO_CONNECT='0',HYPERCAST_SMOKE_SECONDS='300')
    child=None;installed=False
    try:
        assert registration() is None
        subprocess.run([str(baseline),'/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',f'/DIR={FIXTURE}'],check=True,timeout=60,env=env)
        installed=True;assert Path(registration()['InstallLocation']).resolve()==FIXTURE.resolve()
        exe=FIXTURE/'hypercast.exe';shutil.copy2(prototype,exe)
        with (ROOT/'.tools/updater-gui-prototype.log').open('wb') as out:
            child=subprocess.Popen([str(exe),'--gui'],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
            deadline=time.monotonic()+12
            while not invoke(child,exe,'Atualizar',False):
                assert child.poll() is None,'Prototype failed to start'
                assert time.monotonic()<deadline,'Owned UI did not become ready';time.sleep(.5)
            time.sleep(3)
            screenshot(child.pid,ROOT/'.tools/updater-available.png')
            invoke(child,exe,'Baixar atualização')
            # Invoke only when the verified-install action enters the owned accessibility tree.
            deadline=time.monotonic()+180
            while True:
                assert child.poll() is None,'Prototype exited before download completion'
                if invoke(child,exe,'Instalar e reiniciar',False):break
                assert time.monotonic()<deadline,'Verified installer action unavailable; inspect test screenshots/log'
                time.sleep(1)
            assert child.wait(timeout=20)==0,'Updater did not close its caller'
        deadline=time.monotonic()+60
        while True:
            assert Path(registration()['InstallLocation']).resolve()==FIXTURE.resolve()
            if registration()['DisplayVersion']==version and hashlib.sha256(exe.read_bytes()).digest()==expected_hash:break
            assert time.monotonic()<deadline,'Installer did not replace registered target';time.sleep(.5)
        # Query only the test executable path; do not terminate other HyperCast sessions.
        script="$p = Get-CimInstance Win32_Process -Filter \"Name = 'hypercast.exe'\" | Where-Object { $_.ExecutablePath -eq $env:HYPERCAST_UPDATE_TEST_EXE } | Select-Object -ExpandProperty ProcessId; ConvertTo-Json -InputObject @($p) -Compress"
        query_env={**env,'HYPERCAST_UPDATE_TEST_EXE':str(exe)}
        restarted=[]
        for _ in range(15):
            raw=subprocess.check_output(['powershell','-NoProfile','-Command',script],env=query_env,creationflags=subprocess.CREATE_NO_WINDOW)
            restarted=json.loads(raw.decode('utf-8-sig'))
            if restarted:break
            time.sleep(.5)
        assert restarted,'Updated GUI was not restarted'
        class Owned:pid=int(restarted[0])
        time.sleep(1);invoke(Owned,exe,'Atualizar');time.sleep(2)
        screenshot(Owned.pid,ROOT/'.tools/updater-installed-current.png')
        # Graceful close of only the restarted test HWND.
        from tests.integration_desktop import owned_window
        user,hwnd=owned_window(Owned.pid);user.PostMessageW.argtypes=[ctypes.c_void_p,ctypes.c_uint,ctypes.c_size_t,ctypes.c_ssize_t]
        user.PostMessageW(hwnd,0x10,0,0);time.sleep(2)
        print('PASS: installed GUI checks stable release, downloads/verifies, upgrades isolated target and restarts at published binary; manifests restored')
    finally:
        if child and child.poll() is None:child.terminate();child.wait(timeout=10)
        if installed:
            current=registration();assert current and Path(current['InstallLocation']).resolve()==FIXTURE.resolve(),'Refusing unrelated uninstall'
            subprocess.run([str(FIXTURE/'unins000.exe'),'/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART'],env=env,check=True,timeout=60)
            assert registration() is None,'Test registration was not removed'

if __name__=='__main__':main()
