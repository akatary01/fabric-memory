import argparse,hashlib,json,os,shutil,subprocess,sys,tempfile,plistlib,signal,time
from pathlib import Path

def support():
    if sys.platform=='darwin':return Path.home()/'Library/Application Support/local.memoryfabric.mac'
    return Path(os.environ.get('XDG_DATA_HOME',str(Path.home()/'.local/share')))/'fabric-memory'

def fingerprint(root):
    h=hashlib.sha256(str(root).encode())
    paths=[root/'AGENTS.md',root/'README.md',*root.glob('docs/*.md'),root/'Cargo.lock',root/'Cargo.toml',root/'core/Cargo.toml',*root.glob('core/**/*.rs'),*root.glob('apps/mac/src-tauri/src/*.rs'),root/'apps/mac/src-tauri/Cargo.toml',root/'apps/mac/src-tauri/build.rs',root/'apps/mac/src-tauri/Tauri.toml',*root.glob('apps/mac/ui/**/*'),*root.glob('apps/mac/automation/*.swift'),root/'tools/mcp_bridge.py',root/'tools/packaged_services.py',root/'tools/local_beacon.py',root/'tools/workspace.py',root/'tools/sync/host.py',root/'tools/sync/enrollment.py',root/'tools/bundle_git.py',root/'apps/mac/icons/FabricMemory.icns']
    for p in sorted(p for p in paths if p.is_file()):h.update(str(p.relative_to(root)).encode());h.update(p.read_bytes())
    return h.hexdigest()

def stop_instances(executable):
    result=subprocess.run(['ps','-ax','-o','pid=,comm='],check=True,capture_output=True,text=True)
    pids=[int(line.strip().split(None,1)[0]) for line in result.stdout.splitlines() if len(line.strip().split(None,1))==2 and line.strip().split(None,1)[1]==str(executable)]
    for pid in pids:
        try:os.kill(pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for _ in range(30):
        remaining=[]
        for pid in pids:
            try:os.kill(pid,0);remaining.append(pid)
            except ProcessLookupError:pass
        if not remaining:return
        time.sleep(.1)
    if pids:raise ValueError('Close fabric-memory, then run fabric-memory run again to finish updating it.')

APPLICATIONS_APP = Path('/Applications/fabric-memory.app')

def install_mac_app(root, binary, env, destination=None):
    app = destination or support()/'App/fabric-memory.app'
    app.parent.mkdir(parents=True, exist_ok=True)
    source = fingerprint(root)
    with tempfile.TemporaryDirectory(prefix='.fabric-stage-',dir=app.parent) as stage_dir:
        stage=Path(stage_dir)/'fabric-memory.app';contents=stage/'Contents';(contents/'MacOS').mkdir(parents=True);(contents/'Resources').mkdir()
        shutil.copy2(binary,contents/'MacOS/fabric-mac')
        subprocess.run(['xcrun','swiftc','-O',str(root/'apps/mac/automation/ComputerUse.swift'),'-o',str(contents/'MacOS/fabric-computer-use')],check=True)
        shutil.copyfile(root/'apps/mac/icons/FabricMemory.icns',contents/'Resources/FabricMemory.icns')
        runtime_env={key:env[key] for key in ['FABRIC_ASSET_DIR','ORT_DYLIB_PATH','FABRIC_DATA_DIR','FABRIC_ENV_FILE','FABRIC_PYTHON'] if key in env}
        info={'CFBundleExecutable':'fabric-mac','CFBundleIdentifier':'local.memoryfabric.mac','CFBundleName':'Fabric Memory','CFBundleDisplayName':'Fabric Memory','CFBundleIconFile':'FabricMemory.icns','CFBundlePackageType':'APPL','CFBundleShortVersionString':'0.2.0','CFBundleVersion':'2','NSHighResolutionCapable':True,'NSPrincipalClass':'NSApplication','NSMicrophoneUsageDescription':'Record microphone audio when you request a screen recording.','NSAudioCaptureUsageDescription':'Record system audio when you request a screen recording.','NSAppleEventsUsageDescription':'Use the Mac apps you choose for agent tasks.','LSMinimumSystemVersion':'14.0','LSEnvironment':runtime_env,'FabricSourceFingerprint':source}
        (contents/'Info.plist').write_bytes(plistlib.dumps(info))
        subprocess.run(['xattr','-cr',str(stage)],check=True)
        subprocess.run(['codesign','--force','--sign','-',str(contents/'MacOS/fabric-computer-use')],check=True)
        subprocess.run(['codesign','--force','--sign','-',str(stage)],check=True)
        subprocess.run(['codesign','--verify','--deep','--strict',str(stage)],check=True)
        stop_instances(app/'Contents/MacOS/fabric-mac')
        previous=Path(stage_dir)/'Previous.app'
        if app.exists():app.rename(previous)
        try:stage.rename(app)
        except Exception:
            if previous.exists():previous.rename(app)
            raise
    if destination is None:
        (app.parent/'mac-source.sha256').write_text(source)
    return app

def installed_development_fingerprint():
    try:
        info = plistlib.loads((APPLICATIONS_APP/'Contents/Info.plist').read_bytes())
        if info.get('CFBundleIdentifier') == 'local.memoryfabric.mac':
            return info.get('FabricSourceFingerprint')
    except (OSError, ValueError, plistlib.InvalidFileException):
        pass
    return None

def installed_mac_app(root):
    app = APPLICATIONS_APP
    source = fingerprint(root)
    if installed_development_fingerprint() == source:
        return app
    try:
        manifest=json.loads((app/'Contents/Resources/standalone.json').read_text())
        if manifest.get('source_fingerprint') == source:
            return app
    except (OSError, ValueError):
        pass
    return None

def mac_app(root,env,rebuild=False):
    if sys.platform!='darwin':raise ValueError('The Mac app requires macOS. The standalone Memory beacon also runs on Linux.')
    installed=installed_mac_app(root)
    if not rebuild and installed:return installed
    destination = APPLICATIONS_APP if installed_development_fingerprint() else None
    app=destination or support()/'App/fabric-memory.app';stamp=app.parent/'mac-source.sha256'
    if destination is None and not rebuild and app.exists() and stamp.exists() and stamp.read_text()==fingerprint(root):return app
    if not shutil.which('cargo'):raise ValueError('Install Rust (rustup.rs) to build the Mac app')
    # Compile in temporary storage and retain only the signed app bundle.
    with tempfile.TemporaryDirectory(prefix='fabric-native-build-') as build_dir:
        build_env=dict(env,CARGO_TARGET_DIR=build_dir,CARGO_BUILD_JOBS='2')
        subprocess.run(['cargo','build','--locked','-p','fabric-mac','-j','2'],cwd=root,env=build_env,check=True)
        return install_mac_app(root,Path(build_dir)/'debug/fabric-mac',env,destination)

def environment(root):
    env=dict(os.environ);env.setdefault('FABRIC_PYTHON',sys.executable);env.setdefault('FABRIC_ASSET_DIR',str(root/'assets'));env.setdefault('FABRIC_DATA_DIR',str(support()))
    if 'ORT_DYLIB_PATH' not in env:
        runtimes=list((root/'.venv/lib').glob('python*/site-packages/onnxruntime/capi/libonnxruntime.*.dylib'))+list((root/'.venv/lib').glob('python*/site-packages/onnxruntime/capi/libonnxruntime.so*'))
        if not runtimes:raise ValueError('Local model runtime is missing. Run fabric-memory run --prepare first.')
        env['ORT_DYLIB_PATH']=str(runtimes[0])
    dotenv=root/'apps/mac/.env'
    if dotenv.exists():env.setdefault('FABRIC_ENV_FILE',str(dotenv))
    return env

def open_mac_app(app,env):
    if app == APPLICATIONS_APP:
        subprocess.run(['open', '-a', str(app)], check=True)
        return
    binary=app/'Contents/MacOS/fabric-mac'
    processes=subprocess.run(['ps','-ax','-o','comm='],check=True,capture_output=True,text=True)
    if str(binary) in [line.strip() for line in processes.stdout.splitlines()]:
        subprocess.run(['open','-a',str(app)],check=True)
        return
    # Launch the signed bundle's executable from the CLI, preserving its environment
    # and terminal's file access context. Launch Services can stall while opening
    # a freshly replaced development bundle from Documents.
    logs=support()/'Logs';logs.mkdir(parents=True,exist_ok=True)
    log_path=logs/'mac.log'
    with log_path.open('w') as log:
        os.chmod(log_path,0o600)
        subprocess.Popen([str(binary)],env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=log,start_new_session=True)

def main(root):
    parser=argparse.ArgumentParser(prog='fabric-memory',description='Your local-first memory workspace.')
    commands=parser.add_subparsers(dest='command',required=True)
    run=commands.add_parser('run',help='Open the native Mac app')
    run.add_argument('--prepare',action='store_true',help='Prepare the Mac encoder and Python runtime');run.add_argument('--no-open',action='store_true',help='Build the Mac app without opening it');run.add_argument('--rebuild',action='store_true')
    git=commands.add_parser('launch-git',help='Run only the Memory beacon')
    git.add_argument('--url',default='https://akatary.com/fabric-git',help='Public HTTPS beacon URL');git.add_argument('--bind',default='127.0.0.1');git.add_argument('--port',type=int,default=8766);git.add_argument('--name',default='akatary Memory beacon');git.add_argument('--data-dir',default=str(support()/'Beacon'));git.add_argument('--tls-cert');git.add_argument('--tls-key')
    git.add_argument('--invite',action='store_true',help='Refresh the pairing QR without restarting an existing beacon')
    package=commands.add_parser('package',help='Build a standalone universal Mac disk image')
    package.add_argument('--output',type=Path,default=root/'dist')
    package.add_argument('--identity',help='Developer ID Application signing identity')
    package.add_argument('--notary-profile',help='Existing notarytool Keychain profile')
    commands.add_parser('install',help='Install the fabric-memory command into ~/.local/bin')
    cleanup=commands.add_parser('clean',help='Move unused build/test caches to Trash; preserve models and memory')
    cleanup.add_argument('--dry-run',action='store_true',help='Preview without moving files')
    args=parser.parse_args()
    try:
        if args.command=='package':
            from package_mac import package
            package(root,args.output,args.identity,args.notary_profile);return
        if args.command=='clean':
            from clean import clean
            clean(root,support(),args.dry_run);return
        if args.command=='install':
            destination=Path.home()/'.local/bin/fabric-memory';destination.parent.mkdir(parents=True,exist_ok=True)
            if destination.exists() or destination.is_symlink():
                if destination.resolve()!=root/'bin/fabric-memory':raise ValueError('An unrelated fabric-memory command already exists')
            else:destination.symlink_to(root/'bin/fabric-memory')
            print('Installed fabric-memory in ~/.local/bin.');return
        if args.command=='launch-git':
            from beacon_launch import launch
            launch(args,root);return
        if args.prepare:
            uv = shutil.which('uv')
            if not uv:
                raise ValueError('Install uv to prepare the Mac encoder.')
            python = root/'.venv/bin/python'
            if not python.is_file():
                subprocess.run([uv, 'venv', '.venv', '--python', '3.12'], cwd=root, check=True)
            subprocess.run([uv, 'pip', 'install', '--python', str(python), '-r',
                            str(root/'tools/requirements.lock')], cwd=root, check=True)
            subprocess.run([str(python), str(root/'tools/prepare.py')], cwd=root, check=True)
        installed=installed_mac_app(root)
        candidate=installed or support()/'App/fabric-memory.app'
        stamp=candidate.parent/'mac-source.sha256'
        bundled=(candidate/'Contents/Resources/standalone.json').is_file() and (bool(installed) or (stamp.is_file() and stamp.read_text()==fingerprint(root))) and not args.rebuild
        if sys.platform=='darwin' and not bundled:
            subprocess.run([sys.executable, str(root/'tools/local_llm.py')], cwd=root, check=True)
            if not os.getenv('FABRIC_SEARCH_URL'):
                result = subprocess.run([sys.executable, str(root/'tools/web_search.py')], cwd=root)
                if result.returncode:
                    print('Web search setup failed. Public links can still be read.', file=sys.stderr)
        env=environment(root);app=mac_app(root,env,args.rebuild)
        if not args.no_open:
            open_mac_app(app,env)
            print('Opened fabric-memory for Mac.')
        else:print('Mac app ready: '+str(app))
    except KeyboardInterrupt:print('\nfabric-memory stopped.')
    except (ValueError,OSError,subprocess.CalledProcessError) as error:parser.exit(1,str(error)+'\n')
