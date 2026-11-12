use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use std::path::PathBuf;
const DEFAULT_BUDGET_MINUTES: u64 = 15;
fn default_budget() -> u64 { DEFAULT_BUDGET_MINUTES }
#[derive(Serialize,Deserialize)]pub struct Config{
 #[serde(default)]pub auto_approve:bool,
 #[serde(default = "default_budget")] task_budget_minutes: u64,
 #[serde(default)]pub computer_provider:Option<String>,
}
impl Default for Config {
 fn default() -> Self { Self { auto_approve: false, computer_provider: None, task_budget_minutes: default_budget() } }
}
impl Config{fn allows_computer(&self,provider:&str)->bool{{let _=provider;false}}}
fn path()->PathBuf{PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join("agent-settings.json")}
fn config()->Config{std::fs::read(path()).ok().and_then(|d|serde_json::from_slice(&d).ok()).unwrap_or_default()}
pub fn auto_approve()->bool{config().auto_approve}
pub fn computer_cloud()->bool{false}
pub fn task_budget_minutes() -> u64 { config().task_budget_minutes.clamp(1, 120) }
pub fn status()->Value{let c=config();json!({"auto_approve":c.auto_approve,"task_budget_minutes":c.task_budget_minutes.clamp(1, 120),"computer_cloud":c.allows_computer(&crate::cloud_settings::provider_id()),"provider":crate::cloud_settings::provider_name()})}
pub fn save(auto_approve:Option<bool>,computer_cloud:Option<bool>,budget:Option<u64>)->Result<Value,String>{if computer_cloud==Some(true){return Err("Cloud control is disabled. All device actions and action planning stay local.".into());}static SAVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
let _guard = SAVE.lock().map_err(|_| "Agent settings unavailable")?;
let mut c=config();
if let Some(minutes) = budget {
    if !(1..=120).contains(&minutes) { return Err("Enter a task budget from 1 to 120 minutes.".into()); }
    c.task_budget_minutes = minutes;
}
c.computer_provider=None;if let Some(value)=auto_approve{c.auto_approve=value;}if let Some(value)=computer_cloud{c.computer_provider=if value{Some(crate::cloud_settings::provider_id())}else{None};}fabric_core::write_json(&path(),&c).map_err(|e|e.to_string())?;Ok(status())}
#[cfg(test)]mod tests{use super::*;#[test]fn upgrades_do_not_enable_auto_approval_or_desktop_sharing(){let c:Config=serde_json::from_str("{}").unwrap();assert!(!c.auto_approve);assert!(c.computer_provider.is_none());let c:Config=serde_json::from_str(r#"{"auto_approve":true,"computer_provider":"provider-one"}"#).unwrap();assert!(c.auto_approve);assert!(!c.allows_computer("provider-one"));assert!(!c.allows_computer("provider-two"));}}
