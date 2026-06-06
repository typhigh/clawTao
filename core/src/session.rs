//! Simple session management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // "function"
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String, // "user" | "assistant" | "tool"
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub timestamp: i64,
}

impl Message {
    /// Convert message to LLM API format (OpenAI Chat Completions)
    pub fn to_llm_message(&self) -> serde_json::Value {
        match self.role.as_str() {
            "tool" => serde_json::json!({
                "role": "tool",
                "tool_call_id": self.tool_call_id,
                "content": self.content,
            }),
            "assistant" if self.tool_calls.is_some() => serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": self.tool_calls,
            }),
            _ => serde_json::json!({
                "role": self.role,
                "content": self.content,
            }),
        }
    }
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
            tool_calls: None,
            tool_call_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        session.messages.push(message.clone());
        session.updated_at = chrono::Utc::now().timestamp_millis();
        let session_clone = session.clone();
        let _ = self.save(&session_clone);
        Some(message)
    }

    pub fn add_assistant_tool_calls(&mut self, session_id: &str, tool_calls: Vec<ToolCall>) -> Option<Message> {
        let session = self.sessions.get_mut(session_id)?;
        let message = Message {
            id: Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        session.messages.push(message.clone());
        session.updated_at = chrono::Utc::now().timestamp_millis();
        let session_clone = session.clone();
        let _ = self.save(&session_clone);
        Some(message)
    }

    pub fn add_tool_result(&mut self, session_id: &str, tool_call_id: &str, content: &str) -> Option<Message> {
        let session = self.sessions.get_mut(session_id)?;
        let message = Message {
            id: Uuid::new_v4().to_string(),
            role: "tool".to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        };
        session.messages.push(message.clone());
        session.updated_at = chrono::Utc::now().timestamp_millis();
        let session_clone = session.clone();
        let _ = self.save(&session_clone);
        Some(message)
    }

    pub fn list_sessions(&self) -> Vec<Session> {
        let mut sessions: Vec<_> = self.sessions.values().cloned().collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        sessions
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
