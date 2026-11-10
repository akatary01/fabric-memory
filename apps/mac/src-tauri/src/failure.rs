//! Deterministic failure explanations: no model calls, no invented recovery steps.
use serde_json::{json,Value};
pub fn report(error:&str,steps:&Value)->Value{
 let lower=error.to_lowercase();
 let (reason,help,settings,local)=if lower.contains("task stopped"){
 ("You stopped this task.","Tell me what you want to continue. I should check the current state before repeating any action.",false,false)
 }else if lower.contains("no progress") {
 ("The agent repeated an action without making progress.","I stopped the repetition. The last error or unchanged result is below. Correct the path or working folder, give me the missing information, or choose another planning model; completed changes are retained.",true,false)
 }else if lower.contains("computer")||lower.contains("accessibility")||lower.contains("screen recording") {
 ("The requested computer action did not finish.","Check the detail below. You can open Connections to check Computer use and macOS access, tell me the browser or app name, or change the local model in Settings → General. Then I can continue from the last confirmed state.",false,false)
 }else if lower.contains("private") {
 ("This request was kept on this device by the privacy settings.","Choose Local to continue privately, or remove the private details before using cloud or web access. What would you prefer?",false,true)
 }else if lower.contains("401")||lower.contains("403")||lower.contains("token")||lower.contains("keychain"){
 ("The cloud provider could not authorize this request.","Check the provider token and model access in Settings → Cloud, or choose Local. Tell me when you have updated it.",true,true)
 }else if lower.contains("429"){
 ("The cloud provider rejected the request because of a rate or usage limit.","Check your provider's quota or billing, wait and try again, or choose Local. Which would you like to do?",true,true)
 }else if lower.contains("timeout")||lower.contains("timed out")||lower.contains("connect")||lower.contains("network"){
 ("A service did not respond successfully.","Check the connection and service address. Tell me what is available now so I can continue. A timed-out request may still have run; I should check before repeating actions.",true,true)
 }else if lower.contains("model")||lower.contains("ollama"){
 ("The selected model could not complete the request.","Check that the model is loaded, refresh the cloud model list, or select another model. Tell me which model you want to try.",true,true)
 }else if lower.contains("workspace")||lower.contains("permission")||lower.contains("no such file")||lower.contains("not found"){
 ("A required file, permission or program was unavailable.","Check the working folder and the detail below. Tell me the correct path, grant the requested permission, or install the missing program; then I can continue from the current files.",true,false)
 }else if lower.contains("limit")||lower.contains("64"){
 ("This task reached a limit before completion.","Completed changes are retained. Tell me which part to continue, or narrow the next step. I should inspect the current state first.",false,false)
 }else{
 ("The task stopped because an operation failed.","The detail below is the error I received. Tell me anything that changed, provide the missing information, or edit the request. I can use that to work out the next step.",false,false)
 };
 let phase=steps.as_array().into_iter().flatten().rev().find(|s|s.get("phase").is_some() || s["kind"]=="phase").and_then(|s|s["summary"].as_str());
 let completed=steps.as_array().into_iter().flatten().filter(|s|s["kind"]=="done").count();
 let last=steps.as_array().into_iter().flatten().rev().find(|s|s["kind"]=="done" && s["background"]!=true).and_then(|s|s["summary"].as_str()).map(|s|s.chars().take(240).collect::<String>());
 let detail=error.chars().take(1600).collect::<String>();
 let mut content=reason.to_string();if let Some(phase)=phase{content.push_str(&format!("\n\nWhile: {phase}."));}if let Some(last)=last{content.push_str(&format!("\n\nLast completed operation: {last}. This does not confirm the entire task succeeded."));}
 content.push_str(&format!("\n\nDetail: {detail}\n\n{help}"));
 json!({"content":content,"failure":{"detail":detail,"settings":settings,"local":local,"connections":lower.contains("computer")||lower.contains("accessibility")||lower.contains("screen recording"),"completed_operations":completed}})
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn failures_explain_the_problem_and_preserve_uncertainty(){let result=report("Cloud HTTP 429",&json!([{"kind":"phase","summary":"Writing reply"},{"kind":"done","summary":"Updated file"}]));let text=result["content"].as_str().unwrap();assert!(text.contains("quota"));assert!(text.contains("Updated file"));assert!(text.contains("does not confirm"));assert_eq!(result["failure"]["completed_operations"],1);}
 #[test]fn stop_is_not_a_provider_failure_and_details_are_bounded(){assert!(report("Task stopped. Completed actions remain in Activity.",&json!([]))["content"].as_str().unwrap().starts_with("You stopped"));assert_eq!(report(&"x".repeat(5000),&json!([]))["failure"]["detail"].as_str().unwrap().len(),1600);}
}
