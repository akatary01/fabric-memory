#!/usr/bin/env python3
"""Verify code and sealed resource bytes, then ZIP without Finder/xattr metadata."""
import argparse
import hashlib
import plistlib
import subprocess
import zipfile
from pathlib import Path

def main():
    parser=argparse.ArgumentParser();parser.add_argument("archive",type=Path);parser.add_argument("output",type=Path)
    args=parser.parse_args()
    app=args.archive/"Products/Applications/FabricRunner.app"
    subprocess.run(["codesign","--verify","--deep","--ignore-resources",str(app)],check=True)
    checked=0
    for manifest in app.rglob("_CodeSignature/CodeResources"):
        bundle=manifest.parent.parent
        resources=plistlib.loads(manifest.read_bytes())
        for name,entry in resources.get("files2",{}).items():
            if not isinstance(entry,dict) or "hash2" not in entry: continue
            path=bundle/name
            if not path.exists() and entry.get("optional"): continue
            if hashlib.sha256(path.read_bytes()).digest()!=entry["hash2"]:
                raise ValueError(f"Signed resource changed: {name}")
            checked+=1
    # zipfile writes contents, names and POSIX mode. It does not preserve the
    # File Provider/Finder metadata that can invalidate bundles under Documents.
    with zipfile.ZipFile(args.output,"w",compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
        for path in sorted(args.archive.rglob("*")):
            if path.is_file(): archive.write(path,path.relative_to(args.archive.parent))
        for name in ("TESTFLIGHT.md","SYNC.md","PHONE-LOCAL-LLM.md"):
            archive.write(Path(__file__).resolve().parents[1]/"docs"/name,name)
    print(f"Verified {checked} sealed resources; packaged {args.output}")

if __name__=="__main__":main()
