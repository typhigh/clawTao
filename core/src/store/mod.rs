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
    pub timestamp: i64,
}

impl Message {
    pub fn to_llm_message(&self) -> serde_json::Value {
        match self.role.as_str() {
            "tool" => serde_json::json!({
                "role": "tool", "tool_call_id": self.tool_call_id, "content": self.content,
            }),
            "assistant" if self.tool_calls.is_some() => serde_json::json!({
                "role": "assistant", "content": null, "tool_calls": self.tool_calls,
            }),
            _ => serde_json::json!({
                "role": self.role, "content": self.content,
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
    store: Box<dyn SessionStore>,
}

impl SessionManager {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self { store }
    }

    pub fn create_session(&mut self) -> Result<Session> {
        let now = chrono::Utc::now().timestamp_millis();
        let session = Session { id: Uuid::new_v4().to_string(), created_at: now, updated_at: now, messages: vec![] };
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
            tool_calls: None, tool_call_id: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn add_message(&mut self, session_id: &str, role: &str, content: &str) -> Result<Message> {
        let msg = Self::new_msg(role, content);
        self.store.add_message(session_id, &msg)?;
        Ok(msg)
    }

    pub fn add_assistant_tool_calls(&mut self, session_id: &str, tool_calls: Vec<ToolCall>) -> Result<()> {
        let mut msg = Self::new_msg("assistant", "");
        msg.tool_calls = Some(tool_calls);
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
