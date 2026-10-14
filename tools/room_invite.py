"""Operator CLI: create/revoke invitations using scoped short-lived admin JWTs.

No API secret is distributed to desktop clients. Print only the created invite
when explicitly invoked by the operator, or save it into a private output file.
"""
import argparse
import json
from pathlib import Path
import time
import urllib.request
from tools import room_token as rt

def request(path, body, env_file):
    key,secret=rt.load_env(env_file)
    now=int(time.time())
    token=rt.sign({'iss':key,'sub':'hypercast-operator','aud':'hypercast-invites',
        'scope':'invites:write','nbf':now-5,'exp':now+90},secret)
    url=__import__('os').environ['LIVEKIT_URL'].rstrip('/')
    if not url.startswith('https://'): raise ValueError('Invitation administrator requires HTTPS')
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self,*args,**kwargs): return None
    req=urllib.request.Request(url+path,data=json.dumps(body).encode(),headers={
        'Authorization':'Bearer '+token,'Content-Type':'application/json','User-Agent':'HyperCast-Operator/0.1'})
    with urllib.request.build_opener(NoRedirect).open(req,timeout=10) as response:
        return json.loads(response.read(16384))

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--env-file',type=Path,required=True)
    parser.add_argument('--room'); parser.add_argument('--role',choices=['publisher','viewer'],default='viewer')
    parser.add_argument('--days',type=int,default=7); parser.add_argument('--max-uses',type=int,default=12)
    parser.add_argument('--revoke'); parser.add_argument('--output',type=Path)
    parser.add_argument('--creator','--access-key',dest='creator',action='store_true'); parser.add_argument('--label')
    parser.add_argument('--revoke-creator','--revoke-access',dest='revoke_creator')
    args=parser.parse_args()
    if args.revoke_creator:
        result=request('/api/admin/revoke-creator',{'creator_id':args.revoke_creator},args.env_file)
    elif args.creator:
        if not args.label or not args.output: parser.error('--creator requires --label and --output (private file)')
        result=request('/api/admin/creators',{'label':args.label,'days':args.days},args.env_file)
    elif args.revoke:
        result=request('/api/admin/revoke',{'invitation_id':args.revoke},args.env_file)
    else:
        if not args.room: parser.error('--room is required')
        result=request('/api/admin/invites',{'room':args.room,'role':args.role,'days':args.days,'max_uses':args.max_uses},args.env_file)
    if args.output:
        with args.output.open('x',encoding='utf-8') as stream: json.dump(result,stream,indent=2)
        print('Invitation saved; keep this file private')
    else: print(json.dumps(result,indent=2))

if __name__=='__main__': main()
