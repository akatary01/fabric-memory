//! Account-discovered model pool; complexity determines the capability tier.
use serde_json::{json,Value};
use std::path::PathBuf;
fn path()->PathBuf{PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join(if crate::cloud_settings::provider_id()=="openai"{"cloud-model-catalog.json".into()}else{format!("cloud-model-catalog-{}.json",crate::cloud_settings::provider_id())})}
fn choice_path()->PathBuf{path().with_file_name(format!("cloud-model-choice-{}.json",crate::cloud_settings::provider_id()))}
pub fn invalidate(){let _=std::fs::remove_file(path());}
pub fn compatible(id:&str)->bool {
 if let Some(fine_tuned)=id.strip_prefix("ft:"){return compatible(fine_tuned.split(':').next().unwrap_or(""));}
 id!="gpt-4" && id!="o1-preview" && (id.starts_with("gpt-") || id.starts_with("chatgpt-") || id.starts_with("codex-") || (id.starts_with('o') && id.chars().nth(1).is_some_and(|c|c.is_ascii_digit()))) && !["audio","realtime","transcrib","tts","image","search-preview","search-api","instruct","embedding","moderation"].iter().any(|p|id.contains(p)) && !["gpt-3","gpt-4-","gpt-4-turbo"].iter().any(|p|id.starts_with(p))
}
pub fn catalog(refresh:bool)->Result<Value,String>{
 let file=path();let provider=crate::cloud_settings::provider_name();let official=crate::cloud_settings::provider_id()=="openai";let endpoint=crate::cloud_settings::endpoint("models");
 let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs();
 let cached=std::fs::read(&file).ok().and_then(|d|serde_json::from_slice::<Value>(&d).ok());
 if !refresh {if let Some(v)=cached.as_ref(){if now.saturating_sub(v["fetched_at"].as_u64().unwrap_or(0))<86400 {return Ok(v.clone());}}}
 let token=crate::cloud_settings::token().ok_or("Add a cloud API token first")?;
 let result=(|| {
  let response=reqwest::blocking::Client::builder().timeout(crate::agent::bounded_timeout(20)?).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())?.get(endpoint).bearer_auth(token).send().map_err(|_|"Could not discover provider models")?;
  if !response.status().is_success(){return Err(format!("Model discovery returned HTTP {}",response.status().as_u16()));}
  use std::io::Read;let mut bytes=Vec::new();response.take(1048577).read_to_end(&mut bytes).map_err(|e|e.to_string())?;if bytes.len()>1048576{return Err("Model catalog exceeds limit".into());}
  let value:Value=serde_json::from_slice(&bytes).map_err(|_|"Invalid provider model catalog")?;
  let today=time::OffsetDateTime::now_utc().date().to_string();
  let all=value["data"].as_array().ok_or("Missing models")?;
  let mut models=all.iter().filter_map(|v|v["id"].as_str().filter(|id|(!official || compatible(id)) && v["shutdown_date"].as_str().is_none_or(|date|date>today.as_str())).map(str::to_owned)).collect::<Vec<_>>();models.sort();models.dedup();
  if models.is_empty(){return Err("This token exposes no compatible text/reasoning models".into());}
  let value=json!({"provider_id":crate::cloud_settings::provider_id(),"provider":provider,"models":models,"provider_models":all.len(),"fetched_at":now});fabric_core::write_json(&file,&value).map_err(|e|e.to_string())?;Ok(value)
 })();
 if !refresh {if let (Err(_),Some(mut cached))=(&result,cached){cached["stale"]=json!(true);return Ok(cached);}}
 result
}
fn capability(id: &str) -> Option<u8> {
    let name = id.to_ascii_lowercase();
    if ["embed", "moderation", "image", "audio", "realtime", "transcrib", "rerank", "tts", "video"]
        .iter().any(|kind| name.contains(kind))
    {
        return None;
    }
    let has = |words: &[&str]| name.split(|c: char| !c.is_ascii_alphanumeric()).any(|part| words.contains(&part));
    if has(&["astra", "opus", "pro"]) || ["o1", "o3"].contains(&name.as_str()) { return Some(4); }
    if has(&["sol", "codex", "sonnet", "reasoner", "thinking"]) { return Some(3); }
    if has(&["nano", "luna", "haiku", "lite"]) { return Some(1); }
    if has(&["mini", "flash", "terra", "instant"]) { return Some(2); }
    if compatible(&name) || has(&["claude", "gemini", "deepseek", "llama", "qwen", "mistral", "grok"]) {
        return Some(3);
    }
    None
}

fn rank(ids: &[String], tier: &str, coding: bool, official: bool) -> Option<String> {
    let desired = match tier { "simple" => 1, "standard" => 2, "expert" => 4, _ => 3 };
    ids.iter().filter_map(|id| {
        if official && !compatible(id) { return None; }
        let level = capability(id)?;
        let distance = if level < desired { (desired - level) * 2 } else { level - desired };
        let snapshot = id.split('-').any(|part| part.len() == 4 && part.starts_with("20") && part.bytes().all(|b| b.is_ascii_digit()));
        let experimental = id.contains("preview") || id.contains("experimental") || id.starts_with("ft:");
        let version = id.strip_prefix("gpt-").unwrap_or("").split('-').next().unwrap_or("");
        let mut parts = version.split('.').filter_map(|part| part.parse::<u16>().ok());
        let version = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
        Some(((distance, experimental, !(coding && id.contains("codex")), snapshot, std::cmp::Reverse(version), id), id))
    }).min_by(|a, b| a.0.cmp(&b.0)).map(|(_, id)| id.clone())
}

pub fn choose(ids: &[String], tier: &str, coding: bool) -> Result<String, String> {
    rank(ids, tier, coding, true).ok_or("No compatible cloud model available".into())
}

pub fn select(question: &str, tier: &str) -> Result<(String, String), String> {
    let pinned = crate::cloud_settings::model();
    if !crate::cloud_settings::automatic() {
        return Ok((pinned, "Pinned cloud model".into()));
    }
    let official = crate::cloud_settings::provider_id() == "openai";
    let (model, reason) = match catalog(false) {
        Ok(value) => {
            let ids = value["models"].as_array().into_iter().flatten()
                .filter_map(|id| id.as_str().map(str::to_owned)).collect::<Vec<_>>();
            match rank(&ids, tier, crate::routing::coding_request(question), official) {
                Some(model) => (model, format!("{tier} task matched to the provider catalog")),
                None => (pinned, "No recognized model profile. Using the saved model.".into()),
            }
        }
        Err(_) => (pinned, "Model discovery unavailable. Using the saved model.".into()),
    };
    let selected = json!({"model":model,"complexity":tier,"reason":reason});
    let _ = fabric_core::write_json(&choice_path(), &selected);
    Ok((model, reason))
}
pub fn last()->Option<String>{std::fs::read(choice_path()).ok().and_then(|d|serde_json::from_slice::<Value>(&d).ok()).and_then(|v|v["model"].as_str().map(str::to_owned))}
#[cfg(test)]mod tests{use super::*;
    #[test]
    fn compatible_providers_rank_text_models_and_ignore_unrelated_endpoints() {
        let ids = ["google/gemini-flash", "anthropic/claude-opus", "anthropic/claude-sonnet",
            "anthropic/claude-haiku", "example/image-pro", "example/embed-pro"].map(str::to_owned);
        assert_eq!(rank(&ids, "simple", false, false).as_deref(), Some("anthropic/claude-haiku"));
        assert_eq!(rank(&ids, "standard", false, false).as_deref(), Some("google/gemini-flash"));
        assert_eq!(rank(&ids, "expert", false, false).as_deref(), Some("anthropic/claude-opus"));
    }
 #[test]fn complexity_changes_the_model_without_a_whitelist(){let ids=["gpt-6-astra","gpt-6.1-sol","gpt-6.1-terra","gpt-6-luna","gpt-image-2","gpt-6-realtime"].map(str::to_owned);assert_eq!(choose(&ids,"simple",false).unwrap(),"gpt-6-luna");assert_eq!(choose(&ids,"standard",false).unwrap(),"gpt-6.1-terra");assert_eq!(choose(&ids,"complex",true).unwrap(),"gpt-6.1-sol");assert_eq!(choose(&ids,"expert",false).unwrap(),"gpt-6-astra");let future=vec!["gpt-7".into()];assert_eq!(choose(&future,"expert",false).unwrap(),"gpt-7");}
}

pub fn comparison(tier:&str)->String {
    if crate::cloud_settings::automatic(){if let Ok(data)=std::fs::read(path()){if let Ok(catalog)=serde_json::from_slice::<Value>(&data){let ids=catalog["models"].as_array().into_iter().flatten().filter_map(|v|v.as_str().map(str::to_owned)).collect::<Vec<_>>();if let Ok(model)=choose(&ids,tier,false){return model;}}}}
    crate::cloud_settings::model()
}
