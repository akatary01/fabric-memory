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
pub fn enabled() -> bool {
    std::fs::read(config())
        .ok()
        .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
        .is_some_and(|v| v["enabled"] == true)
}
pub fn set_enabled(value: bool) -> Result<Value, String> {
    fabric_core::write_json(&config(), &json!({"enabled":value})).map_err(|e| e.to_string())?;
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
pub fn status() -> Result<Value, String> {
    let mut value = helper(&json!({"action":"permissions"}))
        .unwrap_or(json!({"accessibility":false,"screen_recording":false}));
    value["enabled"] = json!(enabled());
    Ok(value)
}
pub fn ensure_ready()->Result<(),String>{
 if !enabled(){return Err("Computer use is off. Enable it in Connections → Computer use.".into());}
 let state=helper(&json!({"action":"permissions"})).map_err(|e|format!("Computer controller unavailable: {e}"))?;
 if state["accessibility"]!=true{return Err("Computer control needs macOS Accessibility access for Memory Fabric. Open Connections → macOS access, enable Memory Fabric under Accessibility, then tell me to continue. Auto-approval cannot grant macOS permissions.".into());}Ok(())
}
pub fn request_permissions() -> Result<Value, String> {
    helper(&json!({"action":"request_permissions"}))
}
pub fn call(args: &Value) -> Result<Value, String> {
    let mut args=normalize_action(args)?;let target=last_target().lock().map_err(|_|"Computer target unavailable")?.clone();bind_target(&mut args,target.as_deref())?;let args=&args;
    if !enabled() {
        return Err("Enable Computer use in Connections first.".into());
    }
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
    Ok(json!(std::fs::read(file).map_err(|e| e.to_string())?))
}
pub fn tool() -> Value {
    json!({"name":"computer","description":"Control this Mac. resolve_app accepts an installed app name or bundle ID; app=browser or web resolves the default browser. open_app accepts those same names. search_browser opens a Google query in the actual default browser (query parameter); open_url navigates an HTTP/HTTPS URL in that browser. Navigation dispatch does not confirm page load; observe the resolved browser to verify. Observe the window after navigation to verify the page. First observe before clicking or typing; observations return element paths. click with element plus expected label, or x/y screen coordinates; type into a focused field; key with Cmd+A etc; scroll; open_app by bundle ID. screenshot captures the screen for user review. Requires macOS permissions.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["observe","resolve_app","open_url","search_browser","click","type","key","scroll","open_app","screenshot"]},"app":{"type":"string"},"url":{"type":"string"},"query":{"type":"string"},"element":{"type":"string"},"expected":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"text":{"type":"string"},"key":{"type":"string"},"pixels":{"type":"integer"}},"required":["action"]}})
}

/// A model must not replace a failed or skipped computer task with generic web evidence.
pub fn check_outcome(steps:&Value)->Result<(),String>{
 let calls=steps.as_array().into_iter().flatten().filter(|s|s["tool"]=="computer" && matches!(s["kind"].as_str(),Some("done"|"error"|"denied"))).collect::<Vec<_>>();
 let Some(last)=calls.last() else{return Err("Computer use is available, but the selected local model ended without attempting the requested action. Try a stronger tool-capable local model in Settings → General, or give me the target browser name.".into());};
 if last["kind"]!="done" {return Err(format!("Computer action did not finish: {}. Completed actions are retained. Check Connections → Computer use and macOS access, or tell me the correct app name so I can continue.",last["summary"].as_str().unwrap_or("Action declined")));}
 if !calls.iter().any(|s|s["kind"]=="done" && effective_action(s)!="resolve_app"){return Err("The app was identified, but the requested computer action was not performed. Tell me which installed browser to use, or choose a stronger local model.".into());}
 Ok(())
}
pub fn check_task_outcome(question:&str,steps:&Value)->Result<(),String>{
 check_outcome(steps)?;let text=question.to_lowercase();
 let browser_search=text.contains("search") && ["browser","chrome","safari","firefox","edge"].iter().any(|s|text.contains(s));
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
 let navigation=done.iter().rev().find_map(|s|s["result"]["navigated_url"].as_str());
 let title=done.iter().rev().find_map(|s|s["result"]["window_title"].as_str().filter(|s|!s.is_empty()));
 if let Some(url)=navigation {format!("Opened {url} in your default browser.{}",title.map(|t|format!(" The browser reports: {t}.")).unwrap_or_else(||" Page contents have not been verified.".into()))}
 else {let action=done.iter().rev().find(|s|!matches!(s["action"].as_str(),Some("observe"|"resolve_app")));format!("{} The full requested outcome has not been verified.",action.and_then(|s|s["summary"].as_str()).unwrap_or("Read the requested app observation."))}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn a_failed_browser_action_is_not_successful_web_research(){let steps=json!([{"tool":"computer","kind":"error","summary":"App web not found"},{"tool":"web_search","kind":"done"}]);let error=check_outcome(&steps).unwrap_err();assert!(error.contains("App web not found"));assert!(error.contains("Connections"));assert!(check_outcome(&json!([])).is_err());assert!(check_outcome(&json!([{"tool":"computer","kind":"done","action":"resolve_app"}])).is_err());}
 #[test]fn claims_are_grounded_in_actual_browser_receipts(){let steps=json!([{"tool":"computer","kind":"done","action":"open_url","result":{"navigated_url":"https://example.com"}},{"tool":"computer","kind":"done","action":"observe","result":{"window_title":"Example"}}]);assert!(check_outcome(&steps).is_ok());assert!(receipt_summary(&steps).contains("https://example.com"));assert!(receipt_summary(&steps).contains("Example"));assert!(denies_access("I can’t actually open a real browser"));assert!(!denies_access("Opened the browser."));}
}

#[cfg(test)]mod browser_completion_tests{use super::*;#[test]fn opening_an_app_is_not_a_verified_browser_search(){let mut steps=json!([{"kind":"done","tool":"computer","action":"open_app","result":{"opened":"com.apple.Safari"}}]);assert!(check_task_outcome("open a browser and search hello world",&steps).unwrap_err().contains("no browser search"));steps.as_array_mut().unwrap().push(json!({"kind":"done","tool":"computer","action":"open_url","result":{"navigated_url":"https://www.google.com/search?q=hello%20world"}}));assert!(check_task_outcome("open a browser and search hello world",&steps).unwrap_err().contains("not observed"));steps.as_array_mut().unwrap().push(json!({"kind":"done","tool":"computer","action":"observe","result":{"window_title":"hello world - Search"}}));assert!(check_task_outcome("open a browser and search hello world",&steps).is_ok());}}

fn normalize_action(args:&Value)->Result<Value,String>{let mut value=args.clone();let action=value["action"].as_str().unwrap_or("");let canonical=match action{"open_browser"=>"open_app","launch_app"=>"open_app","navigate"|"navigate_url"=>"open_url","type_text"=>"type","press_key"=>"key","inspect"|"observe_app"=>"observe","enable_permission"|"grant_permission"|"enable_accessibility"=>return Err("Computer tools cannot grant macOS permissions. Open Connections → macOS access and enable Accessibility yourself; then continue the task.".into()),_=>action}.to_string();value["action"]=json!(canonical);if value["app"].is_null() && canonical=="open_app"{value["app"]=json!("browser");}if value["key"].is_null()&&canonical=="key"&&value["keys"].is_string(){value["key"]=value["keys"].clone();}if value["text"].is_null()&&canonical=="type"&&value["value"].is_string(){value["text"]=value["value"].clone();}Ok(value)}
#[cfg(test)]mod action_alias_tests{use super::*;#[test]fn common_action_names_work_without_exposing_permission_grants(){assert_eq!(normalize_action(&json!({"action":"open_browser"})).unwrap()["app"],"browser");assert_eq!(normalize_action(&json!({"action":"type_text","value":"hello"})).unwrap()["text"],"hello");assert!(normalize_action(&json!({"action":"enable_permission"})).unwrap_err().contains("cannot grant"));}}

fn effective_action(step:&Value)->&str{step["result"]["executed_action"].as_str().or_else(||step["action"].as_str()).unwrap_or("")}

#[cfg(test)]mod target_tests{use super::*;#[test]fn approval_focus_cannot_redirect_typing_to_fabric(){let mut args=json!({"action":"type","text":"hello world"});bind_target(&mut args,Some("com.google.Chrome")).unwrap();assert_eq!(args["app"],"com.google.Chrome");assert!(bind_target(&mut json!({"action":"type","text":"hello"}),None).is_err());let mut explicit=json!({"action":"key","key":"Return","app":"com.apple.Safari"});bind_target(&mut explicit,Some("com.google.Chrome")).unwrap();assert_eq!(explicit["app"],"com.apple.Safari");}}
