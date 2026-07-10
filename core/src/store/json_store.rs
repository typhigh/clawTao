use super::{Message, Session};
use super::store_trait::SessionStore;
use anyhow::Result;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;

/// Append-only JSONL session store. One `.jsonl` file per session.
/// Thread-safe via internal `Mutex<HashMap>` on the in-memory cache;
/// file I/O is append-only so writes from different threads don't conflict.
pub struct JsonSessionStore {
    dir: PathBuf,
    cache: Mutex<HashMap<String, Session>>,
}

impl JsonSessionStore {
    pub fn new(dir: PathBuf) -> Self {
        let store = Self { dir, cache: Mutex::new(HashMap::new()) };
        store.load_index();
        store
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.jsonl"))
    }

    fn load_index(&self) {
        let _ = fs::create_dir_all(&self.dir);
        let mut cache = self.cache.lock().unwrap();
        if let Ok(entries) = fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "jsonl") { continue; }
                let id = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let session = self.replay_session(&id).unwrap_or_else(|| Session {
                    id: id.clone(), created_at: 0, updated_at: 0, messages: Vec::new(), title: String::new(),
                    compacted_summary: None, compacted_message_id: None,
                });
                cache.insert(id, session);
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
                    compacted_summary: None, compacted_message_id: None,
                });
                s.updated_at = s.updated_at.max(msg.timestamp);
                s.messages.push(msg);
            } else if let Ok(s) = serde_json::from_str::<Session>(&line) {
                // Merge: preserve messages accumulated so far and
                // overlay the header metadata from the Session line.
                if let Some(ref mut cur) = session {
                    cur.id = s.id;
                    cur.created_at = s.created_at;
                    cur.updated_at = cur.updated_at.max(s.updated_at);
                    cur.title = s.title;
                    cur.compacted_summary = s.compacted_summary;
                    cur.compacted_message_id = s.compacted_message_id;
                    // Keep existing messages (header has messages: vec![]).
                } else {
                    session = Some(s);
                }
            }
        }
        session
    }
}

impl SessionStore for JsonSessionStore {
    fn create(&self, session: &Session) -> Result<()> {
        let file = File::create(self.session_path(&session.id))?;
        let mut writer = std::io::BufWriter::new(file);
        serde_json::to_writer(&mut writer, session)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        self.cache.lock().unwrap().insert(session.id.clone(), session.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Session>> {
        Ok(self.cache.lock().unwrap().get(id).cloned())
    }

    fn list(&self) -> Result<Vec<Session>> {
        let cache = self.cache.lock().unwrap();
        let mut sessions: Vec<_> = cache.values().cloned().collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        Ok(sessions)
    }

    fn add_message(&self, session_id: &str, msg: &Message) -> Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(self.session_path(session_id))?;
        let line = serde_json::to_string(msg)?;
        writeln!(file, "{line}")?;
        let mut cache = self.cache.lock().unwrap();
        if let Some(s) = cache.get_mut(session_id) {
            if s.title.is_empty() && !msg.content.is_empty() {
                s.title = msg.content.chars().take(50).collect();
            }
            s.messages.push(msg.clone());
            s.updated_at = s.updated_at.max(msg.timestamp);
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<()> {
        let path = self.session_path(id);
        if path.exists() { fs::remove_file(&path)?; }
        self.cache.lock().unwrap().remove(id);
        Ok(())
    }

    fn update_compaction(
        &self,
        session_id: &str,
        summary: Option<&str>,
        last_msg_id: Option<&str>,
    ) -> Result<()> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(s) = cache.get_mut(session_id) {
            s.compacted_summary = summary.map(|s| s.to_string());
            s.compacted_message_id = last_msg_id.map(|s| s.to_string());
            s.updated_at = chrono::Utc::now().timestamp_millis();

            // Persist compaction to the JSONL file so it survives restarts.
            // The header on line 1 carries only metadata (no messages);
            // subsequent lines hold the messages independently.
            let header = super::Session {
                messages: vec![], // messages live on their own lines
                ..s.clone()
            };
            let messages = s.messages.clone();
            drop(cache);

            let file = std::fs::File::create(self.session_path(session_id))?;
            let mut writer = std::io::BufWriter::new(file);
            serde_json::to_writer(&mut writer, &header)?;
            writer.write_all(b"\n")?;
            for m in &messages {
                let line = serde_json::to_string(m)?;
                writeln!(writer, "{line}")?;
            }
            writer.flush()?;
        }
        Ok(())
    }
}
