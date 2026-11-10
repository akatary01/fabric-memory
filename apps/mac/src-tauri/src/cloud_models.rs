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
  let response=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())?.get(endpoint).bearer_auth(token).send().map_err(|_|"Could not discover provider models")?;
  if !response.status().is_success(){return Err(format!("Model discovery returned HTTP {}",response.status().as_u16()));}
  use std::io::Read;let mut bytes=Vec::new();response.take(1048577).read_to_end(&mut bytes).map_err(|e|e.to_string())?;if bytes.len()>1048576{return Err("Model catalog exceeds limit".into());}
  let value:Value=serde_json::from_slice(&bytes).map_err(|_|"Invalid provider model catalog")?;
  let today=time::OffsetDateTime::now_utc().date().to_string();
  let all=value["data"].as_array().ok_or("Missing models")?;
  let mut models=all.iter().filter_map(|v|v["id"].as_str().filter(|id|(!official || compatible(id)) && v["shutdown_date"].as_str().is_none_or(|date|date>today.as_str())).map(str::to_owned)).collect::<Vec<_>>();models.sort();models.dedup();
  if models.is_empty(){return Err("This token exposes no compatible text/reasoning models".into());}
  let value=json!({"provider":provider,"models":models,"provider_models":all.len(),"fetched_at":now});fabric_core::write_json(&file,&value).map_err(|e|e.to_string())?;Ok(value)
 })();
 if !refresh {if let (Err(_),Some(mut cached))=(&result,cached){cached["stale"]=json!(true);return Ok(cached);}}
 result
}
fn score(id:&str,tier:&str,coding:bool)->i64{
 let version=id.strip_prefix("gpt-").unwrap_or("").chars().take_while(|c|c.is_ascii_digit()||*c=='.').collect::<String>();
 let mut parts=version.split('.');let generation=parts.next().and_then(|x|x.parse::<i64>().ok()).unwrap_or(4)*100+parts.next().and_then(|x|x.parse::<i64>().ok()).unwrap_or(0)*10;
 let small=id.contains("mini")||id.contains("nano")||id.contains("luna");let premium=id.contains("pro")||id.contains("astra");
 let fit=match tier{
  "simple"=>if small {600}else{-600},
  "standard"=>if id.contains("terra"){200}else if small{100}else if premium{-100}else{50},
  "expert"=>if premium{300}else if small{-500}else{100},
  _=>if id.contains("sol")|| (coding && id.contains("codex")){250}else if small{-400}else if premium{100}else{150},
 };
 // Prefer portable aliases over date snapshots of the same model.
 generation+fit-if id.split('-').any(|p|p.len()==4 && p.starts_with("20")){2}else{0}
}
pub fn choose(ids:&[String],tier:&str,coding:bool)->Result<String,String>{ids.iter().filter(|id|compatible(id)).max_by_key(|id|score(id,tier,coding)).cloned().ok_or("No compatible cloud model available".into())}
pub fn select(question:&str,tier:&str)->Result<(String,String),String>{
 if !crate::cloud_settings::automatic(){return Ok((crate::cloud_settings::model(),"Pinned cloud model".into()));}
 let value=catalog(false)?;let ids=value["models"].as_array().ok_or("Missing model pool")?.iter().filter_map(|v|v.as_str().map(str::to_owned)).collect::<Vec<_>>();
 let model=if crate::cloud_settings::provider_id()=="openai"{choose(&ids,tier,crate::routing::coding_request(question))?}else{ids.iter().max_by_key(|id|score(id,tier,crate::routing::coding_request(question))).cloned().ok_or("No provider models available")?};
 let reason=format!("{tier} complexity · {model} selected from {} available models",ids.len());
 let selected=json!({"model":model,"complexity":tier,"reason":reason});fabric_core::write_json(&choice_path(),&selected).map_err(|e|e.to_string())?;
 Ok((model,reason))
}
pub fn last()->Option<String>{std::fs::read(choice_path()).ok().and_then(|d|serde_json::from_slice::<Value>(&d).ok()).and_then(|v|v["model"].as_str().map(str::to_owned))}
#[cfg(test)]mod tests{use super::*;
 #[test]fn complexity_changes_the_model_without_a_whitelist(){let ids=["gpt-6-astra","gpt-6.1-sol","gpt-6.1-terra","gpt-6-luna","gpt-image-2","gpt-6-realtime"].map(str::to_owned);assert_eq!(choose(&ids,"simple",false).unwrap(),"gpt-6-luna");assert_eq!(choose(&ids,"standard",false).unwrap(),"gpt-6.1-terra");assert_eq!(choose(&ids,"complex",true).unwrap(),"gpt-6.1-sol");assert_eq!(choose(&ids,"expert",false).unwrap(),"gpt-6-astra");let future=vec!["gpt-7".into()];assert_eq!(choose(&future,"expert",false).unwrap(),"gpt-7");}
}

pub fn comparison(tier:&str)->String {
    if crate::cloud_settings::automatic(){if let Ok(data)=std::fs::read(path()){if let Ok(catalog)=serde_json::from_slice::<Value>(&data){let ids=catalog["models"].as_array().into_iter().flatten().filter_map(|v|v.as_str().map(str::to_owned)).collect::<Vec<_>>();if let Ok(model)=choose(&ids,tier,false){return model;}}}}
    crate::cloud_settings::model()
}
