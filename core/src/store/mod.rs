//! Session and message persistence.
//!
//! `SessionManager` wraps a `SessionStore` implementation (JSONL or SQLite).
//! Messages include optional `tool_calls` and `tool_call_id`, serialized to
//! match the OpenAI Chat Completions message format.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod store_trait;
pub mod json_store;
pub mod sqlite_store;

use store_trait::SessionStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
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
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Assistant thinking text (extended thinking). Persisted so it can be
    /// replayed to the model on subsequent turns and shown in history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub title: String,
}

pub struct SessionManager {
    store: Box<dyn SessionStore>,
}

impl SessionManager {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self { store }
    }

    pub fn create_session(&mut self) -> Result<Session> {
        let now = chrono::Utc::now().timestamp_millis();
        let session = Session { id: Uuid::new_v4().to_string(), created_at: now, updated_at: now, messages: vec![], title: "".into() };
        self.store.create(&session)?;
        Ok(session)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        self.store.get(id)
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        self.store.list()
    }

    pub fn delete_session(&mut self, id: &str) -> Result<()> {
        self.store.delete(id)
    }

    fn new_msg(role: &str, content: &str) -> Message {
        Message {
            id: Uuid::new_v4().to_string(), role: role.into(), content: content.into(),
            tool_calls: None, tool_call_id: None, thinking: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn add_message(&mut self, session_id: &str, role: &str, content: &str) -> Result<Message> {
        let msg = Self::new_msg(role, content);
        self.store.add_message(session_id, &msg)?;
        Ok(msg)
    }

    /// Add the final assistant message, optionally carrying thinking text.
    pub fn add_assistant_message(&mut self, session_id: &str, content: &str, thinking: Option<&str>) -> Result<()> {
        let mut msg = Self::new_msg("assistant", content);
        msg.thinking = thinking.map(|s| s.to_string());
        self.store.add_message(session_id, &msg)?;
        Ok(())
    }

    pub fn add_assistant_tool_calls(&mut self, session_id: &str, tool_calls: Vec<ToolCall>, content: &str, thinking: Option<&str>) -> Result<()> {
        let mut msg = Self::new_msg("assistant", content);
        msg.tool_calls = Some(tool_calls);
        msg.thinking = thinking.map(|s| s.to_string());
        self.store.add_message(session_id, &msg)?;
        Ok(())
    }

    pub fn add_tool_result(&mut self, session_id: &str, tool_call_id: &str, content: &str) -> Result<()> {
        let mut msg = Self::new_msg("tool", content);
        msg.tool_call_id = Some(tool_call_id.into());
        self.store.add_message(session_id, &msg)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/store_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/json_store_tests.rs"]
mod json_store_tests;

#[cfg(test)]
#[path = "../tests/sqlite_store_tests.rs"]
mod sqlite_store_tests;
