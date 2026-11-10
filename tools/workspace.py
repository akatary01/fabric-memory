"""Fabric-owned workspace tools and bounded, sandboxed command execution."""
import json,os,signal,subprocess,sys,threading,time,tempfile,stat
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

def sandbox(root,scratch,network):
    # No user home, Keychain, SSH credentials, or app data is readable by default.
    allowed=[root,scratch,Path('/System'),Path('/usr'),Path('/bin'),Path('/sbin'),Path('/Library'),Path('/private/etc'),Path('/opt/homebrew'),Path(sys.prefix)]
    executable=Path(sys.executable).resolve()
    if '.app/Contents/' in str(executable):allowed.append(Path(str(executable).split('.app/Contents/')[0]+'.app/Contents/Resources'))
    quote=lambda p:json.dumps(str(p),ensure_ascii=False)
    profile='(version 1)(deny default)(allow process*)(allow sysctl-read)(allow file-read-metadata)(allow signal (target self))'
    profile+='(allow file-read* '+''.join('(subpath '+quote(p)+')' for p in allowed)+ '(literal "/")(literal "/dev/null")(literal "/dev/random")(literal "/dev/urandom"))'
    profile+='(allow file-write* (subpath '+quote(root)+')(subpath '+quote(scratch)+')(literal "/dev/null"))'
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
    if action!='run':raise ValueError('Unknown workspace action')
    command=args.get('command','')
    if not isinstance(command,str) or not command.strip() or len(command)>16000:raise ValueError('Enter a command under 16 KB')
    if not path.is_dir():raise ValueError('Command directory does not exist')
    timeout=max(1,min(120,int(args.get('timeout_seconds',60))))
    scratch=resolve(root,'.fabric');scratch.mkdir(exist_ok=True);scratch.chmod(0o700);(scratch/'.gitignore').write_text('*\n')
    for folder in ('home','tmp','cache'):(scratch/folder).mkdir(exist_ok=True)
    base=Path(python or sys.executable).parent
    search=[str(base),str(Path(git).parent) if git else '', '/opt/homebrew/bin','/usr/local/bin','/usr/bin','/bin','/usr/sbin','/sbin']
    env={'PATH':os.pathsep.join(p for p in search if p),'HOME':str(scratch/'home'),'TMPDIR':str(scratch/'tmp'),'XDG_CACHE_HOME':str(scratch/'cache'),'LANG':'en_US.UTF-8','PYTHONDONTWRITEBYTECODE':'1','PIP_DISABLE_PIP_VERSION_CHECK':'1','GIT_TERMINAL_PROMPT':'0','NO_COLOR':'1'}
    if os.environ.get('PYTHONHOME'):env['PYTHONHOME']=os.environ['PYTHONHOME']
    if os.environ.get('SSL_CERT_FILE'):env['SSL_CERT_FILE']=os.environ['SSL_CERT_FILE']
    if git and Path(git).with_name('git-http-backend').is_file():env['GIT_EXEC_PATH']=str(Path(git).parent)
    if sys.platform!='darwin' or not Path('/usr/bin/sandbox-exec').exists():raise ValueError('Sandboxed execution requires macOS')
    child=subprocess.Popen(['/usr/bin/sandbox-exec','-p',sandbox(root,scratch,args.get('network') is True),'/bin/zsh','-f','-c',command],cwd=path,env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
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
