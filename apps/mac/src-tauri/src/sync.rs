use fabric_core::Event;
use serde_json::{json, Value};
use std::path::Path;

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    token: String,
}
impl Client {
    pub fn load(path: &Path) -> Result<Self, String> {
        let beacon = path
            .parent()
            .ok_or("Missing beacon settings")?
            .join("beacon.json");
        let config: Value = serde_json::from_slice(
            &std::fs::read(beacon)
                .map_err(|_| "Connect a Memory beacon in Devices to enable sync")?,
        )
        .map_err(|_| "Invalid sync configuration")?;
        let mut builder = reqwest::blocking::Client::builder();
        if config["ca_der"].is_string() {
            return Ok(Self{http:crate::beacon::http_config(&config)?,base:crate::beacon::endpoint(config["url"].as_str().ok_or("Missing beacon URL")?)?,token:config["token"].as_str().ok_or("Missing credential")?.into()});
        }
        if config.get("ca_certificate").is_some() {
            let certificate = std::fs::read(
                config
                    .get("ca_certificate")
                    .unwrap_or(&config["certificate"])
                    .as_str()
                    .ok_or("Missing certificate")?,
            )
            .map_err(|_| "Cannot read sync certificate")?;
            let cert = reqwest::Certificate::from_pem(&certificate)
                .map_err(|_| "Invalid sync certificate")?;
            builder = builder.add_root_certificate(cert);
        }
        let base = config["url"].as_str().ok_or("Missing beacon URL")?;
        let url = reqwest::Url::parse(base).map_err(|_| "Invalid beacon URL")?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Invalid beacon URL".into());
        }
        let http = builder
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(100))
            .build()
            .map_err(|_| "Cannot initialize sync client")?;
        // The desktop uses loopback; the paired iPhone uses the Mac's .local name.
        Ok(Self {
            http,
            base: base.trim_end_matches('/').into(),
            token: config["token"]
                .as_str()
                .ok_or("Missing pairing credential")?
                .into(),
        })
    }
    fn response(response: reqwest::blocking::Response) -> Result<Value, String> {
        let ok = response.status().is_success();
        let value: Value = response
            .json()
            .map_err(|_| "Invalid response from Mac sync service")?;
        if !ok {
            return Err(value["error"]
                .as_str()
                .unwrap_or("Sync service rejected request")
                .into());
        }
        Ok(value)
    }
    pub fn permits(&self, event: &Event, actor: &str) -> bool {
        event.actor == actor
            || event.access.scope == "public"

    }
    pub fn push(&self, events: &[Event]) -> Result<u64, String> {
        if events.is_empty() {
            return Ok(0);
        }
        let response = self
            .http
            .post(format!("{}/v1/events", self.base))
            .bearer_auth(&self.token)
            .json(&json!({"events":events}))
            .send()
            .map_err(|error| format!("Mac sync service is unavailable: {error:?}"))?;
        Ok(Self::response(response)?["inserted"].as_u64().unwrap_or(0))
    }
    pub fn pull(&self, commit: Option<&str>, after: Option<&str>) -> Result<Value, String> {
        let mut request = self
            .http
            .get(format!("{}/v1/events", self.base))
            .bearer_auth(&self.token);
        if let Some(v) = commit {
            request = request.query(&[("commit", v)])
        }
        if let Some(v) = after {
            request = request.query(&[("after", v)])
        }
        Self::response(
            request
                .send()
                .map_err(|error| format!("Mac sync service is unavailable: {error:?}"))?,
        )
    }
}

// A single worker coalesces writes while retaining a periodic pull/retry.
#[derive(Default)]
pub struct Wake {
    pending: std::sync::Mutex<bool>,
    changed: std::sync::Condvar,
}
impl Wake {
    pub fn request(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            *pending = true;
            self.changed.notify_one();
        }
    }
    pub fn wait(&self, timeout: std::time::Duration) {
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let (mut pending, _) = self
            .changed
            .wait_timeout_while(pending, timeout, |p| !*p)
            .unwrap_or_else(|e| e.into_inner());
        *pending = false;
    }
}

#[cfg(test)]
mod wake_tests {
    #[test]
    fn changes_before_wait_are_not_lost_and_requests_coalesce() {
        let wake = super::Wake::default();
        wake.request();
        wake.request();
        wake.wait(std::time::Duration::from_secs(60));
        assert!(!*wake.pending.lock().unwrap());
    }
}
