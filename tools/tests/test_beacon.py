import concurrent.futures,json,secrets,shutil,subprocess,sys,tempfile,threading,time,unittest,uuid
from pathlib import Path
from http.server import ThreadingHTTPServer
from urllib.request import Request,urlopen
from urllib.error import HTTPError
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'sync'))
from enrollment import Enrollment
from host import Handler,Store

class BeaconTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.admin=secrets.token_urlsafe(32)
        config={'root':str(self.root),'git':shutil.which('git'),'spaces':{'a'*64:2},'token':self.admin,'allow_remote':True,'path_prefix':'/fabric-git','public_url':'https://beacon.example/fabric-git'}
        subprocess.run([config['git'],'init','--bare','--initial-branch=main',str(self.root/'repositories/fabric.git')],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        self.store=Store(config);tree=self.store.git('mktree',data=b'').decode().strip();commit=self.store.git('commit-tree',tree,data=b'init\n').decode().strip();self.store.git('update-ref','refs/heads/main',commit)
        self.enrollment=Enrollment(self.root,config['public_url']);self.server=ThreadingHTTPServer(('127.0.0.1',0),Handler);self.server.daemon_threads=True;self.server.config=config;self.server.store=self.store;self.server.enrollment=self.enrollment
        self.worker=threading.Thread(target=self.server.serve_forever,daemon=True);self.worker.start();self.base=f'http://127.0.0.1:{self.server.server_port}/fabric-git'
    def tearDown(self):self.server.shutdown();self.server.server_close();self.worker.join();self.temp.cleanup()
    def request(self,path,body=None,token=None):
        request=Request(self.base+path,data=json.dumps(body).encode() if body is not None else None,headers={'Content-Type':'application/json',**({'Authorization':'Bearer '+token} if token else {})})
        try:
            with urlopen(request,timeout=5) as r:return r.status,json.load(r)
        except HTTPError as r:return r.code,json.load(r)
    def join(self):
        status,invite=self.request('/v1/invitations',{},self.admin);self.assertEqual(status,200)
        status,pairing=self.request('/v1/join',{'invitation':invite['invitation'],'device_name':'Test phone'});self.assertEqual(status,200);return invite,pairing
    def test_pair_sync_replay_and_revoke(self):
        self.assertEqual(self.request('/v1/events')[0],401)
        status,discovery=self.request('/v1/beacon');self.assertEqual(status,200);self.assertNotIn('token',discovery)
        invite,pairing=self.join();token=pairing['token'];self.assertNotEqual(token,self.admin)
        self.assertEqual(self.request('/v1/join',{'invitation':invite['invitation'],'device_name':'Replay'})[0],400)
        event={'version':1,'id':str(uuid.uuid4()),'actor':str(uuid.uuid4()),'clock':1,'memory':'test','deleted':False,'text':'A shared fact','embedding':{'space':'a'*64,'values':[1.,0.]}}
        self.assertEqual(self.request('/v1/events',{'events':[event]},token)[1]['inserted'],1)
        self.assertEqual(self.request('/v1/events',{'events':[event]},token)[1]['inserted'],0)
        self.assertEqual(self.request('/v1/events',token=token)[1]['events'],[event])
        self.assertEqual(self.request('/v1/revoke',{'device_id':pairing['device_id']},self.admin)[0],200)
        self.assertEqual(self.request('/v1/events',token=token)[0],401)
        self.assertEqual(self.request('/v1/invitations',{},token)[0],401)
    def test_expired_invitation_and_concurrent_redeem(self):
        invite=self.enrollment.invite()
        with self.enrollment.db() as db:db.execute('UPDATE invitations SET expires=?',(int(time.time())-1,))
        self.assertEqual(self.request('/v1/join',{'invitation':invite['invitation'],'device_name':'Expired'})[0],400)
        invite=self.enrollment.invite()
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results=list(pool.map(lambda _:self.request('/v1/join',{'invitation':invite['invitation'],'device_name':'Concurrent'})[0],range(2)))
        self.assertEqual(sorted(results),[200,400])
    def test_only_public_memories_sync_and_legacy_records_stay_hidden(self):
        _,owner=self.join();_,allowed=self.join();_,other=self.join()
        event={'version':1,'id':str(uuid.uuid4()),'actor':str(uuid.uuid4()),'clock':1,'memory':'restricted-test','deleted':False,'text':'Confidential project','embedding':{'space':'a'*64,'values':[1.,0.]},'access':{'scope':'restricted','allowed_devices':[allowed['device_id']],'reason':'Project information'}}
        self.assertEqual(self.request('/v1/events',{'events':[event]},owner['token'])[0],400)
        self.store.append([event]);self.enrollment.claim(event['memory'],owner['device_id'])
        for device in [owner,allowed,other]:self.assertEqual(len(self.request('/v1/events',token=device['token'])[1]['events']),0)
        old=self.store.head()
        attack={**event,'id':str(uuid.uuid4()),'clock':2,'access':{'scope':'public','allowed_devices':[],'reason':'attack'}}
        self.assertEqual(self.request('/v1/events',{'events':[attack]},allowed['token'])[0],403)
        private={**event,'id':str(uuid.uuid4()),'clock':3,'memory':'private-test','access':{'scope':'private','allowed_devices':[]}}
        self.assertEqual(self.request('/v1/events',{'events':[private]},owner['token'])[0],400)
        self.assertEqual(len(self.request('/v1/events?commit='+old,token=other['token'])[1]['events']),0)
    def test_prefix_and_git_transport(self):
        _,pairing=self.join();status,value=self.request('/v1/status',token=pairing['token']);self.assertEqual(status,200);self.assertEqual(value['service'],'memory-fabric')
        request=Request(self.base+'/fabric.git/info/refs?service=git-upload-pack',headers={'Authorization':'Bearer '+self.admin})
        self.assertEqual(self.request('/fabric.git/info/refs?service=git-upload-pack',token=pairing['token'])[0],403)
        with urlopen(request,timeout=5) as response:self.assertEqual(response.status,200);self.assertIn(b'git-upload-pack',response.read())

if __name__=='__main__':unittest.main()
