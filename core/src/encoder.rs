use crate::{normalized, sha256, Embedding};
use anyhow::{ensure, Result};
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tokenizers::Tokenizer;

pub fn initialize_runtime(path: &str) -> Result<()> {
    ort::init_from(path).commit()?;
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EncoderManifest {
    pub version: u32,
    pub model: String,
    pub revision: String,
    pub dimension: usize,
    pub max_tokens: usize,
    pub space: String,
    pub files: HashMap<String, String>,
    pub pipeline: String,
}

pub struct Encoder {
    pub manifest: EncoderManifest,
    tokenizer: Tokenizer,
    session: Session,
}

impl Encoder {
    pub fn load(folder: &Path, threads: usize) -> Result<Self> {
        let manifest: EncoderManifest =
            serde_json::from_slice(&std::fs::read(folder.join("encoder.json"))?)?;
        ensure!(
            manifest.version == 1 && manifest.max_tokens == 256 && manifest.dimension <= 4096,
            "unsupported encoder configuration"
        );
        for (name, hash) in &manifest.files {
            ensure!(
                !name.contains('/') && !name.contains('\\') && !name.starts_with('.'),
                "invalid asset path"
            );
            ensure!(
                sha256(&std::fs::read(folder.join(name))?) == *hash,
                "asset checksum mismatch: {name}"
            );
        }
        let mut tokenizer = Tokenizer::from_file(folder.join("tokenizer.json"))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        tokenizer
            .with_truncation(Some(tokenizers::utils::truncation::TruncationParams {
                max_length: manifest.max_tokens,
                ..Default::default()
            }))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        tokenizer.with_padding(None);
        let session = Session::builder()?
            .with_intra_threads(threads)?
            .commit_from_file(folder.join("model.onnx"))?;
        Ok(Self {
            manifest,
            tokenizer,
            session,
        })
    }

    pub fn encode(&mut self, text: &str) -> Result<Embedding> {
        let enc = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let len = enc.len();
        let tensor = |v: Vec<i64>| Tensor::from_array(([1usize, len], v.into_boxed_slice()));
        let ids = tensor(enc.get_ids().iter().map(|v| *v as i64).collect())?;
        let mask = tensor(enc.get_attention_mask().iter().map(|v| *v as i64).collect())?;
        let mut inputs = ort::inputs! {"input_ids" => ids, "attention_mask" => mask};
        if self
            .session
            .inputs
            .iter()
            .any(|v| v.name == "token_type_ids")
        {
            inputs.push((
                "token_type_ids".into(),
                tensor(enc.get_type_ids().iter().map(|v| *v as i64).collect())?.into(),
            ));
        }
        let outputs = self.session.run(inputs)?;
        let (shape, values) = outputs[0].try_extract_tensor::<f32>()?;
        let dim = self.manifest.dimension;
        ensure!(
            shape.len() == 3 && shape[0] == 1 && shape[1] == len as i64 && shape[2] == dim as i64,
            "unexpected encoder output"
        );
        let mut pooled = vec![0f32; dim];
        let mut count = 0f32;
        for (row, mask) in values.chunks_exact(dim).zip(enc.get_attention_mask()) {
            if *mask != 0 {
                count += 1.;
                for (p, v) in pooled.iter_mut().zip(row) {
                    *p += v;
                }
            }
        }
        for p in &mut pooled {
            *p /= count;
        }
        Ok(Embedding {
            space: self.manifest.space.clone(),
            values: normalized(&pooled)?,
        })
    }
}
