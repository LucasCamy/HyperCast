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

    def invite_code(self,room):
        # Stable retry response, while SQLite stores only the invite hash.
        raw=hmac.new(self.admission.secret.encode(),('hypercast-room-invite:'+room).encode(),hashlib.sha256).digest()
        return base64.urlsafe_b64encode(raw).rstrip(b'=').decode()

    def ensure(self,db,room):
        record=db.execute('SELECT * FROM rooms WHERE room=?',(room,)).fetchone()
        if record is None: return
        if record['expires']<=int(time.time()): self.reject(401,'Room invitation expired')
        live_info=self.api('ListRooms',{}).get('rooms',[])
        live={r['name'] for r in live_info}
        if room in live: return
        owned={r[0] for r in db.execute('SELECT room FROM rooms WHERE creator=?',(record['creator'],))}
        for item in live_info:
            try: metadata=json.loads(item.get('metadata','{}'))
            except (ValueError,TypeError): continue
            if isinstance(metadata,dict) and metadata.get('hypercast_creator')==record['creator']:
                owned.add(item['name'])
        if live & owned: self.reject(409,'Creator already has an active room')
        if len(live)>=self.max_rooms: self.reject(409,'Server room limit reached')
        self.api('CreateRoom',{'name':room,'max_participants':12,'empty_timeout':600,
            'departure_timeout':600,'metadata':json.dumps({'hypercast_name':record['title'],'hypercast_creator':record['creator']},ensure_ascii=False)})

    def ensure_join(self,room):
        with closing(self.admission.db()) as db:
            if db.execute('SELECT 1 FROM rooms WHERE room=?',(room,)).fetchone() is None: return
            db.execute('BEGIN IMMEDIATE')
            try: self.ensure(db,room);db.commit()
            except Exception: db.rollback();raise

    def create(self,session,title,request_id):
        access=self.authorize(session)
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
                previous=db.execute('SELECT * FROM rooms WHERE creator=? AND request_id=?',(access['owner'],request_id)).fetchone()
                if previous:
                    if previous['title']!=title: self.reject(409,'Request ID already used for another room name')
                    room=previous['room'];expires=previous['expires']
                else:
                    if db.execute('SELECT COUNT(*) FROM rooms WHERE (creator=? OR creator LIKE ?) AND created>?',(actor['id'],actor['id']+':%',now-86400)).fetchone()[0]>=10:
                        self.reject(429,'Daily room creation limit reached')
                    room='hcr-'+hmac.new(self.admission.secret.encode(),('hypercast-room:'+access['owner']+':'+request_id).encode(),hashlib.sha256).hexdigest()[:32];expires=now+86400
                    db.execute('INSERT INTO rooms VALUES(?,?,?,?,?,?)',(room,access['owner'],request_id,title,now,expires))
                    invitation=uuid.uuid4().hex
                    db.execute('INSERT INTO invites(id,hash,room,role,expires,max_uses) VALUES(?,?,?,?,?,?)',
                        (invitation,hashlib.sha256(self.invite_code(room).encode()).hexdigest(),room,'publisher',expires,12))
                self.ensure(db,room)
                # Room ID/code are deterministic: a lost RPC response cannot create duplicates on retry.
                db.commit()
            except Exception: db.rollback();raise
        return {'room':room,'room_name':title,'invite':self.invite_code(room),'expires_in':max(0,expires-now)}
