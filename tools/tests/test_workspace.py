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

@unittest.skipUnless(sys.platform=='darwin' and Path('/Applications/Blender.app').is_dir(),'installed Blender on macOS')
class BlenderTests(unittest.TestCase):
 def test_dent_changes_saved_sphere_geometry(self):
  with tempfile.TemporaryDirectory() as folder:
   root=Path(folder)
   original=module.execute(root,{'action':'blender','operation':'create','operations':[{'op':'add_object','primitive':'sphere','name':'Sphere'}],'output_file':'source.blend'})
   self.assertTrue(original['verified'],original)
   source=(root/'source.blend').read_bytes()
   result=module.execute(root,{'action':'blender','operation':'edit','input_file':'source.blend','operations':[{'op':'dent','name':'sphere'}],'output_file':'dented.blend'})
   self.assertTrue(result['verified'],result)
   self.assertEqual((root/'source.blend').read_bytes(),source)
   verify=module.execute(root,{'action':'blender','operation':'edit','input_file':'dented.blend','code':"mesh = bpy.data.objects['Sphere'].data\nassert max(v.co.z for v in mesh.vertices) < 0.95\nassert min(v.co.z for v in mesh.vertices) < -0.99"})
   self.assertEqual(verify['exit_code'],0,verify)

 def test_create_reopen_modify_and_sandbox(self):
  with tempfile.TemporaryDirectory() as folder,tempfile.TemporaryDirectory() as outside:
   root=Path(folder)
   first=module.execute(root,{'action':'blender','operation':'create','input_file':'fabricTest.blend','operations':[{'op':'remove_object','name':'cube'}],'code':"return {'ok':True}",'output_file':'fabricTest.blend'})
   self.assertEqual(first['exit_code'],0,first);self.assertTrue(first['verified']);self.assertEqual({o['name'] for o in first['objects']},{'Camera','Light'})
   with self.assertRaises(ValueError):module.execute(root,{'action':'blender','code':'','output_file':'fabricTest.blend'})
   second=module.execute(root,{'action':'blender','input_file':'fabricTest.blend','operations':[{'op':'add_object','primitive':'sphere','name':'FabricSphere','location':[1,2,3]}],'output_file':'modified.blend'})
   self.assertTrue(second['verified']);self.assertIn('FabricSphere',[o['name'] for o in second['objects']]);self.assertNotIn('Cube',[o['name'] for o in second['objects']]);self.assertEqual(next(o for o in second['objects'] if o['name']=='FabricSphere')['location'],[1,2,3])
   secret=Path(outside)/'secret';secret.write_text('private fixture')
   denied=module.execute(root,{'action':'blender','code':f"open({str(secret)!r}).read()"});self.assertNotEqual(denied['exit_code'],0);self.assertNotIn('private fixture',denied['stdout'])
   failure=module.execute(root,{'action':'blender','code':"raise RuntimeError('fixture failure')",'output_file':'failure.blend'});self.assertNotEqual(failure['exit_code'],0);self.assertFalse((root/'failure.blend').exists())
   with self.assertRaises(ValueError):module.execute(root,{'action':'blender','operation':'edit','code':''})
   for field in ('input_file','output_file'):
    with self.assertRaises(ValueError):module.execute(root,{'action':'blender','code':'',field:'../escape.blend'})

class BlenderValidationTests(unittest.TestCase):
 def test_inspection_cannot_silently_accept_edit_fields(self):
  for op in [{'op':'inspect','primitive':'cube'}, {'op':'inspect','scale':[1,1,1]}, {'op':'remove_object'}, {'op':'transform_object','name':'Cube'}, {'op':'python','code':''}]:
   with self.assertRaises(ValueError):module.validate_blender_operations([op])
  module.validate_blender_operations([{'op':'remove_object','name':'Cube'}])

@unittest.skipUnless(sys.platform=='darwin' and Path('/Applications/Blender.app').is_dir(),'installed Blender on macOS')
class BlenderReadOnlyTests(unittest.TestCase):
 def test_inspect_cannot_save_or_reset_the_partial_file(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);module.execute(root,{'action':'blender','operation':'create','operations':[{'op':'remove_object','name':'Cube'}],'output_file':'partial.blend'})
   original=(root/'partial.blend').read_bytes()
   with self.assertRaises(ValueError):module.execute(root,{'action':'blender','operation':'inspect','input_file':'partial.blend','output_file':'partial.blend','overwrite':True})
   with self.assertRaises(ValueError):module.execute(root,{'action':'blender','operation':'edit','input_file':'partial.blend','operations':[{'op':'inspect','primitive':'cube'}],'output_file':'partial.blend','overwrite':True})
   self.assertEqual((root/'partial.blend').read_bytes(),original)
   result=module.execute(root,{'action':'blender','operation':'inspect','input_file':'partial.blend'});self.assertNotIn('saved_file',result);self.assertNotIn('Cube',[o['name'] for o in result['objects']]);self.assertEqual((root/'partial.blend').read_bytes(),original)
   scripted=module.execute(root,{'action':'blender','operation':'edit','input_file':'partial.blend','operations':[{'op':'python','code':"bpy.ops.mesh.primitive_uv_sphere_add()\nreturn {'ok': True}"}],'output_file':'scripted.blend'});self.assertTrue(scripted['verified']);self.assertIn('Sphere',[o['name'] for o in scripted['objects']])
