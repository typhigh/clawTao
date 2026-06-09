use super::adapter::{ApiAdapter, HttpRequest};
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
                if !m.content.is_empty() {
                    blocks.insert(0, json!({"type": "text", "text": m.content}));
                }
                json!({"role": "assistant", "content": blocks})
            } else if m.role == "tool" {
                json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": m.tool_call_id, "content": m.content}]})
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
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut current_tool: Option<(String, String, String)> = None; // (id, name, args_json)

        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            // Anthropic SSE: "event: type\ndata: {...}"
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
                        let args = block["input"].to_string(); // full input from start event
                        if !id.is_empty() {
                            // Finalize previous tool if any
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
                    // Finalize current tool
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

        // Don't forget final tool if stream ended without content_block_stop
        if let Some((id, name, args)) = current_tool.take() {
            if !id.is_empty() && serde_json::from_str::<Value>(&args).is_ok() {
                tool_calls.push(ToolCall {
                    id, call_type: "function".into(),
                    function: crate::store::ToolCallFunction { name, arguments: args },
                });
            }
        }

        Ok(LlmResponse { text, tool_calls })
    }
}
