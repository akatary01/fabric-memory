"""Build a relocatable universal Mac app and a drag-to-Applications disk image."""
import hashlib, json, os, pathlib, plistlib, shutil, subprocess, tarfile, tempfile, zipfile
from pathlib import Path
from cli import support, fingerprint

PYTHON_RELEASE='20261026'
SEARCH_REVISION='3a3dd15bfafcb645a1b45c2c6c0798a8f786aaaf'
SEARCH_HASH='1a10d9278f9b75aafab368f9f03ebc9bec653390b28091d60f961f325acb52ed'
PYTHON_VERSION='3.12.16'
PYTHON_HASHES={'arm64':'353d5f1b4fd2715e7053f12f068435d5bbad16b91e057006b5d01e0925709a7e',
               'x86_64':'37c38fda76fec1fbf310adcbafb7f0d96af13b5511bf8ea954c3aa8eec185734'}
TARGETS={'arm64':'aarch64-apple-darwin','x86_64':'x86_64-apple-darwin'}

def run(*args,**kw):
    return subprocess.run([str(a) for a in args],check=True,**kw)

def digest(path):
    with path.open('rb') as source:return hashlib.file_digest(source,'sha256').hexdigest()

def download(url,path,sha):
    if not path.exists() or digest(path)!=sha:
        run('curl','--fail','--location','--retry','3','--output',path,url)
    if digest(path)!=sha:raise ValueError('Download checksum mismatch: '+path.name)

def extract(archive,folder):
    folder.mkdir(parents=True,exist_ok=True)
    with tarfile.open(archive) as bundle:bundle.extractall(folder,filter='data')

def seed_models(source,destination,notices):
    # Allowlist exactly the default model and its content-addressed dependencies.
    relative=Path('manifests/registry.ollama.ai/library/lfm3-vl/latest')
    # The latest tag may resolve to an Apple Silicon-only MLX variant.
    # Use the official GGUF variant for the universal installer.
    manifest=source/relative.with_name('2b-q4_K_M')
    if not manifest.exists():
        manifest=source/relative
        if not manifest.exists() or not any(layer.get('mediaType')=='application/vnd.ollama.image.model' for layer in json.loads(manifest.read_text()).get('layers',[])):
            raise ValueError('Prepare the portable model with FABRIC_LOCAL_MODEL=lfm3-vl:2b-q4_K_M fabric-memory run --rebuild.')
    (destination/relative).parent.mkdir(parents=True,exist_ok=True)
    shutil.copy2(manifest,destination/relative)
    (destination/'blobs').mkdir(parents=True)
    copied=set()
    def visit(record):
        for layer in [record.get('config',{}),*record.get('layers',[])]:
            name=layer.get('digest','')
            if not name.startswith('sha256:') or name in copied:continue
            copied.add(name);src=source/'blobs'/name.replace(':','-')
            if digest(src)!=name.split(':')[1]:raise ValueError('Model blob digest mismatch')
            shutil.copy2(src,destination/'blobs'/src.name)
            if 'license' in layer.get('mediaType',''):
                shutil.copy2(src,notices/'LFM3-LICENSE.txt')
            if src.stat().st_size<100000:
                try: child=json.loads(src.read_text())
                except (ValueError,UnicodeError):continue
                if isinstance(child,dict) and 'layers' in child:visit(child)
    visit(json.loads(manifest.read_text()))

def sign(app,identity):
    # Sign nested native code before the outer bundle; never use --deep to sign.
    identity=identity or '-'
    flags=['--force','--sign',identity]
    if identity!='-':flags+=['--options','runtime','--timestamp']
    entitlements=app.parent/'runtime-entitlements.plist'
    entitlements.write_bytes(plistlib.dumps({'com.apple.security.cs.disable-library-validation':True,
                                            'com.apple.security.cs.allow-jit':True}))
    native=[]
    for p in app.rglob('*'):
        if not p.is_file() or p.is_symlink():continue
        with p.open('rb') as f:magic=f.read(4)
        if magic in [b'\xcf\xfa\xed\xfe',b'\xce\xfa\xed\xfe',b'\xca\xfe\xba\xbe',b'\xca\xfe\xba\xbf']:
            native.append(p)
    for p in sorted(native,key=lambda p:len(p.parts),reverse=True):
        run('codesign',*flags,'--entitlements',entitlements,p,stdout=subprocess.DEVNULL)
    run('codesign',*flags,'--entitlements',entitlements,app)
    run('codesign','--verify','--deep','--strict',app)
    entitlements.unlink()

def package(root,output,identity=None,notary_profile=None):
    if os.uname().sysname!='Darwin':raise ValueError('Build Mac packages on macOS.')
    if notary_profile and not identity:raise ValueError('Notarization requires a Developer ID identity.')
    source_fingerprint=fingerprint(root)
    output=output.expanduser().resolve();output.mkdir(parents=True,exist_ok=True)
    dmg=output/'fabric-memory-0.2.0-universal.dmg'
    if dmg.exists():raise ValueError('Output already exists; choose a new output directory.')
    uv=shutil.which('uv') or str(Path.home()/'.local/bin/uv')
    # Temporary source/runtime/build artifacts are deleted on success or failure.
    with tempfile.TemporaryDirectory(prefix='fabric-package-') as temporary:
        work=Path(temporary);app=work/'image/fabric-memory.app';contents=app/'Contents'
        macos=contents/'MacOS';resources=contents/'Resources';macos.mkdir(parents=True);resources.mkdir()
        guides=resources/'AgentDocs';guides.mkdir()
        for source in [root/'AGENTS.md',root/'README.md',*sorted((root/'docs').glob('*.md'))]:
            target=guides/source.relative_to(root);target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)
        from bundle_git import bundle as bundle_git, VERSION as git_version
        bundle_git(resources/'git',work)
        notices=resources/'Licenses';notices.mkdir()
        shutil.copy2(root/'LICENSE',notices/'MemoryFabric-LICENSE')
        assets=resources/'assets/mac';assets.mkdir(parents=True)
        for name in ['model.onnx','encoder.json','tokenizer.json','LICENSE','README.md']:
            shutil.copy2(root/'assets/mac'/name,assets/name)
        for name in ('foreign-to-mac', 'phone-to-mac'):
            adapter = root/'assets/adapters'/name
            if adapter.is_dir():
                shutil.copytree(adapter, resources/'assets/adapters/foreign-to-mac')
                break
        seed_models(support()/'LocalLLM/models',resources/'models',notices)
        from local_llm import VERSION as ollama_version, SHA256 as ollama_sha
        ollama_archive=support()/('LocalLLM/ollama-v'+ollama_version+'.tgz')
        download('https://github.com/ollama/ollama/releases/download/v'+ollama_version+'/ollama-darwin.tgz',ollama_archive,ollama_sha)
        extract(ollama_archive,resources/'ollama')
        download('https://raw.githubusercontent.com/ollama/ollama/v'+ollama_version+'/LICENSE',notices/'Ollama-LICENSE',
                 '5934ed2ce0d15154bcdb9c85203210abac0da4314af34081e36df4599f90b226')
        search_archive=work/'searxng.tar.gz'
        download('https://codeload.github.com/searxng/searxng/tar.gz/'+SEARCH_REVISION,search_archive,SEARCH_HASH)
        extract(search_archive,work/'search-source')
        search=work/'search-source'/('searxng-'+SEARCH_REVISION)
        shutil.copytree(search/'searx',resources/'search/searx',ignore=shutil.ignore_patterns('__pycache__','*.pyc'))
        shutil.copy2(search/'LICENSE',notices/'SearXNG-LICENSE')
        shutil.copy2(search_archive,notices/'SearXNG-corresponding-source.tar.gz')
        (resources/'search/searx/version_frozen.py').write_text('VERSION_STRING = VERSION_TAG = DOCKER_TAG = '+repr(SEARCH_REVISION)+'\nGIT_URL = \"https://github.com/searxng/searxng\"\nGIT_BRANCH = \"master\"\n')
        binaries=[];helpers=[]
        for arch,target in TARGETS.items():
            print('Preparing '+arch+' runtime…',flush=True)
            runtime=resources/'runtimes'/arch;runtime.mkdir(parents=True)
            archive=work/(arch+'-python.tar.gz')
            name=f'cpython-{PYTHON_VERSION}+{PYTHON_RELEASE}-{target}-install_only_stripped.tar.gz'
            download('https://github.com/astral-sh/python-build-standalone/releases/download/'+PYTHON_RELEASE+'/'+name.replace('+','%2B'),archive,PYTHON_HASHES[arch])
            extract(archive,runtime)
            run(uv,'pip','install','--python-version','3.12','--python-platform',target,'--no-build',
                '--target',runtime/'site-packages','-r',search/'requirements.txt','certifi')
            # Download ONNX from its official PyPI distribution; uv verifies index hashes.
            ort_target=work/('ort-'+arch)
            run(uv,'pip','install','--python-version','3.12','--python-platform',target,'--no-build','--no-deps',
                '--target',ort_target,'onnxruntime==1.23.2')
            library=next(ort_target.glob('onnxruntime/capi/libonnxruntime*.dylib'))
            shutil.copy2(library,runtime/'libonnxruntime.dylib')
            for file in ort_target.glob('onnxruntime*.dist-info/licenses/*'):
                if file.is_file():shutil.copy2(file,notices/('ONNX-'+arch+'-'+file.name))
            run('rustup','target','add',target)
            build=Path(os.environ.get('CARGO_TARGET_DIR',str(work/'cargo')))
            run('cargo','build','--release','--locked','-p','fabric-mac','--target',target,'-j','2',cwd=root,
                env=dict(os.environ,CARGO_TARGET_DIR=str(build),MACOSX_DEPLOYMENT_TARGET='14.0'))
            binaries.append(build/target/'release/fabric-mac')
            helper=work/('computer-'+arch)
            run('xcrun','swiftc','-O','-target',arch+'-apple-macos14.0',root/'apps/mac/automation/ComputerUse.swift','-o',helper)
            helpers.append(helper)
        # UI/source can change while a long universal build runs. Never ship a mixed revision.
        for attempt in range(3):
            if fingerprint(root)==source_fingerprint:break
            print('Source changed during packaging; rebuilding the current revision…',flush=True)
            source_fingerprint=fingerprint(root)
            for arch,target in TARGETS.items():
                run('cargo','build','--release','--locked','-p','fabric-mac','--target',target,'-j','2',cwd=root,
                    env=dict(os.environ,CARGO_TARGET_DIR=str(build),MACOSX_DEPLOYMENT_TARGET='14.0'))
                run('xcrun','swiftc','-O','-target',arch+'-apple-macos14.0',root/'apps/mac/automation/ComputerUse.swift','-o',work/('computer-'+arch))
        else:
            if fingerprint(root)!=source_fingerprint:raise ValueError('Source kept changing during packaging. Run package again when edits are complete.')
        run('lipo','-create',*binaries,'-output',macos/'fabric-mac')
        run('lipo','-create',*helpers,'-output',macos/'fabric-computer-use')
        shutil.copy2(root/'apps/mac/icons/FabricMemory.icns',resources/'FabricMemory.icns')
        info={'CFBundleExecutable':'fabric-mac','CFBundleIdentifier':'local.memoryfabric.mac',
              'CFBundleName':'Fabric Memory','CFBundleDisplayName':'Fabric Memory','CFBundleIconFile':'FabricMemory.icns',
              'CFBundlePackageType':'APPL','CFBundleShortVersionString':'0.2.0','CFBundleVersion':'2',
              'NSHighResolutionCapable':True,'NSPrincipalClass':'NSApplication','LSMinimumSystemVersion':'14.0',
              'NSMicrophoneUsageDescription':'Record microphone audio when you request a screen recording.','NSAudioCaptureUsageDescription':'Record system audio when you request a screen recording.','NSAppleEventsUsageDescription':'Control the Mac apps you choose for agent tasks.'}
        (contents/'Info.plist').write_bytes(plistlib.dumps(info))
        (resources/'standalone.json').write_text(json.dumps({'version':'0.2.0','architectures':list(TARGETS),
            'source_fingerprint':source_fingerprint,'git':git_version,'python':PYTHON_VERSION,'ollama':'0.40.15','searxng':SEARCH_REVISION,'default_model':'lfm3-vl:latest'}))
        metadata=json.loads(subprocess.check_output(['cargo','metadata','--format-version','1','--locked'],cwd=root))
        for dependency in metadata['packages']:
            folder=Path(dependency['manifest_path']).parent
            for pattern in ['LICENSE*','LICENCE*','NOTICE*','COPYING*']:
                for file in folder.glob(pattern):
                    if file.is_file() and file.stat().st_size<500000:
                        target=notices/'Rust'/(dependency['name']+'-'+dependency['version'])/file.name
                        target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(file,target)
        (notices/'Rust-dependencies.json').write_text(json.dumps([{k:p.get(k) for k in ['name','version','license','repository']} for p in metadata['packages']],indent=2))
        # AppleDouble sidecars in vendor archives are converted/dropped by disk-image
        # copying. Strip this metadata before sealing resources, not after signing.
        for sidecar in app.rglob('._*'):
            if sidecar.is_file():
                with sidecar.open('rb') as source:magic=source.read(4)
                if magic==b'\x00\x05\x16\x07':sidecar.unlink()
        run('xattr','-cr',app);sign(app,identity)
        (work/'image/Applications').symlink_to('/Applications')
        (work/'image/Install.txt').write_text('fabric-memory\n\nDrag fabric-memory to Applications, then open it.\nRequires macOS 14 or newer. Apple Silicon and Intel supported.\nA local model is included. Cloud tokens and Memory beacon are optional; configure them in the app.\n'+('' if identity and notary_profile else '\nTEST BUILD: this copy is not notarized for public distribution.\n'))
        if fingerprint(root)!=source_fingerprint:raise ValueError('Source changed after the native build. Re-run packaging to include the latest revision.')
        run('hdiutil','create','-volname','fabric-memory','-srcfolder',work/'image','-format','UDZO','-ov',dmg)
        mount=work/'verified-image';mount.mkdir()
        run('hdiutil','attach','-readonly','-nobrowse','-mountpoint',mount,dmg,stdout=subprocess.DEVNULL)
        try:run('codesign','--verify','--deep','--strict',mount/'fabric-memory.app')
        finally:run('hdiutil','detach',mount,stdout=subprocess.DEVNULL)
        if identity:run('codesign','--sign',identity,'--timestamp',dmg)
        if notary_profile:
            run('xcrun','notarytool','submit',dmg,'--keychain-profile',notary_profile,'--wait')
            run('xcrun','stapler','staple',dmg);run('xcrun','stapler','validate',dmg)
        checksum=digest(dmg)
        dmg.with_suffix('.dmg.sha256').write_text(checksum+'  '+dmg.name+'\n')
        (output/'package-manifest.json').write_text(json.dumps({'artifact':dmg.name,'sha256':checksum,
            'architectures':list(TARGETS),'minimum_macos':'14.0','signed':bool(identity),'notarized':bool(notary_profile),
            'personal_data_included':False,'bundled':['native app','computer-use helper','default local model','SearXNG web search','memory encoder','ONNX Runtime','Python MCP host','Git local memory beacon','sandboxed workspace executor']},indent=2)+'\n')
        print('Standalone installer: '+str(dmg),flush=True)
        if not identity or not notary_profile:print('Test installer only: Developer ID signing and Apple notarization still required for normal public downloads.')
