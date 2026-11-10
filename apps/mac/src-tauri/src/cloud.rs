//! Gated cloud generation. API credentials stay in the Rust process.
use serde_json::{json, Value};

pub fn configured() -> bool {
    crate::cloud_settings::enabled() && crate::cloud_settings::token().is_some()
}
pub fn model() -> String {
    if crate::cloud_settings::automatic(){crate::cloud_models::last().unwrap_or_else(crate::cloud_settings::model)}else{crate::cloud_settings::model()}
}
pub fn answer(question:&str,context:&str,history:&[Value])->Result<Value,String>{answer_options(question,context,history,false)}
pub fn answer_options(question: &str, context: &str, history: &[Value], web:bool) -> Result<Value, String> {
    let tier=crate::routing::fallback(question).complexity;
    let (selected,_)=crate::cloud_models::select(question,&tier)?;
    answer_model(question,context,history,web,&selected)
}
pub fn answer_model(question:&str,context:&str,history:&[Value],web:bool,selected:&str)->Result<Value,String>{answer_model_guided(question,context,history,web,selected,"")}
pub fn answer_model_guided(question:&str,context:&str,history:&[Value],web:bool,selected:&str,guidance:&str)->Result<Value,String>{
    if !configured() {
        return Err("Add an API token and enable cloud answers in Settings → Cloud models.".into());
    }
    if question.len() + context.len() > 65_536 {
        return Err("Cloud input exceeds limit".into());
    }
    let messages = crate::generation::messages_guided(question, context, history,guidance);
    let instructions=messages[0]["content"].as_str().unwrap_or(crate::generation::INSTRUCTIONS);
    let input: Vec<_> = messages
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["role"] != "system")
        .cloned()
        .collect();
    let mut body=json!({"model":selected,"store":false,"max_output_tokens":8192,
        "instructions":instructions,"input":input});
    if web {body["tools"]=json!([{"type":"web_search","search_context_size":"high"}]);body["instructions"]=json!(format!("{} Use live web search to verify the requested external facts. Verify the central claim, including exact labels, numbers, dates and entities, against sources from the originating organization. Follow supporting links when an article only reports aggregate results. A source listing a topic does not prove a claim about that topic. Search again when the cited source does not establish the claim. If verification fails, clearly mark that detail unverified. Prefer primary sources. Never use application receipts as evidence for world facts. Cite your sources inline. Do not invent a proof or describe an AI-unanswered problem as mathematically open.",instructions));}
    let value=request(body)?;
    let (answer,sources)=cited(&value)?;
    Ok(
        json!({"text":answer,"sources":sources,"model":value["model"],"provider":"cloud","tokens_per_second":null,
        "usage":value["usage"],"incomplete":value["status"]=="incomplete"}),
    )
}
/// Replace provider citation markers with the app's clickable, source-backed [Sx] links.
fn cited(value:&Value)->Result<(String,Vec<Value>),String>{
    let mut sources=Vec::<Value>::new();let mut parts=Vec::new();
    for item in value["output"].as_array().into_iter().flatten().filter(|i|i["type"]=="message") {
        for part in item["content"].as_array().into_iter().flatten().filter(|p|p["type"]=="output_text") {
            let mut chars=part["text"].as_str().unwrap_or("").chars().collect::<Vec<_>>();let mut edits=Vec::new();
            for citation in part["annotations"].as_array().into_iter().flatten().filter(|a|a["type"]=="url_citation") {
                let Some(url)=citation["url"].as_str().filter(|u|reqwest::Url::parse(u).is_ok_and(|u|matches!(u.scheme(),"http"|"https") && u.username().is_empty() && u.password().is_none())) else {continue};
                let index=if let Some(index)=sources.iter().position(|s|s["url"]==url){index}else{sources.push(json!({"url":url,"title":citation["title"].as_str().unwrap_or("Source"),"kind":"page","content":""}));sources.len()-1};
                if let (Some(start),Some(end))=(citation["start_index"].as_u64(),citation["end_index"].as_u64()) {if start<=end && end as usize<=chars.len(){edits.push((start as usize,end as usize,format!("[S{}]",index+1)));}}
            }
            edits.sort_by_key(|edit|std::cmp::Reverse(edit.0));
            let mut boundary=chars.len();for (start,end,replacement) in edits {if end<=boundary {chars.splice(start..end,replacement.chars());boundary=start;}}
            parts.push(chars.into_iter().collect::<String>());
        }
    }
    let answer=parts.join("\n");if answer.trim().is_empty(){return Err("Cloud returned no answer text".into());}Ok((answer,sources))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn annotations_become_app_citations(){
        let value=json!({"output":[{"type":"message","content":[{"type":"output_text","text":"Verified. [cite]","annotations":[{"type":"url_citation","url":"https://example.org/source","title":"Evidence","start_index":10,"end_index":16}]}]}]});
        let (text,sources)=cited(&value).unwrap();assert_eq!(text,"Verified. [S1]");assert_eq!(sources[0]["url"],"https://example.org/source");
    }
    #[test]
    fn response_parser_ignores_non_message_output() {
        let value = json!({"output":[{"type":"reasoning","content":[{"type":"output_text","text":"hidden"}]},
            {"type":"message","content":[{"type":"output_text","text":"Actual answer"}]}]});
        assert_eq!(cited(&value).unwrap().0, "Actual answer");
        assert!(cited(&json!({"output":[]})).is_err());
    }
}

/// Cloud selects function calls; Fabric reviews and executes every local action.
pub fn tool_request(model:&str,messages:Value,tools:Value)->Result<Value,String>{
    let mut input=Vec::new();let mut instructions=String::new();
    for message in messages.as_array().into_iter().flatten(){
        match message["role"].as_str(){
            Some("system")=>instructions.push_str(message["content"].as_str().unwrap_or("")),
            Some("assistant")=>{if let Some(output)=message["response_output"].as_array(){input.extend(output.iter().cloned());}},
            Some("tool")=>input.push(json!({"type":"function_call_output","call_id":message["tool_call_id"],"output":message["content"]})),
            Some("user")=>input.push(json!({"role":"user","content":message["content"]})),_=>{}
        }
    }
    let definitions=tools.as_array().into_iter().flatten().map(|tool|json!({"type":"function","name":tool["function"]["name"],"description":tool["function"]["description"],"parameters":tool["function"]["parameters"],"strict":false})).collect::<Vec<_>>();
    let value=request(json!({"model":model,"store":false,"input":input,"instructions":instructions,"tools":definitions,"parallel_tool_calls":false,"include":["reasoning.encrypted_content"],"max_output_tokens":8192}))?;
    let calls=value["output"].as_array().into_iter().flatten().filter(|item|item["type"]=="function_call").map(|item|json!({"id":item["call_id"],"function":{"name":item["name"],"arguments":item["arguments"]}})).collect::<Vec<_>>();
    Ok(json!({"role":"assistant","content":"","tool_calls":calls,"response_output":value["output"]}))
}
fn request(body:Value)->Result<Value,String>{
    let key=crate::cloud_settings::token().ok_or("Add a cloud token first")?;
    crate::usage::cloud_call();
    let response=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(300)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())?.post(crate::cloud_settings::endpoint("responses")).bearer_auth(key).json(&body).send().map_err(|e|if e.is_timeout(){"The cloud provider timed out after five minutes. It may still have processed the request."}else if e.is_connect(){"Could not connect to the cloud provider. Check your internet connection and provider URL."}else{"The connection to the cloud provider failed before a response was received."})?;
    if !response.status().is_success(){return Err(format!("Cloud model {} returned HTTP {}. Refresh available models or pin another model in Settings.",body["model"].as_str().unwrap_or(""),response.status().as_u16()));}
    let value:Value=response.json().map_err(|_|"Invalid cloud model response")?;crate::usage::cloud_usage(value["model"].as_str().unwrap_or(""),&value["usage"]);Ok(value)
}
