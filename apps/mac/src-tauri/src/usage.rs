use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Totals {
    cloud_calls: u64,
    historical_cloud_replies: u64,
    local_answers: u64,
    local_calls: u64,
    estimated_saved_usd: f64,
    estimated_cloud_usd: f64,
    unpriced_answers: u64,
    baseline_model:String,
}
fn path() -> PathBuf {
    PathBuf::from(std::env::var_os("FABRIC_DATA_DIR").unwrap_or_default()).join("usage.json")
}
fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
fn totals() -> Totals {
    std::fs::read(path())
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default()
}
fn rates(model: &str) -> Option<(f64, f64, f64)> {
    let custom = || {
        Some((
            std::env::var("FABRIC_CLOUD_INPUT_PER_MILLION")
                .ok()?
                .parse::<f64>()
                .ok()?,
            std::env::var("FABRIC_CLOUD_OUTPUT_PER_MILLION")
                .ok()?
                .parse::<f64>()
                .ok()?,
            std::env::var("FABRIC_CLOUD_INPUT_PER_MILLION")
                .ok()?
                .parse::<f64>()
                .ok()?,
        ))
    };
    let values =
        custom().or_else(|| {
            [("gpt-4.1-mini",(0.4,1.6,0.1)),("gpt-6.1-sol",(2.,10.,0.1)),("gpt-6-astra",(10.,50.,1.)),("gpt-6.1-terra",(1.,5.,0.05)),("gpt-6-luna",(0.1,0.5,0.01))].into_iter().find(|(name,_)|model==*name || model.strip_prefix(&format!("{name}-")).is_some_and(|suffix|suffix.starts_with("20") && suffix.chars().take(4).all(|c|c.is_ascii_digit()))).map(|(_,price)|price)
        })?;
    [values.0, values.1, values.2]
        .iter()
        .all(|n| n.is_finite() && *n >= 0.)
        .then_some(values)
}
fn price(model: &str, input: u64, output: u64, cached: u64) -> Option<f64> {
    let (mut i, mut o, mut c) = rates(model)?;
    if model.starts_with("gpt-6") && input>272000{i*=2.;c*=2.;o*=1.5;}
    let cached = cached.min(input);
    Some(((input - cached) as f64 * i + cached as f64 * c + output as f64 * o) / 1_000_000.)
}
fn change(f: impl FnOnce(&mut Totals)) {
    if let Ok(_guard) = lock().lock() {
        let mut total = totals();
        f(&mut total);
        let _ = fabric_core::write_json(&path(), &total);
    }
}
pub fn local_call() {
    change(|v| v.local_calls += 1)
}
pub fn cloud_call() {
    change(|v| v.cloud_calls += 1)
}
pub fn cloud_usage(model: &str, usage: &Value) {
    if crate::cloud_settings::provider_id()!="openai" {change(|v|v.unpriced_answers+=1);return;}
    change(|v| {
        if let Some(cost) = price(
            model,
            usage["input_tokens"].as_u64().unwrap_or(0),
            usage["output_tokens"].as_u64().unwrap_or(0),
            usage["input_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(0),
        ) {
            let writes=usage["input_tokens_details"]["cache_write_tokens"].as_u64().unwrap_or(0).min(usage["input_tokens"].as_u64().unwrap_or(0));
            let extra=if model.starts_with("gpt-6"){rates(model).map_or(0.,|r|writes as f64*r.0*0.25*if usage["input_tokens"].as_u64().unwrap_or(0)>272000{2.}else{1.}/1_000_000.)}else{0.};
            v.estimated_cloud_usd += cost+extra;
        } else {
            v.unpriced_answers += 1;
        }
    })
}
pub fn local_answer(answer: &Value,baseline:&str) {
    let qualified=if crate::cloud_settings::provider_id()!="openai"{format!("{} / {baseline}",crate::cloud_settings::provider_name())}else{baseline.into()};let baseline=qualified.as_str();
    change(|v| {
        v.local_answers += 1;
        v.baseline_model=baseline.into();
        if let Some(cost) = price(
            baseline,
            answer["usage"]["input_tokens"].as_u64().unwrap_or(0),
            answer["usage"]["output_tokens"].as_u64().unwrap_or(0),
            0,
        ) {
            v.estimated_saved_usd += cost
        } else {
            v.unpriced_answers += 1;
        }
    })
}
pub fn status() -> Value {
    let _guard = lock().lock().ok();
    let t = totals();
    let baseline=if t.baseline_model.is_empty(){crate::cloud_models::comparison("simple")}else{t.baseline_model.clone()};
    json!({"cloud_calls":t.cloud_calls,"local_calls":t.local_calls,"local_answers":t.local_answers,"historical_cloud_replies":t.historical_cloud_replies,"estimated_saved_usd":t.estimated_saved_usd,"estimated_cloud_usd":t.estimated_cloud_usd,"unpriced_answers":t.unpriced_answers,"baseline_model":baseline,"priced":rates(&baseline).is_some(),"note":"Calls count generation requests, including routing, privacy checks, and action planning; failed requests count too. Since tracking began. Savings estimate each local answer at the complexity-matched cloud model’s standard rates (configured fallback before model discovery), before caching or discounts; exclude hardware, electricity, and tool costs. Not a billing statement.","pricing_source":"https://developers.openai.com/api/docs/pricing","pricing_checked":"2026-11-09"})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_tokens_cost_less() {
        assert!((price("gpt-4.1-mini", 1000000, 1000000, 500000).unwrap() - 1.85).abs() < 1e-8);
    }
    #[test]
    fn unknown_models_are_not_given_fake_prices() {
        assert!(price("unrecognized-model", 5, 5, 0).is_none());
    }
}

pub fn seed_history(store: &crate::conversations::Store) {
    let Ok(_guard) = lock().lock() else { return };
    if path().exists() {
        return;
    }
    let count = store
        .conversations
        .iter()
        .chain(store.deleted.iter())
        .flat_map(|c| c.turns.iter())
        .filter(|t| t["role"] == "assistant" && t["provider"] == "cloud")
        .count() as u64;
    let total = Totals {
        cloud_calls: count,
        historical_cloud_replies: count,
        ..Default::default()
    };
    let _ = fabric_core::write_json(&path(), &total);
}
