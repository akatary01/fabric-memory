use serde_json::{json,Value};
fn operation_schema()->Value{
 let vector=json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3});
 json!({"anyOf":[
 {"type":"object","additionalProperties":false,"required":["op","name"],"properties":{"op":{"const":"remove_object"},"name":{"type":"string","minLength":1}}},
 {"type":"object","additionalProperties":false,"required":["op","primitive"],"properties":{"op":{"const":"add_object"},"primitive":{"type":"string","enum":["cube","sphere","monkey"]},"name":{"type":"string"},"location":vector,"scale":vector,"rotation_euler":vector}},
 {"type":"object","additionalProperties":false,"required":["op","name"],"anyOf":[{"required":["location"]},{"required":["scale"]},{"required":["rotation_euler"]}],"properties":{"op":{"const":"transform_object"},"name":{"type":"string","minLength":1},"location":vector,"scale":vector,"rotation_euler":vector}},
 {"type":"object","additionalProperties":false,"required":["op","name","new_name"],"properties":{"op":{"const":"rename_object"},"name":{"type":"string","minLength":1},"new_name":{"type":"string","minLength":1}}},
 {"type":"object","additionalProperties":false,"required":["op","name"],"properties":{"op":{"const":"dent"},"name":{"type":"string","minLength":1}}},
 {"type":"object","additionalProperties":false,"required":["op","code"],"properties":{"op":{"const":"python"},"code":{"type":"string","minLength":1,"maxLength":16000}}}
 ]})
}
pub fn tool()->Value{
 let props=json!({"operation":{"type":"string"},"operations":{"type":"array","minItems":1,"maxItems":100,"items":operation_schema()},"input_file":{"type":"string"},"output_file":{"type":"string"},"overwrite":{"type":"boolean"},"code":{"type":"string","maxLength":16000},"timeout_seconds":{"type":"integer"}});
 let mut create=props.clone();create["operation"]=json!({"const":"create"});create["operations"]["minItems"]=json!(0);create.as_object_mut().unwrap().remove("input_file");
 let mut edit=props;edit["operation"]=json!({"const":"edit"});
 json!({"name":"blender","description":"Operate on actual workspace .blend files locally. inspect is read-only: provide input_file and no operations, code or output_file. create starts a new General scene; edit loads input_file. For changes choose typed operations: remove_object needs name; add_object needs primitive=cube, sphere, or monkey (Suzanne); transform_object needs name and a transform; rename_object needs name and new_name. For a simple dent on a rounded mesh use op=dent and the exact object name; this makes a smooth indentation on its local top side. For other edits use op=python with a bpy Python function body. Set output_file to save; saved files are independently reopened and verified. Existing outputs need overwrite=true only when replacement is authorized. Do not recreate an existing partial file when resuming it. Use current file context. GUI permissions are not needed; the unsaved GUI scene is separate.","inputSchema":{"type":"object","anyOf":[
 {"type":"object","additionalProperties":false,"required":["operation","input_file"],"properties":{"operation":{"const":"inspect"},"input_file":{"type":"string"},"timeout_seconds":{"type":"integer"}}},
 {"type":"object","additionalProperties":false,"required":["operation","operations","output_file"],"properties":create},
 {"type":"object","additionalProperties":false,"required":["operation","operations","input_file"],"properties":edit}
 ]}})
}
pub fn task_tool(task:&str,resumed:Option<&str>)->Value{let mut tool=tool();let edit_requested=wants_mutation(task);let pinned=resumed.filter(|f|basic_named_task(task,f));let branches=tool["inputSchema"]["anyOf"].as_array_mut().unwrap();if pinned.is_some(){branches.retain(|b|b["properties"]["operation"]["const"]!="create");}for branch in branches{let mode=branch["properties"]["operation"]["const"].as_str().unwrap_or("").to_string();if mode=="create"&&edit_requested{branch["properties"]["operations"]["minItems"]=json!(1);}if mode=="edit"&&edit_requested{branch["required"].as_array_mut().unwrap().push(json!("output_file"));}if let Some(file)=pinned{branch["properties"]["input_file"]=json!({"type":"string","enum":[file]});if mode=="edit"{branch["properties"]["output_file"]=json!({"type":"string","enum":[file]});branch["properties"]["overwrite"]=json!({"const":true});branch["required"].as_array_mut().unwrap().push(json!("overwrite"));}}}tool}

// Small local models sometimes mix fields from adjacent operation variants.
// Clean up an unambiguous removal before the approval view. Keep unknown fields
// and code for strict rejection, and never turn inspection into a write.
pub fn normalize_model_arguments(args: &mut Value) {
    if !matches!(args["operation"].as_str(), Some("edit" | "create")) {
        return;
    }
    let Some(ops) = args["operations"].as_array_mut() else { return; };
    for op in ops {
        if op["op"] == "remove_object"
            && op["name"].as_str().is_some_and(|name| !name.trim().is_empty())
        {
            if let Some(fields) = op.as_object_mut() {
                for key in ["primitive", "location", "scale", "rotation_euler"] {
                    fields.remove(key);
                }
            }
        }
    }
}

#[cfg(test)]
mod model_argument_tests {
    use super::*;
    #[test]
    fn removes_unrelated_primitive_fields_before_review_only_for_explicit_edits() {
        let op = json!({"op":"remove_object","name":"Cube","primitive":"cube","location":[0,0,0]});
        let mut args = json!({"operation":"edit","operations":[op]});
        normalize_model_arguments(&mut args);
        assert_eq!(args["operations"][0], json!({"op":"remove_object","name":"Cube"}));
        let mut invalid = json!({"operation":"inspect","operations":[op]});
        let original = invalid.clone();
        normalize_model_arguments(&mut invalid);
        assert_eq!(invalid, original);
    }
}

pub fn planning_context(task:&str,update:&str,file:Option<&str>)->String{format!("Execute this Blender file task locally using the offered tool. Inspect reads; it cannot save or modify. Choose operations that perform the user's changes, not repeated inspection. Verify the goal before finishing. Earlier chat replies about unavailable tools or permissions are not current capability evidence.\nCurrent saved file: {}. {}\nACTIVE USER TASK:\n{task}\nLATEST USER UPDATE:\n{update}",file.unwrap_or("none"),if file.is_some(){"Continue from that file; do not reset it with a new General scene."}else{"A new General scene already contains Cube, Camera and Light."})}
pub fn resumed_file(task:&str,history:&[Value],root:&str)->Option<String>{
 for turn in history.iter().rev(){if turn["role"]!="assistant"{continue;}if turn["task_state"]["status"]!="blocked"||turn["task_state"]["request"]!=task{continue;}if turn["workspace"].is_string()&&turn["workspace"]!=root{return None;}
 for step in turn["steps"].as_array().into_iter().flatten().rev(){if step["tool"]!="blender"||step["kind"]!="done"||step["result"]["verified"]!=true{continue;}if let Some(file)=step["result"]["saved_file"].as_str(){let base=std::path::Path::new(root).canonicalize().ok()?;let candidate=base.join(file).canonicalize().ok()?;if !std::path::Path::new(file).is_absolute()&&candidate.starts_with(&base)&&candidate.is_file(){return Some(file.into());}}}return None;
 }None
}

pub fn check_outcome(task:&str,steps:&Value)->Result<(),String>{
 let needs_save=["create","make ","new ","delete","remove","modify","edit","save","add ","rename","dent "].iter().any(|v|task.to_lowercase().contains(v));
 let last=steps.as_array().into_iter().flatten().rev().find(|s|s["tool"]=="blender"&&matches!(s["kind"].as_str(),Some("done"|"error"|"denied")));
 if last.is_some_and(|s|s["kind"]=="done"&&s["result"]["verified"]==true&&s["result"]["exit_code"]==0&&(!needs_save||s["result"]["saved_file"].as_str().is_some_and(|p|!p.is_empty()))){let text=task.to_lowercase();if let Some(step)=last{for object in step["result"]["objects"].as_array().into_iter().flatten(){if let Some(name)=object["name"].as_str(){let lower=name.to_lowercase();let remove=[format!("delete {lower}"),format!("delete the {lower}"),format!("delete the default {lower}"),format!("delete default {lower}"),format!("remove the default {lower}"),format!("remove default {lower}"),format!("remove {lower}"),format!("remove the {lower}")].iter().any(|p|text.contains(p));let denied=["do not delete","don't delete","do not remove","don't remove"].iter().any(|p|text.contains(p));if remove&&!denied{return Err(format!("The saved Blender file still contains {name}, which the user asked to remove. The task is unfinished. Continue from the saved file and verify the requested change."));}}}}return Ok(());}
 Err("The Blender file task is unfinished. Choose a valid Blender operation that performs the requested changes, save output_file, and use returned scene verification. Inspection alone does not change a scene. Do not claim GUI permissions are missing unless a computer action actually returned that error.".into())
}
pub fn summary(steps:&Value)->String{let result=steps.as_array().into_iter().flatten().rev().find(|s|s["tool"]=="blender"&&s["kind"]=="done"&&s["result"]["verified"]==true).map(|s|&s["result"]);match result{Some(r)=>format!("{} Verified scene objects: {}.",r["saved_file"].as_str().map(|f|format!("Saved {f} in the selected workspace and reopened it successfully.")).unwrap_or("Inspected the Blender scene.".into()),r["objects"].as_array().into_iter().flatten().filter_map(|o|o["name"].as_str()).collect::<Vec<_>>().join(", ")),None=>"The Blender task has not been verified.".into()}}
#[cfg(test)]mod tests{use super::*;#[test]fn launch_or_failed_script_is_not_file_completion(){assert!(check_outcome("inspect Blender",&json!([{ "tool":"computer","kind":"done","result":{"opened":"Blender"}}])).is_err());assert!(check_outcome("inspect Blender",&json!([{ "tool":"blender","kind":"done","result":{"exit_code":1,"verified":false}}])).is_err());assert!(check_outcome("inspect Blender",&json!([{ "tool":"blender","kind":"done","result":{"exit_code":0,"verified":true}}])).is_ok());assert!(check_outcome("delete the default Cube and save the file",&json!([{ "tool":"blender","kind":"done","result":{"exit_code":0,"verified":true,"saved_file":"test.blend","objects":[{"name":"Cube"}]}}])).is_err());assert!(check_outcome("create a new Blender file",&json!([{ "tool":"blender","kind":"done","result":{"exit_code":0,"verified":true}}])).is_err());}}

// This ends only named, basic file goals proven by actual saved-scene checks.
// It does not choose an operation sequence or finish complex/unknown work early.
pub fn goal_verified(task:&str,steps:&Value)->bool{
 if check_outcome(task,steps).is_err(){return false;}let text=task.to_lowercase();
 if ["render","material","texture","animation","animate","rig ","sculpt","export","modifier","subdiv","rotate","scale","move ","color","colour"].iter().any(|s|text.contains(s)){return false;}
 let Some(r)=steps.as_array().into_iter().flatten().rev().find(|s|s["tool"]=="blender"&&s["kind"]=="done").map(|s|&s["result"])else{return false;};
 let Some(file)=r["saved_file"].as_str()else{return false;};if !basic_named_task(task,file){return false;}

 let remove_cube=["delete cube","delete the cube","delete the default cube","remove cube","remove the cube","remove the default cube"].iter().any(|s|text.contains(s));
 let sphere=["add a sphere","add sphere"].iter().any(|s|text.contains(s));let cube=["add a cube","add cube"].iter().any(|s|text.contains(s));if !(remove_cube||sphere||cube){return false;}
 if (text.contains("add ")&&!sphere&&!cube) || ((text.contains("delete ")||text.contains("remove "))&&!remove_cube){return false;}
 if ["to that file","to it","existing file"].iter().any(|s|text.contains(s))&&r["source_file"].as_str().is_none(){return false;}
 for (asked,primitive,default_name) in [(sphere,"sphere","Sphere"),(cube,"cube","Cube")]{if asked{let op=r["applied_operations"].as_array().into_iter().flatten().find(|o|o["op"]=="add_object"&&o["primitive"]==primitive);let Some(op)=op else{return false;};let name=op["name"].as_str().filter(|n|!n.is_empty()).unwrap_or(default_name);if !r["objects"].as_array().into_iter().flatten().any(|o|o["type"]=="MESH"&&o["name"].as_str().is_some_and(|n|n==name||n.starts_with(&format!("{name}.")))){return false;}}}
 true
}
#[cfg(test)]mod goal_tests{use super::*;#[test]fn proven_simple_goal_finishes_but_extra_work_does_not(){let steps=json!([{"tool":"blender","kind":"done","result":{"exit_code":0,"verified":true,"saved_file":"fabricTest.blend","objects":[{"name":"Camera"},{"name":"Light"}]}}]);assert!(goal_verified("Create a General file, delete the default cube, save as fabricTest.blend",&steps));assert!(!goal_verified("Create a General file, delete the cube and render an animation in fabricTest.blend",&steps));assert!(!goal_verified("Add a sphere and save as fabricTest.blend",&steps));assert!(!goal_verified("Delete the cube, add a torus and save as fabricTest.blend",&steps));}}

pub fn active_file(task:&str,history:&[Value],root:&str)->Option<String>{let text=task.to_lowercase();let contextual=["that file","to it","that scene","existing file"].iter().any(|p|text.contains(p))||(!text.contains(".blend")&&["add ","remove ","delete ","rename ","modify ","edit ","dent "].iter().any(|p|text.starts_with(p)));if !contextual{return None;}
 history.iter().rev().filter(|t|t["role"]=="assistant"&&t["workspace"]==root).flat_map(|t|t["steps"].as_array().into_iter().flatten().rev()).find(|s|s["tool"]=="blender"&&s["kind"]=="done"&&s["result"]["verified"]==true).and_then(|s|s["result"]["saved_file"].as_str()).filter(|file|!std::path::Path::new(file).is_absolute()&&!std::path::Path::new(file).components().any(|p|matches!(p,std::path::Component::ParentDir))).map(str::to_owned)
}
#[cfg(test)]mod context_tests{use super::*;#[test]fn edits_reuse_only_this_chats_verified_file_in_the_same_workspace(){let h=vec![json!({"role":"assistant","workspace":"/project","steps":[{"kind":"done","tool":"blender","result":{"verified":true,"saved_file":"fabricTest.blend"}}]})];assert_eq!(active_file("Add a sphere to that file and save as modified.blend",&h,"/project"),Some("fabricTest.blend".into()));assert!(active_file("Create a new General file",&h,"/project").is_none());assert!(active_file("Add a sphere to that file",&h,"/different").is_none());assert!(active_file("Add a sphere to that file",&[],"/project").is_none());}}

#[cfg(test)]mod retry_tests{use super::*;#[test]fn retry_context_excludes_unrelated_denials_and_preserves_partial_file(){let dir=std::env::temp_dir().join(format!("fabric-resume-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir_all(&dir).unwrap();std::fs::write(dir.join("fabricTest.blend"),b"fixture").unwrap();let h=vec![json!({"role":"assistant","content":"I have no tools","task_state":{"status":"blocked","request":"delete Cube"},"steps":[{"tool":"blender","kind":"done","result":{"verified":true,"saved_file":"fabricTest.blend"}}]})];assert_eq!(resumed_file("delete Cube",&h,dir.to_str().unwrap()),Some("fabricTest.blend".into()));assert!(resumed_file("new task",&h,dir.to_str().unwrap()).is_none());let c=planning_context("delete Cube","try again",Some("fabricTest.blend"));assert!(c.contains("delete Cube"));assert!(!c.contains("I have no tools"));let t=task_tool("delete Cube and save as fabricTest.blend",Some("fabricTest.blend"));assert!(t["inputSchema"]["anyOf"].as_array().unwrap().iter().all(|b|b["properties"]["operation"]["const"]!="create"));std::fs::remove_dir_all(dir).unwrap();}}

pub fn wants_mutation(task:&str)->bool{["delete","remove","add ","modify","change","rename","move ","rotate","scale","dent "].iter().any(|p|task.to_lowercase().contains(p))}
pub fn after_inspection(parameters:&mut Value){if let Some(branches)=parameters["anyOf"].as_array_mut(){branches.retain(|b|b["properties"]["operation"]["const"]!="inspect");}}
#[cfg(test)]mod inspection_budget_tests{use super::*;#[test]fn completed_inspection_leaves_real_edit_choices(){let mut t=task_tool("delete Cube and save as partial.blend",Some("partial.blend"));after_inspection(&mut t["inputSchema"]);let b=t["inputSchema"]["anyOf"].as_array().unwrap();assert_eq!(b.len(),1);assert_eq!(b[0]["properties"]["operation"]["const"],"edit");assert_eq!(b[0]["properties"]["input_file"]["enum"],json!(["partial.blend"]));}}

fn basic_named_task(task:&str,file:&str)->bool{let text=task.to_lowercase();let stem=std::path::Path::new(file).file_stem().and_then(|s|s.to_str()).unwrap_or("").to_lowercase();if stem.is_empty()||!text.contains(&stem){return false;}let plain=text.replace(&file.to_lowercase()," ").replace(&stem," ");let words=["blender","create","a","an","new","general","file","in","delete","remove","the","default","cube","and","save","as","workspace","name","named","call","called","it","add","sphere","to","that","existing","please","can","could","you","my","on","mac","then"];!plain.split(|c:char|!c.is_alphanumeric()).filter(|w|!w.is_empty()).any(|w|!words.contains(&w))}


/// A plain request for one new object needs no model-generated Python.
pub fn simple_scene(task: &str) -> Option<(&'static str, bool)> {
    let text = task.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim_end_matches(['.', '!']).to_lowercase();
    let text = text.strip_prefix("please ").unwrap_or(&text);
    let text = text.strip_suffix(", install blender mcp if you need it").unwrap_or(text);
    for (primitive, article) in [("sphere", "a"), ("cube", "a"), ("monkey", "a"), ("suzanne", "")] {
        for verb in ["make", "create", "add"] {
            let instruction = [verb, article, primitive].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ");
            let primitive = if primitive == "suzanne" { "monkey" } else { primitive };
            if text == format!("{instruction} in blender") {
                return Some((primitive, true));
            }
            if ["and", "then", "and then"].iter().any(|join| text == format!("open blender {join} {instruction}")) {
                return Some((primitive, true));
            }
        }
    }
    None
}

pub fn primitive_name(primitive: &str) -> &'static str {
    match primitive {
        "sphere" => "Sphere",
        "monkey" => "Suzanne",
        _ => "Cube",
    }
}

pub fn ensure_available() -> Result<(), String> {
    let mut apps = vec![std::path::PathBuf::from("/Applications/Blender.app")];
    if let Some(home) = std::env::var_os("HOME") {
        apps.push(std::path::PathBuf::from(home).join("Applications/Blender.app"));
    }
    if apps.iter().any(|app| ["Blender", "blender"].iter()
        .any(|binary| app.join("Contents/MacOS").join(binary).is_file())) {
        Ok(())
    } else {
        Err("Blender is unavailable. Install Blender in Applications, then retry this task.".into())
    }
}

pub fn needs_existing_scene(task: &str) -> bool {
    let text = task.to_lowercase();
    ["that file", "that scene", "existing file", "existing scene", "current scene", "current file",
      "this scene", "this file", "to it", "modify ", "edit ", "inspect ", "dent the ", "dent this "]
        .iter().any(|phrase| text.contains(phrase))
}

pub fn wants_new_scene(task: &str) -> bool {
    let text = task.to_lowercase();
    !needs_existing_scene(task) && ["make ", "create ", "new scene", "new general"]
        .iter().any(|phrase| text.contains(phrase))
}

pub fn workspace_scene(root: &str, file: &str) -> Option<String> {
    let root = std::path::Path::new(root).canonicalize().ok()?;
    let path = root.join(file).canonicalize().ok()?;
    if !path.is_file() || !path.extension().and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("blend")) {
        return None;
    }
    path.strip_prefix(root).ok()?.to_str().map(str::to_owned)
}

pub fn named_input(task: &str) -> Option<String> {
    let lower = task.to_ascii_lowercase();
    let before_output = ["save as ", "save to ", "name it ", "output to "].iter()
        .filter_map(|phrase| lower.find(phrase)).min().unwrap_or(task.len());
    let task = task.get(..before_output)?;
    for (open, close) in [('"', '"'), ('“', '”'), ('`', '`')] {
        if let Some((_, rest)) = task.split_once(open) {
            if let Some((quoted, _)) = rest.split_once(close) {
                if quoted.to_ascii_lowercase().ends_with(".blend") {
                    return Some(quoted.to_owned());
                }
            }
        }
    }
    task.split(|c: char| c.is_whitespace() || matches!(c, '"' | '“' | '”' | '`' | ',' | ';'))
        .map(|word| word.trim_matches(['.', '(', ')', '!', '?']))
        .find(|word| word.to_ascii_lowercase().ends_with(".blend"))
        .map(str::to_owned)
}

pub fn new_primitive_args(primitive: &str) -> Value {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let name = primitive_name(primitive);
    json!({"operation":"create","output_file":format!("{name}-{stamp}.blend"),"operations":[
        {"op":"remove_object","name":"Cube"},
        {"op":"add_object","primitive":primitive,"name":name}
    ]})
}

pub fn open_verified_scene(root: &str, result: &Value) -> Result<Value, String> {
    let relative = result["saved_file"].as_str().ok_or("Missing verified Blender file")?;
    let base = std::path::Path::new(root).canonicalize().map_err(|error| error.to_string())?;
    let file = base.join(relative).canonicalize().map_err(|error| error.to_string())?;
    if result["verified"] != true || !file.starts_with(&base) || !file.is_file()
        || file.extension().and_then(|extension| extension.to_str()) != Some("blend") {
        return Err("Only a verified Blender file inside the workspace can be opened.".into());
    }
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-n", "-a", "/Applications/Blender.app", "--args"]).arg(&file)
        .status().map_err(|error| error.to_string())?;
    if !status.success() { return Err("Blender could not open the verified scene.".into()); }
    Ok(json!({"app":"Blender","file":file,"dispatched":true}))
}


#[cfg(test)]
mod preparation_tests {
    use super::*;

    #[test]
    fn normalizes_a_suzanne_request_and_distinguishes_edit_inputs_from_output_names() {
        assert_eq!(simple_scene(" make   a monkey in Blender. "), Some(("monkey", true)));
        assert_eq!(simple_scene("create Suzanne in Blender"), Some(("monkey", true)));
        assert_eq!(new_primitive_args("monkey")["operations"][1]["name"], "Suzanne");
        assert!(wants_new_scene("create a new scene with a monkey in Blender"));
        assert!(!wants_new_scene("make a change to the current scene"));
        assert_eq!(named_input("edit \"My Scene.blend\" and save as new.blend").as_deref(), Some("My Scene.blend"));
        assert!(named_input("add a monkey to that scene and save as new.blend").is_none());
    }
}
