#!/usr/bin/env python3
"""Install the user's private Mac Git service. Does not change system sharing/firewall."""
import json
import ipaddress
import fcntl
import os
import plistlib
import re
import secrets
import shlex
import shutil
import ssl
import subprocess
import time
from pathlib import Path

PROJECT=Path(__file__).resolve().parents[2]
ROOT=Path.home()/"Library/Application Support/local.memoryfabric.mac/Sync"
LABEL="local.memoryfabric.sync"

def command(*args,**kwargs):
    return subprocess.run([str(a) for a in args],check=True,capture_output=True,**kwargs).stdout

def lan_address():
    # Prefer the active physical network, rather than a VPN or loopback address.
    interfaces=[]
    try:
        route=command("route","-n","get","default").decode()
        interfaces=[line.split(":",1)[1].strip() for line in route.splitlines() if line.strip().startswith("interface:")]
    except subprocess.CalledProcessError:pass
    interfaces=[name for name in interfaces if name.startswith("en")]+["en0","en1"]
    ipv6=[]
    for interface in dict.fromkeys(interfaces):
        try:
            value=command("ipconfig","getifaddr",interface).decode().strip()
            address=ipaddress.ip_address(value)
            if address.version==4 and address.is_private and not address.is_loopback and not address.is_link_local:return value
        except (subprocess.CalledProcessError,ValueError):pass
        try:
            for line in command("ifconfig",interface).decode().splitlines():
                match=re.search(r"\binet6\s+([^\s%]+)(?:%\S+)?\s+prefixlen\s+(\d+)",line)
                if not match or "temporary" in line or "clat46" in line:continue
                address=ipaddress.ip_address(match[1])
                if not address.is_loopback and not address.is_link_local and not address.is_multicast and not address.is_unspecified:
                    ipv6.append(str(address))
        except (subprocess.CalledProcessError,ValueError):pass
    return ipv6[0] if ipv6 else None

def healthy(config):
    try:
        from urllib.request import Request,build_opener,ProxyHandler,HTTPSHandler
        request=Request("https://127.0.0.1:"+str(config["port"])+"/v1/status",headers={"Authorization":"Bearer "+config["token"]})
        opener=build_opener(ProxyHandler({}),HTTPSHandler(context=ssl.create_default_context(cafile=config["ca_certificate"])))
        with opener.open(request,timeout=2) as response:return json.load(response).get("protocol")==1
    except Exception:return False

def main():
    os.umask(0o077);ROOT.mkdir(parents=True,exist_ok=True);ROOT.chmod(0o700)
    # npm launches and setup commands may overlap. Serialize credential/config updates.
    setup_lock=(ROOT/"setup.lock").open("a")
    fcntl.flock(setup_lock,fcntl.LOCK_EX)
    runtime=ROOT/"runtime";runtime.mkdir(exist_ok=True)
    source=Path(__file__).with_name("host.py").read_bytes()
    unchanged=(runtime/"host.py").exists() and (runtime/"host.py").read_bytes()==source
    (runtime/"host.py").write_bytes(source)
    config_path=ROOT/"config.json"
    old_config=None
    if config_path.exists():
        config=json.loads(config_path.read_text());old_config=json.dumps(config,sort_keys=True)
    else:
        hostname=command("scutil","--get","LocalHostName").decode().strip()+".local"
        try:ip=command("ipconfig","getifaddr","en0").decode().strip()
        except subprocess.CalledProcessError:ip="127.0.0.1"
        certificate=ROOT/"server.pem";key=ROOT/"server-key.pem"
        config={"version":1,"root":str(ROOT),"git":shutil.which("git"),"port":8765,"hostname":hostname,
            "token":secrets.token_urlsafe(32),"certificate":str(certificate),"private_key":str(key),"spaces":{}}
    ip=lan_address()
    new_ca=config.get("certificate_profile")!=3
    if new_ca:
        ca_config=ROOT/"ca.cnf"
        ca_config.write_text("[req]\ndistinguished_name=dn\nx509_extensions=ca\nprompt=no\n[dn]\nCN=Memory Fabric Local Root\n[ca]\nbasicConstraints=critical,CA:TRUE,pathlen:0\nkeyUsage=critical,keyCertSign,cRLSign\n")
        command("openssl","req","-x509","-newkey","rsa:2048","-sha256","-days","3650","-nodes","-config",ca_config,"-keyout",ROOT/"ca-key.pem","-out",ROOT/"ca.pem")
        config["certificate_profile"]=3;config["ca_certificate"]=str(ROOT/"ca.pem")
    try:
        certificate_info=ssl._ssl._test_decode_cert(config["certificate"])
        sans={(kind,str(ipaddress.ip_address(value)) if kind=="IP Address" else value)
            for kind,value in certificate_info.get("subjectAltName",[])}
        renew=ssl.cert_time_to_seconds(certificate_info["notAfter"])<time.time()+86400
    except (OSError,ValueError,ssl.SSLError):sans=set();renew=True
    required={("DNS",config["hostname"]),("IP Address","127.0.0.1")}
    if ip:required.add(("IP Address",ip))
    if new_ca or renew or not required.issubset(sans):
        cfg=ROOT/"openssl.cnf"
        cfg.write_text("[req]\ndistinguished_name=dn\nreq_extensions=server\nprompt=no\n[dn]\nCN="+config["hostname"]+
            "\n[server]\nsubjectAltName=DNS:"+config["hostname"]+",DNS:localhost,IP:127.0.0.1"+(",IP:"+ip if ip else "")+
            "\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n")
        command("openssl","req","-new","-newkey","rsa:2048","-sha256","-nodes","-config",cfg,"-keyout",config["private_key"],"-out",ROOT/"server.csr")
        command("openssl","x509","-req","-in",ROOT/"server.csr","-CA",ROOT/"ca.pem","-CAkey",ROOT/"ca-key.pem","-CAcreateserial","-days","365","-sha256","-extfile",cfg,"-extensions","server","-out",config["certificate"])
    config["lan_ip"]=ip
    config["lan_ipv6_network"]=str(ipaddress.ip_network(ip+"/64",strict=False)) if ip and ":" in ip else None
    host=("["+ip+"]") if ip and ":" in ip else (ip or config["hostname"])
    config["pairing_url"]="https://"+host+":"+str(config["port"])
    # LaunchAgents cannot read TCC-protected Documents. Stage the backend-only
    # settings in private Application Support whenever the Mac app is launched.
    cloud_env=ROOT/"cloud.env"
    source_env=PROJECT/"apps/mac/.env"
    cloud_env.write_bytes(source_env.read_bytes() if source_env.exists() else b"")
    cloud_env.chmod(0o600)
    config["dotenv"]=str(cloud_env)
    for name in ["mac","phone"]:
        encoder=json.loads((PROJECT/"assets"/name/"encoder.json").read_text())
        config["spaces"][encoder["space"]]=encoder["dimension"]
    config_path.write_text(json.dumps(config,indent=2));config_path.chmod(0o600)
    repo=ROOT/"repositories/fabric.git"
    if not repo.exists():command(config["git"],"init","--bare","--initial-branch=main",repo)
    from host import Store
    store=Store(config)
    for k,v in [("http.receivepack","true"),("http.getanyfile","false"),("core.fsync","all"),("commit.gpgsign","false"),
                ("core.hooksPath",str(repo/"hooks")),("receive.denyNonFastForwards","true"),("receive.denyDeletes","true"),("receive.fsckObjects","true")]:store.git("config",k,v)
    try:store.head()
    except ValueError:
        tree=store.git("mktree",data=b"").decode().strip()
        commit=store.git("commit-tree",tree,data=b"Initialize private memory ledger\n").decode().strip()
        store.git("update-ref","refs/heads/main",commit)
    hook=repo/"hooks/pre-receive"
    hook.write_text("#!/bin/sh\nset -eu\nwhile read old new ref; do\n  /usr/bin/python3 "+shlex.quote(str(runtime/"host.py"))+" "+shlex.quote(str(config_path))+" --verify \"$old\" \"$new\" \"$ref\"\ndone\n")
    hook.chmod(0o700)
    certificate=Path(config["certificate"]).read_text()
    pairing={"version":1,"type":"memory-fabric-pairing","url":config["pairing_url"],
        "token":config["token"],"certificate_der":ssl.PEM_cert_to_DER_cert(certificate)}
    # JSON carries Base64 public certificate bytes, not the private TLS key.
    import base64
    pairing["certificate_der"]=base64.b64encode(pairing["certificate_der"]).decode()
    pairing["ca_der"]=base64.b64encode(ssl.PEM_cert_to_DER_cert(Path(config["ca_certificate"]).read_text())).decode()
    pair=ROOT/"Pair iPhone.json";pair.write_text(json.dumps(pairing,indent=2));pair.chmod(0o600)
    if unchanged and old_config==json.dumps(config,sort_keys=True):
        if healthy(config):print("Private Git host ready: "+pairing["url"]+"/fabric.git");return
    agents=Path.home()/"Library/LaunchAgents";agents.mkdir(exist_ok=True)
    plist=agents/(LABEL+".plist")
    record={"Label":LABEL,"ProgramArguments":["/usr/bin/python3",str(runtime/"host.py"),str(config_path)],
        "RunAtLoad":True,"KeepAlive":True,"ThrottleInterval":10,"WorkingDirectory":str(ROOT),
        "StandardOutPath":str(ROOT/"service.log"),"StandardErrorPath":str(ROOT/"service-error.log"),
        "EnvironmentVariables":{"PATH":"/usr/bin:/bin:/usr/sbin:/sbin","PYTHONUNBUFFERED":"1"}}
    plist.write_bytes(plistlib.dumps(record));plist.chmod(0o600)
    domain="gui/"+str(os.getuid())
    loaded=subprocess.run(["launchctl","print",domain+"/"+LABEL],capture_output=True).returncode==0
    if not loaded:command("launchctl","bootstrap",domain,plist)
    command("launchctl","kickstart","-k",domain+"/"+LABEL)
    for _ in range(20):
        if healthy(config):break
        time.sleep(0.2)
    else:raise RuntimeError("Git service did not become ready. Check the service-error.log in its private Sync directory.")
    print("Private Git host running: "+pairing["url"]+"/fabric.git")
    print("Use Pair iPhone in the Mac app to transfer its private pairing file.")

if __name__=="__main__":main()
