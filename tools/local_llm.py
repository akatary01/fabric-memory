#!/usr/bin/env python3
"""Prepare a loopback-only local generator; reuse it on subsequent Mac launches."""
import fcntl
import hashlib
import json
import os
import plistlib
import subprocess
import tarfile
import time
from pathlib import Path
from urllib.request import build_opener, ProxyHandler, Request

PROJECT=Path(__file__).resolve().parents[1]
ROOT=Path.home()/"Library/Application Support/local.memoryfabric.mac/LocalLLM"
VERSION="0.40.15"
SHA256="607c79d5d6315ef9d896db83adde736a4c0057196329843544e79b95afb260a8"
LABEL="local.memoryfabric.llm"
HTTP=build_opener(ProxyHandler({}))

def api(path,body=None,timeout=3):
    request=Request("http://127.0.0.1:11434"+path,data=None if body is None else json.dumps(body).encode(),headers={"Content-Type":"application/json"})
    with HTTP.open(request,timeout=timeout) as response:return json.load(response)

def ready():
    try:return bool(api("/api/version").get("version"))
    except Exception:return False

def archive_digest(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download_archive(url, archive):
    # The system curl uses macOS trust independently of Python's CA bundle.
    partial = archive.with_suffix(".partial")
    try:
        subprocess.run([
            "/usr/bin/curl", "--fail", "--silent", "--show-error", "--location",
            "--proto", "=https", "--proto-redir", "=https", "--connect-timeout", "30",
            "--speed-limit", "1", "--speed-time", "60", "--output", str(partial), url,
        ], check=True)
        if archive_digest(partial) != SHA256:
            raise SystemExit("Ollama archive checksum mismatch.")
        partial.replace(archive)
    except subprocess.CalledProcessError:
        raise SystemExit("Could not download Ollama. Check the network and macOS certificate trust.") from None
    finally:
        partial.unlink(missing_ok=True)


def main():
    os.umask(0o077);ROOT.mkdir(parents=True,exist_ok=True)
    lock=(ROOT/"setup.lock").open("a");fcntl.flock(lock,fcntl.LOCK_EX)
    model=os.environ.get("FABRIC_LOCAL_MODEL","lfm3-vl:latest")
    settings=PROJECT/"apps/mac/.env"
    if settings.exists() and "FABRIC_LOCAL_MODEL" not in os.environ:
        for line in settings.read_text().splitlines():
            key,sep,value=line.partition("=")
            if sep and key.strip()=="FABRIC_LOCAL_MODEL":model=value.strip().strip("\"'") or model
    if not ready():
        runtime=ROOT/("ollama-"+VERSION)
        if not (runtime/"installed.json").exists():
            archive=ROOT/("ollama-v"+VERSION+".tgz")
            if not archive.exists() or archive_digest(archive)!=SHA256:
                print("Downloading Ollama "+VERSION+"…",flush=True)
                url="https://github.com/ollama/ollama/releases/download/v"+VERSION+"/ollama-darwin.tgz"
                download_archive(url, archive)
            runtime.mkdir(exist_ok=True)
            with tarfile.open(archive) as bundle:
                for member in bundle.getmembers():
                    path=Path(member.name)
                    if path.is_absolute() or ".." in path.parts or member.isdev() or member.isfifo():raise RuntimeError("Unsafe archive path")
                    if member.issym() or member.islnk():
                        link=Path(member.linkname)
                        if link.is_absolute() or ".." in link.parts:raise RuntimeError("Unsafe archive link")
                bundle.extractall(runtime)
            (runtime/"installed.json").write_text(json.dumps({"version":VERSION,"sha256":SHA256}))
        agents=Path.home()/"Library/LaunchAgents";agents.mkdir(exist_ok=True)
        plist=agents/(LABEL+".plist")
        record={"Label":LABEL,"ProgramArguments":[str(runtime/"ollama"),"serve"],"RunAtLoad":True,"KeepAlive":True,
            "ThrottleInterval":10,"WorkingDirectory":str(runtime),"StandardOutPath":str(ROOT/"service.log"),
            "StandardErrorPath":str(ROOT/"service-error.log"),"EnvironmentVariables":{
                "PATH":"/usr/bin:/bin:/usr/sbin:/sbin","OLLAMA_HOST":"127.0.0.1:11434","OLLAMA_MODELS":str(ROOT/"models"),
                "OLLAMA_NO_CLOUD":"1","OLLAMA_MAX_LOADED_MODELS":"2","OLLAMA_NUM_PARALLEL":"1","OLLAMA_CONTEXT_LENGTH":"4096"}}
        plist.write_bytes(plistlib.dumps(record));plist.chmod(0o600)
        domain="gui/"+str(os.getuid())
        if subprocess.run(["launchctl","print",domain+"/"+LABEL],capture_output=True).returncode!=0:
            subprocess.run(["launchctl","bootstrap",domain,str(plist)],check=True)
        subprocess.run(["launchctl","kickstart","-k",domain+"/"+LABEL],check=True)
        for _ in range(60):
            if ready():break
            time.sleep(0.25)
        else:raise RuntimeError("Local generator did not start. Check LocalLLM/service-error.log.")
    selected=ROOT.parent/"model-config.json"
    if selected.exists():
        try:model=json.loads(selected.read_text()).get("model") or model
        except (OSError,ValueError):pass
    installed=api("/api/tags").get("models",[])
    if not any(item.get("name")==model for item in installed):
        print("Downloading local model "+model+" (first run only)…",flush=True)
        request=Request("http://127.0.0.1:11434/api/pull",data=json.dumps({"model":model,"stream":True}).encode(),headers={"Content-Type":"application/json"})
        last=None;last_bucket=-1
        with HTTP.open(request,timeout=180) as response:
            for line in response:
                value=json.loads(line)
                if "error" in value:raise RuntimeError(value["error"])
                status=value.get("status","");total=value.get("total",0);done=value.get("completed",0)
                bucket=int(done*10/total) if total else -1
                if status!=last or bucket>last_bucket:
                    print(status+(" "+str(bucket*10)+"%" if bucket>=0 else ""),flush=True);last=status;last_bucket=bucket
        installed=api("/api/tags").get("models",[])
    entry=next((item for item in installed if item.get("name")==model),None)
    if not entry:raise RuntimeError("Requested local model is not installed")
    (ROOT/"model.json").write_text(json.dumps({"model":model,"digest":entry.get("digest"),"size":entry.get("size"),"runtime":api("/api/version")},indent=2))
    print("Local generator ready: "+model+" on this Mac",flush=True)

if __name__=="__main__":main()
