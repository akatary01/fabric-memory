use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub renamed: bool,
    pub turns: Vec<Value>,
    pub updated: u128,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Store {
    pub active: String,
    pub conversations: Vec<Conversation>,
    #[serde(default)]
    pub deleted: Vec<Conversation>,
}
fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
impl Store {
    pub fn load(path: &Path, legacy: &Path) -> Result<Self, String> {
        if path.exists() {
            let store: Self = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let mut ids = std::collections::HashSet::new();
            if store.conversations.is_empty()
                || !store.conversations.iter().all(|c| ids.insert(&c.id))
                || !store.conversations.iter().any(|c| c.id == store.active)
            {
                return Err("Invalid conversation index; saved chats were left untouched.".into());
            }
            return Ok(store);
        }
        let turns = match fs::read(legacy) {
            Ok(bytes) => serde_json::from_slice::<Vec<Value>>(&bytes).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.to_string()),
        };
        let store = Self {
            deleted: vec![],
            active: "original".into(),
            conversations: vec![Conversation {
                id: "original".into(),
                title: "Your conversation".into(),
                renamed: false,
                turns,
                updated: now(),
            }],
        };
        store.save(path)?;
        Ok(store)
    }
    pub fn active(&self) -> &Conversation {
        self.conversations
            .iter()
            .find(|c| c.id == self.active)
            .expect("validated active conversation")
    }
    pub fn list(&self) -> Value {
        let mut items = self.conversations.iter().collect::<Vec<_>>();
        items.sort_by_key(|c| std::cmp::Reverse(c.updated));
        json!(items.into_iter().map(|c|json!({"id":c.id,"title":c.title,"messages":c.turns.len(),"active":c.id==self.active})).collect::<Vec<_>>())
    }
    pub fn create(&mut self) {
        let id = format!("chat-{}-{}", now(), self.conversations.len());
        self.conversations.push(Conversation {
            id: id.clone(),
            title: "New conversation".into(),
            renamed: false,
            turns: vec![],
            updated: now(),
        });
        self.active = id;
    }
    pub fn select(&mut self, id: &str) -> Result<(), String> {
        if !self.conversations.iter().any(|c| c.id == id) {
            return Err("Conversation not found".into());
        }
        self.active = id.into();
        Ok(())
    }
    pub fn update(&mut self, turns: Vec<Value>) {
        let chat = self
            .conversations
            .iter_mut()
            .find(|c| c.id == self.active)
            .expect("validated active conversation");
        if !chat.renamed && chat.title == "New conversation" {
            if let Some(text) = turns
                .iter()
                .find(|t| t["role"] == "user")
                .and_then(|t| t["content"].as_str())
            {
                chat.title = text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(48)
                    .collect();
            }
        }
        chat.turns = turns;
        chat.updated = now();
    }
    pub fn rename(&mut self, id: &str, title: &str) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 80 || title.chars().any(char::is_control) {
            return Err("Enter a name of 1–80 characters.".into());
        }
        let chat = self
            .conversations
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or("Conversation not found")?;
        chat.title = title.into();
        chat.renamed = true;
        chat.updated = now();
        Ok(())
    }
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .conversations
            .iter()
            .position(|c| c.id == id)
            .ok_or("Conversation not found")?;
        self.deleted.push(self.conversations.remove(index));
        if self.conversations.is_empty() {
            self.create();
        } else if self.active == id {
            self.active = self
                .conversations
                .iter()
                .max_by_key(|c| c.updated)
                .unwrap()
                .id
                .clone();
        }
        Ok(())
    }
    pub fn restore(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .deleted
            .iter()
            .position(|c| c.id == id)
            .ok_or("Deleted conversation not found")?;
        let mut chat = self.deleted.remove(index);
        chat.updated = now();
        self.active = chat.id.clone();
        self.conversations.push(chat);
        Ok(())
    }
    pub fn deleted_list(&self) -> Value {
        json!(self
            .deleted
            .iter()
            .map(|c| json!({"id":c.id,"title":c.title}))
            .collect::<Vec<_>>())
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let temporary = path.with_extension("next");
        fs::write(
            &temporary,
            serde_json::to_vec(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
        fs::rename(temporary, path).map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!("fabric-chats-{}-{}-{}",std::process::id(), now(), NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn migrates_without_changing_original() {
        let d = Scratch::new();
        let old = d.0.join("old.json");
        let path = d.0.join("chats.json");
        let bytes = b"[{\"role\":\"user\",\"content\":\"Original history\"}]";
        fs::write(&old, bytes).unwrap();
        let s = Store::load(&path, &old).unwrap();
        assert_eq!(s.active().turns[0]["content"], "Original history");
        assert_eq!(fs::read(old).unwrap(), bytes);
    }
    #[test]
    fn independent_chats_survive_restart() {
        let d = Scratch::new();
        let path = d.0.join("chats.json");
        let old = d.0.join("old.json");
        let mut s = Store::load(&path, &old).unwrap();
        s.update(vec![json!({"role":"user","content":"First"})]);
        let first = s.active.clone();
        s.create();
        s.update(vec![json!({"role":"user","content":"Second topic"})]);
        assert_eq!(s.active().title, "Second topic");
        s.save(&path).unwrap();
        let mut loaded = Store::load(&path, &old).unwrap();
        assert_eq!(loaded.active().turns[0]["content"], "Second topic");
        loaded.select(&first).unwrap();
        assert_eq!(loaded.active().turns[0]["content"], "First");
        assert!(loaded.select("missing").is_err());
        assert_eq!(loaded.active, first);
    }
    #[test]
    fn deletion_is_recoverable_after_restart() {
        let d = Scratch::new();
        let path = d.0.join("chats.json");
        let old = d.0.join("old.json");
        let mut s = Store::load(&path, &old).unwrap();
        let id = s.active.clone();
        s.update(vec![json!({"role":"user","content":"Keep this"})]);
        s.delete(&id).unwrap();
        assert_ne!(s.active, id);
        s.save(&path).unwrap();
        let mut s = Store::load(&path, &old).unwrap();
        s.restore(&id).unwrap();
        assert_eq!(s.active().turns[0]["content"], "Keep this");
        assert!(s.deleted.is_empty());
    }
    #[test]
    fn explicit_names_survive_messages_and_restart() {
        let d = Scratch::new();
        let path = d.0.join("chats.json");
        let mut s = Store::load(&path, &d.0.join("old.json")).unwrap();
        let id = s.active.clone();
        s.rename(&id, "New conversation").unwrap();
        s.update(vec![
            json!({"role":"user","content":"Do not turn this into my title"}),
        ]);
        s.save(&path).unwrap();
        let s = Store::load(&path, &d.0.join("old.json")).unwrap();
        assert_eq!(s.active().title, "New conversation");
        assert!(s.active().renamed);
    }
    #[test]
    fn refuses_to_overwrite_corrupt_history() {
        let d = Scratch::new();
        let path = d.0.join("chats.json");
        fs::write(&path, b"broken").unwrap();
        assert!(Store::load(&path, &d.0.join("old.json")).is_err());
        assert_eq!(fs::read(path).unwrap(), b"broken");
    }
}
