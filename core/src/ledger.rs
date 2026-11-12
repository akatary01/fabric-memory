use crate::{adapter::Adapter, normalized, sha256, Embedding, Event, MAX_EVENTS};
use anyhow::{ensure, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path, process::Command};

pub struct Ledger {
    db: Connection,
    pub actor: String,
    pub space: String,
    pub dimension: usize,
}

#[derive(Serialize, Deserialize)]
pub struct Hit {
    pub memory: String,
    pub text: String,
    pub score: f32,
    pub origin: String,
    pub translated: bool,
    pub access: crate::Access,
}

#[derive(Serialize)]
pub struct Status {
    pub actor: String,
    pub events: u64,
    pub live_memories: u64,
    pub tombstones: u64,
    pub state_digest: String,
    pub event_digest: String,
    pub logical_clock: u64,
    pub projection_bytes: u64,
}

impl Ledger {
    pub fn open(path: &Path, space: &str, dimension: usize) -> Result<Self> {
        ensure!(
            space.len() == 64 && (1..=4096).contains(&dimension),
            "invalid index configuration"
        );
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let db = Connection::open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
          CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS events(id TEXT PRIMARY KEY, actor TEXT NOT NULL, clock INTEGER NOT NULL, payload TEXT NOT NULL, UNIQUE(actor,clock));
          CREATE TABLE IF NOT EXISTS memory_dates(memory TEXT PRIMARY KEY, recorded_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS heads(memory TEXT PRIMARY KEY, event TEXT NOT NULL, vector TEXT);
          CREATE INDEX IF NOT EXISTS event_clock ON events(clock);")?;
        db.execute(
            "INSERT OR IGNORE INTO meta VALUES('actor',?1)",
            [uuid::Uuid::new_v4().to_string()],
        )?;
        db.execute("INSERT OR IGNORE INTO meta VALUES('space',?1)", [space])?;
        db.execute(
            "INSERT OR IGNORE INTO meta VALUES('dimension',?1)",
            [dimension.to_string()],
        )?;
        let actor = db.query_row("SELECT value FROM meta WHERE key='actor'", [], |r| r.get(0))?;
        let stored: String =
            db.query_row("SELECT value FROM meta WHERE key='space'", [], |r| r.get(0))?;
        let dim: String =
            db.query_row("SELECT value FROM meta WHERE key='dimension'", [], |r| {
                r.get(0)
            })?;
        ensure!(
            stored == space && dim == dimension.to_string(),
            "database belongs to a different embedding space; rebuild explicitly"
        );
        Ok(Self {
            db,
            actor,
            space: space.into(),
            dimension,
        })
    }

    pub fn put(&mut self, memory: &str, text: &str, embedding: Option<Embedding>) -> Result<Event> {
        self.put_with_access(memory, text, embedding, crate::Access::default())
    }
    pub fn put_with_access(
        &mut self,
        memory: &str,
        text: &str,
        embedding: Option<Embedding>,
        mut access: crate::Access,
    ) -> Result<Event> {
        if access.scope == "restricted" { access = crate::Access { scope:"private".into(), allowed_devices:vec![], reason:"Private memory".into() }; }
        if embedding.is_none() {
            let existing: Option<String> = self
                .db
                .query_row("SELECT event FROM heads WHERE memory=?1", [memory], |r| {
                    r.get(0)
                })
                .optional()?;
            if let Some(raw) = existing {
                access = serde_json::from_str::<Event>(&raw)?.access;
            }
        }
        // Exclusive write transaction also serializes clock allocation across processes.
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let clock: u64 = tx.query_row("SELECT COALESCE(MAX(clock),0)+1 FROM events", [], |r| {
            r.get(0)
        })?;
        let event = Event {
            access,
            version: 1,
            id: uuid::Uuid::new_v4().to_string(),
            actor: self.actor.clone(),
            clock,
            memory: memory.into(),
            deleted: embedding.is_none(),
            text: text.into(),
            embedding,
        };
        event.validate()?;
        if let Some(e) = &event.embedding {
            ensure!(
                e.space == self.space && e.values.len() == self.dimension,
                "native embedding mismatch"
            );
        }
        Self::insert(&tx, &event, event.embedding.as_ref())?;
        tx.commit()?;
        Ok(event)
    }

    pub fn ingest(&mut self, events: &[Event], adapter: Option<&Adapter>) -> Result<usize> {
        ensure!(
            events.len() <= MAX_EVENTS,
            "too many events in one transaction"
        );
        let mut local = Vec::with_capacity(events.len());
        for event in events {
            event.validate()?;
            let projection = match &event.embedding {
                None => None,
                Some(e) if e.space == self.space => {
                    ensure!(
                        e.values.len() == self.dimension,
                        "native dimension mismatch"
                    );
                    Some(Embedding {
                        space: e.space.clone(),
                        values: normalized(&e.values)?,
                    })
                }
                Some(e) => {
                    let a = adapter
                        .ok_or_else(|| anyhow::anyhow!("no adapter for foreign embedding"))?;
                    ensure!(
                        a.manifest.target == self.space && a.manifest.output_dim == self.dimension,
                        "adapter target mismatch"
                    );
                    Some(a.apply(e)?)
                }
            };
            local.push(projection);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut inserted = 0;
        if events
            .iter()
            .any(|e| e.embedding.as_ref().is_some_and(|v| v.space != self.space))
        {
            let adapter = adapter.unwrap(); // Foreign embeddings were checked above.
            let key = format!("adapter:{}", adapter.manifest.source);
            tx.execute(
                "INSERT OR IGNORE INTO meta VALUES(?1,?2)",
                params![key, adapter.id],
            )?;
            let existing: String =
                tx.query_row("SELECT value FROM meta WHERE key=?1", [&key], |r| r.get(0))?;
            ensure!(
                existing == adapter.id,
                "adapter changed; rebuild the foreign projection explicitly"
            );
        }
        for (event, vector) in events.iter().zip(&local) {
            inserted += Self::insert(&tx, event, vector.as_ref())? as usize;
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn memory_recorded_at(&self, memory: &str) -> Result<Option<u64>> {
        Ok(self.db.query_row("SELECT recorded_at FROM memory_dates WHERE memory=?1", [memory], |row| row.get(0)).optional()?)
    }

    fn insert(
        tx: &rusqlite::Transaction<'_>,
        event: &Event,
        vector: Option<&Embedding>,
    ) -> Result<bool> {
        let existing: Option<String> = tx
            .query_row("SELECT payload FROM events WHERE id=?1", [&event.id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(raw) = existing {
            ensure!(
                serde_json::from_str::<Event>(&raw)? == *event,
                "immutable event ID collision"
            );
            return Ok(false);
        }
        let payload = serde_json::to_string(event)?;
        tx.execute(
            "INSERT INTO events VALUES(?1,?2,?3,?4)",
            params![event.id, event.actor, event.clock, payload],
        )?;
        if !event.deleted {
            tx.execute("INSERT OR IGNORE INTO memory_dates VALUES(?1, unixepoch())", [&event.memory])?;
        }
        let head: Option<String> = tx
            .query_row(
                "SELECT event FROM heads WHERE memory=?1",
                [&event.memory],
                |r| r.get(0),
            )
            .optional()?;
        let wins = match head {
            None => true,
            Some(raw) => {
                let old: Event = serde_json::from_str(&raw)?;
                (event.clock, &event.actor, &event.id) > (old.clock, &old.actor, &old.id)
            }
        };
        if wins {
            tx.execute("INSERT INTO heads VALUES(?1,?2,?3) ON CONFLICT(memory) DO UPDATE SET event=excluded.event, vector=excluded.vector",
                params![event.memory, payload, vector.map(serde_json::to_string).transpose()?])?;
        }
        Ok(true)
    }

    pub fn live_count(&self) -> Result<u64> {
        Ok(self.db.query_row("SELECT COUNT(*) FROM heads WHERE vector IS NOT NULL", [], |row| row.get(0))?)
    }

    /// Current live memories for application views; full history remains in events().
    pub fn current(&self, limit: usize) -> Result<Vec<Event>> {
        let mut stmt = self.db.prepare(
            "SELECT event FROM heads WHERE vector IS NOT NULL ORDER BY json_extract(event,'$.clock') DESC, memory LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit.clamp(1, 200) as i64], |r| r.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub fn events(&self) -> Result<Vec<Event>> {
        let mut stmt = self.db.prepare("SELECT payload FROM events ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    /// Forget a current memory once, retaining its visibility for sync.
    pub fn forget(&mut self, memory: &str) -> Result<bool> {
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let head: Option<String> = tx.query_row(
            "SELECT event FROM heads WHERE memory=?1 AND vector IS NOT NULL",
            [memory],
            |row| row.get(0),
        ).optional()?;
        let Some(head) = head else { return Ok(false); };
        let previous: Event = serde_json::from_str(&head)?;
        let clock = tx.query_row("SELECT COALESCE(MAX(clock),0)+1 FROM events", [], |row| row.get(0))?;
        let event = Self::tombstone(&self.actor, previous, clock);
        event.validate()?;
        Self::insert(&tx, &event, None)?;
        tx.commit()?;
        Ok(true)
    }

    fn tombstone(actor: &str, previous: Event, clock: u64) -> Event {
        Event { version: 1, id: uuid::Uuid::new_v4().to_string(), actor: actor.into(), clock,
            memory: previous.memory, deleted: true, text: String::new(), embedding: None, access: previous.access }
    }

    /// Forget every live memory atomically, preserving immutable deletion history.
    pub fn clear(&mut self, expected: usize) -> Result<usize> {
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let live: Vec<Event> = {
            let mut statement=tx.prepare("SELECT event FROM heads ORDER BY memory")?;
            let rows=statement.query_map([],|row|row.get::<_,String>(0))?;
            rows.map(|raw|Ok(serde_json::from_str::<Event>(&raw?)?)).collect::<Result<Vec<_>>>()?.into_iter().filter(|event|!event.deleted).collect()
        };
        ensure!(live.len()==expected,"Memory changed while the confirmation was open. Review the count and try again.");
        let clock:u64=tx.query_row("SELECT COALESCE(MAX(clock),0)+1 FROM events",[],|row|row.get(0))?;
        let count = live.len();
        for (index,previous) in live.into_iter().enumerate() {
            let event=Self::tombstone(&self.actor,previous,clock+index as u64);
            event.validate()?;
            Self::insert(&tx,&event,None)?;
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn export(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("jsonl.tmp");
        let mut file = std::fs::File::create(&tmp)?;
        for event in self.events()? {
            serde_json::to_writer(&mut file, &event)?;
            file.write_all(b"\n")?;
        }
        file.sync_all()?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn search(&self, query: &Embedding, k: usize) -> Result<Vec<Hit>> {
        ensure!(
            query.space == self.space && query.values.len() == self.dimension,
            "query embedding mismatch"
        );
        let q = normalized(&query.values)?;
        let mut stmt = self
            .db
            .prepare("SELECT event,vector FROM heads WHERE vector IS NOT NULL ORDER BY memory")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut hits = Vec::new();
        for row in rows {
            let (raw, vector) = row?;
            let event: Event = serde_json::from_str(&raw)?;
            let vector: Embedding = serde_json::from_str(&vector)?;
            let values = normalized(&vector.values)?;
            let score = q.iter().zip(values).map(|(x, y)| x * y).sum();
            hits.push(Hit {
                access: event.access,
                translated: event
                    .embedding
                    .as_ref()
                    .is_some_and(|e| e.space != self.space),
                memory: event.memory,
                text: event.text,
                score,
                origin: event.actor,
            });
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.memory.cmp(&b.memory)));
        hits.truncate(k.min(100));
        Ok(hits)
    }

    pub fn status(&self) -> Result<Status> {
        let mut state = String::new();
        let mut live = 0;
        let mut dead = 0;
        let mut bytes = 0;
        let mut stmt = self
            .db
            .prepare("SELECT event,vector FROM heads ORDER BY memory")?;
        for row in stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })? {
            let (raw, vector) = row?;
            let event: Event = serde_json::from_str(&raw)?;
            state.push_str(&event.memory);
            state.push('\0');
            state.push_str(&event.id);
            state.push('\n');
            if event.deleted {
                dead += 1;
            } else {
                live += 1;
            }
            bytes += (raw.len() + vector.map_or(0, |v| v.len())) as u64;
        }
        let events = self.events()?;
        let event_ids = events
            .iter()
            .map(|e| format!("{}\n", e.id))
            .collect::<String>();
        Ok(Status {
            actor: self.actor.clone(),
            events: events.len() as u64,
            live_memories: live,
            tombstones: dead,
            state_digest: sha256(state.as_bytes()),
            event_digest: sha256(event_ids.as_bytes()),
            logical_clock: events.iter().map(|e| e.clock).max().unwrap_or(0),
            projection_bytes: bytes,
        })
    }

    pub fn archive_git(&self, folder: &Path) -> Result<()> {
        std::fs::create_dir_all(folder.join("events"))?;
        fn git(folder: &Path, args: &[&str]) -> Result<()> {
            let out = Command::new("git")
                .arg("-C")
                .arg(folder)
                .args(args)
                .output()?;
            ensure!(
                out.status.success(),
                "git failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            Ok(())
        }
        if !folder.join(".git").exists() {
            git(folder, &["init", "--quiet"])?;
        }
        for event in self.events()? {
            let path = folder.join("events").join(format!("{}.json", event.id));
            if path.exists() {
                ensure!(
                    serde_json::from_slice::<Event>(&std::fs::read(path)?)? == event,
                    "Git archive event collision"
                );
            } else {
                crate::write_json(&path, &event)?;
            }
        }
        git(folder, &["add", "--", "events"])?;
        let diff = Command::new("git")
            .arg("-C")
            .arg(folder)
            .args(["diff", "--cached", "--quiet"])
            .status()?;
        if diff.code() == Some(1) {
            git(
                folder,
                &[
                    "-c",
                    "user.name=fabric-memory",
                    "-c",
                    "user.email=local@memory-fabric.invalid",
                    "commit",
                    "--quiet",
                    "-m",
                    "Archive immutable events",
                ],
            )?;
        } else {
            ensure!(diff.success(), "git diff failed");
        }
        Ok(())
    }
}
