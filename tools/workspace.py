"""Fabric-owned workspace tools and bounded, sandboxed command execution."""
import json,os,signal,subprocess,sys,threading,time,tempfile,stat,textwrap,math,re
from pathlib import Path
LIMIT=32768
child=None

def stop(*_):
    global child
    if child and child.poll() is None:
        try:os.killpg(child.pid,signal.SIGKILL)
        except ProcessLookupError:pass
        try:child.wait(timeout=2)
        except Exception:pass
    raise SystemExit(0)

def resolve(root,name):
    if not isinstance(name,str) or len(name)>2048 or Path(name).is_absolute():raise ValueError('Use a relative workspace path')
    path=(root/name).resolve()
    if not path.is_relative_to(root):raise ValueError('Path leaves the selected workspace')
    return path

def sandbox(root,scratch,network,read_paths=()):
    # No user home, Keychain, SSH credentials, or app data is readable by default.
    allowed=[root,scratch,Path('/System'),Path('/usr'),Path('/bin'),Path('/sbin'),Path('/Library'),Path('/private/etc'),Path('/opt/homebrew'),Path(sys.prefix)]
    executable=Path(sys.executable).resolve()
    if '.app/Contents/' in str(executable):allowed.append(Path(str(executable).split('.app/Contents/')[0]+'.app/Contents/Resources'))
    allowed.extend(read_paths)
    quote=lambda p:json.dumps(str(p),ensure_ascii=False)
    profile='(version 1)(deny default)(allow process*)(allow sysctl-read)(allow file-read-metadata)(allow signal (target self))'
    profile+='(allow file-read* '+''.join('(subpath '+quote(p)+')' for p in allowed)+ '(literal "/")(literal "/dev/null")(literal "/dev/random")(literal "/dev/urandom"))'
    profile+='(allow file-write* (subpath '+quote(root)+')(subpath '+quote(scratch)+')(literal "/dev/null"))'
    # Blender initializes Metal even in background mode; these allow GPU discovery, not user-file access.
    if read_paths:profile+='(allow iokit-open)(allow iokit-get-properties)'
    if network:profile+='(allow network-outbound)(allow mach-lookup (global-name "com.apple.mDNSResponder"))'
    return profile

def execute(root,args,python=None,git=None):
    global child
    root=Path(root).resolve();root.mkdir(parents=True,exist_ok=True)
    action=args.get('action');path=resolve(root,args.get('path','.'))
    if action=='list':
        if not path.is_dir():raise ValueError('Not a directory')
        return {'path':str(path.relative_to(root)),'entries':[{'name':p.name,'directory':p.is_dir(),'symlink':p.is_symlink()} for p in sorted(path.iterdir(),key=lambda p:p.name) if p.name!='.fabric'][:250]}
    if action=='read':
        if path.stat().st_size>262144:raise ValueError('File exceeds 256 KB; use a targeted terminal command')
        return {'path':str(path.relative_to(root)),'content':path.read_text()}
    if action=='write':
        text=args.get('content')
        if not isinstance(text,str) or len(text.encode())>262144:raise ValueError('Write content must be text under 256 KB')
        if path==root:raise ValueError('Select a file path')
        path.parent.mkdir(parents=True,exist_ok=True);resolve(root,args['path'])
        mode=stat.S_IMODE(path.stat().st_mode) if path.exists() else 0o600
        fd,name=tempfile.mkstemp(prefix='.fabric-write-',dir=path.parent)
        try:
            with os.fdopen(fd,'w') as output:output.write(text)
            os.chmod(name,mode);os.replace(name,path)
        finally:
            if os.path.exists(name):os.unlink(name)
        return {'path':str(path.relative_to(root)),'written_bytes':len(text.encode())}
    if action in ('list_tools', 'run_tool'):
        return saved_tool(root, path, args, python, git)
    if action=='blender':return blender_execute(root,args,python,git)
    if action!='run':raise ValueError('Unknown workspace action')
    return run_command(root,path,args,python,git)

def saved_tool(root, path, args, python, git):
    directory = root / 'fabric-tools'
    if directory.is_symlink():
        raise ValueError('fabric-tools must be a regular directory in this workspace')

    def valid_name(name):
        return isinstance(name, str) and re.fullmatch(r'[a-z][a-z0-9_-]{0,63}', name)

    if args['action'] == 'list_tools':
        scripts = sorted(
            p.stem for p in directory.glob('*.py')
            if valid_name(p.stem) and not p.is_symlink() and p.is_file()
        )
        return {'directory': 'fabric-tools', 'tools': scripts[:100], 'truncated': len(scripts) > 100}

    name = args.get('name')
    if not valid_name(name):
        raise ValueError('Use a listed tool name without the .py extension')
    script = directory / (name + '.py')
    if script.is_symlink() or not script.is_file():
        raise ValueError(f'No saved Python tool named {name} in this workspace')
    arguments = args.get('arguments', [])
    if not isinstance(arguments, list) or len(arguments) > 32 or any(
        not isinstance(arg, str) or len(arg) > 4096 or chr(0) in arg for arg in arguments
    ):
        raise ValueError('Use up to 32 text arguments of at most 4096 characters each')
    return run_command(
        root, path, {**args, 'network': False}, python, git,
        argv=[python or sys.executable, '-s', str(script), *arguments],
        keep_pythonhome=True,
    )

def run_command(root,path,args,python=None,git=None,argv=None,read_paths=(),keep_pythonhome=False):
    global child
    command=args.get('command','')
    if argv is None and (not isinstance(command,str) or not command.strip() or len(command)>16000):raise ValueError('Enter a command under 16 KB')
    if not path.is_dir():raise ValueError('Command directory does not exist')
    timeout=max(1,min(600,int(args.get('timeout_seconds',60))))
    scratch=resolve(root,'.fabric');scratch.mkdir(exist_ok=True);scratch.chmod(0o700);(scratch/'.gitignore').write_text('*\n')
    for folder in ('home','tmp','cache'):(scratch/folder).mkdir(exist_ok=True)
    base=Path(python or sys.executable).parent
    search=[str(base),str(Path(git).parent) if git else '', '/opt/homebrew/bin','/usr/local/bin','/usr/bin','/bin','/usr/sbin','/sbin']
    env={'PATH':os.pathsep.join(p for p in search if p),'HOME':str(scratch/'home'),'TMPDIR':str(scratch/'tmp'),'XDG_CACHE_HOME':str(scratch/'cache'),'LANG':'en_US.UTF-8','PYTHONDONTWRITEBYTECODE':'1','PIP_DISABLE_PIP_VERSION_CHECK':'1','GIT_TERMINAL_PROMPT':'0','NO_COLOR':'1'}
    if os.environ.get('PYTHONHOME'):env['PYTHONHOME']=os.environ['PYTHONHOME']
    if os.environ.get('SSL_CERT_FILE'):env['SSL_CERT_FILE']=os.environ['SSL_CERT_FILE']
    if git and Path(git).with_name('git-http-backend').is_file():env['GIT_EXEC_PATH']=str(Path(git).parent)
    if sys.platform!='darwin' or not Path('/usr/bin/sandbox-exec').exists():raise ValueError('Sandboxed execution requires macOS')
    if argv is not None and not keep_pythonhome:env.pop('PYTHONHOME',None)
    child=subprocess.Popen(['/usr/bin/sandbox-exec','-p',sandbox(root,scratch,args.get('network') is True,read_paths),*(argv if argv is not None else ['/bin/zsh','-f','-c',command])],cwd=path,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
    buffers={};truncated={}
    def read(name,stream):
        data=bytearray();total=0
        for chunk in iter(lambda:stream.read(4096),b''):
            total+=len(chunk)
            if len(data)<LIMIT:data.extend(chunk[:LIMIT-len(data)])
        buffers[name]=bytes(data).decode(errors='replace');truncated[name]=total>LIMIT;stream.close()
    readers=[threading.Thread(target=read,args=(name,stream)) for name,stream in [('stdout',child.stdout),('stderr',child.stderr)]]
    for reader in readers:reader.start()
    deadline=time.monotonic()+timeout;timed_out=False
    while child.poll() is None:
        if time.monotonic()>=deadline:
            timed_out=True
            try:os.killpg(child.pid,signal.SIGKILL)
            except ProcessLookupError:pass
            break
        time.sleep(.05)
    code=child.wait()
    # Background descendants must not outlive a bounded action or hold its pipes open.
    try:os.killpg(child.pid,signal.SIGKILL)
    except ProcessLookupError:pass
    for reader in readers:reader.join(timeout=3)
    child=None
    return {'exit_code':code,'timed_out':timed_out,'stdout':buffers.get('stdout',''),'stderr':buffers.get('stderr',''),'truncated':any(truncated.values())}

def validate_blender_operations(operations):
    fields={'inspect':{'op'},'dent':{'op','name'},'remove_object':{'op','name'},'add_object':{'op','primitive','name','location','scale','rotation_euler'},'transform_object':{'op','name','location','scale','rotation_euler'},'rename_object':{'op','name','new_name'},'python':{'op','code'}}
    if not isinstance(operations,list) or len(operations)>100:raise ValueError('Use up to 100 Blender operations per action')
    code_bytes=0
    for op in operations:
        if not isinstance(op,dict) or op.get('op') not in fields:raise ValueError('Unsupported Blender operation. Choose '+', '.join(fields)+'. For other mesh edits use python with a bpy function body.')
        kind=op['op'];extra=set(op)-fields[kind]
        if extra:raise ValueError(kind+' does not accept '+', '.join(sorted(extra))+'. Inspection cannot remove or transform an object; choose the actual edit operation.')
        if kind in ('remove_object','transform_object','rename_object','dent') and (not isinstance(op.get('name'),str) or not op['name']):raise ValueError(kind+' requires name')
        if kind=='rename_object' and (not isinstance(op.get('new_name'),str) or not op['new_name']):raise ValueError('rename_object requires new_name')
        if kind=='add_object' and op.get('primitive') not in ('cube','sphere','monkey'):raise ValueError('add_object requires primitive=cube, sphere, or monkey')
        if kind=='transform_object' and not any(k in op for k in ('location','scale','rotation_euler')):raise ValueError('transform_object requires a transform')
        for key in ('location','scale','rotation_euler'):
            if key in op:
                v=op[key]
                if not isinstance(v,list) or len(v)!=3 or not all(isinstance(n,(int,float)) and not isinstance(n,bool) and math.isfinite(n) for n in v):raise ValueError(key+' requires three finite numbers')
        if kind=='python':
            if not isinstance(op.get('code'),str) or not op['code'].strip():raise ValueError('python requires code')
            code_bytes+=len(op['code'].encode())
    if code_bytes>16000:raise ValueError('Python operations must total under 16 KB')

# Blender runs in a separate background process; it cannot read an unsaved GUI scene.
def blender_execute(root,args,python=None,git=None):
    candidates=[Path('/Applications/Blender.app/Contents/MacOS/Blender'),Path('/Applications/Blender.app/Contents/MacOS/blender'),Path.home()/'Applications/Blender.app/Contents/MacOS/Blender']
    binary=next((p for p in candidates if p.is_file()),None)
    if binary is None:raise ValueError('Blender is not installed in Applications. Install Blender, then continue. No GUI permission is needed for background file editing.')
    code=args.get('code','')
    if not isinstance(code,str) or len(code.encode())>16000:raise ValueError('Blender Python must be text under 16 KB')
    source=resolve(root,args['input_file']) if args.get('input_file') else None
    operation=args.get('operation','edit' if source else 'create')
    if operation not in ('create','edit','inspect'):raise ValueError('Blender operation must be create, edit, or inspect')
    if operation=='create':source=None
    elif source is None:raise ValueError('edit/inspect requires input_file naming an existing saved workspace .blend file. Use create for a new General scene. The unsaved GUI scene is separate.')
    operations=args.get('operations',[]);validate_blender_operations(operations)
    if operation=='inspect' and (args.get('output_file') or code.strip() or any(o['op']!='inspect' for o in operations)):raise ValueError('inspect is read-only: omit output_file, code and mutating operations. Choose edit to change and save a file.')
    output=resolve(root,args['output_file']) if args.get('output_file') else None
    if source and (not source.is_file() or source.suffix.lower()!='.blend'):raise ValueError('input_file must be an existing workspace .blend file. For a NEW General scene, choose operation=create and use output_file for its new filename. For editing, list the workspace and use the actual saved input_file.')
    if any(op['op'] == 'dent' for op in operations) and output is None:
        raise ValueError('A dent requires output_file to save the changed mesh.')
    if output and (output==root or output.suffix.lower()!='.blend'):raise ValueError('Output must be a relative .blend file inside the workspace')
    if output and output.exists() and args.get('overwrite') is not True:raise ValueError('Output already exists. Inspect it first, then set overwrite=true only if the user requested replacing it.')
    scratch=resolve(root,'.fabric');scratch.mkdir(exist_ok=True);scratch.chmod(0o700)
    # Metadata comes from reopening the actual saved file, not from the model's prose.
    inspect="import bpy,json\nprint('FABRIC_BLENDER_RESULT='+json.dumps({'objects':[{'name':o.name,'type':o.type,'location':list(o.location),'scale':list(o.scale),'rotation_euler':list(o.rotation_euler)} for o in list(bpy.data.objects)[:200]],'scene':bpy.context.scene.name}))\n"
    base=[str(binary),'--background','--factory-startup','--disable-autoexec','--python-exit-code','1']
    app=Path(str(binary).split('.app/Contents/')[0]+'.app')
    with tempfile.TemporaryDirectory(prefix='blender-',dir=scratch) as folder:
        script=Path(folder)/'action.py'
        edits='import bpy,json\n_fabric_operations=json.loads('+repr(json.dumps(operations))+')\n'
        edits+='''
for op in _fabric_operations:
    kind=op.get('op');name=op.get('name','')
    if kind=='inspect':pass
    elif kind=='python':
        namespace={'bpy':bpy}
        exec('def _fabric_op():'+chr(10)+''.join('    '+line+chr(10) for line in op['code'].splitlines()),namespace)
        namespace['_fabric_op']()
    elif kind=='remove_object':
        if not isinstance(name,str) or not name:raise ValueError('remove_object needs an object name')
        obj=bpy.data.objects.get(name)
        if obj is None:
            matches=[item for item in bpy.data.objects if item.name.casefold()==name.casefold()]
            if len(matches)!=1:raise ValueError('Object name not found or ambiguous: '+name+'; inspect the file for its exact name')
            obj=matches[0]
        bpy.data.objects.remove(obj,do_unlink=True)
    elif kind=='add_object':
        primitive=op.get('primitive')
        if primitive=='sphere':bpy.ops.mesh.primitive_uv_sphere_add()
        elif primitive=='cube':bpy.ops.mesh.primitive_cube_add()
        elif primitive=='monkey':bpy.ops.mesh.primitive_monkey_add()
        else:raise ValueError('add_object primitive must be cube, sphere, or monkey; use Python for other geometry')
        obj=bpy.context.object
        if name:obj.name=name
        for key in ('location','scale','rotation_euler'):
            if key in op:
                value=op[key]
                if not isinstance(value,list) or len(value)!=3 or not all(isinstance(v,(int,float)) for v in value):raise ValueError(key+' needs three numbers')
                setattr(obj,key,value)
    elif kind == 'dent':
        from mathutils import Vector
        matches = [obj for obj in bpy.context.scene.objects if obj.name.casefold() == name.casefold()]
        if len(matches) != 1 or matches[0].type != 'MESH':
            raise ValueError('Dent requires the name of a single mesh object in the scene.')
        obj = matches[0]
        if not obj.data.vertices:
            raise ValueError('The selected mesh has no vertices to dent.')
        # Work in mesh coordinates so an object's scene rotation and scale are preserved.
        bounds = [Vector(corner) for corner in obj.bound_box]
        center = sum(bounds, Vector((0, 0, 0))) / len(bounds)
        radius = max((vertex.co - center).length for vertex in obj.data.vertices)
        if radius <= 0:
            raise ValueError('The selected mesh has no surface to dent.')
        changes = []
        for vertex in obj.data.vertices:
            offset = vertex.co - center
            if offset.length and offset.normalized().z > 0.8:
                weight = ((offset.normalized().z - 0.8) / 0.2) ** 2
                changes.append((vertex.index, offset.normalized() * min(radius * 0.22 * weight, offset.length * 0.5)))
        if not changes:
            raise ValueError('The top of this mesh has no suitable vertices. Subdivide the mesh or use a custom edit.')
        if obj.data.users > 1:
            obj.data = obj.data.copy()
        for index, displacement in changes:
            obj.data.vertices[index].co -= displacement
        obj.data.update()
    elif kind in ('transform_object','rename_object'):
        obj=bpy.data.objects.get(name)
        if obj is None:raise ValueError('Object not found: '+name)
        if kind=='rename_object':
            if not isinstance(op.get('new_name'),str) or not op['new_name']:raise ValueError('rename_object needs new_name')
            obj.name=op['new_name']
        for key in ('location','scale','rotation_euler'):
            if key in op:
                value=op[key]
                if not isinstance(value,list) or len(value)!=3 or not all(isinstance(v,(int,float)) for v in value):raise ValueError(key+' needs three numbers')
                setattr(obj,key,value)
    else:raise ValueError('Unknown Blender operation: '+str(kind))
'''
        script.write_text(edits+'\ndef _fabric_action():\n'+textwrap.indent(code if code.strip() else 'pass','    ')+'\n_fabric_action()\n'+("bpy.ops.wm.save_as_mainfile(filepath="+repr(str(output))+")\n" if output else '')+inspect)
        if output:output.parent.mkdir(parents=True,exist_ok=True)
        run=lambda file,script:run_command(root,root,{'timeout_seconds':args.get('timeout_seconds',60)},python,git,[*base,*([str(file)] if file else []),'--python',str(script)],[app])
        result=run(source,script)
        if result['exit_code']!=0 or result['timed_out']:return result
        if output:
            if not output.is_file() or output.stat().st_size==0:raise ValueError('Blender did not produce the requested output file')
            script.write_text(inspect);check=run(output,script)
            if check['exit_code']!=0 or check['timed_out']:return {**check,'saved_file':str(output.relative_to(root)),'verified':False}
            metadata=check['stdout']
        else:metadata=result['stdout']
        lines=[line.split('FABRIC_BLENDER_RESULT=',1)[1] for line in metadata.splitlines() if line.startswith('FABRIC_BLENDER_RESULT=')]
        if not lines:raise ValueError('Blender finished without scene verification data')
        result.update(json.loads(lines[-1]));result['verified']=True;result['source_file']=str(source.relative_to(root)) if source else None;result['applied_operations']=operations
        if output:result['saved_file']=str(output.relative_to(root));result['bytes']=output.stat().st_size
        return result

if __name__=='__main__':
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
    request=json.loads(sys.stdin.readline())
    def watch():
        sys.stdin.buffer.read()
        stop()
    threading.Thread(target=watch,daemon=True).start()
    try:result={'result':execute(request['root'],request['args'],request.get('python'),request.get('git'))}
    except Exception as error:result={'error':str(error)}
    print(json.dumps(result),flush=True)
