import importlib.util,json,os,sys,tempfile,unittest,subprocess,time
from unittest.mock import patch
from pathlib import Path
spec=importlib.util.spec_from_file_location('fabric_workspace',Path(__file__).resolve().parents[1]/'workspace.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class WorkspaceTests(unittest.TestCase):
 def test_file_paths_and_symlink_escape(self):
  with tempfile.TemporaryDirectory() as folder,tempfile.TemporaryDirectory() as outside:
   root=Path(folder);module.execute(root,{'action':'write','path':'src/main.py','content':'print(42)\n'});self.assertEqual(module.execute(root,{'action':'read','path':'src/main.py'})['content'],'print(42)\n')
   (root/'escape').symlink_to(outside)
   for path in ('../outside','/etc/passwd','escape/private'):
    with self.assertRaises(ValueError):module.execute(root,{'action':'write','path':path,'content':'bad'})
 @unittest.skipUnless(sys.platform=='darwin','macOS sandbox')
 def test_execute_fix_failure_and_bound_output(self):
  with tempfile.TemporaryDirectory() as folder:
   root=Path(folder)
   module.execute(root,{'action':'write','path':'main.py','content':'assert 2+2==5\n'})
   first=module.execute(root,{'action':'run','command':'python3 main.py'});self.assertNotEqual(first['exit_code'],0);self.assertIn('AssertionError',first['stderr'])
   module.execute(root,{'action':'write','path':'main.py','content':'assert 2+2==4\nprint("passed")\n'})
   second=module.execute(root,{'action':'run','command':'python3 main.py'});self.assertEqual(second['exit_code'],0,second);self.assertIn('passed',second['stdout'])
   bounded=module.execute(root,{'action':'run','command':"python3 -c 'print(\"x\"*100000)'"});self.assertTrue(bounded['truncated']);self.assertLessEqual(len(bounded['stdout']),32768)
 @unittest.skipUnless(sys.platform=='darwin','macOS sandbox')
 def test_sandbox_and_timeout(self):
  with tempfile.TemporaryDirectory() as folder,tempfile.TemporaryDirectory() as outside:
   root=Path(folder);secret=Path(outside)/'secret';secret.write_text('not available')
   result=module.execute(root,{'action':'run','command':'cat '+str(secret)});self.assertNotEqual(result['exit_code'],0)
   result=module.execute(root,{'action':'run','command':'touch '+str(Path(outside)/'new')});self.assertNotEqual(result['exit_code'],0);self.assertFalse((Path(outside)/'new').exists())
   result=module.execute(root,{'action':'run','command':'sleep 10','timeout_seconds':1});self.assertTrue(result['timed_out'])

 @unittest.skipUnless(sys.platform=='darwin','macOS sandbox')
 def test_worker_stop_and_environment_isolation(self):
  with tempfile.TemporaryDirectory() as folder:
   root=Path(folder)
   with patch.dict(os.environ,{'FABRIC_TEST_NO_SHARE':'fixture-secret'}):
    result=module.execute(root,{'action':'run','command':"python3 -c 'import os; print(os.getenv(\"FABRIC_TEST_NO_SHARE\",\"absent\"))'"})
   self.assertEqual(result['stdout'].strip(),'absent')
   worker=subprocess.Popen([sys.executable,'-u',str(Path(__file__).resolve().parents[1]/'workspace.py')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   worker.stdin.write(json.dumps({'root':str(root),'args':{'action':'run','command':'echo $$ > child.pid; sleep 20'}})+'\n');worker.stdin.flush()
   deadline=time.monotonic()+5
   while not (root/'child.pid').exists() and time.monotonic()<deadline:time.sleep(.05)
   self.assertTrue((root/'child.pid').exists());pid=int((root/'child.pid').read_text());worker.terminate();worker.wait(timeout=5)
   worker.stdin.close();worker.stdout.close();worker.stderr.close()
   with self.assertRaises(ProcessLookupError):os.kill(pid,0)
