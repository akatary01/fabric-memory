#!/usr/bin/env python3
"""Install a pinned, loopback-only SearXNG search service for the desktop app."""
import fcntl
import json
import os
import plistlib
import secrets
import shutil
import subprocess
import tarfile
import time
from pathlib import Path
from urllib.request import build_opener,ProxyHandler
from verified_download import archive_digest, download_archive

REVISION="3a3dd15bfafcb645a1b45c2c6c0798a8f786aaaf"
SHA256="1a10d9278f9b75aafab368f9f03ebc9bec653390b28091d60f961f325acb52ed"
ROOT=Path.home()/"Library/Application Support/local.memoryfabric.mac/WebSearch"
LABEL="local.memoryfabric.search"
PORT=8877

def ready():
    try:
        with build_opener(ProxyHandler({})).open("http://127.0.0.1:8877/config",timeout=2) as r:
            return r.status==200 and "engines" in json.load(r)
    except Exception:return False

def run(*args,**kwargs):return subprocess.run([str(x) for x in args],check=True,**kwargs)

def main():
    os.umask(0o077);ROOT.mkdir(parents=True,exist_ok=True);ROOT.chmod(0o700)
    lock=(ROOT/"setup.lock").open("a");fcntl.flock(lock,fcntl.LOCK_EX)
    if ready():print("Local web search ready: SearXNG on 127.0.0.1:8877");return
    source=ROOT/("searxng-"+REVISION)
    if not source.exists():
        archive=ROOT/"source.tar.gz"
        if not archive.is_file() or archive_digest(archive) != SHA256:
            print("Downloading the open-source web search service…", flush=True)
            url = "https://codeload.github.com/searxng/searxng/tar.gz/" + REVISION
            download_archive(url, archive, SHA256, "SearXNG")
        with tarfile.open(archive) as bundle:
            bundle.extractall(ROOT, filter="data")
    uv=shutil.which("uv") or str(Path.home()/".local/bin/uv")
    python=ROOT/"venv/bin/python"
    if not python.exists():run(uv,"venv",ROOT/"venv","--python","3.12","--allow-existing")
    stamp=ROOT/"installed-revision"
    if not stamp.exists() or stamp.read_text()!=REVISION:
        print("Installing search dependencies (first run only)…",flush=True)
        run(uv,"pip","install","--python",python,"-r",source/"requirements.txt")
        stamp.write_text(REVISION)
    settings=ROOT/"settings.yml"
    if not settings.exists():
        settings.write_text("""use_default_settings:
  engines:
    keep_only: [duckduckgo, brave, google, bing, wikipedia]
general:
  instance_name: Memory Fabric Search
  debug: false
  enable_metrics: false
search:
  formats: [html, json]
  safe_search: 1
server:
  bind_address: 127.0.0.1
  port: 8877
  secret_key: """+secrets.token_hex(32)+"""
  limiter: false
  public_instance: false
outgoing:
  request_timeout: 8.0
  max_request_timeout: 10.0
engines:
  - name: duckduckgo
    disabled: false
  - name: brave
    disabled: false
  - name: google
    disabled: false
  - name: bing
    disabled: false
  - name: wikipedia
    disabled: false
""");settings.chmod(0o600)
    agents=Path.home()/"Library/LaunchAgents";agents.mkdir(exist_ok=True)
    plist=agents/(LABEL+".plist")
    record={"Label":LABEL,"ProgramArguments":[str(python),"-m","searx.webapp"],
        "RunAtLoad":True,"KeepAlive":True,"ThrottleInterval":10,"WorkingDirectory":str(source),
        "StandardOutPath":str(ROOT/"service.log"),"StandardErrorPath":str(ROOT/"service-error.log"),
        "EnvironmentVariables":{"PATH":"/usr/bin:/bin:/usr/sbin:/sbin","PYTHONPATH":str(source),
            "SEARXNG_SETTINGS_PATH":str(settings),"PYTHONUNBUFFERED":"1"}}
    plist.write_bytes(plistlib.dumps(record));plist.chmod(0o600)
    domain="gui/"+str(os.getuid())
    if subprocess.run(["launchctl","print",domain+"/"+LABEL],capture_output=True).returncode!=0:run("launchctl","bootstrap",domain,plist)
    run("launchctl","kickstart","-k",domain+"/"+LABEL)
    for _ in range(100):
        if ready():print("Local web search ready: SearXNG on 127.0.0.1:8877");return
        time.sleep(0.25)
    raise RuntimeError("Search service did not start; check WebSearch/service-error.log in Application Support")

if __name__=="__main__":main()
