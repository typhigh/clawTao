//! JSON-RPC 2.0 types and communication primitives.
//!
//! All stdin/stdout I/O helpers live here so every handler can import them
//! from one place without depending on `main.rs`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Write};

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC 2.0 Notification (no id, no response expected)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

impl Response {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }

    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(Error { code, message: message.into(), data: None }),
        }
    }
}

impl Notification {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self { jsonrpc: "2.0".to_string(), method: method.into(), params }
    }
}

// ── I/O helpers ──────────────────────────────────────────────────────────

/// Extract a string parameter from a JSON-RPC params object.
pub fn get_param<'a>(params: &'a Option<Value>, key: &str) -> Result<&'a str> {
    params
        .as_ref()
        .and_then(|obj| obj.get(key))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing parameter: {key}"))
}

/// Write a JSON-RPC response to stdout (one line, flushed).
pub fn write_response(response: &Response) -> io::Result<()> {
    let json =
        serde_json::to_string(response).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{json}");
    io::stdout().flush()
}

/// Write a JSON-RPC notification to stdout (one line, flushed).
pub fn write_notification(notification: &Notification) -> io::Result<()> {
    let json = serde_json::to_string(notification)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{json}");
    io::stdout().flush()
}

#[cfg(test)]
#[path = "tests/jsonrpc_tests.rs"]
mod tests;
