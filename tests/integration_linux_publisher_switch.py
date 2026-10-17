"""Exercise the Linux publishing engine with generated sources and two clients."""
import json,os,subprocess,sys,tempfile,time,tomllib,urllib.error,urllib.request,uuid
from pathlib import Path
from tools import room_token as rt
ROOT=Path(__file__).resolve().parent.parent


def linux_probe(binary):
    with tempfile.TemporaryDirectory(prefix='hypercast-switch-') as directory:
        work=Path(directory)
        version=tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
        image=ROOT/'dist'/f'HyperCast-{version}-linux-x86_64.AppImage'
        subprocess.run([str(image),'--appimage-extract'],cwd=work,stdout=subprocess.DEVNULL,check=True,timeout=60)
        app=work/'squashfs-root';lib=app/'usr/lib'
        env={k:v for k,v in os.environ.items() if not k.startswith(('GST_','PIPEWIRE_','SPA_')) and k!='LD_LIBRARY_PATH'}
        env.update(LD_LIBRARY_PATH=str(lib),GST_PLUGIN_SYSTEM_PATH_1_0='',GST_PLUGIN_PATH_1_0=str(lib/'gstreamer-1.0'),GST_PLUGIN_SCANNER=str(lib/'gst-plugin-scanner'),GST_REGISTRY=str(work/'registry.bin'))
        return subprocess.run([binary,'publisher_source_and_codec_switches','--ignored','--nocapture'],env=env,timeout=100).returncode


def main():
    artifacts=[json.loads(line) for line in (ROOT/'.tools/pipewire-frame-build.jsonl').read_text().splitlines() if line.startswith('{')]
    binary=next(a['executable'] for a in artifacts if a.get('reason')=='compiler-artifact' and a.get('target',{}).get('name')=='hypercast' and a.get('executable'))
    assert binary.startswith('/home/lucas/hypercast-target/debug/deps/hypercast-'),'Unexpected test binary'
    key,secret=rt.load_env(ROOT/'.tools/homelab.env')
    room='switch-linux-'+uuid.uuid4().hex[:12];now=int(time.time())
    def token(publish):
        grant={'room':room,'roomJoin':True,'canPublish':publish,'canSubscribe':True,'canPublishData':False}
        if publish:grant['canPublishSources']=['screen_share','screen_share_audio']
        return rt.sign({'iss':key,'sub':'hc-member-'+uuid.uuid4().hex,'nbf':now-5,'exp':now+180,'video':grant},secret)
    env={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_','GH_','GITHUB_'))}
    env.update(HYPERCAST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_PUBLISH_TOKEN=token(True),HYPERCAST_ROOM_TOKEN=token(False),HYPERCAST_AUDIO_TEST_SILENT_OUTPUT='1')
    env['WSLENV']=':'.join(name+'/u' for name in env if name.startswith('HYPERCAST_'))
    try:
        with (ROOT/'.tools/linux-publisher-switch.log').open('wb') as log, (ROOT/'.tools/linux-publisher-switch-stderr.log').open('wb') as errors:
            child=subprocess.run(['wsl','-d','Ubuntu','--','python3','-m','tests.integration_linux_publisher_switch','--linux-probe',binary],env=env,stdout=log,stderr=errors,timeout=100)
            assert child.returncode==0,'Linux publisher switch failed; inspect .tools/linux-publisher-switch.log'
        print('PASS: Linux source replacement, HEVC/AV1/HEVC codec changes, GPU/software/GPU transitions and viewer decoded frames')
    finally:
        admin=rt.sign({'iss':key,'sub':'operator','nbf':now-5,'exp':now+180,'video':{'room':room,'roomAdmin':True,'roomCreate':True}},secret)
        request=urllib.request.Request('https://hypercast.cloudlane.com.br/twirp/livekit.RoomService/DeleteRoom',data=json.dumps({'room':room}).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+admin,'User-Agent':'HyperCast-Test'})
        try:
            with urllib.request.urlopen(request,timeout=10):pass
        except urllib.error.HTTPError as error:
            if error.code!=404:raise


if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--linux-probe':raise SystemExit(linux_probe(sys.argv[2]))
    main()
