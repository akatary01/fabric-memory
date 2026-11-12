use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
fn state() -> &'static Mutex<Value> {
    static STATE: OnceLock<Mutex<Value>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(json!({"loading":false,"status":"Ready"})))
}
fn path() -> PathBuf {
    PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join("model-config.json")
}
pub fn configured() -> Option<String> {
    std::fs::read(path())
        .ok()
        .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
        .and_then(|v| v["model"].as_str().map(str::to_owned))
}
pub fn status() -> Value {
    let mut s = state().lock().unwrap().clone();
    s["model"] = json!(crate::generation::model());
    s
}
fn valid(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 120
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
        && !model.starts_with(['-', '/', '.'])
        && !model.contains("..")
}
fn parse(text: &str) -> Result<String, String> {
    let cleaned = text.trim().trim_end_matches('.');
    let lower = cleaned.to_lowercase();
    let prefixes = [
        "please load model ",
        "please load ",
        "load model ",
        "load ",
        "use model ",
        "use ",
        "switch to model ",
        "switch to ",
        "install model ",
        "install ",
    ];
    let candidate = prefixes
        .iter()
        .find_map(|p| lower.starts_with(p).then(|| cleaned[p.len()..].trim()))
        .unwrap_or(cleaned);
    if valid(candidate) {
        return Ok(candidate.into());
    }
    Err("Enter a model tag, for example: load llama3.2:3b".into())
}
pub fn start(prompt: &str) -> Result<Value, String> {
    let model = parse(prompt)?;
    let mut s = state().lock().map_err(|_| "Model settings unavailable")?;
    if s["loading"] == true {
        return Err("A model is already loading".into());
    }
    *s = json!({"loading":true,"status":"Starting download…","requested_model":model,"progress":0});
    drop(s);
    std::thread::spawn(move || {
        let result = load(&model);
        let mut s = state().lock().unwrap();
        *s = match result {
            Ok(()) => {
                json!({"loading":false,"status":"Ready","requested_model":model,"progress":100})
            }
            Err(e) => json!({"loading":false,"status":e,"error":true,"requested_model":model}),
        };
    });
    Ok(status())
}
fn load(model: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(4))
        .timeout(std::time::Duration::from_secs(3600))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(crate::generation::endpoint("/api/pull"))
        .json(&json!({"model":model,"stream":true}))
        .send()
        .map_err(|_| "Cannot reach Ollama. Start it, then retry.")?;
    if !response.status().is_success() {
        return Err("Could not download that model. Check its Ollama tag.".into());
    }
    for line in BufReader::new(response).lines() {
        let line = line.map_err(|_| "Model download interrupted")?;
        let value: Value =
            serde_json::from_str(&line).map_err(|_| "Invalid model download response")?;
        if let Some(e) = value["error"].as_str() {
            return Err(e.chars().take(180).collect());
        }
        let total = value["total"].as_f64().unwrap_or(0.);
        let progress = if total > 0. {
            value["completed"].as_f64().unwrap_or(0.) / total * 100.
        } else {
            0.
        };
        *state().lock().unwrap() = json!({"loading":true,"requested_model":model,"status":value["status"],"progress":progress});
    }
    let response: Value = client
        .post(crate::generation::endpoint("/api/show"))
        .json(&json!({"model":model}))
        .send()
        .map_err(|_| "Could not inspect the downloaded model")?
        .json()
        .map_err(|_| "Invalid model response")?;
    if !response["capabilities"]
        .as_array()
        .is_some_and(|v| v.iter().any(|x| x == "completion"))
    {
        return Err("Choose a chat/completion model; this model cannot answer messages.".into());
    }
    fabric_core::write_json(&path(), &json!({"model":model})).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_prompts_never_become_shell_commands() {
        assert_eq!(parse("load llama3.2:3b").unwrap(), "llama3.2:3b");
        assert_eq!(parse("please load qwen3:4b").unwrap(), "qwen3:4b");
        assert!(parse("load x; rm -rf /").is_err());
        assert!(parse("../../private").is_err());
    }
}
pub fn supports_tools() -> bool {
    use std::collections::HashMap;
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let model = crate::generation::model();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(value) = cache.lock().unwrap().get(&model) {
        return *value;
    }
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()
        .and_then(|c| {
            c.post(crate::generation::endpoint("/api/show"))
                .json(&json!({"model":model}))
                .send()
                .ok()
        })
        .and_then(|r| r.json::<Value>().ok());
    let available = response.is_some_and(|v| {
        v["capabilities"]
            .as_array()
            .is_some_and(|items| items.iter().any(|v| v == "tools"))
    });
    cache.lock().unwrap().insert(model, available);
    available
}

fn local_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(3)).build().map_err(|e| e.to_string())
}

pub fn installed() -> Result<Vec<String>, String> {
    let tags: Value = local_client()?.get(crate::generation::endpoint("/api/tags")).send()
        .and_then(|r|r.error_for_status()).and_then(|r|r.json()).map_err(|_|"The local model library is not ready. Retry once Fabric has started.")?;
    let mut names = tags["models"].as_array().into_iter().flatten().filter_map(|m| m["name"].as_str())
        .filter(|name|valid(name)).map(str::to_owned).collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

fn capabilities(model: &str) -> Result<Value, String> {
    let result: Value = local_client()?.post(crate::generation::endpoint("/api/show")).json(&json!({"model":model})).send()
        .and_then(|r|r.error_for_status()).and_then(|r|r.json()).map_err(|_|"Could not inspect that installed model.")?;
    Ok(result["capabilities"].clone())
}

pub fn supports_vision() -> bool {
    capabilities(&crate::generation::model()).is_ok_and(|c| c.as_array().is_some_and(|a| a.iter().any(|v|v=="vision")))
}

pub fn select_installed(model: &str) -> Result<(), String> {
    if !installed()?.iter().any(|name|name==model) || !capabilities(model)?.as_array().is_some_and(|a| a.iter().any(|v|v=="completion")) {
        return Err("Choose an installed chat model.".into());
    }
    fabric_core::write_json(&path(), &json!({"model":model})).map_err(|e|e.to_string())
}
