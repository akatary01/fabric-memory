use anyhow::{ensure, Result};
use clap::{Parser, Subcommand};
use fabric_core::{
    adapter::Adapter,
    encoder::{Encoder, EncoderManifest},
    ledger::Ledger,
    read_events, write_json, Embedding,
};
use std::{path::PathBuf, time::Instant};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    runtime: PathBuf,
    #[arg(long)]
    model: PathBuf,
    #[arg(long, default_value = "runs/mac.sqlite")]
    db: PathBuf,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Put {
        memory: String,
        text: String,
    },
    Delete {
        memory: String,
    },
    Query {
        text: String,
        #[arg(short, default_value_t = 5)]
        k: usize,
    },
    Encode {
        input: PathBuf,
        output: PathBuf,
    },
    SearchBatch {
        input: PathBuf,
        output: PathBuf,
        #[arg(short, default_value_t = 5)]
        k: usize,
    },
    Import {
        input: PathBuf,
        #[arg(long)]
        adapter: PathBuf,
    },
    Export {
        output: PathBuf,
    },
    Status,
    Archive {
        folder: PathBuf,
    },
    Stress {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 1000)]
        count: usize,
        #[arg(long)]
        report: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    let manifest: EncoderManifest =
        serde_json::from_slice(&std::fs::read(args.model.join("encoder.json"))?)?;
    let mut ledger = Ledger::open(&args.db, &manifest.space, manifest.dimension)?;
    let needs_encoder = matches!(
        args.command,
        Action::Put { .. } | Action::Query { .. } | Action::Encode { .. }
    );
    let mut encoder = if needs_encoder {
        ort::init_from(args.runtime.to_string_lossy()).commit()?;
        Some(Encoder::load(&args.model, 2)?)
    } else {
        None
    };
    match args.command {
        Action::Put { memory, text } => println!(
            "{}",
            serde_json::to_string(&ledger.put(
                &memory,
                &text,
                Some(encoder.as_mut().unwrap().encode(&text)?)
            )?)?
        ),
        Action::Delete { memory } => {
            ledger.put(&memory, "", None)?;
        }
        Action::Query { text, k } => println!(
            "{}",
            serde_json::to_string_pretty(
                &ledger.search(&encoder.as_mut().unwrap().encode(&text)?, k)?
            )?
        ),
        Action::Encode { input, output } => {
            let texts: Vec<String> = serde_json::from_slice(&std::fs::read(input)?)?;
            let result: Vec<Embedding> = texts
                .iter()
                .map(|t| encoder.as_mut().unwrap().encode(t))
                .collect::<Result<_>>()?;
            write_json(&output, &result)?;
        }
        Action::SearchBatch { input, output, k } => {
            let queries: Vec<Embedding> = serde_json::from_slice(&std::fs::read(input)?)?;
            let hits: Vec<_> = queries
                .iter()
                .map(|q| ledger.search(q, k))
                .collect::<Result<_>>()?;
            write_json(&output, &hits)?;
        }
        Action::Import { input, adapter } => {
            println!(
                "inserted={}",
                ledger.ingest(&read_events(&input)?, Some(&Adapter::load(&adapter)?))?
            );
        }
        Action::Export { output } => ledger.export(&output)?,
        Action::Status => println!("{}", serde_json::to_string_pretty(&ledger.status()?)?),
        Action::Archive { folder } => ledger.archive_git(&folder)?,
        Action::Stress {
            input,
            count,
            report,
        } => {
            ensure!(
                count > 0 && count <= 100_000,
                "count must be between 1 and 100000"
            );
            let pool = read_events(&input)?;
            ensure!(!pool.is_empty(), "empty vector pool");
            let mut times = Vec::new();
            let begin = Instant::now();
            for i in 0..count {
                let source = &pool[i % pool.len()];
                ensure!(!source.deleted, "stress pool contains tombstone");
                let t = Instant::now();
                ledger.put(
                    &format!("stress-{i}"),
                    &source.text,
                    source.embedding.clone(),
                )?;
                times.push(t.elapsed().as_secs_f64() * 1000.);
            }
            let elapsed = begin.elapsed().as_secs_f64();
            times.sort_by(f64::total_cmp);
            let mut queries = Vec::new();
            for event in pool.iter().take(8) {
                let query = event.embedding.as_ref().unwrap();
                let t = Instant::now();
                ledger.search(query, 5)?;
                queries.push(t.elapsed().as_secs_f64() * 1000.);
            }
            queries.sort_by(f64::total_cmp);
            let wal = PathBuf::from(format!("{}-wal", args.db.display()));
            let status = ledger.status()?;
            #[cfg(target_os = "macos")]
            let peak_rss_bytes = {
                let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
                if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } == 0 {
                    Some(usage.ru_maxrss as u64)
                } else {
                    None
                }
            };
            #[cfg(not(target_os = "macos"))]
            let peak_rss_bytes: Option<u64> = None;
            let value = serde_json::json!({"device":"physical-mac", "workload":"durable-single-event-writes-reusing-real-embeddings", "count":count,
                "seconds":elapsed,"events_per_second":count as f64/elapsed,"p50_ms":times[count/2],"p95_ms":times[(count*95/100).min(count-1)],
                "query_p50_ms":queries[queries.len()/2],"query_max_ms":queries.last(),
                "status":status,"peak_rss_bytes":peak_rss_bytes,"database_file_bytes":std::fs::metadata(&args.db)?.len(),"wal_file_bytes":std::fs::metadata(wal).map(|s|s.len()).unwrap_or(0)});
            write_json(&report, &value)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
    }
    Ok(())
}
