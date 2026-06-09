use super::{Message, Session};
use super::store_trait::SessionStore;
use anyhow::Result;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::collections::HashMap;

/// Append-only JSONL session store. One `.jsonl` file per session.
pub struct JsonSessionStore {
    dir: PathBuf,
    cache: HashMap<String, Session>,
}

impl JsonSessionStore {
    pub fn new(dir: PathBuf) -> Self {
        let mut store = Self { dir, cache: HashMap::new() };
        store.load_index();
        store
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.jsonl"))
    }

    fn load_index(&mut self) {
        let _ = fs::create_dir_all(&self.dir);
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "jsonl") { continue; }
                let id = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let session = self.replay_session(&id).unwrap_or_else(|| Session {
                    id: id.clone(), created_at: 0, updated_at: 0, messages: Vec::new(), title: String::new(),
                });
                self.cache.insert(id, session);
            }
        }
    }

    fn replay_session(&self, id: &str) -> Option<Session> {
        let file = File::open(self.session_path(id)).ok()?;
        let mut session: Option<Session> = None;
        for line in BufReader::new(file).lines() {
            let line = line.ok()?;
            if line.trim().is_empty() { continue; }
            if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                let s = session.get_or_insert_with(|| Session {
                    id: id.to_string(), created_at: msg.timestamp, updated_at: msg.timestamp, messages: Vec::new(), title: String::new(),
                });
                s.updated_at = s.updated_at.max(msg.timestamp);
                s.messages.push(msg);
            } else {
                // Try parsing as session metadata line
                if let Ok(s) = serde_json::from_str::<Session>(&line) {
                    session = Some(s);
                }
            }
        }
        session
    }
}

impl SessionStore for JsonSessionStore {
    fn create(&mut self, session: &Session) -> Result<()> {
        let file = File::create(self.session_path(&session.id))?;
        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, session)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        self.cache.insert(session.id.clone(), session.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Session>> {
        Ok(self.cache.get(id).cloned())
    }

    fn list(&self) -> Result<Vec<Session>> {
        let mut sessions: Vec<_> = self.cache.values().cloned().collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        Ok(sessions)
    }

    fn add_message(&mut self, session_id: &str, msg: &Message) -> Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(self.session_path(session_id))?;
        let line = serde_json::to_string(msg)?;
        writeln!(file, "{line}")?;
        if let Some(s) = self.cache.get_mut(session_id) {
            if s.title.is_empty() && !msg.content.is_empty() {
                s.title = msg.content.chars().take(50).collect();
            }
            s.messages.push(msg.clone());
            s.updated_at = s.updated_at.max(msg.timestamp);
        }
        Ok(())
    }

    fn delete(&mut self, id: &str) -> Result<()> {
        let path = self.session_path(id);
        if path.exists() { fs::remove_file(&path)?; }
        self.cache.remove(id);
        Ok(())
    }
}
