use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
fn last_target()-> &'static std::sync::Mutex<Option<String>>{static TARGET:std::sync::OnceLock<std::sync::Mutex<Option<String>>>=std::sync::OnceLock::new();TARGET.get_or_init(||std::sync::Mutex::new(None))}
pub fn begin_task(){if let Ok(mut target)=last_target().lock(){*target=None;}}
fn bind_target(args:&mut Value,last:Option<&str>)->Result<(),String>{let action=args["action"].as_str().unwrap_or("");let needs_target=matches!(action,"click"|"type"|"key"|"scroll");if args["app"].as_str().is_none_or(|s|s.is_empty()) && (needs_target||action=="observe"){if let Some(target)=last{args["app"]=json!(target);}else if needs_target{return Err("Computer input needs a target app. Resolve/open or observe the intended app first; input was not sent to the frontmost app.".into());}}Ok(())}
fn config() -> PathBuf {
    PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join("computer-use.json")
}
fn settings() -> Value {
    std::fs::read(config()).ok().and_then(|v|serde_json::from_slice::<Value>(&v).ok()).filter(Value::is_object).unwrap_or(json!({}))
}
pub fn enabled() -> bool { settings()["enabled"] == true }
pub fn screen_sharing() -> bool { settings()["screen_sharing"] == true }
pub fn save_access(control: bool, screen: bool) -> Result<(),String> {
    fabric_core::write_json(&config(), &json!({"enabled":control,"screen_sharing":screen})).map_err(|e|e.to_string())
}
pub fn set_enabled(value: bool) -> Result<Value, String> {
    save_access(value,screen_sharing())?;
    status()
}
fn helper(args: &Value) -> Result<Value, String> {
    let binary = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Missing app bundle")?
        .join("fabric-computer-use");
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Rebuild the Mac app to install its computer-use helper")?;
    child
        .stdin
        .take()
        .ok_or("Computer helper unavailable")?
        .write_all(args.to_string().as_bytes())
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "Computer helper returned no result")?;
    if let Some(error) = value["error"].as_str() {
        return Err(error.into());
    }
    Ok(value["result"].clone())
}
pub fn vision_image(bytes: &[u8]) -> Result<String, String> {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    helper(&json!({"action":"resize_vision_image","data":data}))?["image"]
        .as_str().map(str::to_owned).ok_or_else(|| "The vision screenshot could not be prepared.".into())
}

pub fn pdf_text(data: &str) -> Result<String,String> {
    helper(&json!({"action":"extract_pdf_text","data":data}))?["text"].as_str().map(str::to_owned).ok_or("Could not read PDF text.".into())
}
pub fn device_usage() -> Result<Value, String> {
    helper(&json!({"action":"device_usage"}))
}

pub fn status() -> Result<Value, String> {
    let mut value = helper(&json!({"action":"permissions"}))
        .unwrap_or(json!({"accessibility":false,"screen_recording":false}));
    value["enabled"] = json!(enabled());
    value["screen_sharing"] = json!(screen_sharing());
    Ok(value)
}
pub fn ensure_ready()->Result<(),String>{
 if !enabled(){return Err("Computer use is off. Enable it in Connections → Computer use.".into());}
 let state=helper(&json!({"action":"permissions"})).map_err(|e|format!("Computer controller unavailable: {e}"))?;
 if state["accessibility"]!=true{return Err("Computer control needs macOS Accessibility access for fabric-memory. Open Connections → macOS access, enable fabric-memory under Accessibility, then tell me to continue. Auto-approval cannot grant macOS permissions.".into());}Ok(())
}
pub fn request_permissions(control: bool, screen: bool) -> Result<Value, String> {
    helper(&json!({"action":"request_permissions","control":control,"screen":screen}))
}
pub fn capture() -> Result<Value, String> {
    if !screen_sharing() { return Err("Enable Screen sharing in Settings → Access & local models.".into()); }
    helper(&json!({"action":"screenshot"}))
}
pub fn preview() -> Result<Vec<u8>, String> {
    let captured = capture()?;
    let file = captured["image_path"].as_str().ok_or("No screenshot was captured")?;
    let result = image_bytes(file);
    let _ = std::fs::remove_file(file);
    result
}
pub fn trash_item(path: &std::path::Path) -> Result<Value, String> {
    helper(&json!({"action":"trash_workspace_item","path":path}))
}
pub fn call(args: &Value) -> Result<Value, String> {
    let mut args=normalize_action(args)?;let target=last_target().lock().map_err(|_|"Computer target unavailable")?.clone();bind_target(&mut args,target.as_deref())?;let args=&args;
    if !enabled() {
        return Err("Enable Computer use in Connections first.".into());
    }
    if (args["action"] == "screenshot" || args.get("screen_x").is_some() || args.get("screen_y").is_some()) && !screen_sharing() { return Err("Enable Screen sharing in Settings → Access & local models.".into()); }
    if ![
        "observe",
        "resolve_app",
        "open_url",
        "search_browser",
        "open_app",
        "screenshot",
        "click",
        "type",
        "key",
        "scroll",
    ]
    .contains(&args["action"].as_str().unwrap_or(""))
    {
        return Err(format!("Unsupported computer action: {}. Use observe, resolve_app, open_app, open_url, search_browser, click, type, key, scroll or screenshot.",args["action"].as_str().unwrap_or("missing")));
    }
    let mut result=helper(args)?;if let Some(app)=result["app"].as_str(){*last_target().lock().map_err(|_|"Computer target unavailable")?=Some(app.into());}result["executed_action"]=args["action"].clone();Ok(result)
}
pub fn image(path: &str) -> Result<Value, String> {
    if !screen_sharing() { return Err("Screen sharing is off.".into()); }
    Ok(json!(image_bytes(path)?))
}
pub fn image_bytes(path: &str) -> Result<Vec<u8>, String> {
    let root = PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").ok_or("Missing app storage")?)
        .join("Captures")
        .canonicalize()
        .map_err(|_| "No captures")?;
    let file = PathBuf::from(path)
        .canonicalize()
        .map_err(|_| "Capture not found")?;
    if !file.starts_with(root)
        || file.extension().and_then(|s| s.to_str()) != Some("png")
        || file.metadata().map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024
    {
        return Err("Invalid capture path".into());
    }
    std::fs::read(file).map_err(|e| e.to_string())
}
pub fn recording_requested(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("record") && ["screen", "display", "video", "audio"].iter().any(|word| text.contains(word))
}

pub fn record(args: &Value, check: impl Fn() -> Result<(), String>) -> Result<Value, String> {
    if !enabled() || !screen_sharing() {
        return Err("Enable Computer use and Screen sharing before recording.".into());
    }
    if status()?["screen_recording"] != true {
        return Err("Allow Screen Recording for fabric-memory in macOS System Settings.".into());
    }
    let seconds = args["seconds"].as_u64().filter(|seconds| (1..=60).contains(seconds))
        .ok_or("Choose a recording duration from 1 to 60 seconds.")?;
    let audio = args["audio"].as_str().unwrap_or("none");
    let audio_flag = match audio {
        "none" => None, "microphone" => Some("-g"), "system" => Some("-A"),
        _ => return Err("Choose none, microphone, or system audio.".into()),
    };
    let workspace = crate::workspace::settings();
    if workspace["enabled"] != true {
        return Err("Enable workspace access to save a recording.".into());
    }
    let root = PathBuf::from(workspace["path"].as_str().ok_or("Select a workspace first.")?);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_millis();
    let folder = root.join(format!("screen-recording-{stamp}"));
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(0o700).create(&folder).map_err(|e| e.to_string())?;
    let output = folder.join("screen.mov");
    let result = (|| {
        check()?;
        let timeout = crate::agent::bounded_timeout(seconds + 10)?;
        if timeout.as_secs() <= seconds { return Err("The task has too little time left to record.".into()); }
        let mut command = Command::new("/usr/sbin/screencapture");
        command.args(["-v", "-x", "-D", "1", "-V", &seconds.to_string()]);
        if let Some(flag) = audio_flag { command.arg(flag); }
        let mut child = command.arg(&output).stdin(Stdio::null()).stdout(Stdio::null())
            .stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
        let started = std::time::Instant::now();
        loop {
            let poll = check().and_then(|_| {
                if started.elapsed() >= timeout { return Err("Screen recording timed out.".into()); }
                child.try_wait().map_err(|e| e.to_string())
            });
            match poll {
                Ok(Some(status)) if status.success() => break,
                Ok(Some(_)) => return Err("macOS could not record the screen. Check screen and audio permissions.".into()),
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            }
        }
        let tracks = helper(&json!({"action":"recording_info","path":output}))?;
        if tracks["video"] != true || (audio_flag.is_some() && tracks["audio"] != true) {
            return Err("The recording is missing a requested video or audio track. Check macOS permissions and the selected audio input.".into());
        }
        Ok(json!({"executed_action":"record","path":output,"seconds":seconds,"audio":audio,"video":true}))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&output);
        let _ = std::fs::remove_dir(&folder);
    }
    result
}

pub fn tool() -> Value {
    json!({"name":"computer","description":"Control this Mac. resolve_app accepts an installed app name or bundle ID; app=browser or web resolves the default browser. open_app accepts those same names. search_browser opens a Google query in the actual default browser (query parameter); open_url navigates an HTTP/HTTPS URL. To target Chrome specifically, pass app=chrome and new_tab=true. Set new_tab=true on each search_browser or open_url call when the user asks for separate tabs. Use a distinct query or URL per requested tab. Navigation dispatch does not confirm page load; observe the resolved browser to verify. Observe the window after navigation to verify the page. First observe before clicking or typing; observations return element paths. click with element plus expected label, or screen_x/screen_y (0 to 1 from the top-left of the main display screenshot); type into a focused field; key with Cmd+A etc; scroll; open_app by bundle ID. screenshot captures the screen for user review. For an explicit recording request, record the main display for 1–60 seconds with audio none, microphone, or system. Requires macOS permissions.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["observe","resolve_app","open_url","search_browser","click","type","key","scroll","open_app","screenshot","record"]},"seconds":{"type":"integer","minimum":1,"maximum":60},"audio":{"type":"string","enum":["none","microphone","system"]},"app":{"type":"string"},"url":{"type":"string"},"query":{"type":"string"},"new_tab":{"type":"boolean"},"element":{"type":"string"},"expected":{"type":"string"},"screen_x":{"type":"number","minimum":0,"maximum":1},"screen_y":{"type":"number","minimum":0,"maximum":1},"text":{"type":"string"},"key":{"type":"string"},"pixels":{"type":"integer"}},"required":["action"]}})
}

pub fn search_tab_count(question: &str) -> Option<usize> {
    let text = question.to_lowercase();
    if ["find ", "search ", "look up "].iter().any(|phrase| text.contains(phrase))
        && !["then ", "download", "submit", "copy ", "summarize"].iter().any(|phrase| text.contains(phrase)) {
        requested_tab_count(&text).filter(|count| *count <= 10)
    } else {
        None
    }
}

pub fn search_tabs_tool(count: usize) -> Value {
    json!({
        "name": "computer",
        "description": format!("Open exactly {count} separate browser search tabs and observe the browser. Provide one distinct search query per requested tab in order. When tabs refer to the same subject, use the same specific subject in each query."),
        "inputSchema": {"type":"object","additionalProperties":false,
            "required":["action","queries"], "properties":{
                "action":{"const":"search_tabs"},
                "app":{"type":"string","description":"Browser to use, such as chrome or browser for the default."},
                "queries":{"type":"array","minItems":count,"maxItems":count,"uniqueItems":true,
                    "items":{"type":"string","minLength":1,"maxLength":2000}}
            }
        }
    })
}

pub fn search_tabs(
    args: &Value,
    count: usize,
    check: impl Fn() -> Result<(), String>,
    record: impl Fn(&str, &str, Value),
) -> Result<Value, String> {
    let queries = args["queries"].as_array().filter(|items| items.len() == count)
        .ok_or_else(|| format!("Provide exactly {count} search queries before opening any tabs."))?;
    let mut unique = std::collections::HashSet::new();
    let mut searches = Vec::with_capacity(count);
    for query in queries {
        let query = query.as_str().unwrap_or("").trim();
        if query.is_empty() || query.len() > 2000 || !unique.insert(query.to_lowercase()) {
            return Err("Provide distinct nonempty search queries up to 2,000 bytes each.".into());
        }
        searches.push(query);
    }
    let browser = args["app"].as_str().unwrap_or("browser");
    let mut tabs = Vec::with_capacity(count);
    for query in searches {
        check()?;
        record("running", "Dispatching a browser search in a new tab", json!({"action":"search_browser"}));
        let result = call(&json!({"action":"search_browser","new_tab":true,"query":query,"app":browser}))
            .map_err(|error| format!("Opened {} of {count} requested search tabs. {error}",tabs.len()))?;
        record("done", "Dispatched a browser search", json!({"action":"search_browser","result":result}));
        tabs.push(result);
    }
    check()?;
    let app = tabs.last().and_then(|result| result["app"].as_str()).ok_or("Could not identify the browser after opening the tabs.")?;
    record("running", "Observing the browser after opening the tabs", json!({"action":"observe"}));
    let observed = call(&json!({"action":"observe","app":app}))
        .map_err(|error| format!("Dispatched {count} tab navigations, but browser observation failed. {error}"))?;
    if observed["app"] != app || !observed["window_title"].as_str().is_some_and(|title| !title.is_empty()) {
        return Err(format!("Dispatched {count} tab navigations, but could not observe a browser window."));
    }
    record("done", "Observed the browser", json!({"action":"observe","result":{
        "app":app,"window_title":observed["window_title"]
    }}));
    Ok(json!({"executed_action":"search_tabs","tabs":tabs,"app":app,"window_title":observed["window_title"]}))
}

/// A model must not replace a failed or skipped computer task with generic web evidence.
pub fn check_outcome(steps:&Value)->Result<(),String>{
 let calls=steps.as_array().into_iter().flatten().filter(|s|s["tool"]=="computer" && matches!(s["kind"].as_str(),Some("done"|"error"|"denied"))).collect::<Vec<_>>();
 let Some(last)=calls.last() else{return Err("Computer use is available, but the selected local model ended without attempting the requested action. Try a stronger tool-capable local model in Settings → General, or give me the target browser name.".into());};
 if last["kind"]!="done" {return Err(format!("Computer action did not finish: {}. Completed actions are retained. Check Connections → Computer use and macOS access, or tell me the correct app name so I can continue.",last["summary"].as_str().unwrap_or("Action declined")));}
 if !calls.iter().any(|s|s["kind"]=="done" && effective_action(s)!="resolve_app"){return Err("The app was identified, but the requested computer action was not performed. Tell me which installed browser to use, or choose a stronger local model.".into());}
 Ok(())
}
fn requested_tab_count(question: &str) -> Option<usize> {
    let words: Vec<_> = question.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty()).collect();
    for (index, word) in words.iter().enumerate() {
        if *word != "tabs" || index == 0 { continue; }
        let previous = if words[index - 1] == "browser" {
            index.checked_sub(2).map(|i| words[i])
        } else { Some(words[index - 1]) };
        let Some(number) = previous else { continue; };
        let count = match number {
            "two" => 2, "three" => 3, "four" => 4, "five" => 5,
            number => number.parse::<usize>().ok().unwrap_or_default(),
        };
        if count > 1 { return Some(count); }
    }
    None
}

pub fn explicit_navigation(question: &str) -> Option<reqwest::Url> {
    let lower = question.to_ascii_lowercase();
    let offset = ["navigate to ", "go to ", "visit "].iter()
        .filter_map(|phrase| lower.find(phrase).map(|index| index + phrase.len())).min()?;
    let target = question.get(offset..)?.split_whitespace().next()?.trim_matches(|c| matches!(c, ',' | '.' | '"' | '\'' | '`' | ')' | ';'));
    if !target.contains('.') || (target.contains("://") && !target.to_ascii_lowercase().starts_with("http")) { return None; }
    let address = if target.starts_with("https://") || target.starts_with("http://") {
        target.to_owned()
    } else {
        format!("https://{target}")
    };
    reqwest::Url::parse(&address).ok().filter(|url|
        matches!(url.scheme(), "https" | "http") && url.host_str().is_some() && url.username().is_empty() && url.password().is_none())
}

pub fn named_browser_link(question: &str) -> Option<String> {
    let text = question.to_lowercase();
    let label = text.split_once("click the ")?.1.split_once(" link")?.0.trim();
    if label.is_empty() { None } else { Some(label.to_owned()) }
}

pub fn navigation_key(args: &Value) -> Option<String> {
    match args["action"].as_str()? {
        "search_browser" => {
            let query = args["query"].as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
            Some(format!("search:{}", query.to_lowercase()))
        }
        "open_url" | "navigate" | "navigate_url" => {
            let mut url = reqwest::Url::parse(args["url"].as_str()?).ok()?;
            url.set_fragment(None);
            Some(format!("url:{url}"))
        }
        _ => None,
    }
}

fn failed_page_title(title: &str) -> bool {
    let title = title.to_ascii_lowercase();
    ["untitled", "page not found", "404 not found", "site can’t be reached", "site can't be reached",
     "just a moment", "problem loading page", "about:blank"]
        .iter().any(|marker| title.contains(marker))
}

pub fn observed_page(result: &Value, expected: &str) -> bool {
    let title = result["window_title"].as_str().unwrap_or("");
    if title.is_empty() || failed_page_title(title) { return false; }
    let Some(actual) = result["window"]["address"].as_str() else { return false; };
    let normalize = |value: &str| {
        let url = reqwest::Url::parse(value).ok()?;
        Some((url.scheme().to_owned(), url.host_str()?.trim_start_matches("www.").to_owned(), url.port_or_known_default(),
            url.path().trim_end_matches('/').to_owned(), url.query().map(str::to_owned)))
    };
    normalize(expected).is_some_and(|target| normalize(actual) == Some(target))
}

pub fn check_task_outcome(question:&str,steps:&Value)->Result<(),String>{
 if recording_requested(question) && !steps.as_array().into_iter().flatten().any(|step|
     step["tool"] == "computer" && step["kind"] == "done" && step["result"]["executed_action"] == "record"
 ) {
    return Err("Screen recording has not completed. Use the record action and verify the saved file.".into());
 }

 check_outcome(steps)?;
 let completed: Vec<_> = steps.as_array().into_iter().flatten()
     .filter(|step| step["tool"] == "computer" && step["kind"] == "done").collect();
 if let Some(index) = completed.iter().rposition(|step| step["result"]["navigated_url"].is_string()) {
     let latest = completed[index+1..].iter().rev().find(|step| effective_action(step) == "observe");
     if latest.and_then(|step|step["result"]["window_title"].as_str()).is_some_and(failed_page_title) {
         return Err("The browser is showing an error or a page that has not loaded. Verify the destination before reporting success.".into());
     }
     if effective_action(completed[index]) == "open_url" {
         let url = completed[index]["result"]["navigated_url"].as_str().unwrap_or("");
         let matched = completed[index+1..].iter().any(|step| effective_action(step) == "observe" && observed_page(&step["result"], url));
         if !matched { return Err("Navigation was sent, but the exact destination was not observed.".into()); }
     }
 }
 let text=question.to_lowercase().replace('’',"'");
 if let Some(url) = explicit_navigation(question) {
  let link = named_browser_link(question);
  let mut observed = false;
  let mut clicked = false;
  let mut verified_click = false;
  for step in steps.as_array().into_iter().flatten().filter(|s| s["tool"] == "computer" && s["kind"] == "done") {
   let result = &step["result"];
   if effective_action(step) == "observe" && observed_page(result, url.as_str()) {
    observed = true;
   }
   if observed && effective_action(step) == "click" && result["clicked"] == true
       && link.as_ref().is_none_or(|label| result["clicked_label"].as_str()
           .is_some_and(|value| value.to_lowercase().contains(label))) { clicked = true; }
   if clicked && effective_action(step) == "observe" { verified_click = true; }
  }
  if !observed {
   return Err(format!("Open {url} directly with computer.open_url and observe its browser window."));
  }
  if text.contains("click ") && !verified_click {
   return Err(format!("Reached {url}, but the requested link click is unverified. Use the matching link from the latest observation and observe after clicking. If the link is absent, explain that without repeating navigation."));
  }
  return Ok(());
 }
 if let Some(expected) = requested_tab_count(&text) {
  let mut opened = std::collections::HashSet::new();
  let mut pending = Vec::new();
  for step in steps.as_array().into_iter().flatten().filter(|s| s["kind"] == "done" && s["tool"] == "computer") {
   let result = &step["result"];
   if effective_action(step) == "search_tabs" {
    for tab in result["tabs"].as_array().into_iter().flatten() {
     if tab["new_tab"] == true && tab["app"] == result["app"] {
      if let Some(url) = tab["navigated_url"].as_str() { opened.insert(url); }
     }
    }
   }
   if result["new_tab"] == true {
    if let (Some(app), Some(url)) = (result["app"].as_str(), result["navigated_url"].as_str()) {
     pending.push((app, url));
    }
   } else if effective_action(step) == "observe" && result["window_title"].as_str().is_some_and(|title| !title.is_empty()) {
    opened.extend(pending.iter().filter(|(app, _)| result["app"] == *app).map(|(_, url)| *url));
   }
  }
  if opened.len() < expected {
   return Err(format!("The browser task is incomplete. Recorded {} of {expected} distinct new-tab navigations followed by browser observation. Use new_tab=true with distinct searches or URLs for the remaining tabs, then observe the browser.", opened.len()));
  }
  return Ok(());
 }
 let browser_search=text.contains("search") && ["browser","chrome","safari","firefox","edge"].iter().any(|s|text.contains(s));
 let typing= !["do not type","don't type","without typing"].iter().any(|s|text.contains(s)) && ["browser","chrome","safari","firefox"].iter().any(|s|text.contains(s)) && (text.contains("type ")||text.contains("type in")||text.contains("enter "));
 if typing {let done=steps.as_array().into_iter().flatten().filter(|s|s["tool"]=="computer"&&s["kind"]=="done").collect::<Vec<_>>();let Some(index)=done.iter().rposition(|s|effective_action(s)=="type"&&s["result"]["typed_characters"].as_u64().unwrap_or(0)>0)else{return Err("Computer action incomplete: the browser may have opened, but no typing action completed. Continue the original task from the current browser state.".into());};if !done[index+1..].iter().any(|s|effective_action(s)=="observe"){return Err("Computer action incomplete: typing was dispatched, but its result was not observed. Verify the current browser state before repeating input.".into());}}
 if browser_search {
  let done=steps.as_array().into_iter().flatten().filter(|s|s["tool"]=="computer"&&s["kind"]=="done").collect::<Vec<_>>();
  let navigation=done.iter().rposition(|s|s["result"]["navigated_url"].as_str().is_some_and(|u|reqwest::Url::parse(u).is_ok_and(|url|url.query().is_some()||url.path().contains("search"))) || (effective_action(s)=="key" && s["result"]["key"].as_str().is_some_and(|k|matches!(k,"return"|"enter")) && done.iter().any(|s|effective_action(s)=="type")));
  let Some(index)=navigation else{return Err("Computer action incomplete: the browser may have opened, but no browser search was performed. The local model stopped early. Tell me which browser to use, or choose a stronger tool-capable local model in Settings → General.".into());};
  if !done[index+1..].iter().any(|s|effective_action(s)=="observe"){return Err("Computer action incomplete: the browser search was dispatched, but the loaded page was not observed. Check Accessibility in Connections → Computer use, then tell me to verify the current browser instead of repeating the search.".into());}
 }
 Ok(())
}
pub fn denies_access(text:&str)->bool{let text=text.to_lowercase().replace('’',"'");["can't actually open","cannot actually open","can't open a real browser","cannot open a real browser","can't perform live","cannot perform live","no browser tool","no indication that a browser","no browser control","can't execute a live browser","cannot execute a live browser","no access to a browser"].iter().any(|p|text.contains(p))}
pub fn receipt_summary(steps:&Value)->String{
 let done=steps.as_array().into_iter().flatten().filter(|s|s["tool"]=="computer" && s["kind"]=="done").collect::<Vec<_>>();
 let mut seen_tabs = std::collections::HashSet::new();
 let tabs: Vec<_> = done.iter().filter(|step| step["result"]["new_tab"] == true)
  .filter_map(|step| step["result"]["navigated_url"].as_str())
  .filter(|url| seen_tabs.insert(*url)).collect();
 if !tabs.is_empty() {
  return format!("Opened these addresses in separate browser tabs and observed the browser:\n\n{}", tabs.join("\n"));
 }
 let navigation=done.iter().rev().find_map(|s|s["result"]["navigated_url"].as_str());
 let typed=done.iter().rev().find_map(|s|s["result"]["typed_characters"].as_u64());
 let title=done.iter().rev().find_map(|s|s["result"]["window_title"].as_str().filter(|s|!s.is_empty()));
 let mut facts=Vec::new();if let Some(url)=navigation{facts.push(format!("Opened {url} in your default browser."));}if let Some(count)=typed{facts.push(format!("Entered {count} characters in the target app."));}if let Some(title)=title{facts.push(format!("Observed window: {title}."));}
 if facts.is_empty(){let action=done.iter().rev().find(|s|!matches!(effective_action(s),"observe"|"resolve_app"));facts.push(action.and_then(|s|s["summary"].as_str()).unwrap_or("Observed the target app.").to_string());}facts.join(" ")

}
#[cfg(test)]mod tests{use super::*;
 #[test]fn a_failed_browser_action_is_not_successful_web_research(){let steps=json!([{"tool":"computer","kind":"error","summary":"App web not found"},{"tool":"web_search","kind":"done"}]);let error=check_outcome(&steps).unwrap_err();assert!(error.contains("App web not found"));assert!(error.contains("Connections"));assert!(check_outcome(&json!([])).is_err());assert!(check_outcome(&json!([{"tool":"computer","kind":"done","action":"resolve_app"}])).is_err());}
 #[test]fn claims_are_grounded_in_actual_browser_receipts(){let steps=json!([{"tool":"computer","kind":"done","action":"open_url","result":{"navigated_url":"https://example.com"}},{"tool":"computer","kind":"done","action":"observe","result":{"window_title":"Example"}}]);assert!(check_outcome(&steps).is_ok());assert!(receipt_summary(&steps).contains("https://example.com"));assert!(receipt_summary(&steps).contains("Example"));assert!(denies_access("I can’t actually open a real browser"));assert!(!denies_access("Opened the browser."));}
}

#[cfg(test)]mod browser_completion_tests{use super::*;#[test]fn opening_an_app_is_not_a_verified_browser_search(){let mut steps=json!([{"kind":"done","tool":"computer","action":"open_app","result":{"opened":"com.apple.Safari"}}]);assert!(check_task_outcome("open a browser and search hello world",&steps).unwrap_err().contains("no browser search"));steps.as_array_mut().unwrap().push(json!({"kind":"done","tool":"computer","action":"open_url","result":{"navigated_url":"https://www.google.com/search?q=hello%20world"}}));assert!(check_task_outcome("open a browser and search hello world",&steps).unwrap_err().contains("not observed"));steps.as_array_mut().unwrap().push(json!({"kind":"done","tool":"computer","action":"observe","result":{"window_title":"hello world - Search","window":{"address":"https://www.google.com/search?q=hello%20world"}}}));assert!(check_task_outcome("open a browser and search hello world",&steps).is_ok());}}

fn normalize_action(args:&Value)->Result<Value,String>{let mut value=args.clone();let action=value["action"].as_str().unwrap_or("");let canonical=match action{"open_browser"=>"open_app","launch_app"=>"open_app","navigate"|"navigate_url"=>"open_url","type_text"=>"type","press_key"=>"key","inspect"|"observe_app"=>"observe","enable_permission"|"grant_permission"|"enable_accessibility"=>return Err("Computer tools cannot grant macOS permissions. Open Connections → macOS access and enable Accessibility yourself; then continue the task.".into()),_=>action}.to_string();value["action"]=json!(canonical);if value["app"].is_null() && canonical=="open_app"{value["app"]=json!("browser");}if value["key"].is_null()&&canonical=="key"&&value["keys"].is_string(){value["key"]=value["keys"].clone();}if value["text"].is_null()&&canonical=="type"&&value["value"].is_string(){value["text"]=value["value"].clone();}Ok(value)}
#[cfg(test)]mod action_alias_tests{use super::*;#[test]fn common_action_names_work_without_exposing_permission_grants(){assert_eq!(normalize_action(&json!({"action":"open_browser"})).unwrap()["app"],"browser");assert_eq!(normalize_action(&json!({"action":"type_text","value":"hello"})).unwrap()["text"],"hello");assert!(normalize_action(&json!({"action":"enable_permission"})).unwrap_err().contains("cannot grant"));}}

fn effective_action(step:&Value)->&str{step["result"]["executed_action"].as_str().or_else(||step["action"].as_str()).unwrap_or("")}

#[cfg(test)]mod target_tests{use super::*;#[test]fn approval_focus_cannot_redirect_typing_to_fabric(){let mut args=json!({"action":"type","text":"hello world"});bind_target(&mut args,Some("com.google.Chrome")).unwrap();assert_eq!(args["app"],"com.google.Chrome");assert!(bind_target(&mut json!({"action":"type","text":"hello"}),None).is_err());let mut explicit=json!({"action":"key","key":"Return","app":"com.apple.Safari"});bind_target(&mut explicit,Some("com.google.Chrome")).unwrap();assert_eq!(explicit["app"],"com.apple.Safari");}}

#[cfg(test)]mod typing_completion_tests{use super::*;#[test]fn permission_continuation_must_execute_typing_not_just_launch(){let mut steps=json!([{"kind":"done","tool":"computer","action":"open_app","result":{"opened":"browser"}}]);assert!(check_task_outcome("open a browser and type hello world",&steps).unwrap_err().contains("no typing"));assert!(check_task_outcome("open a browser but do not type anything",&steps).is_ok());steps.as_array_mut().unwrap().push(json!({"kind":"done","tool":"computer","action":"type","result":{"typed_characters":11}}));assert!(check_task_outcome("open a browser and type hello world",&steps).is_err());steps.as_array_mut().unwrap().push(json!({"kind":"done","tool":"computer","action":"observe","result":{"ui":"hello world"}}));assert!(check_task_outcome("open a browser and type hello world",&steps).is_ok());}}


#[cfg(test)]
mod separate_tab_tests {
    use super::*;

    #[test]
    fn three_tabs_require_three_distinct_dispatches_and_a_browser_observation() {
        let request = "open 3 browser tabs: recipe, ingredients, and video";
        let mut steps = Vec::new();
        for query in ["recipe", "ingredients", "video"] {
            steps.push(json!({"tool":"computer","kind":"done","action":"search_browser","result":{
                "app":"browser","new_tab":true,"navigated_url":format!("https://example.org/?q={query}")
            }}));
        }
        assert!(check_task_outcome(request, &json!(steps)).is_err());
        steps.push(json!({"tool":"computer","kind":"done","action":"observe","result":{
            "app":"browser","window_title":"Video"
        }}));
        assert!(check_task_outcome(request, &json!(steps)).is_ok());
        steps.remove(0);
        assert!(check_task_outcome(request, &json!(steps)).is_err());
    }
}


#[cfg(test)]
mod direct_navigation_tests {
    use super::*;

    #[test]
    fn named_link_requires_the_requested_site_and_click_evidence() {
        let goal = "open a browser and navigate to akatary.com then click the akatary.com link";
        assert_eq!(explicit_navigation(goal).unwrap().as_str(), "https://akatary.com/");
        assert_eq!(explicit_navigation("navigate to https://example.com/Case").unwrap().path(), "/Case");
        assert_eq!(navigation_key(&json!({"action":"search_browser","query":" Akatary.com "})),
                   navigation_key(&json!({"action":"search_browser","query":"akatary.com","app":"chrome"})));
        let mut steps = vec![json!({"tool":"computer","kind":"done","action":"observe","result":{
            "window":{"address":"https://www.google.com/search?q=akatary.com"}
        }})];
        assert!(check_task_outcome(goal, &json!(steps)).is_err());
        let site = json!({"tool":"computer","kind":"done","action":"observe","result":{
            "window_title":"Akatary","window":{"address":"https://akatary.com/"}
        }});
        steps.push(site.clone());
        assert!(check_task_outcome(goal, &json!(steps)).unwrap_err().contains("click is unverified"));
        steps.push(json!({"tool":"computer","kind":"done","action":"click","result":{
            "clicked":true,"clicked_label":"AXLink · akatary.com"
        }}));
        steps.push(site);
        assert!(check_task_outcome(goal, &json!(steps)).is_ok());
    }
}

/// Resolve a planner's numeric AX path against the most recent observation.
pub fn ground_element_click(args: &mut Value, observation: Option<&Value>) -> Result<(), String> {
    let Some(path) = args["element"].as_str() else { return Ok(()); };
    if path.is_empty() || !path.split('.').all(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit())) {
        return Err("The click target must be a numeric element path such as 0.2 from the latest observation. A description is not a path.".into());
    }
    let observation = observation.ok_or("Observe the target app again before clicking its controls.")?;
    let app = observation["app"].as_str().ok_or("The last observation did not identify an app. Observe the target again.")?;
    if let Some(requested) = args["app"].as_str().filter(|s| !s.is_empty()) {
        if !requested.eq_ignore_ascii_case(app) {
            return Err(format!("The latest observation is for {app}. Use that app identifier or observe the requested app before clicking."));
        }
    }
    let label = observation["ui"].as_str().and_then(|ui| {
        let mut lines = ui.lines();
        lines.find_map(|line| {
            let (index, label) = line.split_once(' ')?;
            (index == path && label.contains(" · ")).then_some(label)
        })
    }).ok_or("That path has no labeled control in the latest observation. Observe the app again and select a listed control.")?;
    if args["expected"].as_str().filter(|label| !label.trim().is_empty()).is_some_and(|expected| !label.contains(expected)) {
        return Err("The expected label does not match the observed control. Observe the app again and select the correct path.".into());
    }
    args["app"] = json!(app);
    args["expected"] = json!(label);
    args["expected_exact"] = json!(true);
    args["observed_pid"] = observation["pid"].clone();
    Ok(())
}

#[cfg(test)]
mod click_grounding_tests {
    use super::*;
    #[test]
    fn binds_missing_label_only_to_an_observed_path_and_app() {
        let observation = json!({"app":"com.google.Chrome","ui":"0 AXWindow · Example\n0.1 AXLink · Next\n0.2 AXButton"});
        let mut click = json!({"action":"click","element":"0.1"});
        ground_element_click(&mut click, Some(&observation)).unwrap();
        assert_eq!(click["expected"], "AXLink · Next");
        assert_eq!(click["app"], "com.google.Chrome");
        assert_eq!(click["expected_exact"], true);
        for mut invalid in [
            json!({"element":"the Next link"}),
            json!({"element":"0.9"}),
            json!({"element":"0.2"}),
            json!({"element":"0.1","expected":"Delete"}),
            json!({"element":"0.1","app":"com.apple.Safari"}),
        ] {
            assert!(ground_element_click(&mut invalid, Some(&observation)).is_err());
        }
        assert!(ground_element_click(&mut click, None).is_err());
    }
}


#[cfg(test)]
mod navigation_verification_tests {
    use super::*;
    #[test]
    fn a_navigation_receipt_is_not_a_loaded_page() {
        let target = "https://example.com/recipe/42/";
        let mut steps = json!([{"tool":"computer","kind":"done","action":"open_url","result":{"navigated_url":target}}]);
        let observed = |title: &str, url: &str| json!({"tool":"computer","kind":"done","action":"observe",
            "result":{"window_title":title,"window":{"address":url}}});
        assert!(check_task_outcome("Open the page", &steps).is_err());
        steps.as_array_mut().unwrap().push(observed("Page Not Found", target));
        assert!(check_task_outcome("Open the page", &steps).is_err());
        steps.as_array_mut().unwrap()[1] = observed("Example homepage", "https://example.com/");
        assert!(check_task_outcome("Open the page", &steps).is_err());
        steps.as_array_mut().unwrap()[1] = observed("Example recipe", target);
        assert!(check_task_outcome("Open the page", &steps).is_ok());
    }
}
