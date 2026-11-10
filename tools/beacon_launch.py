"""Standalone Memory beacon. No model downloads or app process required."""
import json,os,secrets,shutil,ssl,subprocess,sys
from http.server import ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit
from qr import terminal

def pairing_page(root,config,enrollment):
    invitation=enrollment.invite()
    print('\nScan to join '+config.get('name','Memory beacon')+'\n',flush=True)
    print(terminal(invitation['uri']),flush=True)
    print('\nOne use · expires in five minutes\n',flush=True)
    print('Invitation code: '+invitation['invitation'],flush=True)
    print('Invitation (one use, five minutes): '+invitation['uri'],flush=True)

def launch(args,project):
    sys.path.insert(0,str(project/'tools/sync'))
    from host import Store,Handler
    from enrollment import Enrollment
    public=args.url.rstrip('/');u=urlsplit(public)
    if u.scheme!='https' or not u.hostname or u.username or u.password or u.query or u.fragment or '%' in u.path or '\\' in u.path:raise ValueError('Use a complete HTTPS public beacon URL')
    if bool(args.tls_cert)!=bool(args.tls_key):raise ValueError('Supply both --tls-cert and --tls-key')
    if args.bind not in ('127.0.0.1','localhost') and not args.tls_cert:raise ValueError('Direct network listening requires --tls-cert and --tls-key. Otherwise use a local HTTPS reverse proxy.')
    os.umask(0o077);root=Path(args.data_dir).expanduser().resolve();root.mkdir(parents=True,exist_ok=True);root.chmod(0o700)
    config_path=root/'beacon-config.json'
    previous=json.loads(config_path.read_text()) if config_path.exists() else {}
    if args.invite:
        if not previous:raise ValueError('Launch this beacon once before refreshing its invitation')
        pairing_page(root,previous,Enrollment(root,previous['public_url']));return
    config={'version':2,'root':str(root),'git':shutil.which('git'),'port':args.port,'token':previous.get('token') or secrets.token_urlsafe(32),'spaces':json.loads((project/'protocol/spaces.json').read_text()),'public_url':public,'path_prefix':u.path.rstrip('/'),'name':args.name,'allow_remote':True,'allow_prompts':False}
    if not config['git']:raise ValueError('Install Git first')
    # Bind before changing configuration: a second launch must not rewrite a live host.
    server=ThreadingHTTPServer((args.bind,args.port),Handler);server.daemon_threads=True
    try:
        config_path.write_text(json.dumps(config,indent=2));config_path.chmod(0o600)
        repo=root/'repositories/fabric.git';repo.parent.mkdir(exist_ok=True)
        if not repo.exists():subprocess.run([config['git'],'init','--bare','--initial-branch=main',str(repo)],check=True,stdout=subprocess.DEVNULL)
        store=Store(config)
        for key,value in [('http.receivepack','true'),('http.getanyfile','false'),('receive.denyNonFastForwards','true'),('receive.denyDeletes','true'),('receive.fsckObjects','true')]:store.git('config',key,value)
        try:store.head()
        except ValueError:
            tree=store.git('mktree',data=b'').decode().strip();commit=store.git('commit-tree',tree,data=b'Initialize private memory ledger\n').decode().strip();store.git('update-ref','refs/heads/main',commit)
        import shlex
        hook=repo/'hooks/pre-receive';hook.write_text('#!/bin/sh\nset -eu\nwhile read old new ref; do\n  '+shlex.quote(sys.executable)+' '+shlex.quote(str(project/'tools/sync/host.py'))+' '+shlex.quote(str(config_path))+' --verify "$old" "$new" "$ref"\ndone\n');hook.chmod(0o700)
        enrollment=Enrollment(root,public);server.config=config;server.store=store;server.enrollment=enrollment
        if args.tls_cert:
            context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.minimum_version=ssl.TLSVersion.TLSv1_2;context.load_cert_chain(args.tls_cert,args.tls_key);server.socket=context.wrap_socket(server.socket,server_side=True)
        print('Memory beacon listening at '+args.bind+':'+str(args.port),flush=True)
        print('Public HTTPS URL: '+public,flush=True)
        pairing_page(root,config,enrollment)
        print('After joining, generate fresh invitations from Devices in the Mac app. Ctrl-C stops the beacon.',flush=True)
        server.serve_forever()
    finally:server.server_close()
