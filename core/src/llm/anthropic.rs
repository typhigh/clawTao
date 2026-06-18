use super::adapter::{ApiAdapter, HttpRequest, StreamEvent};
use super::types::{LlmRequest, LlmResponse};
use crate::store::ToolCall;
use anyhow::Result;
use serde_json::{json, Value};

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicAdapter;

impl ApiAdapter for AnthropicAdapter {
    fn build(&self, req: &LlmRequest, api_key: &str, base_url: &str) -> Result<HttpRequest> {
        let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));

        let messages: Vec<serde_json::Value> = req.messages.iter().map(|m| {
            if let Some(ref tool_calls) = m.tool_calls {
                let mut blocks: Vec<Value> = tool_calls.iter().map(|tc| {
                    let args: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
                    json!({"type": "tool_use", "id": tc.id, "name": tc.function.name, "input": args})
                }).collect();
                // thinking → text → tool_use (chronological order).
                if !m.content.is_empty() {
                    blocks.insert(0, json!({"type": "text", "text": m.content}));
                }
                if let Some(ref thinking) = m.thinking {
                    if !thinking.is_empty() {
                        blocks.insert(0, json!({"type": "thinking", "thinking": thinking}));
                    }
                }
                json!({"role": "assistant", "content": blocks})
            } else if m.role == "tool" {
                json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": m.tool_call_id, "content": m.content}]})
            } else if m.role == "assistant" {
                // Assistant text message: include thinking block for multi-turn replay.
                let mut blocks: Vec<Value> = Vec::new();
                if let Some(ref thinking) = m.thinking {
                    if !thinking.is_empty() {
                        blocks.push(json!({"type": "thinking", "thinking": thinking}));
                    }
                }
                blocks.push(json!({"type": "text", "text": m.content}));
                json!({"role": "assistant", "content": blocks})
            } else {
                json!({"role": m.role, "content": [{"type": "text", "text": m.content}]})
            }
        }).collect();

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

            let Ok(event) = serde_json::from_str::<Value>(&data) else { continue; };
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
                    return Err(anyhow::anyhow!("Anthropic API error: {}", event["error"]["message"].as_str().unwrap_or("unknown")));
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

#[cfg(test)]
#[path = "tests/anthropic_tests.rs"]
mod tests;
