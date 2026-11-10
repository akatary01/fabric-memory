use crate::routing::{Mode, WebMode};
use fabric_core::{adapter::Adapter, encoder::Encoder, ledger::Ledger};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

pub struct Backend {
    ledger: Mutex<Ledger>,
    agent: crate::agent::Runner,
    mcp: crate::mcp::Host,
    local_beacon: crate::local_beacon::Host,
    workspace: crate::workspace::Host,
    encoder: Mutex<Encoder>,
    adapter: Adapter,
    sync_config: PathBuf,
    syncing: Mutex<()>,
    pub sync_wake: crate::sync::Wake,
    automatic_sync_status: Mutex<String>,
    beacon_connected: AtomicBool,
    chatting: Mutex<()>,
    conversation: Mutex<crate::conversations::Store>,
    conversation_path: PathBuf,
}
impl Backend {
    fn cloud_change_allowed(&self)->Result<(),String>{if self.agent.status()["running"]==true{Err("Wait for the current task to finish or stop it before changing cloud providers or credentials.".into())}else{Ok(())}}
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
        value["beacon_connected"] =
            serde_json::json!(self.beacon_connected.load(Ordering::Relaxed));
        value["usage"] = crate::usage::status();
        Ok(value)
    }
    pub fn open(default_data: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let assets = PathBuf::from(
            std::env::var_os("FABRIC_ASSET_DIR")
                .ok_or("set FABRIC_ASSET_DIR to the prepared assets directory")?,
        );
        let data = std::env::var_os("FABRIC_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or(default_data);
        let sync_config = data.join("Sync/config.json");
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
        let conversation_path = data.join("mac-conversations.json");
        let conversation = crate::conversations::Store::load(
            &conversation_path,
            &data.join("mac-conversation.json"),
        )?;
        crate::usage::seed_history(&conversation);
        let backend=Self {
            ledger: Mutex::new(ledger),
            agent: Default::default(),
            mcp: Default::default(),
            local_beacon: Default::default(),
            workspace: Default::default(),
            encoder: Mutex::new(encoder),
            adapter,
            sync_config,
            syncing: Mutex::new(()),
            sync_wake: crate::sync::Wake::default(),
            automatic_sync_status: Mutex::new("Automatic sync starting…".into()),
            beacon_connected: AtomicBool::new(false),
            chatting: Mutex::new(()),
            conversation: Mutex::new(conversation),
            conversation_path,
        };
        backend.local_beacon.resume(&data.join("Sync/beacon.json"));
        Ok(backend)
    }
    pub fn answer_with_options(
        &self,
        text: String,
        mode: Mode,
        web: WebMode,
    ) -> Result<serde_json::Value, String> {
        let _guard = self.chatting.lock().map_err(failed)?;
        self.agent.start()?;
        let result = self.answer_inner(text.clone(), mode, web);
        let output=match &result {
            Err(error) if !text.trim().is_empty() && text.len()<=4000 => (|| {
                let mut turn=crate::failure::report(error,&self.agent.receipts());
                turn["role"]=serde_json::json!("assistant");turn["access"]=serde_json::json!({"scope":"private"});turn["steps"]=self.agent.receipts();turn["original_prompt"]=serde_json::json!(text);
                let mut store=self.conversation.lock().map_err(failed)?;
                let mut updated=store.clone();let mut history=updated.active().turns.clone();
                history.push(serde_json::json!({"role":"user","content":text,"access":{"scope":"private"}}));history.push(turn.clone());updated.update(history);
                match updated.save(&self.conversation_path){Ok(())=>{*store=updated;Ok(serde_json::json!({"text":turn["content"],"failure":turn["failure"]}))},Err(save_error)=>Err(format!("{error} (Could not save the failure explanation: {save_error})"))}
            })(),_=>result.clone()
        };
        self.agent.finish(&result);
        output
    }
    fn available_tools(
        &self,
        public: bool,
        web: WebMode,
        needs_memory: bool,
        needs_web: bool,
    ) -> Vec<serde_json::Value> {
        use serde_json::json;
        let mut tools = Vec::new();
        if needs_memory {
            tools.push(json!({"name":"memory_search","description":"Quietly recall relevant personal context. Only when earlier personal facts are needed. Optional, at most once per task; returns up to two relevant notes, 800 characters total. Never narrate the lookup.","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}));
        }
        if public && (web == WebMode::On || (web == WebMode::Auto && needs_web)) {
            tools.push(json!({"name":"web_search","description":"Search the internet and retrieve public pages for a query","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}));
        }
        if crate::computer::enabled() {
            tools.push(crate::computer::tool());
        }
        if let Ok(catalog) = self.mcp.request("catalog", json!({})) {
            for server in catalog.as_array().into_iter().flatten() {
                if !public && server["transport"] == "http" {
                    continue;
                }
                let name = server["name"].as_str().unwrap_or("");
                for tool in server["tools"].as_array().into_iter().flatten() {
                    let mut tool = tool.clone();
                    tool["original_name"] = tool["name"].clone();
                    tool["name"] = json!(format!(
                        "mcp::{name}::{}",
                        tool["name"].as_str().unwrap_or("tool")
                    ));
                    tool["server"] = json!(name);
                    tool["transport"] = server["transport"].clone();
                    tool["method"] = json!("tools/call");
                    tools.push(tool);
                }
                for (key, method, param) in [
                    ("resources", "resources/read", "uri"),
                    ("prompts", "prompts/get", "name"),
                ] {
                    for item in server[key].as_array().into_iter().flatten() {
                        let target = item[param].as_str().unwrap_or("");
                        tools.push(json!({"name":format!("mcp::{name}::{method}::{target}"),"description":item["description"].as_str().unwrap_or(target),"server":name,"transport":server["transport"],"method":method,"target":target,"inputSchema":{"type":"object","properties":{"arguments":{"type":"object"}}}}));
                    }
                }
            }
        }
        tools
    }
    fn execute_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
        tools: &[serde_json::Value],
    ) -> Result<serde_json::Value, String> {
        use serde_json::json;
        match name {
            "fabric_docs" => crate::agent_docs::read(args["path"].as_str().ok_or("Missing guide name")?),
            "workspace" | "run_code" => self.workspace.call(name,args),
            "computer" => crate::computer::call(args),
            "memory_search" => {
                let query = args["query"].as_str().ok_or("Missing memory query")?;
                if query.len() > 16000 {
                    return Err("Memory query exceeds limit".into());
                }
                let vector = self
                    .encoder
                    .lock()
                    .map_err(failed)?
                    .encode(query)
                    .map_err(failed)?;
                let hits = self
                    .ledger
                    .lock()
                    .map_err(failed)?
                    .search(&vector, 8)
                    .map_err(failed)?;
                Ok(json!(crate::recall::select(hits)))
            }
            "web_search" => {
                let query = args["query"].as_str().ok_or("Missing web query")?;
                if query.len() > 4000 {
                    return Err("Web query exceeds limit".into());
                }
                if crate::privacy::classify(query).scope != "public" {
                    return Err(
                        "The query contains non-public context; it stays on this Mac.".into(),
                    );
                }
                Ok(json!(crate::web::gather(query)?))
            }
            _ => {
                let tool = tools
                    .iter()
                    .find(|t| t["name"] == name)
                    .ok_or("Unknown tool")?;
                if tool["transport"] == "http"
                    && !args.as_object().is_some_and(|a| a.is_empty())
                    && crate::privacy::classify(&args.to_string()).scope != "public"
                {
                    return Err(
                        "Remote tool arguments contain non-public context; they stay on this Mac."
                            .into(),
                    );
                }
                let method = tool["method"].as_str().ok_or("Unknown MCP method")?;
                let params = match method {
                    "tools/call" => json!({"name":tool["original_name"],"arguments":args}),
                    "resources/read" => json!({"uri":tool["target"]}),
                    "prompts/get" => json!({"name":tool["target"],"arguments":args["arguments"]}),
                    _ => return Err("Unsupported tool method".into()),
                };
                self.mcp.request(
                    "call",
                    json!({"server":tool["server"],"method":method,"params":params}),
                )
            }
        }
    }
    fn answer_inner(
        &self,
        text: String,
        mode: Mode,
        web: WebMode,
    ) -> Result<serde_json::Value, String> {
        if text.trim().is_empty() || text.len() > 4000 {
            return Err("Enter a prompt of 1–4,000 bytes.".into());
        }
        self.agent.step("running","Checking request…",serde_json::json!({"phase":"privacy"}));
        let prompt_access = crate::privacy::classify(&text);
        self.agent.check()?;
        let routing_history=self.conversation.lock().map_err(failed)?.active().turns.clone();
        let computer_task=crate::routing::computer_intent(&text) || (crate::generation::use_history(&text) && crate::routing::context_task(&routing_history).is_some_and(crate::routing::computer_intent));
        if computer_task {crate::computer::begin_task();crate::computer::ensure_ready()?;}
        let computer_consent=computer_task && mode!=Mode::Local && crate::agent_settings::computer_cloud() && !crate::privacy::credentials(&text);
        let cloud_available = (!computer_task || computer_consent) && mode!=Mode::Local && crate::cloud::configured();
        if mode == Mode::Cloud && !cloud_available && (!computer_task || computer_consent) {
            return Err("Add an API token and enable cloud answers in Settings → Cloud.".into());
        }
        self.agent.step("running","Selecting route…",serde_json::json!({"phase":"routing"}));
        let mut plan = if mode == Mode::Auto || web == WebMode::Auto {
            crate::routing::plan(&text,&routing_history)
        } else {
            crate::routing::fallback_context(&text,&routing_history)
        };
        if computer_consent && cloud_available {plan.route=Mode::Cloud;if plan.complexity!="expert"{plan.complexity="complex".into();}}
        let mut decision = if computer_task {crate::routing::decide(if computer_consent && cloud_available{Mode::Cloud}else{Mode::Local},WebMode::Off,&plan,cloud_available)?}else{crate::routing::decide(mode, web, &plan, cloud_available)?};
        if computer_task {decision.reason=if decision.provider==Mode::Cloud{"Cloud plans with approved app observations; Fabric executes controls on this Mac."}else{"Fabric computer controls and planning stay on this Mac."}.into();}
        let coding=crate::routing::coding_request(&text) || (crate::generation::use_history(&text) && self.conversation.lock().map_err(failed)?.active().turns.iter().rev().filter(|t|t["role"]=="user").take(2).any(|t|crate::routing::coding_request(t["content"].as_str().unwrap_or(""))));
        let workspace_prompt_allowed=coding && crate::workspace::settings()["cloud_allowed"]==true && !crate::privacy::credentials(&text);
        if prompt_access.scope != "public" && !workspace_prompt_allowed && !computer_consent {
            if decision.web {
                return Err("Web access was kept off because this request contains private information. Remove the private details before searching.".into());
            }
            if mode == Mode::Cloud && !computer_task {
                return Err(
                    "This prompt is private. Choose Local to keep it on this device."
                        .into(),
                );
            }
            decision.provider = Mode::Local;
            decision.web = false;
            decision.reason = "Memory access policy keeps this task on this Mac.".into();
        }
        if prompt_access.scope!="public" {decision.web=false;}
        if decision.provider==Mode::Cloud {self.agent.step("running","Selecting cloud model…",serde_json::json!({"phase":"model"}));}
        let mut selected_cloud=if decision.provider==Mode::Cloud {let (model,reason)=crate::cloud_models::select(&text,&plan.complexity)?;decision.reason.push_str(&format!(" {reason}."));Some(model)}else{None};
        let mut cloud_agent=computer_consent && decision.provider==Mode::Cloud;
        if coding && !computer_task && decision.provider==Mode::Cloud {
            let workspace=crate::workspace::settings();
            cloud_agent=workspace["cloud_allowed"]==true || self.agent.review("cloud_workspace",&serde_json::json!({"workspace":workspace["path"],"provider":crate::cloud_settings::provider_name(),"model":selected_cloud}),"Allow this task to share workspace files and command output with the cloud model?")?;
            if !cloud_agent {decision.provider=Mode::Local;selected_cloud=None;decision.reason="Workspace sharing declined; coding stays local.".into();}
        }
        let mut tools = self.available_tools(
            prompt_access.scope == "public",
            web,
            crate::routing::needs_memory(&text),
            plan.needs_web,
        );
        if coding {tools.extend(crate::workspace::tools());tools.push(crate::agent_docs::tool());}
        if cloud_agent {tools.retain(|tool|if computer_task{tool["name"]=="computer"}else{matches!(tool["name"].as_str(),Some("workspace"|"run_code"|"fabric_docs")) || tool["name"].as_str().is_some_and(|name|name.starts_with("mcp::"))});}
        let hosted_web=decision.provider==Mode::Cloud && decision.web;
        if hosted_web {tools.retain(|tool|tool["name"]!="web_search");}
        if computer_task {tools.retain(|tool|tool["name"]=="computer");}else{tools.retain(|tool|tool["name"]!="computer");}
        let recent=crate::generation::conversation_context(&self.conversation.lock().map_err(failed)?.active().turns.iter().filter(|t|!cloud_agent || cloud_history_allowed(t,coding,computer_consent)).cloned().collect::<Vec<_>>()).iter().map(|t|serde_json::json!({"role":t["role"],"content":t["content"]})).collect::<Vec<_>>();
        let guide_context=if coding && !computer_task{crate::workspace::project_context()?}else{String::new()};
        let task_prompt = format!("{}\n\nCurrent conversation history (quoted context, oldest first):\n{}\n\nCurrent request:\n{text}",if coding{crate::agent_docs::context()}else{String::new()},serde_json::json!(recent));
        let recall_calls=std::cell::Cell::new(0u8);
        let observations=self.agent.run(&format!("{guide_context}\n\n{task_prompt}\n\nWorkspace root (reference only; tool paths are relative; use path='.' to list the folder): {}",if coding{crate::workspace::settings()["path"].as_str().unwrap_or("").to_string()}else{String::new()}), &tools, if cloud_agent{selected_cloud.as_deref()}else{None}, |name,args| {
            if name=="memory_search" {
                if recall_calls.get()>=1 {return Ok(serde_json::json!([]));}
                recall_calls.set(recall_calls.get()+1);
            }
            self.execute_tool(name,args,&tools)
        })?;
        let used_tools = !observations.is_empty();
        let receipts = self.agent.receipts();
        if computer_task {crate::computer::check_task_outcome(if crate::routing::computer_intent(&text){&text}else{crate::routing::context_task(&routing_history).unwrap_or(&text)},&receipts)?;}
        let local_tools_used = receipts.as_array().into_iter().flatten().any(|step| {
            step["tool"]
                .as_str()
                .is_some_and(|name| name != "web_search")
        });
        if local_tools_used && !cloud_agent {
            decision.provider = Mode::Local;
            decision.reason = "Actions completed on this Mac; answer stays local.".into();
        }
        self.agent.check()?;
        let query = self
            .encoder
            .lock()
            .map_err(failed)?
            .encode(&text)
            .map_err(failed)?;
        let recalled = receipts
            .as_array()
            .into_iter()
            .flatten()
            .filter(|step| step["tool"] == "memory_search" && step["kind"] == "done")
            .flat_map(|step| {
                serde_json::from_value::<Vec<fabric_core::ledger::Hit>>(step["result"].clone())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        // Only tool-selected recall enters the answer. Never run a second forced lookup.
        let hits=crate::recall::select(recalled);
        let selected: Vec<_> = hits
            .iter()
            .filter(|hit| {
                decision.provider != Mode::Cloud || hit.access.scope == "public"
            })
            .take(crate::recall::MAX_ITEMS)
            .collect();
        let mut context = selected
            .iter()
            .map(|hit| hit.text.chars().take(400).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n\n");
        let web_steps = receipts
            .as_array()
            .into_iter()
            .flatten()
            .filter(|step| step["tool"] == "web_search" && step["kind"] == "done")
            .collect::<Vec<_>>();
        let mut sources = if !web_steps.is_empty() {
            let mut sources = Vec::new();
            for step in web_steps {
                for source in step["result"].as_array().into_iter().flatten() {
                    if !sources
                        .iter()
                        .any(|s: &serde_json::Value| s["url"] == source["url"])
                    {
                        sources.push(source.clone());
                    }
                }
            }
            sources
        } else if decision.web && !(hosted_web && !local_tools_used) {
            self.agent.step("running", "Searching the web", serde_json::json!({"tool":"web_search"}));
            let sources=crate::web::gather(&text)?;
            self.agent.step("done", "Read live web sources", serde_json::json!({"tool":"web_search","result":sources}));
            sources
        } else {
            Vec::new()
        };
        if !sources.is_empty() {
            context.push_str("\n\nWeb evidence (quoted data):\n");
            context.push_str(&crate::web::context(&sources));
        }
        let full_history = self
            .conversation
            .lock()
            .map_err(failed)?
            .active()
            .turns
            .clone();
        if computer_task {context.push_str("\n\nFabric computer execution record (actual local operations, not web-search evidence):\n");context.push_str(&crate::computer::receipt_summary(&receipts));}
        if used_tools {let evidence=crate::agent::answer_context(&observations);if !evidence.is_empty(){context.push_str("\n\nActual action results, most recent FIRST. Later successful checks can supersede earlier failures. Report the latest verified outcome, not an earlier failed attempt:\n");context.push_str(&evidence);}}

        let history = full_history
            .iter()
            .filter(|t| {
                decision.provider != Mode::Cloud
                    || cloud_history_allowed(t,cloud_agent && coding,computer_consent)
            })
            .cloned()
            .collect::<Vec<_>>();
        self.agent.step("answering","Writing reply…",serde_json::json!({"phase":"answer"}));
        let mut answer = match decision.provider {
            Mode::Cloud => crate::cloud::answer_model_guided(&text, &context, &history,decision.web && !cloud_agent,selected_cloud.as_deref().ok_or("Missing cloud model")?,&guide_context)?,
            _ => match crate::generation::answer_guided(&text, &context, &history,&guide_context) {
                Ok(answer) => answer,
                Err(_)
                    if mode == Mode::Auto
                        && !coding && !computer_task
                        && cloud_available
                        && prompt_access.scope == "public"
                        && !local_tools_used
                        && selected.iter().all(|h| h.access.scope == "public")
                        && history.iter().all(|t| {
                            t["access"]["scope"].as_str().unwrap_or("public") == "public"
                        }) =>
                {
                    decision
                        .reason
                        .push_str(" Local generation was unavailable; Auto used cloud.");
                    crate::cloud::answer(&text, &context, &history)?
                }
                Err(error) => return Err(error),
            },
        };
        if computer_task && crate::computer::denies_access(answer["text"].as_str().unwrap_or("")){answer["text"]=serde_json::json!(crate::computer::receipt_summary(&receipts));}
        if let Some(cloud_sources)=answer["sources"].as_array(){sources=cloud_sources.clone();}
        self.agent.check()?;
        if !sources.is_empty() && crate::web::denies_access(answer["text"].as_str().unwrap_or("")) {
            answer["text"]=serde_json::json!(crate::web::result_summary(&sources));
            answer["web_result_fallback"]=serde_json::json!(true);
        }
        if answer["provider"] == "local" {
            crate::usage::local_answer(&answer,&crate::cloud_models::comparison(&plan.complexity));
        }
        let response = answer["text"]
            .as_str()
            .ok_or("Missing model answer")?
            .to_string();
        let response_access = crate::privacy::combine(
            crate::privacy::classify(&response),
            &prompt_access,
            &selected.iter().map(|h| &h.access).collect::<Vec<_>>(),
            local_tools_used,
        );
        self.agent.check()?;
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
                .put_with_access(
                    &format!("mac-chat-{id}-user"),
                    &text,
                    Some(query),
                    prompt_access.clone(),
                )
                .map_err(failed)?;
            ledger
                .put_with_access(
                    &format!("mac-chat-{id}-assistant"),
                    &response,
                    Some(response_vector),
                    response_access.clone(),
                )
                .map_err(failed)?;
        }
        self.agent.check()?;
        self.sync_wake.request();
        let mut history = full_history;
        history.push(serde_json::json!({"role":"user","content":text,"access":prompt_access,"workspace":if coding{crate::workspace::settings()["path"].clone()}else{serde_json::Value::Null},"computer_shared":computer_consent && cloud_agent,"cloud_shared":cloud_agent,"cloud_provider":crate::cloud_settings::provider_id()}));
        history.push(serde_json::json!({"role":"assistant","content":response,"access":response_access,"workspace":if coding{crate::workspace::settings()["path"].clone()}else{serde_json::Value::Null},"computer_shared":computer_consent && cloud_agent,"cloud_shared":cloud_agent,"cloud_provider":crate::cloud_settings::provider_id(),"thinking":answer["thinking"],"provider":answer["provider"],"model":answer["model"],"route_reason":decision.reason,"sources":sources,"steps":self.agent.receipts()}));
        let mut store = self.conversation.lock().map_err(failed)?;
        let mut updated = store.clone();
        updated.update(history);
        updated.save(&self.conversation_path)?;
        *store = updated;
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
                Err(error) => format!("Sync waiting: {error}"),
            };
        }
    }
    pub fn sync_once(&self) -> Result<serde_json::Value, String> {
        let _guard = self.syncing.lock().map_err(failed)?;
        let result = self.sync_with_beacon();
        self.beacon_connected
            .store(result.is_ok(), Ordering::Relaxed);
        result
    }
    fn sync_with_beacon(&self) -> Result<serde_json::Value, String> {
        let client = crate::sync::Client::load(&self.sync_config)?;
        let events = self
            .ledger
            .lock()
            .map_err(failed)?
            .events()
            .map_err(failed)?
            .into_iter()
            .filter(|e| e.access.syncable())
            .collect::<Vec<_>>();
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
            let actor = self.ledger.lock().map_err(failed)?.actor.clone();
            let events = events
                .into_iter()
                .filter(|event| client.permits(event, &actor))
                .collect::<Vec<_>>();
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

impl Backend {
    pub fn command(
        &self,
        command: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        use serde_json::json;
        let text = |key: &str| -> Result<&str, String> {
            args[key].as_str().ok_or_else(|| format!("Missing {key}"))
        };
        let beacon = self
            .sync_config
            .parent()
            .ok_or("Missing settings directory")?
            .join("beacon.json");
        match command {
            "clipboard_read" => {
                let output = std::process::Command::new("/usr/bin/pbpaste")
                    .output()
                    .map_err(failed)?;
                if !output.status.success() {
                    return Err("Could not read clipboard".into());
                }
                Ok(json!(String::from_utf8_lossy(&output.stdout)))
            }
            "clipboard_write" => {
                use std::io::Write;
                let mut child = std::process::Command::new("/usr/bin/pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(failed)?;
                child
                    .stdin
                    .take()
                    .ok_or("Clipboard unavailable")?
                    .write_all(text("text")?.as_bytes())
                    .map_err(failed)?;
                if !child.wait().map_err(failed)?.success() {
                    return Err("Could not copy text".into());
                }
                Ok(json!(null))
            }
            "agent_history" => Ok(crate::agent::history()),
            "agent_status" => Ok(self.agent.status()),
            "agent_review" => {
                self.agent.approve(
                    args["id"].as_u64().ok_or("Missing review ID")?,
                    args["allow"].as_bool().ok_or("Missing decision")?,
                )?;
                Ok(json!(null))
            }
            "agent_stop" => {
                self.agent.stop();
                self.mcp.stop();
                self.workspace.stop();
                Ok(json!(null))
            }
            "mcp_catalog" => self.mcp.request("catalog", json!({})),
            "mcp_config" => self.mcp.request("config", json!({})),
            "mcp_save" => self.mcp.request("save", json!({"config":args["config"]})),
            "mcp_restart" => self
                .mcp
                .request("restart", json!({"server":text("server")?})),
            "mcp_call" => self.mcp.request("call", args.clone()),
            "workspace_settings" => Ok(crate::workspace::settings()),
            "workspace_save" => crate::workspace::save(text("path")?,args["cloud_allowed"]==true),
            "computer_status" => crate::computer::status(),
            "computer_enable" => {
                crate::computer::set_enabled(args["enabled"].as_bool().ok_or("Missing setting")?)
            }
            "computer_permissions" => crate::computer::request_permissions(),
            "computer_image" => crate::computer::image(text("path")?),
            "beacon_devices" => crate::beacon::devices(&beacon),
            "cloud_provider_add" => {let _guard=self.chatting.try_lock().map_err(|_|"Stop the current task before changing cloud settings")?;self.cloud_change_allowed()?;crate::cloud_settings::add_provider(text("name")?,text("base_url")?)},
            "cloud_provider_select" => {let _guard=self.chatting.try_lock().map_err(|_|"Stop the current task before changing cloud settings")?;self.cloud_change_allowed()?;crate::cloud_settings::select_provider(text("id")?)},
            "cloud_models" => {let _guard=self.chatting.try_lock().map_err(|_|"Model discovery is available after the current task finishes")?;crate::cloud_models::catalog(args["refresh"]==true)},
            "cloud_keychain_retry" => crate::cloud_settings::retry_keychain(),
            "agent_settings" => Ok(crate::agent_settings::status()),
            "agent_settings_save" => {
                let _guard=if args["computer_cloud"].is_boolean(){if args["provider_id"]!=crate::cloud_settings::provider_id(){return Err("The selected provider changed. Reopen Cloud settings before allowing computer sharing.".into());}Some(self.chatting.try_lock().map_err(|_|"Stop the current task before changing computer sharing")?)}else{None};
                if !args["auto_approve"].is_boolean() && !args["computer_cloud"].is_boolean(){return Err("Missing agent preference".into());}
                crate::agent_settings::save(args["auto_approve"].as_bool(),args["computer_cloud"].as_bool())
            },
            "cloud_settings" => Ok(crate::cloud_settings::status()),
            "cloud_save" => {let _guard=self.chatting.try_lock().map_err(|_|"Stop the current task before changing cloud settings")?;self.cloud_change_allowed()?;crate::cloud_settings::save(
                text("model")?,
                args["enabled"].as_bool().ok_or("Missing cloud setting")?,
                args["token"].as_str().unwrap_or(""),
                args["automatic"].as_bool().unwrap_or(true),
            )},
            "cloud_remove_token" => {let _guard=self.chatting.try_lock().map_err(|_|"Stop the current task before changing cloud settings")?;self.cloud_change_allowed()?;crate::cloud_settings::remove()},
            "model_status" => Ok(crate::model_config::status()),
            "model_load" => crate::model_config::start(text("prompt")?),
            "qr_code" => Ok(json!(crate::qr::svg(text("text")?)?)),
            "open_web_source" => {
                crate::web::open_source(text("url")?)?;
                Ok(json!(null))
            }
            "export_memory_file" => {
                let path = self
                    .conversation_path
                    .parent()
                    .ok_or("Missing app storage")?
                    .join("exchange/outgoing.jsonl");
                std::fs::create_dir_all(path.parent().unwrap()).map_err(failed)?;
                self.ledger
                    .lock()
                    .map_err(failed)?
                    .export(&path)
                    .map_err(failed)?;
                let status = std::process::Command::new("open")
                    .arg("-R")
                    .arg(&path)
                    .status()
                    .map_err(failed)?;
                if !status.success() {
                    return Err("Export saved; Finder could not reveal it".into());
                }
                Ok(json!(null))
            }
            "status" => self.status_json(),
            "conversation" => {
                let store = self.conversation.lock().map_err(failed)?;
                Ok(
                    json!({"id":store.active,"title":store.active().title,"conversations":store.list(),"deleted":store.deleted_list(),"model":crate::generation::model(),"cloud_model":crate::cloud::model(),"cloud_available":crate::cloud::configured(),"turns":store.active().turns}),
                )
            }
            "conversation_create"
            | "conversation_select"
            | "conversation_delete"
            | "conversation_restore"
            | "conversation_rename" => {
                let _guard = self.chatting.lock().map_err(failed)?;
                let mut store = self.conversation.lock().map_err(failed)?;
                let mut updated = store.clone();
                match command {
                    "conversation_create" => updated.create(),
                    "conversation_delete" => updated.delete(text("id")?)?,
                    "conversation_restore" => updated.restore(text("id")?)?,
                    "conversation_rename" => updated.rename(text("id")?, text("title")?)?,
                    _ => updated.select(text("id")?)?,
                }
                updated.save(&self.conversation_path)?;
                *store = updated;
                Ok(json!(null))
            }
            "send_prompt" => self.answer_with_options(
                text("text")?.into(),
                serde_json::from_value(args["mode"].clone()).map_err(failed)?,
                serde_json::from_value(args["web"].clone()).map_err(failed)?,
            ),
            "remember" => {
                let note = text("text")?;
                if note.trim().is_empty() || note.len() > fabric_core::MAX_TEXT_BYTES {
                    return Err("Enter a memory within the size limit".into());
                }
                let access = crate::privacy::classify(note);
                let vector = self
                    .encoder
                    .lock()
                    .map_err(failed)?
                    .encode(note)
                    .map_err(failed)?;
                let event = self
                    .ledger
                    .lock()
                    .map_err(failed)?
                    .put_with_access(text("memory")?, note, Some(vector), access)
                    .map_err(failed)?;
                self.sync_wake.request();
                Ok(json!(event.id))
            }
            "clear_memories" => {
                if args["confirmation"]!="clear-saved-memories" {return Err("Confirm clearing saved memories first.".into());}
                let expected=args["expected_count"].as_u64().ok_or("Review the memory count first")? as usize;
                let _guard=self.chatting.lock().map_err(failed)?;
                let count=self.ledger.lock().map_err(failed)?.clear(expected).map_err(failed)?;
                self.sync_wake.request();
                Ok(json!({"cleared":count}))
            }
            "forget" => {
                self.ledger
                    .lock()
                    .map_err(failed)?
                    .put(text("memory")?, "", None)
                    .map_err(failed)?;
                self.sync_wake.request();
                Ok(json!(null))
            }
            "search" => {
                let note = text("text")?;
                if note.len() > fabric_core::MAX_TEXT_BYTES {
                    return Err("Query exceeds limit".into());
                }
                let vector = self
                    .encoder
                    .lock()
                    .map_err(failed)?
                    .encode(note)
                    .map_err(failed)?;
                serde_json::to_value(
                    self.ledger
                        .lock()
                        .map_err(failed)?
                        .search(&vector, 5)
                        .map_err(failed)?,
                )
                .map_err(failed)
            }
            "list_memories" => {
                let ledger = self.ledger.lock().map_err(failed)?;
                let rows:Vec<_>=ledger.current(200).map_err(failed)?.into_iter().map(|e|json!({"memory":e.memory,"text":e.text,"clock":e.clock,"access":e.access,"translated":e.embedding.as_ref().is_some_and(|v|v.space!=ledger.space)})).collect();
                Ok(json!(rows))
            }
            "import_memories" => {
                let payload = text("payload")?;
                if payload.len() > fabric_core::MAX_TRANSFER_BYTES {
                    return Err("Export exceeds 32 MB".into());
                }
                let events: Vec<fabric_core::Event> =
                    match serde_json::from_str::<serde_json::Value>(payload) {
                        Ok(v) if v.is_array() => serde_json::from_value(v).map_err(failed)?,
                        Ok(v)
                            if v["type"] == "memory-fabric-device-export"
                                && v["schema_version"] == 1 =>
                        {
                            serde_json::from_value(v["events"].clone()).map_err(failed)?
                        }
                        _ => payload
                            .lines()
                            .filter(|l| !l.trim().is_empty())
                            .map(|l| serde_json::from_str(l).map_err(failed))
                            .collect::<Result<_, _>>()?,
                    };
                if events.is_empty() || events.len() > fabric_core::MAX_EVENTS {
                    return Err("Choose 1–10,000 events".into());
                }
                let count = self
                    .ledger
                    .lock()
                    .map_err(failed)?
                    .ingest(&events, Some(&self.adapter))
                    .map_err(failed)?;
                self.sync_wake.request();
                Ok(json!(count))
            }
            "export_events_text" => {
                let events = self
                    .ledger
                    .lock()
                    .map_err(failed)?
                    .events()
                    .map_err(failed)?;
                let lines = events
                    .iter()
                    .map(serde_json::to_string)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(failed)?
                    .join("\n");
                Ok(json!(lines))
            }
            "sync_memories" => self.sync_once(),
            "local_beacon_status" => self.local_beacon.status(),
            "local_beacon_deploy" => {let value=self.local_beacon.deploy(&beacon)?;self.beacon_connected.store(false,Ordering::Relaxed);self.sync_wake.request();Ok(value)},
            "local_beacon_stop" => {let value=self.local_beacon.stop()?;self.beacon_connected.store(false,Ordering::Relaxed);Ok(value)},
            "beacon_settings" => {let mut value=crate::beacon::settings(&beacon);value["local_owned"]=json!(self.local_beacon.owns(&beacon));Ok(value)},
            "beacon_save_url" => crate::beacon::save_address(&beacon, text("url")?),
            "beacon_connect" => {
                let _guard = self.syncing.lock().map_err(failed)?;
                let value = crate::beacon::connect(
                    &beacon,
                    text("url")?,
                    args["code"].as_str().unwrap_or(""),
                    text("name")?,
                )?;
                self.beacon_connected.store(false, Ordering::Relaxed);
                self.sync_wake.request();
                Ok(value)
            }
            "beacon_invite" => crate::beacon::invite(&beacon),
            "beacon_disconnect" => {
                let _guard = self.syncing.lock().map_err(failed)?;
                self.beacon_connected.store(false, Ordering::Relaxed);
                if beacon.exists() {
                    std::fs::remove_file(&beacon).map_err(failed)?;
                }
                Ok(json!(null))
            }
            _ => Err("Unknown app command".into()),
        }
    }
}
fn ort_init(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    fabric_core::encoder::initialize_runtime(path)?;
    Ok(())
}
fn failed(error: impl std::fmt::Display) -> String {
    format!("Could not complete the memory operation: {error}")
}

#[cfg(test)]
mod coding_tests {
    use super::*;
    #[test]
    #[ignore = "uses a real provider token and isolated local model service"]
    fn cloud_coding_fixture_reads_fails_fixes_and_retests(){
        let root=PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").expect("isolated data required"));
        assert!(root.to_string_lossy().contains("coding-check"));
        let project=root.join("project");std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("convert.py"),"def fahrenheit(c):\n    return c + 32\n").unwrap();
        std::fs::write(project.join("test_convert.py"),"import unittest\nfrom convert import fahrenheit\nclass Conversion(unittest.TestCase):\n def test_zero(self): self.assertEqual(fahrenheit(0),32)\n def test_boiling(self): self.assertEqual(fahrenheit(100),212)\n def test_negative(self): self.assertEqual(fahrenheit(-40),-40)\n").unwrap();
        crate::workspace::save(project.to_str().unwrap(),true).unwrap();
        let backend=std::sync::Arc::new(Backend::open(root).unwrap());let worker=backend.clone();
        let thread=std::thread::spawn(move||worker.answer_with_options("Implement the temperature conversion in convert.py. Run the existing tests first with python3 -m unittest -v, inspect the failure, fix convert.py, and rerun until the tests pass. Do not change test_convert.py. Work on the actual files.".into(),Mode::Cloud,WebMode::Off));
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(240);let mut reviewed=0;
        while !thread.is_finished(){
            assert!(std::time::Instant::now()<deadline,"coding fixture timed out");
            let status=backend.agent.status();let pending=&status["pending"];
            if let Some(id)=pending["id"].as_u64(){
                let args=&pending["arguments"];let name=pending["tool"].as_str().unwrap_or("");
                let allowed=(name=="workspace" && args["action"]=="write" && args["path"]=="convert.py") || (name=="run_code" && args["command"].as_str().is_some_and(|command|command.trim()=="python3 -m unittest -v") && args["network"]!=true);
                backend.agent.approve(id,allowed).unwrap();reviewed+=1;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let result=thread.join().unwrap().unwrap();assert_eq!(result["provider"],"cloud");assert!(reviewed>=3);
        let receipts=backend.agent.receipts();let runs=receipts.as_array().unwrap().iter().filter(|s|s["tool"]=="run_code"&&s["kind"]=="done").collect::<Vec<_>>();
        assert!(runs.iter().any(|s|s["result"]["exit_code"].as_i64().is_some_and(|n|n!=0)));assert!(runs.iter().any(|s|s["result"]["exit_code"]==0));
        println!("CODING_FIXTURE {}",serde_json::json!({"model":result["model"],"reviews":reviewed,"terminal_runs":runs.len(),"final_answer":result["text"]}));
    }
}

fn cloud_history_allowed(turn:&serde_json::Value,workspace_consent:bool,computer_consent:bool)->bool {
    turn["access"]["scope"].as_str().unwrap_or("public")=="public" || (workspace_consent && turn["cloud_provider"].as_str().unwrap_or("openai")==crate::cloud_settings::provider_id() && turn["cloud_shared"]==true && turn["workspace"]==crate::workspace::settings()["path"]) || (computer_consent && turn["computer_shared"]==true && turn["cloud_provider"]==crate::cloud_settings::provider_id())
}
