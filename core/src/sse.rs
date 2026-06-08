//! SSE response parser for OpenAI chat completions streaming format.
//!
//! Handles MiniMax-specific quirks: tool_call arguments split across chunks,
//! continuation chunks without `id` or `name`.

use crate::store::{ToolCall, ToolCallFunction};

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
pub(crate) fn parse_sse_response(body_str: &str) -> SseResult {
    let mut text = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut pending_tools: Vec<(String, String, String)> = Vec::new(); // (id, name, args_json)

    for line in body_str.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" { continue; }
            let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else { continue; };
            let delta = event.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta"));

            if let Some(content) = delta.and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                text.push_str(content);
            }

            if let Some(tcs) = delta.and_then(|d| d.get("tool_calls")).and_then(|tc| tc.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(pending_tools.len() as u64) as usize;
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let f = tc.get("function");
                    let name = f.and_then(|v| v.get("name")).and_then(|v| v.as_str()).unwrap_or("");
                    let args = f.and_then(|v| v.get("arguments")).and_then(|v| v.as_str()).unwrap_or("");

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

    SseResult { text, tool_calls }
}

#[cfg(test)]
#[path = "tests/sse_tests.rs"]
mod tests;

