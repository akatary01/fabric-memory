pub mod adapter;
pub mod encoder;
pub mod ledger;

use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const MAX_TEXT_BYTES: usize = 65_536;
pub const MAX_EVENTS: usize = 10_000;
pub const MAX_TRANSFER_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Embedding {
    pub space: String,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Access {
    pub scope: String,
    #[serde(default)]
    pub allowed_devices: Vec<String>,
    #[serde(default)]
    pub reason: String,
}
impl Default for Access {
    fn default() -> Self {
        Self {
            scope: "public".into(),
            allowed_devices: vec![],
            reason: String::new(),
        }
    }
}
impl Access {
    pub fn legacy(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            ["private", "public", "restricted"].contains(&self.scope.as_str()),
            "Invalid memory visibility"
        );
        ensure!(
            self.reason.len() <= 1000 && self.allowed_devices.len() <= 64,
            "Invalid memory access metadata"
        );
        ensure!(
            self.allowed_devices.iter().all(|id| id.len() == 32
                && id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())),
            "Invalid approved device ID"
        );
        ensure!(
            self.scope == "restricted" || self.allowed_devices.is_empty(),
            "Only restricted memories have approved devices"
        );
        Ok(())
    }
    pub fn syncable(&self) -> bool {
        self.scope == "public"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub version: u32,
    pub id: String,
    pub actor: String,
    pub clock: u64,
    pub memory: String,
    pub deleted: bool,
    pub text: String,
    pub embedding: Option<Embedding>,
    #[serde(default, skip_serializing_if = "Access::legacy")]
    pub access: Access,
}

impl Event {
    pub fn validate(&self) -> Result<()> {
        self.access.validate()?;
        ensure!(self.version == 1, "unsupported event version");
        for id in [&self.id, &self.actor] {
            ensure!(
                uuid::Uuid::parse_str(id)?.to_string() == *id,
                "non-canonical UUID"
            );
        }
        ensure!(
            self.clock > 0 && self.clock < 9_007_199_254_740_991,
            "invalid logical clock"
        );
        ensure!(
            !self.memory.is_empty()
                && self.memory.len() <= 128
                && self.memory.bytes().all(|b| (33..=126).contains(&b)),
            "invalid memory ID"
        );
        ensure!(
            self.text.len() <= MAX_TEXT_BYTES,
            "memory text exceeds limit"
        );
        if self.deleted {
            ensure!(
                self.text.is_empty() && self.embedding.is_none(),
                "tombstone contains content"
            );
        } else {
            let e = self
                .embedding
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing embedding"))?;
            ensure!(
                e.space.len() == 64
                    && e.space
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "invalid embedding space"
            );
            ensure!(
                (1..=4096).contains(&e.values.len()),
                "invalid embedding dimension"
            );
            normalized(&e.values)?;
        }
        Ok(())
    }
}

pub fn normalized(values: &[f32]) -> Result<Vec<f32>> {
    ensure!(
        !values.is_empty() && values.iter().all(|v| v.is_finite()),
        "invalid vector"
    );
    let norm = values
        .iter()
        .map(|v| (*v as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    ensure!(norm.is_finite() && norm > 1e-12, "zero vector");
    Ok(values.iter().map(|v| (*v as f64 / norm) as f32).collect())
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn read_events(path: &Path) -> Result<Vec<Event>> {
    ensure!(
        path.metadata()?.len() <= MAX_TRANSFER_BYTES as u64,
        "transfer exceeds 32 MiB"
    );
    let bytes = std::fs::read(path)?;
    ensure!(bytes.len() <= MAX_TRANSFER_BYTES, "transfer exceeds limit");
    let mut events = Vec::new();
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        if events.len() >= MAX_EVENTS {
            bail!("transfer exceeds event limit");
        }
        let event: Event = serde_json::from_slice(line)?;
        event.validate()?;
        events.push(event);
    }
    Ok(events)
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let data = serde_json::to_vec_pretty(value)?;
    use std::io::Write;
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&data)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
