//! SSE response parser for OpenAI chat completions streaming format.
//!
//! Handles MiniMax-specific quirks: tool_call arguments split across chunks,
//! continuation chunks without `id` or `name`.
//!
//! Detects `{"error": ...}` SSE events and returns them as structured errors
//! so callers can distinguish API errors from successful-but-empty responses.

use crate::store::{ToolCall, ToolCallFunction};
use anyhow::Result;
use tracing::trace;

#[derive(Debug)]
pub(crate) struct SseResult {
    pub(crate) text: String,
    pub(crate) tool_calls: Vec<ToolCall>,
}

/// Parse an OpenAI chat completions SSE response body into text and tool calls.
///
/// Tool calls are accumulated by their `index` position within the `tool_calls`
/// array, because some providers (e.g. MiniMax) split a single tool call's
/// arguments across multiple SSE chunks. Continuation chunks may carry only a
/// partial `arguments` string with no `id` or `name`.
///
/// Returns `Err` when the stream contains an `{"error": ...}` event so callers
/// can surface the API error instead of silently producing an empty response.
pub(crate) fn parse_sse_response(body_str: &str) -> Result<SseResult> {
    let mut text = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut pending_tools: Vec<(String, String, String)> = Vec::new(); // (id, name, args_json)

    for line in body_str.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" { continue; }
            let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
                trace!("SSE: skipping malformed data line");
                continue;
            };

            // ── Error event detection ──────────────────────────────
            // OpenAI-compatible APIs signal errors inline in the SSE
            // stream: `data: {"error": {"message": "...", "type": "..."}}`
            if let Some(error) = event.get("error") {
                let msg = error
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown API error");
                let err_type = error.get("type").and_then(|v| v.as_str()).unwrap_or("");
                return Err(anyhow::anyhow!(
                    crate::error::ChatError::BadRequest {
                        detail: if err_type.is_empty() {
                            msg.to_string()
                        } else {
                            format!("{err_type}: {msg}")
                        }
                    }
                ));
            }

            let delta = event.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta"));

            if let Some(content) = delta.and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                text.push_str(content);
            }

            if let Some(tool_calls) = delta.and_then(|d| d.get("tool_calls")).and_then(|tc| tc.as_array()) {
                for tool in tool_calls {
                    let idx = tool.get("index").and_then(|v| v.as_u64()).unwrap_or(pending_tools.len() as u64) as usize;
                    let id = tool.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let func = tool.get("function");
                    let name = func.and_then(|v| v.get("name")).and_then(|v| v.as_str()).unwrap_or("");
                    let args = func.and_then(|v| v.get("arguments")).and_then(|v| v.as_str()).unwrap_or("");

                    while pending_tools.len() <= idx {
                        pending_tools.push((String::new(), String::new(), String::new()));
                    }
                    let pending = &mut pending_tools[idx];
                    if !id.is_empty() { pending.0 = id.to_string(); }
                    if !name.is_empty() { pending.1 = name.to_string(); }
                    pending.2.push_str(args);
                }
            }
        }
    }

    for (id, name, args) in pending_tools {
        if id.is_empty() || name.is_empty() { continue; }
        if serde_json::from_str::<serde_json::Value>(&args).is_ok() {
            tool_calls.push(ToolCall {
                id,
                call_type: "function".to_string(),
                function: ToolCallFunction { name, arguments: args },
            });
        }
    }

    Ok(SseResult { text, tool_calls })
}

#[cfg(test)]
#[path = "tests/sse_tests.rs"]
mod tests;
