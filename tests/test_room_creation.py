import concurrent.futures
from contextlib import closing
import json
from pathlib import Path
import tempfile
import unittest
import uuid
from unittest.mock import patch
from infra.auth_service import Admission, Rejected
from infra.room_creation import RoomCreation

class RoomCreationTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.live={}
        self.app=Admission(Path(self.tmp.name)/'db.sqlite','key','x'*64,active=lambda _:set())
        def api(method,body):
            if method=='ListRooms':return {'rooms':[{'name':r,'metadata':v.get('metadata','{}')} for r,v in self.live.items()]}
            self.live[body['name']]=body;return body
        self.api=api;self.app.rooms=RoomCreation(self.app,api,max_rooms=2)
        self.key=self.app.rooms.issue_creator('Amigos')['access_key']
        self.creator=self.app.rooms.login(self.key,'a'*32)['token']
    def tearDown(self):self.tmp.cleanup()
    def test_additive_migration_keeps_legacy_rows_and_six_column_inserts(self):
        import time
        with closing(self.app.db()) as db:
            db.execute('INSERT INTO rooms VALUES(?,?,?,?,?,?)',('legacy', 'old-owner','request','Nome antigo',0,int(time.time())+86400))
        restarted=RoomCreation(self.app,self.api,max_rooms=2)
        with closing(self.app.db()) as db:
            record=db.execute('SELECT * FROM room_catalog WHERE room=?',('legacy',)).fetchone()
        self.assertEqual(record['title'],'Nome antigo');self.assertEqual(record['code_version'],1)
        self.assertEqual(record['persistent'],0)
    def test_permanent_listing_recreation_and_short_auth(self):
        first=self.app.rooms.create(self.creator,'Minha sala',uuid.uuid4().hex,True)
        self.assertRegex(first['invite'],r'^[A-Z]{8}$')
        self.assertTrue(first['persistent']);self.assertEqual(first['expires_at'],0)
        self.live.clear()
        other=self.app.rooms.login(self.key,'b'*32)['token']
        catalog=self.app.rooms.listing(other)['rooms']
        self.assertEqual(catalog[0]['invite'],first['invite'])
        self.assertEqual(catalog[0]['room_name'],'Minha sala')
        with self.assertRaises(Rejected):self.app.join(first['invite'],'Lucas','b'*32)
        joined=self.app.join(first['invite'].lower(),'Lucas','b'*32,other)
        self.assertEqual(joined['room_name'],'Minha sala');self.assertIn(first['room'],self.live)
        # Permanent room membership must not expire after twelve devices ever.
        for i in range(13):
            device=f'{i+10:032x}';session=self.app.rooms.login(self.key,device)['token']
            self.app.join(first['invite'],'Amigo',device,session)
        duplicate=self.app.rooms.create(other,'MINHA SALA',uuid.uuid4().hex,True)
        self.assertEqual(duplicate['invite'],first['invite'])
        self.app.rooms.keep(other,first['room'],False)
        with closing(self.app.db()) as db:db.execute('UPDATE rooms SET expires=1 WHERE room=?',(first['room'],));db.execute('UPDATE invites SET expires=1 WHERE room=?',(first['room'],))
        with self.assertRaises(Rejected):self.app.join(first['invite'],'Lucas','b'*32,other)
        self.app.rooms.keep(other,first['room'],True)
        self.assertTrue(self.app.join(first['invite'],'Lucas','b'*32,other)['token'])
        foreign=self.app.rooms.issue_creator('Other')['access_key'];foreign=self.app.rooms.login(foreign,'c'*32)['token']
        self.assertEqual(self.app.rooms.listing(foreign)['rooms'],[])
        with self.assertRaises(Rejected):self.app.rooms.keep(foreign,first['room'],False)

    def test_legacy_code_and_short_collision(self):
        old=self.app.rooms.create(self.creator,'Antiga',uuid.uuid4().hex,short_code=False)
        self.assertGreater(len(old['invite']),8)
        self.assertTrue(self.app.join(old['invite'],'Lucas','a'*32,self.creator)['token'])
        original=self.app.rooms.invite_code
        def codes(room,version=1,nonce=0):
            return 'AAAAAAAA' if version==2 and nonce==0 else original(room,version,nonce)
        self.live.clear()
        with patch.object(self.app.rooms,'invite_code',side_effect=codes):
            first=self.app.rooms.create(self.creator,'Nova',uuid.uuid4().hex)
            second=self.app.rooms.create(self.creator,'Outra',uuid.uuid4().hex)
            self.assertNotEqual(first['invite'],second['invite'])
    def test_unicode_title_private_id_and_stable_retry(self):
        request=uuid.uuid4().hex
        first=self.app.rooms.create(self.creator,'Jogos com amigos \u00e7',request)
        retry=self.app.rooms.create(self.creator,'Jogos com amigos \u00e7',request)
        self.assertEqual(first,retry);self.assertTrue(first['room'].startswith('hcr-'))
        self.assertEqual(len(self.live),1)
        self.assertEqual(self.live[first['room']]['max_participants'],12)
        member=self.app.join(first['invite'],'Lucas','a'*32,self.creator)
        self.assertTrue(member['token'])
        with closing(self.app.db()) as db:
            self.assertNotEqual(db.execute('SELECT hash FROM creators').fetchone()[0],self.key)
            self.assertNotEqual(db.execute('SELECT hash FROM invites').fetchone()[0],first['invite'])
    def test_same_creator_race_and_global_quota(self):
        def create(_):
            try:return self.app.rooms.create(self.creator,'Sala'+str(_),uuid.uuid4().hex)
            except Rejected as e:return e.status
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:results=list(pool.map(create,range(3)))
        self.assertEqual(sum(isinstance(r,dict) for r in results),2)
        self.live.clear()
        second=self.app.rooms.login(self.key,'b'*32)['token']
        self.app.rooms.create(second,'Segunda',uuid.uuid4().hex)
        self.app.rooms.create(second,'Terceira',uuid.uuid4().hex)
        third=self.app.rooms.login(self.key,'c'*32)['token']
        with self.assertRaises(Rejected) as rejected:self.app.rooms.create(third,'Quarta',uuid.uuid4().hex)
        self.assertEqual(rejected.exception.status,409)
    def test_revocation_invalid_fields_and_no_room_token_authority(self):
        with closing(self.app.db()) as db:actor=db.execute('SELECT id FROM creators').fetchone()[0]
        for code,title in [('bad','Sala'),(self.creator,'\u202eHidden'),(self.creator,'x'*65)]:
            with self.assertRaises(Rejected):self.app.rooms.create(code,title,uuid.uuid4().hex)
        self.app.rooms.revoke_creator(actor)
        with self.assertRaises(Rejected):self.app.rooms.create(self.creator,'Sala',uuid.uuid4().hex)
    def test_sessions_are_scoped_bound_expiring_and_revocable(self):
        from tools import room_token as rt
        import base64, time
        access=self.app.rooms.authorize(self.creator)
        self.assertEqual(access['device_id'],'a'*32)
        claims=json.loads(base64.urlsafe_b64decode(self.creator.split('.')[1]+'=='))
        self.assertNotIn('video',claims)
        for patch in [{'aud':'hypercast-invites'},{'scope':'invites:write'},{'exp':int(time.time())-1},{'device_id':'bad'}]:
            with self.assertRaises(Rejected):self.app.rooms.authorize(rt.sign(dict(claims,**patch),'x'*64))
        with self.assertRaises(Rejected):self.app.rooms.authorize(self.key)
        with self.assertRaises(Rejected):self.app.rooms.authorize(self.creator[:-6]+'AAAAAA')
        self.app.rooms.revoke_creator(access['id'])
        with self.assertRaises(Rejected):self.app.rooms.login(self.key,'a'*32)
        with self.assertRaises(Rejected):self.app.rooms.authorize(self.creator)
    def test_daily_quota_is_shared_across_devices(self):
        import time
        access=self.app.rooms.authorize(self.creator)
        with closing(self.app.db()) as db:
            for i in range(10):db.execute('INSERT INTO rooms(room,creator,request_id,title,created,expires) VALUES(?,?,?,?,?,?)',
                ('old-'+str(i),access['id']+':'+str(i),uuid.uuid4().hex,'Old',int(time.time()),int(time.time())+86400))
        other=self.app.rooms.login(self.key,'d'*32)['token']
        with self.assertRaises(Rejected) as error:self.app.rooms.create(other,'New',uuid.uuid4().hex)
        self.assertEqual(error.exception.status,429);self.assertEqual(len(self.live),0)
    def test_recreation_checks_capacity_and_failed_rpc_retry_is_stable(self):
        request=uuid.uuid4().hex;seen=[]
        def failing(method,body):
            result=self.api(method,body)
            if method=='CreateRoom':seen.append(body['name']);raise OSError('lost response')
            return result
        self.app.rooms.api=failing
        with self.assertRaises(OSError):self.app.rooms.create(self.creator,'Sala',request)
        self.app.rooms.api=self.api
        self.app.rooms.create(self.creator,'Other request',uuid.uuid4().hex)
        result=self.app.rooms.create(self.creator,'Sala',request)
        self.assertEqual(result['room'],seen[0]);self.assertEqual(len(self.live),2)
        self.live.clear();self.live.update({'busy1':{},'busy2':{}})
        with self.assertRaises(Rejected):self.app.join(result['invite'],'Lucas','b'*32,self.app.rooms.login(self.key,'b'*32)['token'])

if __name__=='__main__':unittest.main()
