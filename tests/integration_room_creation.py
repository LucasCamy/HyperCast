"""Production API/SFU test, isolated rooms; no secrets printed or recorded."""
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.request
import uuid
from tools import room_invite as invites, room_token as rt
ROOT=Path(__file__).resolve().parent.parent

def post(path,body,code):
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self,*args,**kwargs):return None
    request=urllib.request.Request(os.environ['LIVEKIT_URL'].rstrip('/')+path,data=json.dumps(body).encode(),
        headers={'Authorization':'Bearer '+code,'Content-Type':'application/json','User-Agent':'HyperCast-Desktop/0.6'})
    try:
        with urllib.request.build_opener(NoRedirect).open(request,timeout=20) as response:return json.loads(response.read(16384))
    except urllib.error.HTTPError as error:
        payload=error.read(16384)
        try:message=json.loads(payload).get('error','')
        except ValueError:message='non-JSON response'
        print('HTTP failure:',error.code,error.headers.get('Server'),error.headers.get('cf-mitigated'),message)
        raise

def cleanup(room,key,secret):
    now=int(time.time());token=rt.sign({'iss':key,'sub':'hc-test','nbf':now-5,'exp':now+60,
        'video':{'room':room,'roomCreate':True}},secret)
    request=urllib.request.Request(os.environ['LIVEKIT_URL'].rstrip('/')+'/twirp/livekit.RoomService/DeleteRoom',
        data=json.dumps({'room':room}).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json','User-Agent':'HyperCast-Operator/0.6'})
    with urllib.request.urlopen(request,timeout=10) as response:response.read(16384)

def main():
    env_file=ROOT/'.tools/homelab.env';key,secret=rt.load_env(env_file)
    actor=invites.request('/api/admin/creators',{'label':'QA-'+uuid.uuid4().hex[:8],'days':1},env_file)
    created=None;other=None
    device=uuid.uuid4().hex
    session=post('/api/login',{'device_id':device},actor['access_key'])['token']
    try:
        body={'room_name':'Teste de cria\u00e7\u00e3o','request_id':uuid.uuid4().hex}
        created=post('/api/rooms',body,session)
        assert post('/api/rooms',body,session)['invite']==created['invite']
        member=post('/api/join',{'invite':created['invite'],'name':'QA','request_id':device},session)
        try:post('/api/join',{'invite':created['invite'],'name':'QA','request_id':uuid.uuid4().hex},session)
        except urllib.error.HTTPError as error:assert error.code==403
        else:raise AssertionError('Session device binding bypassed')
        payload=json.loads(__import__('base64').urlsafe_b64decode(member['token'].split('.')[1]+'=='))
        assert payload['video']['room']==created['room'] and payload['video']['canPublish']
        assert payload['video']['canPublishSources']==['screen_share','screen_share_audio']
        try:post('/api/rooms',{'room_name':'Forbidden','request_id':uuid.uuid4().hex},created['invite'])
        except urllib.error.HTTPError as error:assert error.code==401
        else:raise AssertionError('A room invitation granted creator authority')
        try:post('/api/rooms',{'room_name':'Second','request_id':uuid.uuid4().hex},session)
        except urllib.error.HTTPError as error:assert error.code==409
        else:raise AssertionError('Owner room cap bypassed')
        other_session=post('/api/login',{'device_id':uuid.uuid4().hex},actor['access_key'])['token']
        other=post('/api/rooms',{'room_name':'Outro dispositivo','request_id':uuid.uuid4().hex},other_session)
        assert other['room']!=created['room']
        if os.environ.get('HYPERCAST_TEST_STRICT_ACCESS')=='1':
            try:post('/api/join',{'invite':created['invite'],'name':'QA','request_id':device},'')
            except urllib.error.HTTPError as error:assert error.code==401
            else:raise AssertionError('Unauthenticated room entry accepted')
        invites.request('/api/admin/revoke-creator',{'creator_id':actor['creator_id']},env_file)
        try:post('/api/rooms',body,session)
        except urllib.error.HTTPError as error:assert error.code==401
        else:raise AssertionError('Revoked creator accepted')
        print('PASS: HTTPS room creation, idempotent retry, room invitation/token, owner quota and creator revocation')
    finally:
        invites.request('/api/admin/revoke-creator',{'creator_id':actor['creator_id']},env_file)
        if created:cleanup(created['room'],key,secret)
        if other:cleanup(other['room'],key,secret)

if __name__=='__main__':main()
