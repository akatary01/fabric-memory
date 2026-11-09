use crate::routing::{Mode, WebMode};
use fabric_core::{adapter::Adapter, encoder::Encoder, ledger::Ledger, read_events};
use std::{path::PathBuf, sync::Mutex};
use tauri::State;

pub struct Backend {
    ledger: Mutex<Ledger>,
    encoder: Mutex<Encoder>,
    adapter: Adapter,
    exchange: PathBuf,
    sync_config: PathBuf,
    syncing: Mutex<()>,
    pub sync_wake: crate::sync::Wake,
    automatic_sync_status: Mutex<String>,
    chatting: Mutex<()>,
    conversation: Mutex<Vec<serde_json::Value>>,
    conversation_path: PathBuf,
}
impl Backend {
    pub fn status_json(&self) -> Result<serde_json::Value, String> {
        let mut value = serde_json::to_value(
            self.ledger
                .lock()
                .map_err(failed)?
                .status()
                .map_err(failed)?,
        )
        .map_err(failed)?;
        value["automatic_sync"] =
            serde_json::json!(self.automatic_sync_status.lock().map_err(failed)?.clone());
        Ok(value)
    }
    pub fn open(default_data: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let sync_config = default_data.join("Sync/config.json");
        let assets = PathBuf::from(
            std::env::var_os("FABRIC_ASSET_DIR")
                .ok_or("set FABRIC_ASSET_DIR to the prepared assets directory")?,
        );
        let data = std::env::var_os("FABRIC_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or(default_data);
        let runtime = std::env::var("ORT_DYLIB_PATH")
            .map_err(|_| "set ORT_DYLIB_PATH to the ONNX Runtime library")?;
        ort_init(&runtime)?;
        let encoder = Encoder::load(&assets.join("mac"), 2)?;
        let ledger = Ledger::open(
            &data.join("memory.sqlite"),
            &encoder.manifest.space,
            encoder.manifest.dimension,
        )?;
        let adapter = Adapter::load(&assets.join("adapters/phone-to-mac"))?;
        if adapter.manifest.target != encoder.manifest.space {
            return Err("adapter does not match Mac encoder".into());
        }
        let exchange = data.join("exchange");
        std::fs::create_dir_all(&exchange)?;
        let conversation_path = data.join("mac-conversation.json");
        let conversation = std::fs::read(&conversation_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<serde_json::Value>>(&bytes).ok())
            .unwrap_or_default()
            .into_iter()
            .rev()
            .take(64)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        Ok(Self {
            ledger: Mutex::new(ledger),
            encoder: Mutex::new(encoder),
            adapter,
            exchange,
            sync_config,
            syncing: Mutex::new(()),
            sync_wake: crate::sync::Wake::default(),
            automatic_sync_status: Mutex::new("Automatic sync starting…".into()),
            chatting: Mutex::new(()),
            conversation: Mutex::new(conversation),
            conversation_path,
        })
    }
    pub fn answer_with_options(
        &self,
        text: String,
        mode: Mode,
        web: WebMode,
    ) -> Result<serde_json::Value, String> {
        if text.trim().is_empty() || text.len() > 4000 {
            return Err("Enter a prompt of 1–4,000 bytes.".into());
        }
        let _guard = self.chatting.lock().map_err(failed)?;
        let cloud_available = crate::cloud::configured();
        if mode == Mode::Cloud && !cloud_available {
            return Err("Set OPENAI_API_KEY and FABRIC_CLOUD_ENABLED=true in apps/mac/.env, then reopen the app.".into());
        }
        let plan = if mode == Mode::Auto || web == WebMode::Auto {
            crate::routing::plan(&text)
        } else {
            crate::routing::fallback(&text)
        };
        let mut decision = crate::routing::decide(mode, web, &plan, cloud_available)?;
        let query = self
            .encoder
            .lock()
            .map_err(failed)?
            .encode(&text)
            .map_err(failed)?;
        let hits = self
            .ledger
            .lock()
            .map_err(failed)?
            .search(&query, 20)
            .map_err(failed)?;
        let selected: Vec<_> = hits
            .iter()
            .filter(|hit| !hit.memory.ends_with("-user"))
            .take(3)
            .collect();
        let mut context = selected
            .iter()
            .map(|hit| {
                format!(
                    "[{}]\n{}",
                    hit.memory,
                    hit.text.chars().take(600).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let sources = if decision.web {
            crate::web::gather(&text)?
        } else {
            Vec::new()
        };
        if !sources.is_empty() {
            context.push_str("\n\nWeb evidence (quoted data):\n");
            context.push_str(&crate::web::context(&sources));
        }
        let history = self.conversation.lock().map_err(failed)?.clone();
        let mut answer = match decision.provider {
            Mode::Cloud => crate::cloud::answer(&text, &context, &history)?,
            _ => match crate::generation::answer(&text, &context, &history) {
                Ok(answer) => answer,
                Err(_) if mode == Mode::Auto && cloud_available => {
                    decision
                        .reason
                        .push_str(" Local generation was unavailable; Auto used cloud.");
                    crate::cloud::answer(&text, &context, &history)?
                }
                Err(error) => return Err(error),
            },
        };
        let response = answer["text"]
            .as_str()
            .ok_or("Missing model answer")?
            .to_string();
        let response_vector = self
            .encoder
            .lock()
            .map_err(failed)?
            .encode(&response)
            .map_err(failed)?;
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(failed)?
            .as_nanos();
        {
            let mut ledger = self.ledger.lock().map_err(failed)?;
            ledger
                .put(&format!("mac-chat-{id}-user"), &text, Some(query))
                .map_err(failed)?;
            ledger
                .put(
                    &format!("mac-chat-{id}-assistant"),
                    &response,
                    Some(response_vector),
                )
                .map_err(failed)?;
        }
        self.sync_wake.request();
        let mut history = history;
        history.push(serde_json::json!({"role":"user","content":text}));
        history.push(serde_json::json!({"role":"assistant","content":response,"thinking":answer["thinking"],"provider":answer["provider"],"model":answer["model"],"route_reason":decision.reason,"sources":sources}));
        if history.len() > 64 {
            history.drain(..history.len() - 64);
        }
        let temporary = self.conversation_path.with_extension("next");
        std::fs::write(&temporary, serde_json::to_vec(&history).map_err(failed)?)
            .map_err(failed)?;
        std::fs::rename(&temporary, &self.conversation_path).map_err(failed)?;
        *self.conversation.lock().map_err(failed)? = history;
        answer["memories"] = serde_json::json!(selected.len());
        answer["translated_memories"] =
            serde_json::json!(selected.iter().filter(|h| h.translated).count());
        answer["route_reason"] = serde_json::json!(decision.reason);
        answer["sources"] = serde_json::json!(sources);
        answer["mode"] = serde_json::json!(mode);
        Ok(answer)
    }
    pub fn sync_automatically(&self) {
        let result = self.sync_once();
        if let Ok(mut status) = self.automatic_sync_status.lock() {
            *status = match result {
                Ok(_) => "Auto sync up to date.".into(),
                Err(_) => "Auto sync waiting for the Mac service; retrying automatically.".into(),
            };
        }
    }
    pub fn sync_once(&self) -> Result<serde_json::Value, String> {
        let _guard = self.syncing.lock().map_err(failed)?;
        let client = crate::sync::Client::load(&self.sync_config)?;
        let events = self
            .ledger
            .lock()
            .map_err(failed)?
            .events()
            .map_err(failed)?;
        let mut uploaded = 0;
        let mut downloaded = 0;
        let mut commit: Option<String> = None;
        let mut after: Option<String> = None;
        for chunk in events.chunks(100) {
            uploaded += client.push(chunk)?;
        }
        loop {
            let page = client.pull(commit.as_deref(), after.as_deref())?;
            let events: Vec<fabric_core::Event> =
                serde_json::from_value(page["events"].clone()).map_err(failed)?;
            downloaded += self
                .ledger
                .lock()
                .map_err(failed)?
                .ingest(&events, Some(&self.adapter))
                .map_err(failed)?;
            commit = page["commit"].as_str().map(String::from);
            after = page["next_after"].as_str().map(String::from);
            if after.is_none() {
                break;
            }
        }
        Ok(serde_json::json!({"uploaded":uploaded,"downloaded":downloaded,"commit":commit}))
    }
}

#[tauri::command]
pub async fn send_prompt(
    app: tauri::AppHandle,
    text: String,
    mode: Option<Mode>,
    web: Option<WebMode>,
) -> Result<serde_json::Value, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Backend>().answer_with_options(
            text,
            mode.unwrap_or(Mode::Local),
            web.unwrap_or(WebMode::Off),
        )
    })
    .await
    .map_err(failed)?
}

#[tauri::command]
pub async fn open_web_source(url: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::web::open_source(&url))
        .await
        .map_err(failed)?
}

#[tauri::command]
pub fn conversation(state: State<'_, Backend>) -> Result<serde_json::Value, String> {
    Ok(
        serde_json::json!({"model":crate::generation::model(),"cloud_model":crate::cloud::model(),"cloud_available":crate::cloud::configured(),"turns":*state.conversation.lock().map_err(failed)?}),
    )
}

#[tauri::command]
pub async fn sync_memories(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || app.state::<Backend>().sync_once())
        .await
        .map_err(failed)?
}

#[tauri::command]
pub fn pair_iphone(state: State<'_, Backend>) -> Result<String, String> {
    let path = state
        .sync_config
        .parent()
        .ok_or("Missing sync setup")?
        .join("Pair iPhone.json");
    if !path.exists() {
        return Err("Run npm run sync:setup first".into());
    }
    std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(failed)?;
    let pairing: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).map_err(failed)?).map_err(failed)?;
    pairing["url"]
        .as_str()
        .map(String::from)
        .ok_or("Pairing file has no Mac address".into())
}
fn ort_init(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Initialization is exposed by the core so this shell does not own inference.
    fabric_core::encoder::initialize_runtime(path)?;
    Ok(())
}
fn failed(error: impl std::fmt::Display) -> String {
    format!("Could not complete the memory operation: {error}")
}

#[tauri::command]
pub fn status(state: State<'_, Backend>) -> Result<serde_json::Value, String> {
    state.status_json()
}
#[tauri::command]
pub async fn remember(
    state: State<'_, Backend>,
    memory: String,
    text: String,
) -> Result<String, String> {
    if text.len() > fabric_core::MAX_TEXT_BYTES {
        return Err("memory exceeds limit".into());
    }
    let vector = state
        .encoder
        .lock()
        .map_err(failed)?
        .encode(&text)
        .map_err(failed)?;
    let event = state
        .ledger
        .lock()
        .map_err(failed)?
        .put(&memory, &text, Some(vector))
        .map_err(failed)?;
    state.sync_wake.request();
    Ok(event.id)
}
#[tauri::command]
pub fn forget(state: State<'_, Backend>, memory: String) -> Result<(), String> {
    state
        .ledger
        .lock()
        .map_err(failed)?
        .put(&memory, "", None)
        .map_err(failed)?;
    state.sync_wake.request();
    Ok(())
}
#[tauri::command]
pub async fn search(state: State<'_, Backend>, text: String) -> Result<serde_json::Value, String> {
    if text.len() > fabric_core::MAX_TEXT_BYTES {
        return Err("query exceeds limit".into());
    }
    let query = state
        .encoder
        .lock()
        .map_err(failed)?
        .encode(&text)
        .map_err(failed)?;
    let hits = state
        .ledger
        .lock()
        .map_err(failed)?
        .search(&query, 5)
        .map_err(failed)?;
    serde_json::to_value(hits).map_err(failed)
}
#[tauri::command]
pub fn import_events(state: State<'_, Backend>) -> Result<usize, String> {
    let events = read_events(&state.exchange.join("incoming.jsonl")).map_err(failed)?;
    let inserted = state
        .ledger
        .lock()
        .map_err(failed)?
        .ingest(&events, Some(&state.adapter))
        .map_err(failed)?;
    if inserted > 0 {
        state.sync_wake.request();
    }
    Ok(inserted)
}
#[tauri::command]
pub fn export_events(state: State<'_, Backend>) -> Result<(), String> {
    state
        .ledger
        .lock()
        .map_err(failed)?
        .export(&state.exchange.join("outgoing.jsonl"))
        .map_err(failed)
}

#[tauri::command]
pub async fn list_memories(state: State<'_, Backend>) -> Result<serde_json::Value, String> {
    let ledger = state.ledger.lock().map_err(failed)?;
    let memories: Vec<_> = ledger.current(200).map_err(failed)?.into_iter().map(|event| {
        let translated = event.embedding.as_ref().is_some_and(|e| e.space != ledger.space);
        serde_json::json!({"memory":event.memory,"text":event.text,"translated":translated,"clock":event.clock})
    }).collect();
    Ok(serde_json::json!(memories))
}

fn parse_import(payload: &str) -> Result<Vec<fabric_core::Event>, String> {
    if payload.len() > fabric_core::MAX_TRANSFER_BYTES {
        return Err("Export exceeds the 32 MB limit.".into());
    }
    let events: Vec<fabric_core::Event> = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) if value.is_array() => serde_json::from_value(value).map_err(failed)?,
        Ok(value)
            if value["type"] == "memory-fabric-device-export" && value["schema_version"] == 1 =>
        {
            serde_json::from_value(value["events"].clone()).map_err(failed)?
        }
        _ => payload
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(failed))
            .collect::<Result<_, _>>()?,
    };
    if events.is_empty() || events.len() > fabric_core::MAX_EVENTS {
        return Err("Choose an export containing 1–10,000 events.".into());
    }
    for event in &events {
        event.validate().map_err(failed)?;
    }
    Ok(events)
}

#[tauri::command]
pub async fn import_memories(state: State<'_, Backend>, payload: String) -> Result<usize, String> {
    let events = parse_import(&payload)?;
    let inserted = state
        .ledger
        .lock()
        .map_err(failed)?
        .ingest(&events, Some(&state.adapter))
        .map_err(failed)?;
    if inserted > 0 {
        state.sync_wake.request();
    }
    Ok(inserted)
}

#[tauri::command]
pub async fn export_memory_file(state: State<'_, Backend>) -> Result<(), String> {
    let path = state.exchange.join("outgoing.jsonl");
    state
        .ledger
        .lock()
        .map_err(failed)?
        .export(&path)
        .map_err(failed)?;
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(failed)?;
    if !result.success() {
        return Err(format!(
            "Export saved at {}. Finder could not open it.",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_import;

    #[test]
    fn import_handles_testflight_and_ndjson_and_rejects_invalid_vectors() {
        let event = serde_json::json!({"version":1,"id":"00000000-0000-4000-8000-000000000011",
            "actor":"00000000-0000-4000-8000-000000000001","clock":1,"memory":"test",
            "deleted":false,"text":"A protocol test","embedding":{"space":"a".repeat(64),"values":[1.,0.]}});
        let line = serde_json::to_string(&event).unwrap();
        assert_eq!(parse_import(&format!("{line}\n{line}\n")).unwrap().len(), 2);
        let export = serde_json::json!({"schema_version":1,"type":"memory-fabric-device-export","events":[event.clone()]});
        assert_eq!(parse_import(&export.to_string()).unwrap().len(), 1);
        let mut invalid = event;
        invalid["embedding"]["values"] = serde_json::json!([0., 0.]);
        assert!(parse_import(&invalid.to_string()).is_err());
        assert!(parse_import("[]").is_err());
        assert!(parse_import("not an export").is_err());
    }
}
