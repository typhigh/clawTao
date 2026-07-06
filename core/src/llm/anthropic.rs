use super::adapter::{ApiAdapter, HttpRequest, StreamEvent};
use super::types::{LlmMessage, LlmRequest, LlmResponse};
use crate::error::ChatError;
use crate::store::ToolCall;
use anyhow::Result;
use serde_json::{json, Value};
use tracing::trace;

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicAdapter;

impl ApiAdapter for AnthropicAdapter {
    fn build(&self, req: &LlmRequest, api_key: &str, base_url: &str) -> Result<HttpRequest> {
        let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));

        let messages = Self::convert_messages(&req.messages);

        let tools: Vec<Value> = req.tools.iter().map(|t| {
            json!({"name": t.name, "description": t.description, "input_schema": t.parameters})
        }).collect();

        let mut body = json!({
            "model": req.model,
            "max_tokens": 4096,
            "messages": messages,
            "stream": true,
        });
        if !req.system.is_empty() {
            body["system"] = json!(req.system);
        }
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        if req.thinking_enabled {
            body["thinking"] = json!({"type": "adaptive"});
        } else {
            body["thinking"] = json!({"type": "disabled"});
        }

        Ok(HttpRequest {
            url,
            headers: vec![
                ("x-api-key".into(), api_key.to_string()),
                ("anthropic-version".into(), ANTHROPIC_VERSION.into()),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: serde_json::to_string(&body)?,
        })
    }

    fn parse_stream(&self, body: &str) -> Result<LlmResponse> {
        let mut text = String::new();
        let mut thinking = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut current_tool: Option<(String, String, String)> = None;

        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            let data = if let Some(d) = line.strip_prefix("data: ") {
                d.to_string()
            } else {
                continue;
            };

            let Ok(event) = serde_json::from_str::<Value>(&data) else {
                trace!("Anthropic SSE: skipping malformed data line");
                continue;
            };
            let ev_type = event["type"].as_str().unwrap_or("");

            match ev_type {
                "content_block_delta" => {
                    let delta = &event["delta"];
                    if delta["type"] == "text_delta" {
                        if let Some(t) = delta["text"].as_str() {
                            text.push_str(t);
                        }
                    } else if delta["type"] == "thinking_delta" {
                        if let Some(t) = delta["thinking"].as_str() {
                            thinking.push_str(t);
                        }
                    } else if delta["type"] == "input_json_delta" {
                        if let Some(partial) = delta["partial_json"].as_str() {
                            if let Some(ref mut pending) = current_tool {
                                pending.2.push_str(partial);
                            }
                        }
                    }
                }
                "content_block_start" => {
                    let block = &event["content_block"];
                    if block["type"] == "tool_use" {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let input_val = &block["input"];
                        let args = if input_val.as_object().is_none_or(|o| o.is_empty()) {
                            String::new()
                        } else {
                            input_val.to_string()
                        };
                        if !id.is_empty() {
                            if let Some((prev_id, prev_name, prev_args)) = current_tool.take() {
                                if serde_json::from_str::<Value>(&prev_args).is_ok() {
                                    tool_calls.push(ToolCall {
                                        id: prev_id, call_type: "function".into(),
                                        function: crate::store::ToolCallFunction { name: prev_name, arguments: prev_args },
                                    });
                                }
                            }
                            current_tool = Some((id, name, args));
                        }
                    }
                }
                "content_block_stop" => {
                    if let Some((id, name, args)) = current_tool.take() {
                        if !id.is_empty() && serde_json::from_str::<Value>(&args).is_ok() {
                            tool_calls.push(ToolCall {
                                id, call_type: "function".into(),
                                function: crate::store::ToolCallFunction { name, arguments: args },
                            });
                        }
                    }
                }
                "error" => {
                    let msg = event["error"]["message"]
                        .as_str()
                        .unwrap_or("unknown");
                    let err_type = event["error"]["type"]
                        .as_str()
                        .unwrap_or("");
                    let detail = if err_type.is_empty() {
                        msg.to_string()
                    } else {
                        format!("{err_type}: {msg}")
                    };
                    return Err(anyhow::anyhow!(ChatError::BadRequest { detail }));
                }
                _ => {}
            }
        }

        if let Some((id, name, args)) = current_tool.take() {
            if !id.is_empty() && serde_json::from_str::<Value>(&args).is_ok() {
                tool_calls.push(ToolCall {
                    id, call_type: "function".into(),
                    function: crate::store::ToolCallFunction { name, arguments: args },
                });
            }
        }

        Ok(LlmResponse {
            text,
            thinking: if thinking.is_empty() { None } else { Some(thinking) },
            tool_calls,
        })
    }

    fn stream_events(&self, event: &serde_json::Value) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        let delta = match event.get("delta") {
            Some(d) => d,
            None => return out,
        };
        match delta.get("type").and_then(|v| v.as_str()) {
            Some("text_delta") => {
                if let Some(t) = delta.get("text").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        out.push(StreamEvent { kind: "text".into(), delta: t.to_string() });
                    }
                }
            }
            Some("thinking_delta") => {
                if let Some(t) = delta.get("thinking").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        out.push(StreamEvent { kind: "thinking".into(), delta: t.to_string() });
                    }
                }
            }
            _ => {}
        }
        out
    }
}

// ── Message conversion helpers ─────────────────────────────────────────
//
// These convert the internal LlmMessage representation into Anthropic's
// content-block wire format ({type, role, content: [...]}).

impl AnthropicAdapter {
    /// Convert a slice of `LlmMessage` into Anthropic message JSON.
    ///
    /// Consecutive `role: "tool"` messages are merged into a single user
    /// message so that all `tool_result` blocks sit together — required
    /// by the Anthropic API for parallel tool calls.
    fn convert_messages(msgs: &[LlmMessage]) -> Vec<Value> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < msgs.len() {
            if msgs[i].role == "tool" {
                // Collect a run of consecutive tool messages.
                let start = i;
                while i < msgs.len() && msgs[i].role == "tool" {
                    i += 1;
                }
                out.push(Self::build_tool_results(&msgs[start..i]));
            } else if msgs[i].tool_calls.is_some() {
                out.push(Self::build_assistant_tool_use(&msgs[i]));
                i += 1;
            } else if msgs[i].role == "assistant" {
                out.push(Self::build_assistant_text(&msgs[i]));
                i += 1;
            } else {
                out.push(Self::build_text_message(&msgs[i]));
                i += 1;
            }
        }
        out
    }

    /// User or system message. When images are attached, they become
    /// `{"type": "image", "source": ...}` blocks before the text block.
    fn build_text_message(msg: &LlmMessage) -> Value {
        if let Some(ref images) = msg.images {
            let mut blocks: Vec<Value> = Vec::new();
            for img in images {
                blocks.push(json!({
                    "type": "image",
                    "source": {"type": "url", "url": format!("data:{};base64,{}", img.media_type, img.base64)}
                }));
            }
            if !msg.content.is_empty() {
                blocks.push(json!({"type": "text", "text": msg.content}));
            }
            json!({"role": msg.role, "content": blocks})
        } else {
            json!({"role": msg.role, "content": [{"type": "text", "text": msg.content}]})
        }
    }

    /// Assistant text reply with optional thinking block.
    ///
    /// Output order: `thinking` (if present), then `text`.
    fn build_assistant_text(msg: &LlmMessage) -> Value {
        let mut blocks: Vec<Value> = Vec::new();
        if let Some(ref thinking) = msg.thinking {
            if !thinking.is_empty() {
                blocks.push(json!({"type": "thinking", "thinking": thinking}));
            }
        }
        blocks.push(json!({"type": "text", "text": msg.content}));
        json!({"role": "assistant", "content": blocks})
    }

    /// Assistant message that contains tool calls.
    ///
    /// Output order follows Anthropic's chronological requirement:
    /// `thinking` → `text` → `tool_use` (one block per call).
    fn build_assistant_tool_use(msg: &LlmMessage) -> Value {
        let tool_calls = msg.tool_calls.as_ref().expect("build_assistant_tool_use called without tool_calls");
        let mut blocks: Vec<Value> = Vec::new();
        if let Some(ref thinking) = msg.thinking {
            if !thinking.is_empty() {
                blocks.push(json!({"type": "thinking", "thinking": thinking}));
            }
        }
        if !msg.content.is_empty() {
            blocks.push(json!({"type": "text", "text": msg.content}));
        }
        for tc in tool_calls {
            let args: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
            blocks.push(json!({"type": "tool_use", "id": tc.id, "name": tc.function.name, "input": args}));
        }
        json!({"role": "assistant", "content": blocks})
    }

    /// Pack one or more consecutive tool-result messages into a single
    /// `user` message with multiple `tool_result` blocks.
    ///
    /// The Anthropic protocol requires this: when an assistant makes N
    /// parallel tool calls, all N results must live in the next user
    /// message, not in N separate user messages.
    fn build_tool_results(msgs: &[LlmMessage]) -> Value {
        let blocks: Vec<Value> = msgs.iter().map(|tm| {
            if let Some(ref images) = tm.images {
                let mut content_blocks: Vec<Value> = Vec::new();
                for img in images {
                    content_blocks.push(json!({
                        "type": "image",
                        "source": {"type": "base64", "media_type": img.media_type, "data": img.base64}
                    }));
                }
                if !tm.content.is_empty() {
                    content_blocks.push(json!({"type": "text", "text": tm.content}));
                }
                json!({"type": "tool_result", "tool_use_id": tm.tool_call_id, "content": content_blocks})
            } else {
                json!({"type": "tool_result", "tool_use_id": tm.tool_call_id, "content": tm.content})
            }
        }).collect();
        json!({"role": "user", "content": blocks})
    }
}

#[cfg(test)]
#[path = "tests/anthropic_tests.rs"]
mod tests;
