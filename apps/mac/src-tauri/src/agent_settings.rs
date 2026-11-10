use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::path::PathBuf;
#[derive(Default,Serialize,Deserialize)]pub struct Config{
 #[serde(default)]pub auto_approve:bool,
 #[serde(default)]pub computer_provider:Option<String>,
}
impl Config{fn allows_computer(&self,provider:&str)->bool{self.computer_provider.as_deref()==Some(provider)}}
fn path()->PathBuf{PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join("agent-settings.json")}
fn config()->Config{std::fs::read(path()).ok().and_then(|d|serde_json::from_slice(&d).ok()).unwrap_or_default()}
pub fn auto_approve()->bool{config().auto_approve}
pub fn computer_cloud()->bool{config().allows_computer(&crate::cloud_settings::provider_id())}
pub fn status()->Value{let c=config();json!({"auto_approve":c.auto_approve,"computer_cloud":c.allows_computer(&crate::cloud_settings::provider_id()),"provider":crate::cloud_settings::provider_name()})}
pub fn save(auto_approve:Option<bool>,computer_cloud:Option<bool>)->Result<Value,String>{let mut c=config();if let Some(value)=auto_approve{c.auto_approve=value;}if let Some(value)=computer_cloud{c.computer_provider=if value{Some(crate::cloud_settings::provider_id())}else{None};}fabric_core::write_json(&path(),&c).map_err(|e|e.to_string())?;Ok(status())}
#[cfg(test)]mod tests{use super::*;#[test]fn upgrades_do_not_enable_auto_approval_or_desktop_sharing(){let c:Config=serde_json::from_str("{}").unwrap();assert!(!c.auto_approve);assert!(c.computer_provider.is_none());let c:Config=serde_json::from_str(r#"{"auto_approve":true,"computer_provider":"provider-one"}"#).unwrap();assert!(c.auto_approve);assert!(c.allows_computer("provider-one"));assert!(!c.allows_computer("provider-two"));}}
