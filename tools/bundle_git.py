"""Build the small, relocatable Git server runtime used by local beacons."""
import os,shutil,subprocess,tarfile
from pathlib import Path
VERSION='2.56.0'
SHA256='26c56c296b38c0695b26fa95f475f1d01704d2d38e73465ca30b0b2f5dc789d3'
def bundle(destination,work):
    from package_mac import download
    cache=os.environ.get('FABRIC_GIT_BUNDLE')
    if cache:
        from package_mac import digest
        cache=Path(cache)
        if (cache/'VERSION').read_text().strip()!=VERSION or digest(cache/('git-'+VERSION+'.tar.xz'))!=SHA256:raise ValueError('Invalid Git runtime cache')
        for name in ('git','git-http-backend'):
            if not {'arm64','x86_64'}<=set(subprocess.check_output(['lipo','-archs',str(cache/name)],text=True).split()):raise ValueError('Git cache must be universal')
        shutil.copytree(cache,destination,symlinks=True);return
    archive=work/('git-'+VERSION+'.tar.xz')
    download('https://www.kernel.org/pub/software/scm/git/'+archive.name,archive,SHA256)
    builds=[]
    for arch in ('arm64','x86_64'):
        folder=work/('git-'+arch);folder.mkdir(exist_ok=True)
        with tarfile.open(archive) as tar:tar.extractall(folder,filter='data')
        source=folder/('git-'+VERSION)
        options=['NO_RUST=YesPlease','NO_CURL=YesPlease','NO_EXPAT=YesPlease','NO_GETTEXT=YesPlease','NO_TCLTK=YesPlease','NO_PERL=YesPlease','NO_PYTHON=YesPlease','NO_INSTALL_HARDLINKS=YesPlease','NO_OPENSSL=YesPlease','NO_REGEX=YesPlease', 'USE_LIBPCRE=','CC=clang','CFLAGS=-O2 -arch '+arch+' -mmacosx-version-min=14.0','LDFLAGS=-arch '+arch+' -mmacosx-version-min=14.0','prefix=/fabric-bundled-git']
        subprocess.run(['make','-j2',*options,'git','git-http-backend'],cwd=source,check=True,stdout=subprocess.DEVNULL)
        builds.append(source)
    destination.mkdir(parents=True,exist_ok=True)
    for name in ('git','git-http-backend'):
        subprocess.run(['lipo','-create',*[str(s/name) for s in builds],'-output',str(destination/name)],check=True)
    for name in ('git-upload-pack','git-receive-pack','git-upload-archive'):(destination/name).symlink_to('git')
    shutil.copy2(archive,destination/archive.name) # Corresponding GPL source, including build recipe.
    shutil.copy2(builds[0]/'COPYING',destination/'COPYING')
    shutil.copy2(__file__,destination/'bundle_git.py')
    (destination/'VERSION').write_text(VERSION+'\n')
