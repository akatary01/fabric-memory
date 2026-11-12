"""Memory Fabric MCP host: newline JSON-RPC stdio and Streamable HTTP.
The native process owns this worker. Configuration is private and never sent to models.
"""
import json,os,queue,signal,subprocess,sys,threading,urllib.request,urllib.parse,urllib.error
from pathlib import Path
LIMIT=8*1024*1024
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args):return None
class Session:
    def __init__(self,name,config,timeout=120):
        self.timeout=timeout;self.name=name;self.config=config;self.counter=0;self.version=None;self.session=None;self.process=None;self.pending_id=None
        if config.get('command'):
            allowed=('PATH','HOME','USER','LOGNAME','LANG','LC_ALL','TMPDIR','SHELL')
            env={k:v for k,v in os.environ.items() if k in allowed};extra=[str(Path.home()/'.local/bin'),'/opt/homebrew/bin','/usr/local/bin'];env['PATH']=os.pathsep.join(dict.fromkeys([*env.get('PATH','/usr/bin:/bin').split(os.pathsep),*extra]));env.update(config.get('env',{}))
            try:self.process=subprocess.Popen([config['command'],*config.get('args',[])],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,env=env,cwd=config.get('cwd') or None,start_new_session=True)
            except FileNotFoundError:raise ValueError('Command not found: '+config['command']+'. Install its runtime or use the executable’s full path.') from None
            self.messages=queue.Queue()
            threading.Thread(target=self.read,daemon=True).start()
        else:
            url=urllib.parse.urlsplit(config.get('url',''))
            if url.username or url.password or url.fragment or not(url.scheme=='https' or (url.scheme=='http' and url.hostname in ('localhost','127.0.0.1','::1'))):raise ValueError('MCP requires HTTPS, or HTTP on localhost')
            self.opener=urllib.request.build_opener(NoRedirect())
        try:init=self.rpc('initialize',{'protocolVersion':'2025-11-25','capabilities':{'roots':{'listChanged':False}},'clientInfo':{'name':'Memory Fabric','version':'0.2.0'}})
        except Exception:self.close();raise
        self.version=init.get('protocolVersion')
        if self.version not in ('2024-11-05','2025-03-26','2025-06-18','2025-11-25'):self.close();raise ValueError('Unsupported MCP protocol version')
        self.capabilities=init.get('capabilities',{});self.info=init.get('serverInfo',{})
        self.rpc('notifications/initialized',{},notification=True)
    def read(self):
        try:
            while True:
                line=self.process.stdout.readline(LIMIT+1)
                if not line:break
                if len(line)>LIMIT:self.messages.put(ValueError('MCP message exceeds limit'));return
                try:self.messages.put(json.loads(line))
                except ValueError:self.messages.put(ValueError('MCP server wrote invalid JSON'))
        finally:self.messages.put(EOFError('MCP server disconnected'))
    def server_request(self,message):
        if message.get('method')=='roots/list':
            roots=[{'uri':Path(root).resolve().as_uri(),'name':Path(root).name} for root in self.config.get('roots',[])]
            return {'jsonrpc':'2.0','id':message['id'],'result':{'roots':roots}}
        return {'jsonrpc':'2.0','id':message['id'],'error':{'code':-32601,'message':'Client sampling and elicitation are not enabled'}}
    def rpc(self,method,params,notification=False):
        self.counter+=1;message={'jsonrpc':'2.0','method':method,'params':params}
        if not notification:message['id']=self.counter;self.pending_id=self.counter
        if self.process:
            self.send(message)
            if notification:return None
            while True:
                try:response=self.messages.get(timeout=self.timeout)
                except queue.Empty:
                    self.send({'jsonrpc':'2.0','method':'notifications/cancelled','params':{'requestId':message['id'],'reason':'Timed out'}});raise ValueError('MCP request timed out')
                if isinstance(response,Exception):raise response
                if 'method' in response:
                    if 'id' in response:self.send(self.server_request(response))
                    continue
                if response.get('id')==message['id']:break
        else:
            headers={'Content-Type':'application/json','Accept':'application/json, text/event-stream',**self.config.get('headers',{})}
            if self.session:headers['Mcp-Session-Id']=self.session
            if self.version:headers['MCP-Protocol-Version']=self.version
            request=urllib.request.Request(self.config['url'],data=json.dumps(message).encode(),headers=headers,method='POST')
            try:raw=self.opener.open(request,timeout=self.timeout)
            except urllib.error.HTTPError as e:
                if e.code in (401,403):raise ValueError('MCP authentication failed. Configure the server’s authorization header.')
                raise ValueError('MCP server returned HTTP '+str(e.code))
            with raw:
                self.session=raw.headers.get('Mcp-Session-Id',self.session)
                if notification:return None
                if 'text/event-stream' in raw.headers.get('Content-Type',''):
                    data=[];total=0;response=None
                    for line in raw:
                        total+=len(line)
                        if total>LIMIT:raise ValueError('MCP response exceeds limit')
                        line=line.decode().rstrip('\r\n')
                        if line.startswith('data:'):data.append(line[5:].lstrip())
                        elif line=='':
                            if not data:continue
                            item=json.loads('\n'.join(data));data=[]
                            if item.get('id')==message['id'] and 'method' not in item:response=item;break
                            if 'method' in item and 'id' in item:self.http_reply(self.server_request(item))
                    if response is None:raise ValueError('MCP stream ended before the response')
                else:
                    data=raw.read(LIMIT+1)
                    if len(data)>LIMIT:raise ValueError('MCP response exceeds limit')
                    response=json.loads(data)
        self.pending_id=None
        if response.get('error'):raise ValueError(str(response['error'].get('message','MCP request failed'))[:400])
        return response.get('result',{})
    def http_reply(self,message):
        headers={'Content-Type':'application/json','Accept':'application/json, text/event-stream',**self.config.get('headers',{})}
        if self.session:headers['Mcp-Session-Id']=self.session
        if self.version:headers['MCP-Protocol-Version']=self.version
        with self.opener.open(urllib.request.Request(self.config['url'],data=json.dumps(message).encode(),headers=headers,method='POST'),timeout=10) as r:r.read(1024)
    def send(self,message):
        self.process.stdin.write(json.dumps(message,separators=(',',':')).encode()+b'\n');self.process.stdin.flush()
    def listing(self,kind):
        if kind not in self.capabilities:return []
        items=[];cursor=None
        for _ in range(20):
            result=self.rpc(kind+'/list',{'cursor':cursor} if cursor else {});items.extend(result.get(kind,[]));cursor=result.get('nextCursor')
            if not cursor:return items
        raise ValueError('MCP listing exceeds page limit')
    def cancel(self):
        if self.pending_id is None:return
        notice={'jsonrpc':'2.0','method':'notifications/cancelled','params':{'requestId':self.pending_id,'reason':'Task stopped'}}
        try:
            if self.process:self.send(notice)
            else:self.http_reply(notice)
        except Exception:pass
        self.pending_id=None
    def close(self):
        self.cancel()
        if self.process and self.process.poll() is None:
            try:os.killpg(self.process.pid,signal.SIGTERM)
            except ProcessLookupError:pass
            try:self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:os.killpg(self.process.pid,signal.SIGKILL);self.process.wait(timeout=2)
        if self.process:
            for stream in (self.process.stdin,self.process.stdout):
                if stream:stream.close()
class Host:
    def __init__(self,path):self.path=Path(path);self.sessions={}
    def config(self):return json.loads(self.path.read_text()) if self.path.exists() else {'mcpServers':{}}
    @staticmethod
    def validate(value):
        if not isinstance(value,dict) or set(value)!={'mcpServers'} or not isinstance(value['mcpServers'],dict):raise ValueError('Use {"mcpServers": {"name": {"command": ..., "args": [...]}}}')
        for name,c in value['mcpServers'].items():
            if not name or not isinstance(c,dict) or not (bool(c.get('command')) ^ bool(c.get('url'))):raise ValueError('Each MCP server needs a command or URL')
            if c.get('command') and (not isinstance(c['command'],str) or not isinstance(c.get('args',[]),list) or any(not isinstance(a,str) for a in c.get('args',[]))):raise ValueError('Commands and arguments must be strings')
            if not isinstance(c.get('env',{}),dict) or any(not isinstance(v,str) for v in c.get('env',{}).values()):raise ValueError('Environment values must be strings')
            if not isinstance(c.get('headers',{}),dict) or any(not isinstance(v,str) for v in c.get('headers',{}).values()):raise ValueError('Headers must be strings')
            if c.get('cwd') and (not Path(c['cwd']).is_absolute() or not Path(c['cwd']).is_dir()):raise ValueError('Working directory must be an existing absolute path')
            if not isinstance(c.get('roots',[]),list) or any(not isinstance(r,str) or not Path(r).is_absolute() for r in c.get('roots',[])):raise ValueError('Roots must be absolute paths')
    def save(self,value):
        self.validate(value)
        self.close();self.path.parent.mkdir(parents=True,exist_ok=True);temporary=self.path.with_suffix('.next');temporary.write_text(json.dumps(value,indent=2));temporary.chmod(0o600);temporary.replace(self.path)
    def session(self,name):
        config=self.config()['mcpServers'].get(name)
        if not config or config.get('disabled'):raise ValueError('MCP server is disabled or missing')
        if name not in self.sessions:self.sessions[name]=Session(name,config)
        return self.sessions[name]
    def catalog(self):
        results=[]
        for name,config in self.config()['mcpServers'].items():
            item={'name':name,'enabled':not config.get('disabled',False),'transport':'stdio' if config.get('command') else 'http','tools':[],'resources':[],'prompts':[]}
            if item['enabled']:
                try:
                    s=self.session(name);item.update({'status':'Connected','server':s.info,'tools':s.listing('tools'),'resources':s.listing('resources'),'prompts':s.listing('prompts')})
                except Exception as e:item.update({'status':str(e)[:300]})
            else:item['status']='Disabled'
            results.append(item)
        return results
    def command(self,command,args):
        if command=='test':
            self.validate({'mcpServers':{'connection-test':args['server']}})
            session=Session('connection-test',args['server'],timeout=12)
            try:return {'connected':True,'server':session.info,'tools':len(session.listing('tools')),'resources':len(session.listing('resources')),'prompts':len(session.listing('prompts'))}
            finally:session.close()
        if command=='save':self.save(args['config']);return {'saved':True}
        if command=='config':return self.config()
        if command=='catalog':return self.catalog()
        if command=='restart':
            s=self.sessions.pop(args['server'],None)
            if s:s.close()
            return {'restarted':True}
        if command=='call':
            method=args['method']
            if method not in ('tools/call','resources/read','resources/list','resources/templates/list','prompts/get','prompts/list','completion/complete'):raise ValueError('Unsupported MCP method')
            return self.session(args['server']).rpc(method,args.get('params',{}))
        raise ValueError('Unknown MCP host command')
    def close(self):
        for s in self.sessions.values():s.close()
        self.sessions={}
def main():
    host=Host(sys.argv[1])
    def stop(*_):host.close();sys.exit(0)
    signal.signal(signal.SIGTERM,stop)
    try:
        for line in sys.stdin.buffer:
            try:
                if len(line)>LIMIT:raise ValueError('Host request exceeds limit')
                request=json.loads(line);result=host.command(request['command'],request.get('args',{}));response={'result':result}
            except Exception as e:response={'error':str(e)[:500]}
            print(json.dumps(response,separators=(',',':')),flush=True)
    finally:host.close()
if __name__=='__main__':main()
