#!/usr/bin/env python3
"""Prepare a loopback-only local generator; reuse it on subsequent Mac launches."""
import fcntl
import json
import os
import plistlib
import subprocess
import tarfile
import time
from pathlib import Path
from urllib.request import build_opener, ProxyHandler, Request
from verified_download import archive_digest, download_archive

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

KV_CACHE_ENV = {"OLLAMA_FLASH_ATTENTION": "1", "OLLAMA_KV_CACHE_TYPE": "q8_0"}

def load_launch_agent(plist, domain):
    result = subprocess.run(["launchctl", "bootstrap", domain, str(plist)], capture_output=True, text=True)
    if result.returncode == 0:
        return
    service = domain + "/" + LABEL
    if subprocess.run(["launchctl", "print", service], capture_output=True).returncode != 0:
        # Some GUI sessions accept the legacy per-user load but reject bootstrap.
        subprocess.run(["launchctl", "load", str(plist)], check=True)
    if subprocess.run(["launchctl", "print", service], capture_output=True).returncode != 0:
        raise RuntimeError(result.stderr.strip() or "Could not register Fabric's Ollama LaunchAgent.")

def update_managed_cache(plist, domain):
    if not plist.is_file():
        return
    original = plist.read_bytes()
    record = plistlib.loads(original)
    executable = Path(record.get("ProgramArguments", [""])[0])
    if record.get("Label") != LABEL or not executable.is_relative_to(ROOT):
        return
    environment = record.setdefault("EnvironmentVariables", {})
    if all(environment.get(key) == value for key, value in KV_CACHE_ENV.items()):
        return
    service = domain + "/" + LABEL
    if subprocess.run(["launchctl", "print", service], capture_output=True).returncode != 0:
        return
    environment.update(KV_CACHE_ENV)
    subprocess.run(["launchctl", "bootout", service], check=True)
    try:
        plist.write_bytes(plistlib.dumps(record))
        load_launch_agent(plist, domain)
        for _ in range(60):
            if ready():
                return
            time.sleep(0.25)
        raise RuntimeError("Fabric's Ollama service did not restart with the new KV cache settings.")
    except (OSError, subprocess.CalledProcessError, RuntimeError):
        subprocess.run(["launchctl", "bootout", service], capture_output=True)
        plist.write_bytes(original)
        load_launch_agent(plist, domain)
        raise

def main():
    os.umask(0o077);ROOT.mkdir(parents=True,exist_ok=True)
    lock=(ROOT/"setup.lock").open("a");fcntl.flock(lock,fcntl.LOCK_EX)
    model=os.environ.get("FABRIC_LOCAL_MODEL","lfm3-vl:latest")
    settings=PROJECT/"apps/mac/.env"
    if settings.exists() and "FABRIC_LOCAL_MODEL" not in os.environ:
        for line in settings.read_text().splitlines():
            key,sep,value=line.partition("=")
            if sep and key.strip()=="FABRIC_LOCAL_MODEL":model=value.strip().strip("\"'") or model
    agents=Path.home()/"Library/LaunchAgents"
    plist=agents/(LABEL+".plist")
    domain="gui/"+str(os.getuid())
    if ready():
        update_managed_cache(plist, domain)
    else:
        runtime=ROOT/("ollama-"+VERSION)
        if not (runtime/"installed.json").exists():
            archive=ROOT/("ollama-v"+VERSION+".tgz")
            if not archive.exists() or archive_digest(archive)!=SHA256:
                print("Downloading Ollama "+VERSION+"…",flush=True)
                url="https://github.com/ollama/ollama/releases/download/v"+VERSION+"/ollama-darwin.tgz"
                download_archive(url, archive, SHA256, "Ollama")
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
        agents.mkdir(exist_ok=True)
        record={"Label":LABEL,"ProgramArguments":[str(runtime/"ollama"),"serve"],"RunAtLoad":True,"KeepAlive":True,
            "ThrottleInterval":10,"WorkingDirectory":str(runtime),"StandardOutPath":str(ROOT/"service.log"),
            "StandardErrorPath":str(ROOT/"service-error.log"),"EnvironmentVariables":{
                "PATH":"/usr/bin:/bin:/usr/sbin:/sbin","OLLAMA_HOST":"127.0.0.1:11434","OLLAMA_MODELS":str(ROOT/"models"),
                "OLLAMA_NO_CLOUD":"1","OLLAMA_MAX_LOADED_MODELS":"2","OLLAMA_NUM_PARALLEL":"1","OLLAMA_CONTEXT_LENGTH":"4096",
                **KV_CACHE_ENV}}
        plist.write_bytes(plistlib.dumps(record));plist.chmod(0o600)
        if subprocess.run(["launchctl","print",domain+"/"+LABEL],capture_output=True).returncode!=0:
            load_launch_agent(plist, domain)
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
