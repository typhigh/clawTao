use super::adapter::{ApiAdapter, HttpRequest};
use super::types::{LlmRequest, LlmResponse};
use crate::sse::parse_sse_response;
use anyhow::Result;
use serde_json::json;

pub struct OpenAiAdapter;

impl ApiAdapter for OpenAiAdapter {
    fn build(&self, req: &LlmRequest, api_key: &str, base_url: &str) -> Result<HttpRequest> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let model = req.model.clone();

        let messages: Vec<serde_json::Value> = std::iter::once(json!({
            "role": "system", "content": req.system,
        }))
        .chain(req.messages.iter().map(|m| match m.role.as_str() {
            "tool" => json!({"role": "tool", "tool_call_id": m.tool_call_id, "content": m.content}),
            "assistant" if m.tool_calls.is_some() => json!({"role": "assistant", "content": null, "tool_calls": m.tool_calls}),
            _ => json!({"role": m.role, "content": m.content}),
        }))
        .collect();

        let tools: Vec<serde_json::Value> = req.tools.iter().map(|t| {
            json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}})
        }).collect();

        let body = json!({"model": model, "messages": messages, "stream": true, "tools": tools});

        Ok(HttpRequest {
            url,
            headers: vec![
                ("Authorization".into(), format!("Bearer {api_key}")),
                ("Content-Type".into(), "application/json".into()),
            ],
            body: serde_json::to_string(&body)?,
        })
    }

    fn parse_stream(&self, body: &str) -> Result<LlmResponse> {
        let result = parse_sse_response(body);
        Ok(LlmResponse { text: result.text, tool_calls: result.tool_calls })
    }
}

#[cfg(test)]
#[path = "tests/openai_tests.rs"]
mod tests;
