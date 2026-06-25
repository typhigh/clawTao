//! Session and message persistence.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use store_trait::SessionStore;

pub mod store_trait;
pub mod json_store;
pub mod sqlite_store;

// ── Data types ────────────────────────────────────────────────────────

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

// ── Constructors ──────────────────────────────────────────────────────

pub fn new_msg(role: &str, content: &str) -> Message {
    Message {
        id: Uuid::new_v4().to_string(),
        role: role.into(),
        content: content.into(),
        tool_calls: None,
        tool_call_id: None,
        thinking: None,
        timestamp: chrono::Utc::now().timestamp_millis(),
    }
}

pub fn new_session() -> Session {
    let now = chrono::Utc::now().timestamp_millis();
    Session {
        id: Uuid::new_v4().to_string(),
        created_at: now,
        updated_at: now,
        messages: vec![],
        title: String::new(),
    }
}

/// Add a plain user/assistant message to a session.
pub fn add_message(store: &dyn SessionStore, session_id: &str, role: &str, content: &str) -> Result<Message> {
    let msg = new_msg(role, content);
    store.add_message(session_id, &msg)?;
    Ok(msg)
}

/// Add an assistant message carrying thinking text.
pub fn add_assistant_message(store: &dyn SessionStore, session_id: &str, content: &str, thinking: Option<&str>) -> Result<()> {
    let mut msg = new_msg("assistant", content);
    msg.thinking = thinking.map(|s| s.to_string());
    store.add_message(session_id, &msg)?;
    Ok(())
}

/// Add an assistant message with tool calls and optional thinking.
pub fn add_assistant_tool_calls(store: &dyn SessionStore, session_id: &str, tool_calls: Vec<ToolCall>, content: &str, thinking: Option<&str>) -> Result<()> {
    let mut msg = new_msg("assistant", content);
    msg.tool_calls = Some(tool_calls);
    msg.thinking = thinking.map(|s| s.to_string());
    store.add_message(session_id, &msg)?;
    Ok(())
}

/// Add a tool result message.
pub fn add_tool_result(store: &dyn SessionStore, session_id: &str, tool_call_id: &str, content: &str) -> Result<()> {
    let mut msg = new_msg("tool", content);
    msg.tool_call_id = Some(tool_call_id.into());
    store.add_message(session_id, &msg)?;
    Ok(())
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
