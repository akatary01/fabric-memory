//! Gated cloud generation. API credentials stay in the Rust process.
use serde_json::{json, Value};

pub fn configured() -> bool {
    std::env::var("FABRIC_CLOUD_ENABLED").as_deref() == Ok("true")
        && std::env::var("OPENAI_API_KEY").is_ok_and(|key| !key.trim().is_empty())
}
pub fn model() -> String {
    std::env::var("OPENAI_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "gpt-4.1-mini".into())
}
fn text(value: &Value) -> Result<String, String> {
    let answer = value["output"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "message")
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter(|item| item["type"] == "output_text")
        .filter_map(|item| item["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if answer.trim().is_empty() {
        Err("Cloud returned no answer text".into())
    } else {
        Ok(answer)
    }
}
pub fn answer(question: &str, context: &str, history: &[Value]) -> Result<Value, String> {
    if !configured() {
        return Err("Configure OPENAI_API_KEY and FABRIC_CLOUD_ENABLED=true in apps/mac/.env, then reopen the Mac app.".into());
    }
    if question.len() + context.len() > 65_536 {
        return Err("Cloud input exceeds limit".into());
    }
    let key = std::env::var("OPENAI_API_KEY").map_err(|_| "Missing API key")?;
    let messages = crate::generation::messages(question, context, history);
    let input: Vec<_> = messages
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["role"] != "system")
        .cloned()
        .collect();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot initialize cloud client")?;
    let response = client
        .post("https://api.openai.com/v1/responses")
        .bearer_auth(key)
        .json(
            &json!({"model":model(),"store":false,"max_output_tokens":1024,
            "instructions":crate::generation::INSTRUCTIONS,"input":input}),
        )
        .send()
        .map_err(|_| "Cloud request failed; check the internet connection")?;
    if !response.status().is_success() {
        return Err(format!(
            "Cloud returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let value: Value = response.json().map_err(|_| "Invalid cloud response")?;
    Ok(
        json!({"text":text(&value)?,"model":value["model"],"provider":"cloud","tokens_per_second":null,
        "usage":value["usage"],"incomplete":value["status"]=="incomplete"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn response_parser_ignores_non_message_output() {
        let value = json!({"output":[{"type":"reasoning","content":[{"type":"output_text","text":"hidden"}]},
            {"type":"message","content":[{"type":"output_text","text":"Actual answer"}]}]});
        assert_eq!(text(&value).unwrap(), "Actual answer");
        assert!(text(&json!({"output":[]})).is_err());
    }
}
