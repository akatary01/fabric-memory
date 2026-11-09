use crate::{normalized, sha256, Embedding};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
    pub version: u32,
    pub source: String,
    pub target: String,
    pub input_dim: usize,
    pub output_dim: usize,
    pub source_mean: Vec<f32>,
    pub target_mean: Vec<f32>,
    pub weights_sha256: String,
    pub training_sha256: String,
    pub ridge: f64,
}

pub struct Adapter {
    pub id: String,
    pub manifest: AdapterManifest,
    weights: Vec<f32>,
}

impl Adapter {
    pub fn load(folder: &Path) -> Result<Self> {
        let manifest_bytes = std::fs::read(folder.join("adapter.json"))?;
        let id = sha256(&manifest_bytes);
        let manifest: AdapterManifest = serde_json::from_slice(&manifest_bytes)?;
        ensure!(
            manifest.version == 1 && manifest.source != manifest.target,
            "invalid adapter manifest"
        );
        ensure!(
            (1..=4096).contains(&manifest.input_dim) && (1..=4096).contains(&manifest.output_dim),
            "invalid dimensions"
        );
        ensure!(
            manifest.source_mean.len() == manifest.input_dim
                && manifest.target_mean.len() == manifest.output_dim,
            "invalid centering vectors"
        );
        ensure!(
            manifest
                .source_mean
                .iter()
                .chain(&manifest.target_mean)
                .all(|v| v.is_finite()),
            "invalid centering values"
        );
        let bytes = std::fs::read(folder.join("weights.f32"))?;
        ensure!(
            bytes.len() == manifest.input_dim * manifest.output_dim * 4,
            "incorrect weight size"
        );
        ensure!(
            sha256(&bytes) == manifest.weights_sha256,
            "adapter checksum mismatch"
        );
        let weights: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        ensure!(
            weights.iter().all(|w| w.is_finite()),
            "invalid adapter weights"
        );
        Ok(Self {
            id,
            manifest,
            weights,
        })
    }

    pub fn apply(&self, input: &Embedding) -> Result<Embedding> {
        let m = &self.manifest;
        ensure!(
            input.space == m.source && input.values.len() == m.input_dim,
            "adapter source mismatch"
        );
        let source = normalized(&input.values)?;
        let mut output = m.target_mean.clone();
        for (i, x) in source.iter().enumerate() {
            let centered = x - m.source_mean[i];
            for (j, y) in output.iter_mut().enumerate() {
                *y += centered * self.weights[i * m.output_dim + j];
            }
        }
        Ok(Embedding {
            space: m.target.clone(),
            values: normalized(&output)?,
        })
    }
}
