use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static VAULT_PENDING:AtomicBool=AtomicBool::new(false);
static VAULT_REVISION:AtomicU64=AtomicU64::new(0);
fn vault_error()-> &'static Mutex<Option<String>>{static ERROR:OnceLock<Mutex<Option<String>>>=OnceLock::new();ERROR.get_or_init(||Mutex::new(None))}
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
#[derive(Serialize, Deserialize)]
struct Config {
    enabled: bool,
    model: String,
    #[serde(default="default_automatic")]
    automatic: bool,
}
fn default_automatic()->bool{true}
pub fn automatic()->bool{config().map(|c|c.automatic).unwrap_or(true)}
fn path() -> PathBuf {
    PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join(if provider_id()=="openai" {"cloud-config.json".into()} else {format!("cloud-config-{}.json",provider_id())})
}
fn config() -> Option<Config> {
    std::fs::read(path())
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
}
fn cache() -> &'static Mutex<Option<Option<String>>> {
    static CACHE: OnceLock<Mutex<Option<Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}
#[cfg(target_os = "macos")]
fn vault(args: Value) -> Result<Value, String> {
    use security_framework::passwords::{
        delete_generic_password, generic_password, set_generic_password, PasswordOptions,
    };
    const SERVICE: &str = "local.memoryfabric.cloud";
    let account=args["provider"].as_str().map(str::to_owned).unwrap_or_else(provider_id);
    match args["action"].as_str() {
        Some("read") => {
            match generic_password(PasswordOptions::new_generic_password(SERVICE, &account)) {
                Ok(bytes) => {
                    Ok(json!({"token":String::from_utf8(bytes).map_err(|_|"Invalid saved token")?}))
                }
                Err(error) if error.code() == -25300 => Ok(json!({"token":null})),
                Err(_) => Err("Unlock Keychain or re-enter your API token".into()),
            }
        }
        Some("save") => {
            set_generic_password(
                SERVICE,
                &account,
                args["token"].as_str().ok_or("Missing token")?.as_bytes(),
            )
            .map_err(|_| "Could not save the token in Keychain")?;
            Ok(json!({"saved":true}))
        }
        Some("remove") => {
            match delete_generic_password(SERVICE, &account) {
                Ok(()) => {}
                Err(error) if error.code() == -25300 => {}
                Err(_) => return Err("Could not remove the saved token".into()),
            };
            Ok(json!({"removed":true}))
        }
        _ => Err("Unknown credential operation".into()),
    }
}
#[cfg(not(target_os = "macos"))]
fn vault(_args: Value) -> Result<Value, String> {
    Err("Keychain is available in the Mac app".into())
}
pub fn token() -> Option<String> {
    let mut current=cache().lock().ok()?;
    if current.is_none(){
        *current=Some(None);let provider=provider_id();let revision=VAULT_REVISION.fetch_add(1,Ordering::SeqCst)+1;
        VAULT_PENDING.store(true,Ordering::SeqCst);drop(current);
        let (tx,rx)=std::sync::mpsc::channel();
        std::thread::spawn(move||{
            let result=vault(json!({"action":"read","provider":provider}));
            let token=result.as_ref().ok().and_then(|v|v["token"].as_str().map(str::to_owned));
            if let Ok(mut state)=cache().lock(){if VAULT_REVISION.load(Ordering::SeqCst)==revision{
                *state=Some(token.clone());if let Ok(mut error)=vault_error().lock(){*error=result.err();}VAULT_PENDING.store(false,Ordering::SeqCst);
            }}let _=tx.send(token);
        });
        let _=rx.recv_timeout(std::time::Duration::from_millis(350));
        current=cache().lock().ok()?;
    }
    current.as_ref().cloned().flatten().or_else(||if provider_id()=="openai"{std::env::var("OPENAI_API_KEY").ok().filter(|key|!key.trim().is_empty())}else{None})
}
fn reset_vault(){VAULT_REVISION.fetch_add(1,Ordering::SeqCst);VAULT_PENDING.store(false,Ordering::SeqCst);if let Ok(mut error)=vault_error().lock(){*error=None;}}
pub fn retry_keychain()->Result<Value,String>{if VAULT_PENDING.load(Ordering::SeqCst){return Err("Keychain is still waiting. Complete its access prompt, or enter the token again.".into());}reset_vault();*cache().lock().map_err(|_|"Cloud settings unavailable")?=None;Ok(status())}

pub fn enabled() -> bool {
    config()
        .map(|c| c.enabled)
        .unwrap_or_else(|| std::env::var("FABRIC_CLOUD_ENABLED").as_deref() == Ok("true"))
}
pub fn model() -> String {
    config()
        .map(|c| c.model)
        .or_else(|| std::env::var("OPENAI_MODEL").ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "gpt-4.1-mini".into())
}
pub fn status() -> Value {
    let configured = token().is_some();
    let source = if cache()
        .lock()
        .ok()
        .is_some_and(|c| c.as_ref().is_some_and(|v| v.is_some()))
    {
        "keychain"
    } else if configured {
        "environment"
    } else {
        "none"
    };
    json!({"keychain_pending":VAULT_PENDING.load(Ordering::SeqCst),"credential_error":vault_error().lock().ok().and_then(|e|e.clone()),"provider":provider_name(),"provider_id":provider_id(),"providers":providers()["items"],"base_url":base_url(),"model":model(),"enabled":enabled(),"token_configured":configured,"credential_source":source,"automatic":automatic(),"last_selected":crate::cloud_models::last()})
}
pub fn save(model: &str, enabled: bool, new_token: &str, automatic:bool) -> Result<Value, String> {
    let model = model.trim();
    if model.is_empty()
        || model.len() > 120
        || !model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
    {
        return Err("Enter a valid cloud model name".into());
    }
    let new_token = new_token.trim();
    if !new_token.is_empty() {
        if new_token.len() > 8192 || new_token.bytes().any(|b| b <= 32 || b >= 127) {
            return Err("The token contains invalid characters".into());
        }
        vault(json!({"action":"save","token":new_token}))?;
        crate::cloud_models::invalidate();
        reset_vault();
        *cache().lock().map_err(|_| "Cloud settings unavailable")? = Some(Some(new_token.into()));
    }
    if enabled && token().is_none() {
        return Err("Enter an API token before enabling cloud answers".into());
    }
    fabric_core::write_json(
        &path(),
        &Config {
            enabled,
            model: model.into(),
            automatic,
        },
    )
    .map_err(|_| "Could not save cloud settings")?;
    Ok(status())
}
pub fn remove() -> Result<Value, String> {
    vault(json!({"action":"remove"}))?;
    crate::cloud_models::invalidate();
    reset_vault();
    *cache().lock().map_err(|_| "Cloud settings unavailable")? = Some(None);
    fabric_core::write_json(
        &path(),
        &Config {
            enabled: false,
            model: model(),
            automatic:automatic(),
        },
    )
    .map_err(|_| "Could not save cloud settings")?;
    Ok(status())
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use security_framework::passwords::{
        delete_generic_password, generic_password, set_generic_password, PasswordOptions,
    };
    #[test]
    fn keychain_roundtrip_uses_an_isolated_fixture() {
        let service = format!(
            "local.memoryfabric.test.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete_generic_password(&self.0, "fixture");
            }
        }
        let _cleanup = Cleanup(service.clone());
        set_generic_password(&service, "fixture", b"fixture-only-not-an-api-key")
            .expect("Keychain write");
        assert_eq!(
            generic_password(PasswordOptions::new_generic_password(&service, "fixture")).unwrap(),
            b"fixture-only-not-an-api-key"
        );
        set_generic_password(&service, "fixture", b"replacement-fixture").unwrap();
        assert_eq!(
            generic_password(PasswordOptions::new_generic_password(&service, "fixture")).unwrap(),
            b"replacement-fixture"
        );
        delete_generic_password(&service, "fixture").unwrap();
        assert_eq!(
            generic_password(PasswordOptions::new_generic_password(&service, "fixture"))
                .unwrap_err()
                .code(),
            -25300
        );
    }
}

#[cfg(test)]mod selection_tests {use super::*;
 #[test]fn existing_settings_adopt_automatic_selection_and_pinning_is_explicit(){let existing:Config=serde_json::from_value(json!({"enabled":true,"model":"gpt-4.1-mini"})).unwrap();assert!(existing.automatic);let pinned:Config=serde_json::from_value(json!({"enabled":true,"model":"gpt-6.1-sol","automatic":false})).unwrap();assert!(!pinned.automatic);}
}

// Provider endpoints use the Responses API protocol. Credentials remain in Keychain.
fn providers_path()->PathBuf{PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join("cloud-providers.json")}
fn providers()->Value { std::fs::read(providers_path()).ok().and_then(|v|serde_json::from_slice::<Value>(&v).ok()).unwrap_or_else(||json!({"active":"openai","items":[{"id":"openai","name":"OpenAI","base_url":"https://api.openai.com/v1"}]})) }
pub fn provider_id()->String{providers()["active"].as_str().unwrap_or("openai").to_string()}
fn active_provider()->Value {let all=providers();all["items"].as_array().into_iter().flatten().find(|p|p["id"]==all["active"]).cloned().unwrap_or_else(||json!({"name":"OpenAI","base_url":"https://api.openai.com/v1"}))}
pub fn provider_name()->String{active_provider()["name"].as_str().unwrap_or("OpenAI").into()}
pub fn base_url()->String{active_provider()["base_url"].as_str().unwrap_or("https://api.openai.com/v1").into()}
pub fn endpoint(route:&str)->String{format!("{}/{}",base_url(),route)}
fn validate_endpoint(input:&str)->Result<String,String>{
 let url=reqwest::Url::parse(input.trim()).map_err(|_|"Enter an HTTPS API base URL")?;
 if url.scheme()!="https" || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some(){return Err("Use an HTTPS base URL without credentials, query or fragment".into());}
 Ok(url.as_str().trim_end_matches('/').into())
}
fn changed()->Result<Value,String>{reset_vault();*cache().lock().map_err(|_|"Cloud settings unavailable")?=None;crate::cloud_models::invalidate();Ok(status())}
pub fn select_provider(id:&str)->Result<Value,String>{let mut all=providers();if !all["items"].as_array().is_some_and(|a|a.iter().any(|p|p["id"]==id)){return Err("Unknown provider".into());}all["active"]=json!(id);fabric_core::write_json(&providers_path(),&all).map_err(|e|e.to_string())?;changed()}
pub fn add_provider(name:&str,url:&str)->Result<Value,String>{
 let name=name.trim();if name.is_empty()||name.chars().count()>60{return Err("Enter a provider name (up to 60 characters)".into());}
 let base_url=validate_endpoint(url)?;let mut all=providers();let items=all["items"].as_array_mut().ok_or("Invalid provider settings")?;
 if items.len()>=20{return Err("Maximum 20 providers".into());}
 if items.iter().any(|p|p["base_url"]==base_url){return Err("This provider endpoint is already added".into());}
 let id=format!("provider-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos());
 items.push(json!({"id":id,"name":name,"base_url":base_url}));all["active"]=json!(id);fabric_core::write_json(&providers_path(),&all).map_err(|e|e.to_string())?;changed()
}
#[cfg(test)]mod provider_tests{use super::*;#[test]fn endpoint_rejects_embedded_secrets_and_unsafe_protocols(){assert_eq!(validate_endpoint("https://example.com/v1/").unwrap(),"https://example.com/v1");for url in ["http://example.com","https://token@example.com","https://example.com?key=secret","https://example.com/#fragment","file:///tmp"]{assert!(validate_endpoint(url).is_err());}}}
