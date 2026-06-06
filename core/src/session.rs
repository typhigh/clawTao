//! Simple session management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String, // "user" | "assistant" | "tool"
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<Message>,
}

pub struct SessionManager {
    sessions: HashMap<String, Session>,
    storage_path: PathBuf,
}

impl SessionManager {
    pub fn new(storage_path: PathBuf) -> Self {
        let mut manager = Self {
            sessions: HashMap::new(),
            storage_path,
        };
        manager.load();
        manager
    }

    fn load(&mut self) {
        if let Ok(entries) = fs::read_dir(&self.storage_path) {
            for entry in entries.flatten() {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(session) = serde_json::from_str::<Session>(&content) {
                        self.sessions.insert(session.id.clone(), session);
                    }
                }
            }
        }
    }

    fn save(&self, session: &Session) -> io::Result<()> {
        fs::create_dir_all(&self.storage_path)?;
        let path = self.storage_path.join(format!("{}.json", session.id));
        let content = serde_json::to_string_pretty(session)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, content)
    }

    pub fn create_session(&mut self) -> Session {
        let now = chrono::Utc::now().timestamp_millis();
        let session = Session {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };
        let session_clone = session.clone();
        self.sessions.insert(session.id.clone(), session);
        let _ = self.save(&session_clone);
        session_clone
    }

    pub fn get_session(&self, id: &str) -> Option<Session> {
        self.sessions.get(id).cloned()
    }

    pub fn add_message(&mut self, session_id: &str, role: &str, content: &str) -> Option<Message> {
        let session = self.sessions.get_mut(session_id)?;
        let message = Message {
            id: Uuid::new_v4().to_string(),
            role: role.to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        session.messages.push(message.clone());
        session.updated_at = chrono::Utc::now().timestamp_millis();
        let session_clone = session.clone();
        drop(session);
        let _ = self.save(&session_clone);
        Some(message)
    }

    pub fn list_sessions(&self) -> Vec<Session> {
        let mut sessions: Vec<_> = self.sessions.values().cloned().collect();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }
}

use std::io;
