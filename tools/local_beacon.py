"""App-owned LAN beacon. Its lifetime follows the owning app's stdin pipe."""
import base64,json,os,secrets,ssl,subprocess,sys,threading
from http.server import ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlencode
from host import Store,Handler
from enrollment import Enrollment

def serve(config_path):
    os.umask(0o077)
    config=json.loads(Path(config_path).read_text());root=Path(config['root'])
    if Path(config['git']).with_name('git-http-backend').is_file():os.environ['GIT_EXEC_PATH']=str(Path(config['git']).parent)
    class LocalEnrollment(Enrollment):
        def invite(self):
            value=super().invite()
            encoded=config['ca_der'].rstrip('=').replace('+','-').replace('/','_')
            value['uri']='fabric-memory://pair?'+urlencode({'url':self.public_url,'certificate':encoded})+'#invite='+value['invitation']
            return value
        def join(self,invitation,name):
            value=super().join(invitation,name)
            value.update(certificate_der=config['certificate_der'],ca_der=config['ca_der'])
            return value
    server=ThreadingHTTPServer((config.get('bind','0.0.0.0'),config['port']),Handler);server.daemon_threads=True
    repo=root/'repositories/fabric.git';repo.parent.mkdir(exist_ok=True)
    if not repo.exists():subprocess.run([config['git'],'init','--bare','--initial-branch=main',str(repo)],check=True,stdout=subprocess.DEVNULL)
    store=Store(config)
    for key,value in [('http.receivepack','true'),('http.getanyfile','false'),('receive.denyNonFastForwards','true'),('receive.denyDeletes','true'),('receive.fsckObjects','true')]:store.git('config',key,value)
    try:store.head()
    except ValueError:
        tree=store.git('mktree',data=b'').decode().strip();commit=store.git('commit-tree',tree,data=b'Initialize memory beacon\n').decode().strip();store.git('update-ref','refs/heads/main',commit)
    import shlex
    (repo/'hooks').mkdir(exist_ok=True)
    hook=repo/'hooks/pre-receive';hook.write_text('#!/bin/sh\nset -eu\nwhile read old new ref; do\n '+shlex.quote(sys.executable)+' '+shlex.quote(str(Path(__file__).with_name('host.py')))+' '+shlex.quote(str(config_path))+' --verify "$old" "$new" "$ref"\ndone\n');hook.chmod(0o700)
    server.config=config;server.store=store;server.enrollment=LocalEnrollment(root,config['public_url'])
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);context.minimum_version=ssl.TLSVersion.TLSv1_2;context.load_cert_chain(config['certificate'],config['private_key']);server.socket=context.wrap_socket(server.socket,server_side=True)
    worker=threading.Thread(target=server.serve_forever,daemon=True);worker.start()
    print('ready',flush=True)
    try:sys.stdin.buffer.read() # Closing the owning app closes this pipe.
    finally:server.shutdown();server.server_close();worker.join(timeout=3)
if __name__=='__main__':serve(sys.argv[1])
