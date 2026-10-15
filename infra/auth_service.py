"""Invite-based HyperCast admission. Loopback service behind the HTTPS gateway.

Room permissions and identity are server-owned. Never log request bodies,
Authorization, invite codes or JWTs. SQLite lives on a persistent volume.
"""
import base64
from contextlib import closing
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import secrets
import sqlite3
import threading
import time
import urllib.error
import uuid
from tools import room_token as rt

class Rejected(Exception):
    def __init__(self, status, message): self.status, self.message = status, message

def digest(value): return hashlib.sha256(value.encode()).hexdigest()

class Admission:
    def __init__(self, path, key, secret, active=None):
        self.path, self.key, self.secret = str(path), key, secret
        self.rejection = Rejected
        self.active = active or (lambda room: rt.participants(room, key, secret))
        with closing(self.db()) as db:
            db.executescript('''
                CREATE TABLE IF NOT EXISTS invites (
                    id TEXT PRIMARY KEY, hash TEXT UNIQUE, room TEXT, role TEXT,
                    expires INTEGER, max_uses INTEGER, uses INTEGER DEFAULT 0,
                    revoked INTEGER DEFAULT 0);
                CREATE TABLE IF NOT EXISTS members (
                    id TEXT PRIMARY KEY, invite TEXT, request_id TEXT, name TEXT,
                    UNIQUE(invite,request_id));
            ''')

        from infra.room_creation import RoomCreation
        self.rooms = RoomCreation(self, max_rooms=int(os.environ.get("HYPERCAST_MAX_ROOMS", "4")))

    def db(self):
        db = sqlite3.connect(self.path, timeout=10, isolation_level=None)
        db.row_factory = sqlite3.Row
        return db

    def create(self, room, role, days=7, max_uses=12):
        if not isinstance(room,str) or not rt.NAME.fullmatch(room) or role not in ('publisher','viewer') or type(days) is not int or type(max_uses) is not int or not 1<=days<=30 or not 1<=max_uses<=12:
            raise Rejected(400,'Invalid invitation settings')
        code, invitation = secrets.token_urlsafe(32), uuid.uuid4().hex
        with closing(self.db()) as db:
            db.execute('INSERT INTO invites(id,hash,room,role,expires,max_uses) VALUES(?,?,?,?,?,?)',
                (invitation,digest(code),room,role,int(time.time())+days*86400,max_uses))
        return {'invitation_id':invitation,'invite':code,'room':room,'role':role}

    def revoke(self, invitation):
        with closing(self.db()) as db:
            db.execute('UPDATE invites SET revoked=1 WHERE id=?',(invitation,))

    def join(self, code, name, request_id, session=""):
        short=isinstance(code,str) and re.fullmatch(r'[A-Za-z]{8}',code) is not None
        if short:code=code.upper()
        if not isinstance(code,str) or not (short or re.fullmatch(r'[A-Za-z0-9_-]{32,64}',code)):
            raise Rejected(401,'Invalid invitation')
        if not isinstance(name,str) or not rt.NAME.fullmatch(name) or not isinstance(request_id,str) or not re.fullmatch(r'[A-Za-z0-9_-]{32,64}',request_id):
            raise Rejected(400,'Name/request ID must use ASCII letters, digits, underscore or hyphen')
        if short or session or os.environ.get('HYPERCAST_REQUIRE_ACCESS')=='1':
            access=self.rooms.authorize(session)
            if request_id!=access['device_id']:raise Rejected(403,'Device does not match server session')
        now = int(time.time())
        with closing(self.db()) as db:
            invite = db.execute('SELECT * FROM invites WHERE hash=?',(digest(code),)).fetchone()
            if not invite or invite['revoked'] or (invite['expires']!=0 and invite['expires']<=now):
                raise Rejected(401,'Invitation expired, revoked or invalid')
            # Fail closed on SFU availability before reserving any publisher identity.
            self.rooms.ensure_join(invite['room'])
            if invite['role']=='publisher': self.active(invite['room'])
            db.execute('BEGIN IMMEDIATE')
            try:
                invite = db.execute('SELECT * FROM invites WHERE id=?',(invite['id'],)).fetchone()
                if invite['revoked'] or (invite['expires']!=0 and invite['expires']<=now): raise Rejected(401,'Invitation expired or revoked')
                member = db.execute('SELECT * FROM members WHERE invite=? AND request_id=?',(invite['id'],request_id)).fetchone()
                new_member = member is None
                if new_member and invite['expires']!=0 and invite['uses']>=invite['max_uses']: raise Rejected(403,'Invitation use limit reached')
                member_id = member['id'] if member else uuid.uuid4().hex
                # Everyone may enter; screen quota is enforced atomically by the SFU.
                identity = 'hc-member-'+member_id
                if new_member:
                    db.execute('INSERT INTO members VALUES(?,?,?,?)',(member_id,invite['id'],request_id,name))
                    db.execute('UPDATE invites SET uses=uses+1 WHERE id=?',(invite['id'],))
                db.commit()
            except Exception:
                db.rollback(); raise
        grants = {'room':invite['room'],'roomJoin':True,'canSubscribe':True,
            'canPublish':invite['role']=='publisher','canPublishData':False,'canUpdateOwnMetadata':False}
        if grants['canPublish']: grants['canPublishSources']=['screen_share','screen_share_audio']
        token = rt.sign({'iss':self.key,'sub':identity,'name':member['name'] if member else name,
            'nbf':now-5,'exp':now+300,'video':grants},self.secret)
        with closing(self.db()) as db:
            record=db.execute('SELECT title FROM rooms WHERE room=?',(invite['room'],)).fetchone()
        return {'token':token,'room':invite['room'],'room_name':record['title'] if record else invite['room'],'expires_in':300}

    def verify_admin(self, bearer):
        try:
            header, body, signature = bearer.split('.')
            def decode(s): return base64.urlsafe_b64decode(s+'='*(-len(s)%4))
            if json.loads(decode(header)).get('alg')!='HS256': raise ValueError()
            expected=hmac.new(self.secret.encode(),(header+'.'+body).encode(),hashlib.sha256).digest()
            if not hmac.compare_digest(expected,decode(signature)): raise ValueError()
            claims=json.loads(decode(body)); now=int(time.time())
            if claims.get('iss')!=self.key or claims.get('aud')!='hypercast-invites' or claims.get('scope')!='invites:write': raise ValueError()
            if not now-125<=claims.get('nbf',0)<=now+5 or not now<claims.get('exp',0)<=now+120: raise ValueError()
        except (ValueError,TypeError,KeyError,AttributeError): raise Rejected(401,'Administrator authentication required')

def main():
    key,secret=os.environ['LIVEKIT_KEYS'].strip().split(':',1)
    os.environ['LIVEKIT_URL']='http://127.0.0.1:8107'
    database=Path('/data/admission.sqlite3')
    admission=Admission(database,key.strip(),secret.strip())
    def cleanup_rooms():
        while True:
            try:
                removed=admission.rooms.cleanup()
                if removed:print(f'Expired empty temporary rooms removed: {removed}',flush=True)
            except Exception:
                # No cleanup if SFU participant state is unavailable.
                print('Room cleanup deferred; could not confirm server state',flush=True)
            time.sleep(60)
    threading.Thread(target=cleanup_rooms,daemon=True).start()
    budget=[0,0]; lock=threading.Lock()
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args): pass
        def setup(self):
            super().setup(); self.connection.settimeout(8)
        def reply(self,status,obj):
            data=json.dumps(obj).encode()
            self.send_response(status); self.send_header('Content-Type','application/json')
            self.send_header('Cache-Control','no-store'); self.send_header('Content-Length',str(len(data)))
            self.end_headers(); self.wfile.write(data)
        def do_GET(self):
            self.reply(200 if self.path=='/health' else 404,{'ok':self.path=='/health'})
        def do_POST(self):
            try:
                with lock:
                    now=int(time.time())//60
                    if budget[0]!=now: budget[:]=[now,0]
                    budget[1]+=1
                    if budget[1]>120: raise Rejected(429,'Try again later')
                length=int(self.headers.get('Content-Length','0'))
                if not 1<=length<=4096 or self.headers.get('Transfer-Encoding'): raise Rejected(400,'Invalid request size')
                body=json.loads(self.rfile.read(length))
                if not isinstance(body,dict): raise Rejected(400,'Invalid request')
                if self.path=='/api/login':
                    if set(body)!={'device_id'}: raise Rejected(400,'Unexpected fields')
                    code=self.headers.get('Authorization','').removeprefix('Bearer ')
                    data=admission.rooms.login(code,body['device_id'])
                elif self.path=='/api/join':
                    if set(body)!={'invite','name','request_id'}: raise Rejected(400,'Unexpected fields')
                    bearer=self.headers.get('Authorization','').removeprefix('Bearer ')
                    data=admission.join(body['invite'],body['name'],body['request_id'],bearer)
                elif self.path=='/api/rooms':
                    if set(body) not in ({'room_name','request_id'},{'room_name','request_id','persistent'}): raise Rejected(400,'Unexpected fields')
                    code=self.headers.get('Authorization','').removeprefix('Bearer ')
                    data=admission.rooms.create(code,body['room_name'],body['request_id'],body.get('persistent',False),short_code='persistent' in body)
                elif self.path=='/api/rooms/list':
                    if body:raise Rejected(400,'Unexpected fields')
                    data=admission.rooms.listing(self.headers.get('Authorization','').removeprefix('Bearer '))
                elif self.path=='/api/rooms/join':
                    if set(body)!={'room','name','request_id'}:raise Rejected(400,'Unexpected fields')
                    data=admission.rooms.join(self.headers.get('Authorization','').removeprefix('Bearer '),body['room'],body['name'],body['request_id'])
                elif self.path=='/api/rooms/keep':
                    if set(body)!={'room','persistent'}:raise Rejected(400,'Unexpected fields')
                    data=admission.rooms.keep(self.headers.get('Authorization','').removeprefix('Bearer '),body['room'],body['persistent'])
                elif self.path in ('/api/admin/creators','/api/admin/revoke-creator','/api/admin/access-keys','/api/admin/revoke-access'):
                    admission.verify_admin(self.headers.get('Authorization','').removeprefix('Bearer '))
                    if self.path.endswith(('revoke-creator','revoke-access')):
                        admission.rooms.revoke_creator(body.get('access_id',body.get('creator_id'))); data={'revoked':True}
                    else: data=admission.rooms.issue_creator(body.get('label',''),body.get('days',30))
                elif self.path in ('/api/admin/invites','/api/admin/revoke'):
                    admission.verify_admin(self.headers.get('Authorization','').removeprefix('Bearer '))
                    if self.path.endswith('/revoke'):
                        admission.revoke(body.get('invitation_id')); data={'revoked':True}
                    else: data=admission.create(body.get('room',''),body.get('role',''),body.get('days',7),body.get('max_uses',12))
                else: raise Rejected(404,'Not found')
                self.reply(200,data)
            except Rejected as e: self.reply(e.status,{'error':e.message})
            except (ValueError,TypeError): self.reply(400,{'error':'Invalid request'})
            except Exception: self.reply(503,{'error':'Admission unavailable; retry later'})
    class Server(ThreadingHTTPServer):
        daemon_threads=True
        slots=threading.BoundedSemaphore(32)
        def process_request(self,request,address):
            if not self.slots.acquire(blocking=False): self.shutdown_request(request); return
            try: super().process_request(request,address)
            except Exception: self.slots.release(); raise
        def process_request_thread(self,request,address):
            try: super().process_request_thread(request,address)
            finally: self.slots.release()
    Server(('127.0.0.1',8106),Handler).serve_forever()

if __name__=='__main__': main()
