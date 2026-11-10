use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::path::Path;
pub const DEFAULT_URL: &str = "https://akatary.com/fabric-git";

pub fn endpoint(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value.trim()).map_err(|_| "Enter a complete HTTPS beacon URL")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path().contains(['%', '\\'])
    {
        return Err("Use an HTTPS beacon URL without credentials, a query, or a fragment".into());
    }
    Ok(url.as_str().trim_end_matches('/').into())
}
pub fn invitation(text: &str, code: &str) -> Result<(String, String), String> {
    let text = text.trim();
    if text.starts_with("fabric-memory://") {
        let uri = reqwest::Url::parse(text).map_err(|_| "Invalid pairing QR")?;
        if uri.host_str() != Some("pair") {
            return Err("Not a Memory Fabric pairing QR".into());
        }
        let urls: Vec<_> = uri.query_pairs().filter(|(k, _)| k == "url").collect();
        if urls.len() != 1 {
            return Err("Pairing QR must contain one beacon URL".into());
        }
        let code = uri
            .fragment()
            .and_then(|s| s.strip_prefix("invite="))
            .unwrap_or(code);
        Ok((endpoint(&urls[0].1)?, code.into()))
    } else {
        if let Ok(mut uri) = reqwest::Url::parse(text) {
            if let Some(invite) = uri
                .fragment()
                .and_then(|v| v.strip_prefix("join="))
                .map(String::from)
            {
                uri.set_fragment(None);
                return Ok((endpoint(uri.as_str())?, invite));
            }
        }
        Ok((endpoint(text)?, code.trim().into()))
    }
}
pub fn local_host(base:&str)->bool {
    reqwest::Url::parse(base).ok().and_then(|u|u.host_str().map(str::to_owned)).is_some_and(|host| {
        host=="localhost" || host.ends_with(".local") || host.parse::<std::net::Ipv4Addr>().is_ok_and(|ip|ip.is_private() || ip.is_loopback())
    })
}
pub fn certificate(text:&str)->Result<Option<String>,String> {
    use base64::{engine::general_purpose::{URL_SAFE_NO_PAD,STANDARD},Engine};
    let uri=reqwest::Url::parse(text).map_err(|_|"Invalid pairing link")?;
    let certs=uri.query_pairs().filter(|(k,_)|k=="certificate").map(|(_,v)|v.into_owned()).collect::<Vec<_>>();
    if certs.is_empty(){return Ok(None);}
    let (base,_)=invitation(text,"")?;
    if certs.len()!=1 || !local_host(&base) || certs[0].len()>8192 {return Err("Certificate pairing is only available for local beacons".into());}
    let bytes=URL_SAFE_NO_PAD.decode(&certs[0]).map_err(|_|"Invalid beacon certificate")?;
    reqwest::Certificate::from_der(&bytes).map_err(|_|"Invalid beacon certificate")?;
    Ok(Some(STANDARD.encode(bytes)))
}
pub fn http_config(config:&Value)->Result<Client,String> {http_certificate(config["ca_der"].as_str(),config["url"].as_str().unwrap_or(""))}
fn http_certificate(certificate:Option<&str>,base:&str)->Result<Client,String> {
    use base64::{engine::general_purpose::STANDARD,Engine};
    let mut builder=Client::builder().https_only(true).redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(20));
    if let Some(certificate)=certificate {
        if !local_host(base){return Err("Pinned certificates require a local beacon".into());}
        let bytes=STANDARD.decode(certificate).map_err(|_|"Invalid beacon certificate")?;
        builder=builder.tls_built_in_root_certs(false).add_root_certificate(reqwest::Certificate::from_der(&bytes).map_err(|_|"Invalid beacon certificate")?);
    }
    builder.build().map_err(|_|"Cannot initialize beacon connection".into())
}
fn response(response: reqwest::blocking::Response) -> Result<Value, String> {
    use std::io::Read;
    let success = response.status().is_success();
    let mut data = Vec::new();
    response
        .take(65537)
        .read_to_end(&mut data)
        .map_err(|_| "Could not read beacon response")?;
    if data.len() > 65536 {
        return Err("Beacon response exceeds limit".into());
    }
    let value: Value = serde_json::from_slice(&data)
        .map_err(|_| "The server is not a Fabric beacon, or has not been deployed yet")?;
    if !success {
        return Err(value["error"]
            .as_str()
            .unwrap_or("Beacon rejected the request")
            .into());
    }
    Ok(value)
}
pub fn settings(path: &Path) -> Value {
    let value = std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice::<Value>(&data).ok());
    let saved = std::fs::read(path.with_extension("address.json"))
        .ok()
        .and_then(|d| serde_json::from_slice::<Value>(&d).ok());
    json!({"url":value.as_ref().and_then(|v|v["url"].as_str()).or_else(||saved.as_ref().and_then(|v|v["url"].as_str())).unwrap_or(DEFAULT_URL),"connected":value.is_some(),"name":value.as_ref().and_then(|v|v["device_name"].as_str()).unwrap_or("This computer")})
}
pub fn save_address(path: &Path, url: &str) -> Result<Value, String> {
    let (url, _) = invitation(url, "")?;
    let parent = path.parent().ok_or("Missing settings directory")?;
    std::fs::create_dir_all(parent).map_err(|_| "Cannot create settings directory")?;
    std::fs::write(
        path.with_extension("address.json"),
        serde_json::to_vec(&json!({"url":url})).map_err(|_| "Invalid address")?,
    )
    .map_err(|_| "Cannot save beacon address")?;
    Ok(json!({"url":url}))
}
pub fn connect(path: &Path, url: &str, code: &str, name: &str) -> Result<Value, String> {
    let ca=certificate(url)?;
    let (base, code) = invitation(url, code)?;
    if code.len() < 20 || code.len() > 256 {
        return Err(
            "Enter the invitation code from the beacon, or scan its current join QR".into(),
        );
    }
    if name.trim().is_empty() || name.len() > 80 {
        return Err("Choose a device name of 1–80 characters".into());
    }
    let value = response(
        http_certificate(ca.as_deref(),&base)?
            .post(format!("{base}/v1/join"))
            .json(&json!({"invitation":code,"device_name":name.trim()}))
            .send()
            .map_err(|_| {
                "Beacon unavailable. Check the URL and that launch-git is running behind HTTPS"
            })?,
    )?;
    if value["version"] != 2
        || value["type"] != "memory-fabric-pairing"
        || value["url"] != base
        || value["token"]
            .as_str()
            .is_none_or(|s| s.len() < 32 || s.len() > 256)
    {
        return Err("Invalid beacon credentials".into());
    }
    let mut value = value;
    if let Some(ca)=ca {value["ca_der"]=json!(ca);} else {value.as_object_mut().unwrap().remove("ca_der");value.as_object_mut().unwrap().remove("certificate_der");}
    value["device_name"] = json!(name.trim());
    let parent = path.parent().ok_or("Invalid beacon settings path")?;
    std::fs::create_dir_all(parent).map_err(|_| "Cannot create beacon settings")?;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "Cannot protect settings")?;
    let temporary = path.with_extension("next");
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| "Cannot save beacon settings")?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "Cannot protect credentials")?;
    file.write_all(&serde_json::to_vec(&value).map_err(|_| "Invalid beacon settings")?)
        .map_err(|_| "Cannot save beacon settings")?;
    file.sync_all().map_err(|_| "Cannot save beacon settings")?;
    std::fs::rename(temporary, path).map_err(|_| "Cannot save beacon settings")?;
    Ok(settings(path))
}
pub fn invite(path: &Path) -> Result<Value, String> {
    let value: Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|_| "Connect this device to a beacon first")?,
    )
    .map_err(|_| "Invalid beacon configuration")?;
    let base = endpoint(value["url"].as_str().ok_or("Missing beacon URL")?)?;
    response(
        http_config(&value)?
            .post(format!("{base}/v1/invitations"))
            .bearer_auth(value["token"].as_str().ok_or("Missing beacon credential")?)
            .json(&json!({}))
            .send()
            .map_err(|_| "Beacon unavailable")?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefixed_url_and_qr_roundtrip() {
        let (compact_url, compact_code) = invitation(
            "https://akatary.com/fabric-git#join=abcdefghijklmnopqrstuvwx",
            "",
        )
        .unwrap();
        assert_eq!(compact_url, DEFAULT_URL);
        assert_eq!(compact_code, "abcdefghijklmnopqrstuvwx");
        let (url,code)=invitation("fabric-memory://pair?url=https%3A%2F%2Fakatary.com%2Ffabric-git#invite=abcdefghijklmnopqrstuvwx","").unwrap();
        assert_eq!(url, DEFAULT_URL);
        assert_eq!(code, "abcdefghijklmnopqrstuvwx");
    }
    #[test]
    fn rejects_untrusted_url_shapes() {
        for u in [
            "http://host",
            "https://user:secret@host",
            "https://host/?token=x",
            "https://host/#x",
            "https://host/%2fprivate",
        ] {
            assert!(endpoint(u).is_err(), "{u}");
        }
    }
}

pub fn devices(path: &Path) -> Result<Value, String> {
    let config: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Connect a Memory beacon first")?)
            .map_err(|_| "Invalid beacon configuration")?;
    let response = http_config(&config)?
        .get(format!(
            "{}/v1/devices",
            config["url"]
                .as_str()
                .ok_or("Missing beacon URL")?
                .trim_end_matches('/')
        ))
        .bearer_auth(config["token"].as_str().ok_or("Missing credential")?)
        .send()
        .map_err(|_| "Memory beacon unavailable")?;
    let success = response.status().is_success();
    let value: Value = response.json().map_err(|_| "Invalid beacon response")?;
    if !success {
        return Err("Could not list paired devices".into());
    }
    Ok(value)
}
