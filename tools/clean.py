"""Clean a fixed set of regenerable outputs, never user data or model weights."""
import datetime,json,os,shutil,subprocess,sys,uuid
from pathlib import Path
from urllib.parse import quote

def size(path):
    count=total=0
    for directory,dirs,files in os.walk(path,followlinks=False):
        dirs[:]=[d for d in dirs if not (Path(directory)/d).is_symlink()]
        for name in files:
            p=Path(directory)/name
            if not p.is_symlink():s=p.stat();count+=1;total+=s.st_size
    return count,total

def trash(path):
    if sys.platform=='darwin':
        original=path.stat()
        # Passing NSURL** out-parameters through JXA can crash the bridge.
        # Use native Trash with null outputs, then locate the moved inode.
        script="""ObjC.import('Foundation'); function run(argv) { var ok=$.NSFileManager.defaultManager.trashItemAtURLResultingItemURLError($.NSURL.fileURLWithPath(argv[0]),null,null); if (!ok) throw Error('macOS could not move this item to Trash'); return 'ok'; }"""
        result=subprocess.run(['/usr/bin/osascript','-l','JavaScript','-e',script,str(path)],check=True,capture_output=True,text=True)
        try:
            for item in (Path.home()/'.Trash').iterdir():
                current=item.lstat()
                if current.st_ino==original.st_ino and current.st_dev==original.st_dev:return str(item)
        except OSError:
            pass # macOS may allow native Trash moves but deny directory enumeration.
        return 'macOS Trash (restore using Finder)'
    # FreeDesktop Trash with restore metadata; no permanent delete fallback.
    root=Path(os.environ.get('XDG_DATA_HOME',str(Path.home()/'.local/share')))/'Trash'
    for name in ['files','info']:(root/name).mkdir(parents=True,exist_ok=True,mode=0o700)
    name=path.name+'-'+uuid.uuid4().hex[:8];destination=root/'files'/name
    info=root/'info'/(name+'.trashinfo')
    info.write_text('[Trash Info]\nPath='+quote(str(path),safe='/')+'\nDeletionDate='+datetime.datetime.now().strftime('%Y-%m-%dT%H:%M:%S')+'\n')
    try:shutil.move(str(path),destination)
    except Exception:info.unlink();raise
    return str(destination)

def clean(project,support,dry_run=False):
    candidates=[project/'target',project/'.build/mac',project/'node_modules',support/'Build/Swift']
    # If inspection is unavailable, leave everything untouched.
    if not shutil.which('lsof'):raise ValueError('Install lsof to check which build files are in use before cleaning')
    check=subprocess.run(['lsof','-nP','-F','n'],capture_output=True,text=True)
    if check.returncode not in (0,1):raise ValueError('Could not check open files; nothing cleaned')
    opened=[line[1:] for line in check.stdout.splitlines() if line.startswith('n/')]
    rows=[]
    for path in candidates:
        if not path.exists():continue
        if not path.is_dir():rows.append({'path':str(path),'status':'skipped','reason':'Not a build directory'});continue
        if path.is_symlink():rows.append({'path':str(path),'status':'skipped','reason':'Symlink'});continue
        if any(p==str(path) or p.startswith(str(path)+'/') for p in opened):rows.append({'path':str(path),'status':'skipped','reason':'In use'});continue
        count,total=size(path);row={'path':str(path),'files':count,'logical_bytes':total,'status':'candidate' if dry_run else 'trashed'}
        if not dry_run:
            try:row['trash_path']=trash(path)
            except (OSError,subprocess.CalledProcessError) as error:row['status']='failed';row['error']=str(error)
        rows.append(row)
        print(f'{row["status"]}: {path} ({total/1e6:.1f} MB)')
    if not rows:print('Nothing to clean.')
    total=sum(r.get('logical_bytes',0) for r in rows if r['status'] in ('candidate','trashed'))
    print(f'{"Preview" if dry_run else "Moved to Trash"}: {total/1e9:.2f} GB. Source, models, archives, settings, and memory data are preserved.')
    if not dry_run and rows:
        folder=support/'Cleanup';folder.mkdir(parents=True,exist_ok=True)
        receipt=folder/('cleanup-'+datetime.datetime.now().strftime('%Y%m%d-%H%M%S')+'-'+uuid.uuid4().hex[:6]+'.json')
        receipt.write_text(json.dumps({'action':'trash','results':rows},indent=2));print('Receipt: '+str(receipt))
    if any(r['status']=='failed' for r in rows):raise ValueError('Some folders could not be moved; see the cleanup receipt')
    return rows
