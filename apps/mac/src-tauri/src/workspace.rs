use serde_json::{json,Value};
use std::{path::{Path,PathBuf},process::{Command,Stdio},sync::atomic::{AtomicU32,Ordering},io::{BufRead,BufReader,Write}};
fn path()->PathBuf{PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join("workspace.json")}
pub fn settings()->Value {
    let fallback=path().parent().unwrap().join("Workspace");
    let saved=std::fs::read(path()).ok().and_then(|data|serde_json::from_slice::<Value>(&data).ok()).unwrap_or(json!({}));
    json!({"path":saved["path"].as_str().unwrap_or(fallback.to_str().unwrap_or("")),"configured":saved["path"].is_string(),"enabled":saved["enabled"].as_bool().unwrap_or_else(|| saved["path"].is_string() || fallback.is_dir()),"cloud_allowed":saved["cloud_allowed"]==true && saved["cloud_provider"].as_str().unwrap_or("openai")==crate::cloud_settings::provider_id()})
}
pub fn save(folder:&str,cloud:bool,enabled:Option<bool>)->Result<Value,String>{
    let enabled=enabled.unwrap_or(settings()["enabled"]==true);
    let folder=Path::new(folder);
    if !folder.exists() && folder==path().parent().unwrap().join("Workspace"){std::fs::create_dir_all(folder).map_err(|e|e.to_string())?;}
    if !folder.is_absolute() || !folder.is_dir(){return Err("Choose an existing absolute folder path".into());}
    let folder=folder.canonicalize().map_err(|e|e.to_string())?;
    if folder==Path::new("/") || folder==PathBuf::from(std::env::var("HOME").unwrap_or_default()){return Err("Choose a project folder, rather than the whole Mac or home folder".into());}
    fabric_core::write_json(&path(),&json!({"path":folder,"enabled":enabled,"cloud_allowed":cloud,"cloud_provider":crate::cloud_settings::provider_id()})).map_err(|e|e.to_string())?;Ok(settings())
}
pub fn tools()->Vec<Value>{vec![
 json!({"name":"workspace","description":"Read applicable scoped project instructions, list, read, or write project files in the selected workspace. Inspect existing files before editing. Tool paths are relative: use action=list and path='.' to list the selected folder. Write complete file contents. File edits require review.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["list","read","write","instructions"]},"path":{"type":"string","description":"Relative path inside the selected workspace. Use . for its root."},"content":{"type":"string"}},"required":["action","path"]}}),
 json!({"name":"run_code","description":"Run a shell command in the selected workspace using Fabric's sandbox. Use this to execute Python/code, run project tests, inspect failures, and verify changes. python3 and Git are included; other runtimes must be installed. No network by default. Commands require review; output and exit status are returned. Use relative path for working directory. Commands default to 60 seconds. Set timeout_seconds for a longer build or test, up to 600 seconds within the task budget. Background processes stop with the action.","inputSchema":{"type":"object","properties":{"command":{"type":"string"},"path":{"type":"string"},"timeout_seconds":{"type":"integer"},"network":{"type":"boolean"}},"required":["command"]}})
]}
#[derive(Default)]pub struct Host{pid:AtomicU32}
impl Drop for Host{fn drop(&mut self){self.stop();}}
impl Host{
 pub fn stop(&self){let pid=self.pid.swap(0,Ordering::Relaxed);if pid!=0 {let _=Command::new("/bin/kill").args(["-TERM",&pid.to_string()]).status();}}
 pub fn call(&self,name:&str,args:&Value,remaining:std::time::Duration)->Result<Value,String>{
    let mut args=args.clone();if name=="run_code"{args["action"]=json!("run");}if name=="blender"{args["action"]=json!("blender");}
    let settings=settings();if settings["enabled"]!=true{return Err("Workspace access is off. Enable it in Settings → Access & local models.".into());}let root=settings["path"].as_str().ok_or("Missing workspace")?;
    std::fs::create_dir_all(root).map_err(|e|e.to_string())?;
    args["path"]=json!(relative_path(Path::new(root),args["path"].as_str().unwrap_or("."))?);
    if args["action"]=="instructions"{return instructions(Path::new(root),args["path"].as_str().unwrap_or("."));}
    let timeout = args["timeout_seconds"].as_u64().unwrap_or(60).clamp(1, 600);
    let timeout = if args["action"] == "run" { timeout } else { timeout.min(120) };
    let timeout = timeout.min(remaining.as_secs().max(1));
    args["timeout_seconds"] = json!(timeout);
    let host_timeout = std::time::Duration::from_secs(
        if args["action"] == "blender" { timeout * 2 + 15 } else { timeout + 15 }
    ).min(remaining);
    let python=std::env::var("FABRIC_PYTHON").unwrap_or_else(|_|"python3".into());
    let resources=std::env::current_exe().map_err(|e|e.to_string())?.parent().ok_or("Missing app")?.join("../Resources");let git=resources.join("git/git");
    let mut child=Command::new(&python).args(["-u","-B","-c",include_str!("../../../../tools/workspace.py")]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e|e.to_string())?;
    self.pid.store(child.id(),Ordering::Relaxed);let mut input=child.stdin.take().ok_or("Missing worker input")?;
    writeln!(input,"{}",json!({"root":root,"args":args,"python":python,"git":if git.is_file(){Some(git)}else{None}})).map_err(|e|e.to_string())?;input.flush().map_err(|e|e.to_string())?;
    let output=child.stdout.take().ok_or("Missing worker output")?;let (tx,rx)=std::sync::mpsc::channel();std::thread::spawn(move||{let mut line=String::new();let result=BufReader::new(output).read_line(&mut line).map(|_|line);let _=tx.send(result);});
    let line=match rx.recv_timeout(host_timeout){Ok(Ok(line))=>line,_=>{self.stop();drop(input);let _=child.wait();return Err("Code execution stopped or timed out".into());}};
    drop(input);let _=child.wait();self.pid.store(0,Ordering::Relaxed);
    let result:Value=serde_json::from_str(&line).map_err(|_|"Workspace action stopped")?;
    if let Some(error)=result["error"].as_str(){return Err(error.into());}Ok(result["result"].clone())
 }
}

fn relative_path(root:&Path,input:&str)->Result<String,String>{let path=Path::new(input);let relative=if path.is_absolute(){path.strip_prefix(root).map_err(|_|"Path is outside the selected workspace. Use relative paths, such as '.' to list this workspace, or change the working folder in Settings → General.")?}else{path};Ok(if relative.as_os_str().is_empty(){".".into()}else{relative.to_str().ok_or("Invalid workspace path")?.into()})}
#[cfg(test)]mod path_tests{use super::*;#[test]fn selected_absolute_paths_normalize_without_expanding_workspace_access(){let root=Path::new("/projects/app");assert_eq!(relative_path(root,"/projects/app").unwrap(),".");assert_eq!(relative_path(root,"/projects/app/src/main.py").unwrap(),"src/main.py");assert_eq!(relative_path(root,".").unwrap(),".");assert!(relative_path(root,"/projects/app-other").is_err());assert!(relative_path(root,"/etc").is_err());}}

pub fn project_context()->Result<String,String>{let state=settings();if state["enabled"]!=true{return Ok(String::new());}Ok(instructions(Path::new(state["path"].as_str().ok_or("Missing workspace")?),".")?["text"].as_str().unwrap_or("").into())}
fn instructions(root:&Path,relative:&str)->Result<Value,String>{
 use std::io::Read;std::fs::create_dir_all(root).map_err(|e|e.to_string())?;let root=root.canonicalize().map_err(|e|e.to_string())?;
 let requested=Path::new(relative);if requested.is_absolute() || requested.components().any(|c|matches!(c,std::path::Component::ParentDir)){return Err("Instructions need a relative path inside the workspace".into());}
 let mut target=root.join(requested);while !target.exists(){if !target.pop(){return Err("Invalid instruction scope".into());}}
 let target=target.canonicalize().map_err(|e|e.to_string())?;if !target.starts_with(&root){return Err("Instruction scope leaves the workspace".into());}let target=if target.is_file(){target.parent().unwrap().to_path_buf()}else{target};
 let mut folders=vec![root.clone()];let mut current=root.clone();for part in target.strip_prefix(&root).unwrap().components().take(16){current.push(part.as_os_str());folders.push(current.clone());}
 let mut text=String::new();let mut files=Vec::new();let mut budget=12000usize;
 for folder in folders{let preferred=if folder.join("AGENTS.override.md").is_file(){"AGENTS.override.md"}else{"AGENTS.md"};let names=if folder==root{vec![preferred,"CLAUDE.md","README.md"]}else{vec![preferred]};
  for name in names{if budget==0{break;}let path=folder.join(name);let Ok(canonical)=path.canonicalize()else{continue};if !canonical.starts_with(&root)||!canonical.is_file(){continue;}let mut bytes=Vec::new();std::fs::File::open(&canonical).map_err(|e|e.to_string())?.take(32768).read_to_end(&mut bytes).map_err(|e|e.to_string())?;let relative=path.strip_prefix(&root).unwrap().display().to_string();let header=format!("\nProject guidance: {relative} (scope: {})\n",folder.strip_prefix(&root).unwrap().display());let header_len=header.chars().count()+1;if header_len>=budget{continue;}let value=String::from_utf8_lossy(&bytes).chars().take((budget-header_len).min(4000)).collect::<String>();budget=budget.saturating_sub(value.chars().count()+header_len);text.push_str(&header);text.push_str(&value);text.push('\n');files.push(relative);}
 }Ok(json!({"text":text,"files":files,"budget_characters":12000}))
}
#[cfg(test)]mod instruction_tests{use super::*;#[test]fn scoped_guidance_is_bounded_and_cannot_read_an_outside_symlink(){let root=std::env::temp_dir().join(format!("fabric-guide-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir_all(root.join("src")).unwrap();std::fs::write(root.join("AGENTS.md"),"root guidance").unwrap();std::fs::write(root.join("README.md"),"x".repeat(20000)).unwrap();std::fs::write(root.join("src/AGENTS.md"),"nested guidance").unwrap();let v=instructions(&root,"src/new.rs").unwrap();assert!(v["text"].as_str().unwrap().contains("nested guidance"));assert!(v["text"].as_str().unwrap().len()<13000);assert!(instructions(&root,"../outside").is_err());#[cfg(unix)]{std::os::unix::fs::symlink("/etc/hosts",root.join("CLAUDE.md")).unwrap();assert!(!instructions(&root,".").unwrap()["files"].as_array().unwrap().iter().any(|p|p=="CLAUDE.md"));}std::fs::remove_dir_all(root).unwrap();}}

fn clean_root() -> Result<PathBuf,String> {
    let value = settings();
    let root = PathBuf::from(value["path"].as_str().ok_or("Choose a workspace first.")?);
    if !root.exists() && root == path().parent().unwrap().join("Workspace") {
        std::fs::create_dir_all(&root).map_err(|e|e.to_string())?;
    }
    root.canonicalize().map_err(|_|"The selected workspace is unavailable.".into())
}
fn clean_item(root:&Path,name:&str) -> Result<PathBuf,String> {
    let mut parts=Path::new(name).components();
    if name.starts_with('.') || !matches!((parts.next(),parts.next()),(Some(std::path::Component::Normal(p)),None) if p == name) {
        return Err("Only visible files or project folders directly in the workspace can be trashed.".into());
    }
    let item=root.join(name);
    let meta=item.symlink_metadata().map_err(|e|e.to_string())?;
    if meta.file_type().is_symlink() || !(meta.is_dir() || meta.is_file()) { return Err("Links and special files are excluded.".into()); }
    Ok(item)
}
pub fn clean_preview() -> Result<Value,String> {
    let root=clean_root()?;
    let mut items=Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|e|e.to_string())? {
        let Ok(entry)=entry else {continue};
        let Ok(name)=entry.file_name().into_string() else {continue};
        let Ok(item)=clean_item(&root,&name) else {continue};
        let Ok(meta)=item.symlink_metadata() else {continue};
        let modified=meta.modified().ok().and_then(|t|t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d|d.as_secs());
        items.push((modified,name,meta.is_dir()));
    }
    items.sort();
    Ok(json!({"root":root,"items":items.into_iter().map(|(modified,name,project)|json!({"name":name,"modified":modified,"project":project})).collect::<Vec<_>>() }))
}
pub fn clean_trash(expected:&str, items:&[Value]) -> Result<Value,String> {
    let root=clean_root()?;
    if root != Path::new(expected) {return Err("The workspace changed. Review the new folder before moving files.".into());}
    if items.is_empty() || items.len()>250 {return Err("Select between 1 and 250 items.".into());}
    let results=items.iter().map(|name|{
        let result=name.as_str().ok_or("Invalid item name".into()).and_then(|name|clean_item(&root,name)).and_then(|item|crate::computer::trash_item(&item));
        match result {Ok(_)=>json!({"name":name,"trashed":true}),Err(error)=>json!({"name":name,"error":error})}
    }).collect::<Vec<_>>();
    Ok(json!(results))
}

#[cfg(test)]
mod clean_tests {
    use super::*;
    #[test]
    fn cleanup_scope_rejects_hidden_paths_traversal_and_symlinks() {
        let root=std::env::temp_dir().join(format!("fabric-clean-scope-{}",std::process::id()));
        std::fs::create_dir_all(root.join("project")).unwrap();
        std::fs::write(root.join("export.txt"),"example").unwrap();
        std::os::unix::fs::symlink("/tmp",root.join("linked")).unwrap();
        for invalid in [".git","linked","../file","/tmp","project/","project/file",""] {
            assert!(clean_item(&root,invalid).is_err(),"{invalid}");
        }
        assert!(clean_item(&root,"project").is_ok());
        assert!(clean_item(&root,"export.txt").is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
}
