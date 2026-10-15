"""Linux egui room transitions against two isolated authorized rooms; no capture."""
import json,os,subprocess,time,urllib.error,urllib.request,uuid
from pathlib import Path
from tools import room_token as rt
ROOT=Path(__file__).resolve().parent.parent

def main():
    key,secret=rt.load_env(ROOT/'.tools/homelab.env')
    url='https://hypercast.cloudlane.com.br'
    def post(path,body,token):
        request=urllib.request.Request(url+path,data=json.dumps(body).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+token,'User-Agent':'HyperCast-Test'})
        with urllib.request.urlopen(request,timeout=15) as response:return json.load(response)
    def admin():
        now=int(time.time())
        return rt.sign({'iss':key,'sub':'linux-catalog-test','aud':'hypercast-invites','scope':'invites:write','nbf':now-5,'exp':now+120},secret)
    def delete(room):
        now=int(time.time());operator=rt.sign({'iss':key,'sub':'linux-room-cleanup','nbf':now-5,'exp':now+60,'video':{'room':room,'roomCreate':True,'roomAdmin':True}},secret)
        try:post('/twirp/livekit.RoomService/DeleteRoom',{'room':room},operator)
        except urllib.error.HTTPError as error:
            if error.code!=404:raise
    access=post('/api/admin/access-keys',{'label':'linux-switch-'+uuid.uuid4().hex[:8],'days':1},admin())
    rooms=[];session=''
    try:
        session=post('/api/login',{'device_id':uuid.uuid4().hex},access['access_key'])['token']
        for title in ['Linux primeira','Linux segunda']:
            room=post('/api/rooms',{'room_name':title,'persistent':True,'request_id':uuid.uuid4().hex},session)
            rooms.append(room);delete(room['room'])
        env={k:v for k,v in os.environ.items() if not k.startswith(('HYPERCAST_','LIVEKIT_','GH_','GITHUB_'))}
        env.update(HYPERCAST_CATALOG_TEST_URL='wss://hypercast.cloudlane.com.br',HYPERCAST_CATALOG_TEST_KEY=access['access_key'],HYPERCAST_CATALOG_TEST_FIRST=rooms[0]['room'],HYPERCAST_CATALOG_TEST_SECOND=rooms[1]['room'])
        env['WSLENV']=':'.join(k+'/u' for k in env if k.startswith('HYPERCAST_'))
        with (ROOT/'.tools/linux-room-switch.log').open('wb') as log:
            result=subprocess.run(['wsl','-d','Ubuntu','--','bash','-lc','source ~/.cargo/env; cd /mnt/f/HyperCast; CC=clang-21 CXX=clang++-21 CARGO_TARGET_DIR=/home/lucas/hypercast-target cargo test --locked --features desktop --lib linux::desktop::tests::room_switch_with_server -- --ignored --nocapture'],env=env,stdout=log,stderr=subprocess.STDOUT,timeout=140)
        assert result.returncode==0,'Linux room switch failed; inspect private test log'
        print('PASS: Linux egui authenticated, entered, switched twice and left the isolated rooms')
    finally:
        for room in rooms:
            if session:post('/api/rooms/keep',{'room':room['room'],'persistent':False},session)
            delete(room['room'])
        post('/api/admin/revoke-access',{'access_id':access['access_id']},admin())

if __name__=='__main__':main()
