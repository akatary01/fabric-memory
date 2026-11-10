//! An explicit, app-owned LAN service. Never launched by merely opening Devices.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json,Value};
use std::{path::{Path,PathBuf},process::{Child,Command,Stdio},sync::Mutex,io::{BufRead,BufReader,Write}};
#[derive(Default)]
pub struct Host { child:Mutex<Option<Child>>, address:Mutex<String>, config:Mutex<Option<PathBuf>>, error:Mutex<String> }
impl Drop for Host { fn drop(&mut self) { if let Ok(child)=self.child.get_mut() { if let Some(mut child)=child.take() { drop(child.stdin.take());let _=child.wait(); } } } }
fn protected(path:&Path,data:&[u8])->Result<(),String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file=std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(path).map_err(|e|e.to_string())?;
    file.write_all(data).map_err(|e|e.to_string())
}
fn root(beacon:&Path)->Result<PathBuf,String> {Ok(beacon.parent().and_then(Path::parent).ok_or("Missing app data")?.join("LocalBeacon"))}
fn address()->Result<String,String> {
    for interface in ["en0","en1","en2"] {
        if let Ok(output)=Command::new("/usr/sbin/ipconfig").args(["getifaddr",interface]).output() {
            let value=String::from_utf8_lossy(&output.stdout).trim().to_string();
            if value.parse::<std::net::Ipv4Addr>().is_ok_and(|ip|ip.is_private()) {return Ok(value);}
        }
    }
    Err("Connect this Mac to Wi-Fi or Ethernet before deploying a local beacon.".into())
}
fn certificates(root:&Path,host:&str)->Result<Value,String> {
    use rcgen::*;
    let cached=root.join("identity.json");
    if let Ok(data)=std::fs::read(&cached){if let Ok(value)=serde_json::from_slice::<Value>(&data){if value["host"]==host && root.join("server.pem").is_file() && root.join("server-key.pem").is_file(){return Ok(value);}}}
    let ca_key=root.join("ca-key.pem");let ca_file=root.join("ca.pem");
    if !ca_key.exists() || !ca_file.exists() {
        let key=KeyPair::generate().map_err(|e|e.to_string())?;
        let mut params=CertificateParams::new(vec![]).map_err(|e|e.to_string())?;
        params.distinguished_name=DistinguishedName::new();params.distinguished_name.push(DnType::CommonName,"Memory Fabric beacon");
        params.is_ca=IsCa::Ca(BasicConstraints::Unconstrained);params.key_usages=vec![KeyUsagePurpose::KeyCertSign,KeyUsagePurpose::CrlSign];
        let ca=params.self_signed(&key).map_err(|e|e.to_string())?;
        protected(&ca_key,key.serialize_pem().as_bytes())?;protected(&ca_file,ca.pem().as_bytes())?;
    }
    let pem=std::fs::read_to_string(&ca_file).map_err(|e|e.to_string())?;
    let key=KeyPair::from_pem(&std::fs::read_to_string(ca_key).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let issuer=Issuer::from_ca_cert_pem(&pem,key).map_err(|e|e.to_string())?;
    let key=KeyPair::generate().map_err(|e|e.to_string())?;
    let mut params=CertificateParams::new(vec![host.into(),"localhost".into(),"127.0.0.1".into()]).map_err(|e|e.to_string())?;
    params.extended_key_usages=vec![ExtendedKeyUsagePurpose::ServerAuth];
    let cert=params.signed_by(&key,&issuer).map_err(|e|e.to_string())?;
    protected(&root.join("server.pem"),format!("{}{}",cert.pem(),pem).as_bytes())?;
    protected(&root.join("server-key.pem"),key.serialize_pem().as_bytes())?;
    let der=pem.lines().filter(|line|!line.starts_with("---")).collect::<String>();
    let identity=json!({"host":host,"ca_der":der,"certificate_der":STANDARD.encode(cert.der())});
    protected(&cached,&serde_json::to_vec(&identity).map_err(|e|e.to_string())?)?;Ok(identity)
}
impl Host {
    pub fn owns(&self,beacon:&Path)->bool {
        let saved=std::fs::read(beacon).ok().and_then(|d|serde_json::from_slice::<Value>(&d).ok());
        let local=root(beacon).ok().and_then(|r|std::fs::read(r.join("config.json")).ok()).and_then(|d|serde_json::from_slice::<Value>(&d).ok());
        matches!((saved,local),(Some(saved),Some(local)) if saved["token"].is_string() && saved["token"]==local["token"])
    }
    fn resume_eligible(&self,beacon:&Path)->bool {self.owns(beacon) && root(beacon).ok().and_then(|r|std::fs::read(r.join("config.json")).ok()).and_then(|d|serde_json::from_slice::<Value>(&d).ok()).is_some_and(|v|v["enabled"]==true)}
    pub fn resume(&self,beacon:&Path){if self.resume_eligible(beacon){if let Err(error)=self.deploy(beacon){if let Ok(mut value)=self.error.lock(){*value=error;}}}}
    pub fn status(&self)->Result<Value,String> {
        let mut child=self.child.lock().map_err(|e|e.to_string())?;
        let running=match child.as_mut(){Some(c)=>c.try_wait().map_err(|e|e.to_string())?.is_none(),None=>false};
        if !running {*child=None;}
        Ok(json!({"running":running,"url":self.address.lock().map_err(|e|e.to_string())?.clone(),"error":self.error.lock().map_err(|e|e.to_string())?.clone()}))
    }
    pub fn stop(&self)->Result<Value,String> {
        if let Some(path)=self.config.lock().map_err(|e|e.to_string())?.as_ref(){if let Ok(bytes)=std::fs::read(path){if let Ok(mut value)=serde_json::from_slice::<Value>(&bytes){value["enabled"]=json!(false);protected(path,&serde_json::to_vec(&value).map_err(|e|e.to_string())?)?;}}}
        if let Some(mut child)=self.child.lock().map_err(|e|e.to_string())?.take() {drop(child.stdin.take());child.wait().map_err(|e|e.to_string())?;}
        self.status()
    }
    pub fn deploy(&self,beacon:&Path)->Result<Value,String> {
        if self.status()?["running"]==true {return self.status();}
        let host=address()?;
        let resources=std::env::current_exe().map_err(|e|e.to_string())?.parent().ok_or("Missing app")?.join("../Resources");
        let bundled=resources.join("git/git");
        let git=if bundled.is_file(){bundled}else{PathBuf::from("/usr/bin/git")};
        self.deploy_at(beacon,&host,&git)
    }
    fn deploy_at(&self,beacon:&Path,host:&str,git:&Path)->Result<Value,String>{
        let mut guard=self.child.lock().map_err(|e|e.to_string())?;
        if guard.is_some(){return Err("Local beacon is already starting".into());}
        let root=root(beacon)?;std::fs::create_dir_all(&root).map_err(|e|e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
        let identity=certificates(&root,host)?;
        if !Command::new(&git).arg("--version").output().is_ok_and(|o|o.status.success()){return Err("Git is unavailable. Install the current standalone Memory Fabric app.".into());}
        let saved=std::fs::read(root.join("config.json")).ok().and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
        let desired=saved.as_ref().and_then(|v|v["port"].as_u64()).unwrap_or(0);
        let listener=std::net::TcpListener::bind(format!("{}:{desired}",if host=="127.0.0.1"{"127.0.0.1"}else{"0.0.0.0"})).map_err(|_|"The local beacon port is occupied. Stop the other service and try again.".to_string())?;let port=listener.local_addr().map_err(|e|e.to_string())?.port();drop(listener);
        let url=format!("https://{host}:{port}");
        let token_path=root.join("owner-token");
        let token=if token_path.exists(){std::fs::read_to_string(&token_path).map_err(|e|e.to_string())?}else{
            // rcgen's cryptographically random private key is used only as random entropy here.
            let token=STANDARD.encode(rcgen::KeyPair::generate().map_err(|e|e.to_string())?.serialize_der());protected(&token_path,token.as_bytes())?;token
        };
        let config=json!({"version":2,"enabled":true,"root":root,"git":git,"port":port,"bind":if host=="127.0.0.1"{"127.0.0.1"}else{"0.0.0.0"},"token":token,"spaces":serde_json::from_str::<Value>(include_str!("../../../../protocol/spaces.json")).map_err(|e|e.to_string())?,"public_url":url,"path_prefix":"","allow_remote":false,"allow_prompts":false,"certificate":root.join("server.pem"),"private_key":root.join("server-key.pem"),"ca_der":identity["ca_der"],"certificate_der":identity["certificate_der"]});
        protected(&root.join("config.json"),&serde_json::to_vec(&config).map_err(|e|e.to_string())?)?;
        for (name,code) in [("local_beacon.py",include_str!("../../../../tools/local_beacon.py")),("host.py",include_str!("../../../../tools/sync/host.py")),("enrollment.py",include_str!("../../../../tools/sync/enrollment.py"))] {protected(&root.join(name),code.as_bytes())?;}
        let log=std::fs::File::create(root.join("service.log")).map_err(|e|e.to_string())?;
        let python=std::env::var("FABRIC_PYTHON").unwrap_or_else(|_|"python3".into());
        let mut child=Command::new(python).arg("-B").arg(root.join("local_beacon.py")).arg(root.join("config.json")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(log).spawn().map_err(|e|e.to_string())?;
        let mut reader=BufReader::new(child.stdout.take().ok_or("Missing beacon output")?);let mut line=String::new();
        reader.read_line(&mut line).map_err(|e|e.to_string())?;
        if line.trim()!="ready" {let _=child.kill();let _=child.wait();return Err("Local beacon could not start. See LocalBeacon/service.log.".into());}
        let owner=json!({"version":2,"type":"memory-fabric-pairing","url":url,"token":token,"device_name":"This Mac","ca_der":identity["ca_der"]});
        if let Some(parent)=beacon.parent(){std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;}
        if let Err(error)=protected(beacon,&serde_json::to_vec(&owner).map_err(|e|e.to_string())?) {drop(child.stdin.take());let _=child.wait();return Err(error);}
        *self.config.lock().map_err(|e|e.to_string())?=Some(root.join("config.json"));*self.error.lock().map_err(|e|e.to_string())?=String::new();
        *self.address.lock().map_err(|e|e.to_string())?=url;*guard=Some(child);drop(guard);self.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_deploy_pair_and_stop_preserves_identity(){
        let temp=std::env::temp_dir().join(format!("fabric-beacon-test-{}",std::process::id()));std::fs::create_dir_all(&temp).unwrap();
        let beacon=temp.join("Sync/beacon.json");let host=Host::default();
        let git=std::env::var("FABRIC_TEST_GIT").unwrap_or_else(|_|"/usr/bin/git".into());
        let status=host.deploy_at(&beacon,"127.0.0.1",Path::new(&git)).unwrap();assert_eq!(status["running"],true);
        let invite=crate::beacon::invite(&beacon).unwrap();let uri=invite["uri"].as_str().unwrap();assert!(uri.contains("certificate="));
        let peer=temp.join("Peer/beacon.json");crate::beacon::connect(&peer,uri,"","Test phone").unwrap();
        assert!(crate::beacon::connect(&temp.join("Replay/beacon.json"),uri,"","Replay").is_err());
        assert_eq!(crate::beacon::devices(&peer).unwrap()["devices"].as_array().unwrap().len(),1);
        let original:Value=serde_json::from_slice(&std::fs::read(root(&beacon).unwrap().join("config.json")).unwrap()).unwrap();
        assert!(host.resume_eligible(&beacon));host.stop().unwrap();assert!(!host.resume_eligible(&beacon));assert_eq!(host.status().unwrap()["running"],false);
        host.deploy_at(&beacon,"127.0.0.1",Path::new(&git)).unwrap();
        let current:Value=serde_json::from_slice(&std::fs::read(root(&beacon).unwrap().join("config.json")).unwrap()).unwrap();assert_eq!(original["ca_der"],current["ca_der"]);assert_eq!(original["certificate_der"],current["certificate_der"]);assert_eq!(original["public_url"],current["public_url"]);
        crate::beacon::devices(&peer).unwrap();
        host.stop().unwrap();std::fs::remove_dir_all(temp).unwrap();
    }
}
