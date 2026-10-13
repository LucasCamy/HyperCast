import base64
from contextlib import closing
import concurrent.futures
import json
from pathlib import Path
import tempfile
import time
import unittest
from infra.auth_service import Admission, Rejected
from tools.room_token import sign

class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.key='test-key'; self.secret='a'*64
        self.app=Admission(Path(self.tmp.name)/'db.sqlite',self.key,self.secret,active=lambda room:set())
    def tearDown(self): self.tmp.cleanup()
    def test_invites_are_hashed_idempotent_and_role_is_server_owned(self):
        invite=self.app.create('amigos','viewer',max_uses=1)
        first=self.app.join(invite['invite'],'Lucas','a'*32)
        again=self.app.join(invite['invite'],'Lucas','a'*32)
        def claims(token):
            body=token.split('.')[1]
            return json.loads(base64.urlsafe_b64decode(body+'='*(-len(body)%4)))
        self.assertEqual(claims(first['token'])['sub'],claims(again['token'])['sub'])
        payload=first['token'].split('.')[1]
        claims=json.loads(base64.urlsafe_b64decode(payload+'='*(-len(payload)%4)))
        self.assertFalse(claims['video']['canPublish'])
        with closing(self.app.db()) as db:
            self.assertNotEqual(db.execute('SELECT hash FROM invites').fetchone()[0],invite['invite'])
        with self.assertRaises(Rejected): self.app.join(invite['invite'],'Other','b'*32)
    def test_all_members_can_enter_and_revoke_denies_reentry(self):
        invite=self.app.create('amigos','publisher')
        def join(i):
            try: return self.app.join(invite['invite'],'Lucas',f'{i:032d}')
            except Rejected as e: return e.status
        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool: results=list(pool.map(join,range(5)))
        self.assertEqual(sum(isinstance(x,dict) for x in results),5)
        identities=[]
        for result in results:
            body=result['token'].split('.')[1]
            claims=json.loads(base64.urlsafe_b64decode(body+'='*(-len(body)%4)))
            self.assertTrue(claims['video']['canPublish'])
            identities.append(claims['sub'])
        self.assertEqual(len(set(identities)),5)
        self.app.revoke(invite['invitation_id'])
        with self.assertRaises(Rejected): self.app.join(invite['invite'],'Lucas',f'{0:032d}')
    def test_room_tokens_cannot_mint_invites_and_admin_signature_is_verified(self):
        now=int(time.time())
        claims={'iss':self.key,'aud':'hypercast-invites','scope':'invites:write','nbf':now-5,'exp':now+60}
        self.app.verify_admin(sign(claims,self.secret))
        with self.assertRaises(Rejected): self.app.verify_admin(sign(claims,'b'*64))
        with self.assertRaises(Rejected): self.app.verify_admin(sign({'iss':self.key,'exp':now+60,'video':{'roomJoin':True}},self.secret))
