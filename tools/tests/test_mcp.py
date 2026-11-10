import json,os,sys,tempfile,threading,unittest
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from mcp_bridge import Host,Session
FIXTURE='''import json,sys,os
count=0
for line in sys.stdin:
 r=json.loads(line);m=r.get('method');p=r.get('params',{})
 if 'id' not in r:continue
 if m=='initialize':v={'protocolVersion':p['protocolVersion'],'capabilities':{'tools':{},'resources':{},'prompts':{}},'serverInfo':{'name':'fixture','version':'1'}}
 elif m=='tools/list':v={'tools':[{'name':'increment','description':'Counter','inputSchema':{'type':'object'}}]}
 elif m=='resources/list':v={'resources':[{'uri':'fixture://note','name':'Note'}]}
 elif m=='prompts/list':v={'prompts':[{'name':'example'}]}
 elif m=='tools/call':
  count+=1;v={'content':[{'type':'text','text':str(count)}],'key_inherited':'OPENAI_API_KEY' in os.environ}
 elif m=='resources/read':v={'contents':[{'uri':'fixture://note','text':'fixture data'}]}
 else:v={}
 print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':v}),flush=True)
'''
class MCPTests(unittest.TestCase):
 def test_stdio_sessions_resources_and_secret_isolation(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);script=root/'server.py';script.write_text(FIXTURE);h=Host(root/'mcp.json');previous=os.environ.get('OPENAI_API_KEY');os.environ['OPENAI_API_KEY']='fixture-only'
   try:
    h.save({'mcpServers':{'fixture':{'command':sys.executable,'args':['-u',str(script)]}}})
    catalog=h.catalog();self.assertEqual(catalog[0]['status'],'Connected');self.assertEqual(len(catalog[0]['resources']),1);self.assertEqual(len(catalog[0]['prompts']),1)
    for expected in ('1','2'):
     result=h.command('call',{'server':'fixture','method':'tools/call','params':{'name':'increment','arguments':{}}});self.assertEqual(result['content'][0]['text'],expected);self.assertFalse(result['key_inherited'])
    self.assertEqual(h.command('call',{'server':'fixture','method':'resources/read','params':{'uri':'fixture://note'}})['contents'][0]['text'],'fixture data')
    process=h.sessions['fixture'].process;h.close();self.assertIsNotNone(process.poll())
   finally:
    h.close()
    if previous is None:os.environ.pop('OPENAI_API_KEY',None)
    else:os.environ['OPENAI_API_KEY']=previous
 def test_http_sse_sessions_and_protocol_headers(self):
  seen=[]
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_POST(self):
    r=json.loads(self.rfile.read(int(self.headers['Content-Length'])));seen.append((r.get('method'),self.headers.get('Mcp-Session-Id'),self.headers.get('MCP-Protocol-Version')))
    if 'id' not in r:self.send_response(202);self.end_headers();return
    result={'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'fixture'}} if r['method']=='initialize' else {'tools':[]}
    body=('event: message\ndata: '+json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result})+'\n\n').encode();self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Mcp-Session-Id','fixture-session');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
  try:
   s=Session('fixture',{'url':f'http://127.0.0.1:{server.server_port}/mcp'});self.assertEqual(s.listing('tools'),[]);self.assertEqual(seen[-1],('tools/list','fixture-session','2025-11-25'))
  finally:server.shutdown();server.server_close();thread.join()
 def test_remote_http_and_redirects_are_not_silently_trusted(self):
  with self.assertRaises(ValueError):Session('bad',{'url':'http://example.com/mcp'})
  with tempfile.TemporaryDirectory() as d:
   h=Host(Path(d)/'config.json')
   with self.assertRaises(ValueError):h.save({'mcpServers':{'bad':{'command':'x','args':'shell text'}}})
if __name__=='__main__':unittest.main()
