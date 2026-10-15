"""Server-owned room creation; bearer creator grants never contain SFU secrets."""
from contextlib import closing
import base64
import hashlib
import hmac
import json
import os
import re
import secrets
import time
import unicodedata
import urllib.parse
import urllib.request
import uuid
from tools import room_token as rt

CODE = re.compile(r"[A-Za-z0-9_-]{32,64}")

def room_api(method, body, key, secret):
    endpoint = os.environ.get('LIVEKIT_URL', 'http://127.0.0.1:8107').rstrip('/')
    parsed = urllib.parse.urlparse(endpoint)
    if parsed.username or parsed.password or parsed.query or parsed.fragment or not (
        parsed.scheme == 'https' or (parsed.scheme == 'http' and parsed.hostname in ('127.0.0.1','localhost','::1'))):
        raise ValueError('Invalid SFU service URL')
    grant = {'roomList':True} if method == 'ListRooms' else {'roomCreate':True}
    if method not in ('ListRooms','CreateRoom'): raise ValueError('Unsupported room service operation')
    now = int(time.time())
    token = rt.sign({'iss':key,'sub':'hypercast-room-service','nbf':now-5,'exp':now+60,'video':grant},secret)
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self,*args,**kwargs): return None
    request = urllib.request.Request(endpoint+'/twirp/livekit.RoomService/'+method,
        data=json.dumps(body).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
    with urllib.request.build_opener(NoRedirect).open(request,timeout=5) as response:
        data=response.read(1024*1024+1)
        if len(data)>1024*1024: raise ValueError('SFU response too large')
        return json.loads(data)

class RoomCreation:
    def __init__(self, admission, api=None, max_rooms=4):
        self.admission=admission
        self.api=api or (lambda method,body:room_api(method,body,admission.key,admission.secret))
        if type(max_rooms) is not int or not 1<=max_rooms<=16: raise ValueError("Room cap must be 1..16")
        self.max_rooms=max_rooms
        with closing(admission.db()) as db:
            db.executescript("""
                CREATE TABLE IF NOT EXISTS creators (
                    id TEXT PRIMARY KEY, hash TEXT UNIQUE, label TEXT,
                    expires INTEGER, revoked INTEGER DEFAULT 0);
                CREATE TABLE IF NOT EXISTS rooms (
                    room TEXT PRIMARY KEY, creator TEXT, request_id TEXT,
                    title TEXT, created INTEGER, expires INTEGER,
                    UNIQUE(creator,request_id));
            """)

        # Additive side table: older servers can still insert legacy room rows.
        with closing(admission.db()) as db:
            db.executescript("""
                CREATE TABLE IF NOT EXISTS room_options (
                    room TEXT PRIMARY KEY, persistent INTEGER NOT NULL DEFAULT 0,
                    code_version INTEGER NOT NULL DEFAULT 1, code_nonce INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE IF NOT EXISTS room_creation_events (
                    room TEXT PRIMARY KEY, actor TEXT NOT NULL, created INTEGER NOT NULL);
                INSERT OR IGNORE INTO room_creation_events
                    SELECT room,CASE WHEN instr(creator,':')>0 THEN substr(creator,1,instr(creator,':')-1) ELSE creator END,created FROM rooms;
                CREATE VIEW IF NOT EXISTS room_catalog AS
                    SELECT rooms.*,COALESCE(o.persistent,0) AS persistent,
                    COALESCE(o.code_version,1) AS code_version,COALESCE(o.code_nonce,0) AS code_nonce
                    FROM rooms LEFT JOIN room_options o ON rooms.room=o.room;
            """)

    def reject(self,status,message):
        raise self.admission.rejection(status,message)

    def issue_creator(self,label,days=30):
        if not isinstance(label,str) or not rt.NAME.fullmatch(label) or type(days) is not int or not 1<=days<=90:
            self.reject(400,'Invalid creator settings')
        code=secrets.token_urlsafe(32);actor=uuid.uuid4().hex
        with closing(self.admission.db()) as db:
            db.execute('INSERT INTO creators(id,hash,label,expires) VALUES(?,?,?,?)',
                (actor,hashlib.sha256(code.encode()).hexdigest(),label,int(time.time())+days*86400))
        return {'creator_id':actor,'creator_code':code,'access_id':actor,'access_key':code,'expires_in':days*86400}

    def login(self,code,device_id):
        if not isinstance(code,str) or not CODE.fullmatch(code): self.reject(401,'Server access key required')
        if not isinstance(device_id,str) or not re.fullmatch(r'[0-9a-f]{32}',device_id): self.reject(400,'Invalid device ID')
        now=int(time.time())
        with closing(self.admission.db()) as db:
            actor=db.execute('SELECT * FROM creators WHERE hash=?',(hashlib.sha256(code.encode()).hexdigest(),)).fetchone()
            if not actor or actor['revoked'] or actor['expires']<=now: self.reject(401,'Server access key expired or revoked')
        token=rt.sign({'iss':self.admission.key,'aud':'hypercast-access','scope':'server:access',
            'sub':actor['id'],'device_id':device_id,'nbf':now-5,'exp':now+900},self.admission.secret)
        return {'token':token,'label':actor['label'],'expires_in':900}

    def authorize(self,bearer):
        try:
            if not isinstance(bearer,str) or len(bearer)>4096: raise ValueError()
            header,body,signature=bearer.split('.')
            def decode(value):return base64.urlsafe_b64decode(value+'='*(-len(value)%4))
            if json.loads(decode(header)).get('alg')!='HS256': raise ValueError()
            expected=hmac.new(self.admission.secret.encode(),(header+'.'+body).encode(),hashlib.sha256).digest()
            if not hmac.compare_digest(expected,decode(signature)): raise ValueError()
            claims=json.loads(decode(body));now=int(time.time())
            if claims.get('iss')!=self.admission.key or claims.get('aud')!='hypercast-access' or claims.get('scope')!='server:access':raise ValueError()
            if type(claims.get('nbf')) is not int or type(claims.get('exp')) is not int or not now-905<=claims['nbf']<=now+5 or not now<claims['exp']<=now+900:raise ValueError()
            actor_id=claims.get('sub');device=claims.get('device_id')
            if not isinstance(actor_id,str) or not re.fullmatch(r'[0-9a-f]{32}',actor_id) or not isinstance(device,str) or not re.fullmatch(r'[0-9a-f]{32}',device):raise ValueError()
            with closing(self.admission.db()) as db:
                actor=db.execute('SELECT * FROM creators WHERE id=?',(actor_id,)).fetchone()
                if not actor or actor['revoked'] or actor['expires']<=now:raise ValueError()
            return {'id':actor_id,'device_id':device,'owner':actor_id+':'+hashlib.sha256(device.encode()).hexdigest()[:32]}
        except (ValueError,TypeError,KeyError,AttributeError):self.reject(401,'Server authentication required')

    def revoke_creator(self,actor):
        if not isinstance(actor,str) or not re.fullmatch(r'[0-9a-f]{32}',actor): self.reject(400,'Invalid creator ID')
        with closing(self.admission.db()) as db: db.execute('UPDATE creators SET revoked=1 WHERE id=?',(actor,))

    def invite_code(self,room,version=1,nonce=0):
        # HMAC makes retries stable; SQLite stores only hashes of the codes.
        if version==1:
            raw=hmac.new(self.admission.secret.encode(),('hypercast-room-invite:'+room).encode(),hashlib.sha256).digest()
            return base64.urlsafe_b64encode(raw).rstrip(b'=').decode()
        raw=hmac.new(self.admission.secret.encode(),f'hypercast-short-room:{room}:{nonce}'.encode(),hashlib.sha256).digest()
        value=int.from_bytes(raw,'big') % (26**8)
        result=''
        for _ in range(8): value,remainder=divmod(value,26);result=chr(65+remainder)+result
        return result

    def result(self,record):
        return {'room':record['room'],'room_name':record['title'],
            'invite':self.invite_code(record['room'],record['code_version'],record['code_nonce']),
            'persistent':bool(record['persistent']),'expires_at':record['expires'],
            'expires_in':None if record['persistent'] else max(0,record['expires']-int(time.time()))}

    def listing(self,session):
        access=self.authorize(session)
        live_info=self.api('ListRooms',{}).get('rooms',[])
        self.cleanup(live_info)
        with closing(self.admission.db()) as db:
            rows=db.execute('SELECT * FROM room_catalog WHERE creator=? OR creator LIKE ? ORDER BY persistent DESC,(expires>?) DESC,created DESC,room LIMIT 32',
                (access['id'],access['id']+':%',int(time.time()))).fetchall()
        live={r['name']:r for r in live_info}
        return {'rooms':[dict(self.result(r),active=r['room'] in live,
            participants=int(live.get(r['room'],{}).get('num_participants',0))) for r in rows]}

    def cleanup(self,live_info=None):
        if live_info is None:live_info=self.api('ListRooms',{}).get('rooms',[])
        participants={r['name']:r.get('num_participants',0) for r in live_info}
        # An admission token issued just before expiry may remain valid for five
        # minutes. Wait for it before deleting records, and never disconnect a
        # live session. The SFU independently retires its empty room instance.
        cutoff=int(time.time())-300
        removed=0
        with closing(self.admission.db()) as db:
            db.execute('BEGIN IMMEDIATE')
            try:
                rows=db.execute('SELECT room FROM room_catalog WHERE persistent=0 AND expires>0 AND expires<=? ORDER BY expires LIMIT 128',(cutoff,)).fetchall()
                for row in rows:
                    room=row['room'];count=participants.get(room,0)
                    if type(count) is not int or count!=0:continue
                    db.execute('DELETE FROM members WHERE invite IN (SELECT id FROM invites WHERE room=?)',(room,))
                    db.execute('DELETE FROM invites WHERE room=?',(room,))
                    db.execute('DELETE FROM room_options WHERE room=?',(room,))
                    db.execute('DELETE FROM rooms WHERE room=?',(room,))
                    removed+=1
                db.commit()
            except Exception:db.rollback();raise
        return removed

    def join(self,session,room,name,request_id):
        access=self.authorize(session)
        if not isinstance(room,str) or not rt.NAME.fullmatch(room):self.reject(400,'Invalid room')
        with closing(self.admission.db()) as db:
            record=db.execute('SELECT * FROM room_catalog WHERE room=? AND (creator=? OR creator LIKE ?)',
                (room,access['id'],access['id']+':%')).fetchone()
        if record is None:self.reject(404,'Room not found')
        # Preserve existing membership/expiry/admission checks; no invitation
        # code needs to be supplied by the client using the room catalog.
        code=self.invite_code(room,record['code_version'],record['code_nonce'])
        return self.admission.join(code,name,request_id,session)

    def delete(self,session,room):
        access=self.authorize(session)
        if not isinstance(room,str) or not rt.NAME.fullmatch(room):self.reject(400,'Invalid room')
        with closing(self.admission.db()) as db:
            db.execute('BEGIN IMMEDIATE')
            try:
                record=db.execute('SELECT room FROM rooms WHERE room=? AND (creator=? OR creator LIKE ?)',
                    (room,access['id'],access['id']+':%')).fetchone()
                if not record:self.reject(404,'Room not found')
                live=self.api('ListRooms',{}).get('rooms',[])
                for item in live:
                    if item['name']==room:
                        count=item.get('num_participants',0)
                        if type(count) is not int or count!=0:self.reject(409,'Room is occupied')
                # Delete only the catalog. Never call DeleteRoom here: a participant
                # with a previously issued JWT may have joined since ListRooms.
                # The SFU retires the empty instance through its own timeout.
                db.execute('DELETE FROM members WHERE invite IN (SELECT id FROM invites WHERE room=?)',(room,))
                db.execute('DELETE FROM invites WHERE room=?',(room,))
                db.execute('DELETE FROM room_options WHERE room=?',(room,))
                db.execute('DELETE FROM rooms WHERE room=?',(room,))
                db.commit()
            except Exception:db.rollback();raise
        return {'deleted':True}

    def keep(self,session,room,persistent):
        access=self.authorize(session)
        if not isinstance(room,str) or not rt.NAME.fullmatch(room) or type(persistent) is not bool:
            self.reject(400,'Invalid room settings')
        with closing(self.admission.db()) as db:
            db.execute('BEGIN IMMEDIATE')
            record=db.execute('SELECT * FROM room_catalog WHERE room=? AND (creator=? OR creator LIKE ?)',
                (room,access['id'],access['id']+':%')).fetchone()
            if not record:self.reject(404,'Room not found')
            if not record['persistent'] and record['expires']<=int(time.time()):
                count=db.execute('SELECT COUNT(*) FROM room_catalog WHERE (creator=? OR creator LIKE ?) AND (persistent=1 OR expires>?)',
                    (access['id'],access['id']+':%',int(time.time()))).fetchone()[0]
                if count>=32:self.reject(409,'Saved room limit reached')
            expires=0 if persistent else int(time.time())+86400
            db.execute('UPDATE rooms SET expires=? WHERE room=?',(expires,room))
            db.execute('INSERT INTO room_options VALUES(?,?,?,?) ON CONFLICT(room) DO UPDATE SET persistent=excluded.persistent',
                (room,int(persistent),record['code_version'],record['code_nonce']))
            db.execute('UPDATE invites SET expires=? WHERE room=? AND revoked=0',(expires,room))
            record=db.execute('SELECT * FROM room_catalog WHERE room=?',(room,)).fetchone();db.commit()
        return self.result(record)

    def ensure(self,db,room):
        record=db.execute('SELECT * FROM room_catalog WHERE room=?',(room,)).fetchone()
        if record is None: return
        if not record['persistent'] and record['expires']<=int(time.time()): self.reject(401,'Room invitation expired')
        live_info=self.api('ListRooms',{}).get('rooms',[])
        live={r['name'] for r in live_info}
        if room in live: return
        # A just-left room may be waiting for the SFU's one-second departure
        # timeout. Wait briefly; never evict an instance from a stale snapshot.
        if len(live)>=self.max_rooms and any(r.get('num_participants',0)==0 for r in live_info):
            for _ in range(5):
                time.sleep(0.4)
                live_info=self.api('ListRooms',{}).get('rooms',[])
                live={r['name'] for r in live_info}
                if room in live:return
                if len(live)<self.max_rooms:break
        if len(live)>=self.max_rooms: self.reject(409,'Server room limit reached')
        self.api('CreateRoom',{'name':room,'max_participants':12,'empty_timeout':60,
            'departure_timeout':1,'metadata':json.dumps({'hypercast_name':record['title'],'hypercast_creator':record['creator']},ensure_ascii=False)})

    def ensure_join(self,room):
        with closing(self.admission.db()) as db:
            if db.execute('SELECT 1 FROM rooms WHERE room=?',(room,)).fetchone() is None: return
            db.execute('BEGIN IMMEDIATE')
            try: self.ensure(db,room);db.commit()
            except Exception: db.rollback();raise

    def create(self,session,title,request_id,persistent=False,short_code=True):
        access=self.authorize(session)
        if type(persistent) is not bool:self.reject(400,'Invalid permanence setting')
        if not isinstance(title,str): self.reject(400,'Invalid room name')
        title=unicodedata.normalize('NFC',title.strip())
        if not 1<=len(title)<=64 or len(title.encode())>256 or any(unicodedata.category(c).startswith('C') for c in title):
            self.reject(400,'Room name must contain 1 to 64 visible characters')
        if not isinstance(request_id,str) or not re.fullmatch(r'[0-9a-f]{32}',request_id): self.reject(400,'Invalid request ID')
        now=int(time.time())
        with closing(self.admission.db()) as db:
            # Serializes both new rooms and joins that recreate an idle room.
            db.execute('BEGIN IMMEDIATE')
            try:
                actor=db.execute('SELECT * FROM creators WHERE id=?',(access['id'],)).fetchone()
                if not actor or actor['revoked'] or actor['expires']<=now: self.reject(401,'Creator authorization expired or revoked')
                previous=db.execute('SELECT * FROM room_catalog WHERE creator=? AND request_id=?',(access['owner'],request_id)).fetchone()
                if previous and (previous['title']!=title or bool(previous['persistent'])!=persistent):
                    self.reject(409,'Request ID already used with different settings')
                if previous is None:
                    # A repeated name retrieves the registered room rather than
                    # creating an inaccessible duplicate with another code.
                    previous=next((r for r in db.execute('SELECT * FROM room_catalog WHERE (creator=? OR creator LIKE ?) AND (persistent=1 OR expires>?)',
                        (actor['id'],actor['id']+':%',now)) if r['title'].casefold()==title.casefold()),None)
                if previous:
                    room=previous['room']
                    if persistent and not previous['persistent']:
                        db.execute('UPDATE rooms SET expires=0 WHERE room=?',(room,))
                        db.execute('INSERT INTO room_options VALUES(?,?,?,?) ON CONFLICT(room) DO UPDATE SET persistent=excluded.persistent',
                            (room,1,previous['code_version'],previous['code_nonce']))
                        db.execute('UPDATE invites SET expires=0 WHERE room=? AND revoked=0',(room,))
                else:
                    db.execute('DELETE FROM room_creation_events WHERE created<=?',(now-86400,))
                    if db.execute('SELECT COUNT(*) FROM room_creation_events WHERE actor=? AND created>?',(actor['id'],now-86400)).fetchone()[0]>=10:
                        self.reject(429,'Daily room creation limit reached')
                    if db.execute('SELECT COUNT(*) FROM room_catalog WHERE (creator=? OR creator LIKE ?) AND (persistent=1 OR expires>?)',
                        (actor['id'],actor['id']+':%',now)).fetchone()[0]>=32:self.reject(409,'Saved room limit reached')
                    room='hcr-'+hmac.new(self.admission.secret.encode(),('hypercast-room:'+access['owner']+':'+request_id).encode(),hashlib.sha256).hexdigest()[:32]
                    if db.execute('SELECT 1 FROM room_creation_events WHERE room=?',(room,)).fetchone():self.reject(409,'Deleted room request')
                    expires=0 if persistent else now+86400;version=2 if short_code else 1;nonce=0
                    while db.execute('SELECT 1 FROM invites WHERE hash=?',(hashlib.sha256(self.invite_code(room,version,nonce).encode()).hexdigest(),)).fetchone():
                        if version==1 or nonce>=32:self.reject(503,'Unable to allocate room code')
                        nonce+=1
                    db.execute('INSERT INTO rooms(room,creator,request_id,title,created,expires) VALUES(?,?,?,?,?,?)',
                        (room,access['owner'],request_id,title,now,expires))
                    db.execute('INSERT INTO room_creation_events VALUES(?,?,?)',(room,actor['id'],now))
                    db.execute('INSERT INTO room_options VALUES(?,?,?,?)',(room,int(persistent),version,nonce))
                    invitation=uuid.uuid4().hex
                    db.execute('INSERT INTO invites(id,hash,room,role,expires,max_uses) VALUES(?,?,?,?,?,?)',
                        (invitation,hashlib.sha256(self.invite_code(room,version,nonce).encode()).hexdigest(),room,'publisher',expires,12))
                # Saved rooms do not reserve a media slot; admission opens it.
                record=db.execute('SELECT * FROM room_catalog WHERE room=?',(room,)).fetchone()
                db.commit()
            except Exception: db.rollback();raise
        return self.result(record)
