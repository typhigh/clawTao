//! chat.send handler — the core LLM interaction loop.

use anyhow::{Context, Result};
use crate::config::LlmConfig;
use crate::jsonrpc::{Notification, Response};
use crate::llm::{ApiAdapter, AnthropicAdapter, LlmMessage, LlmRequest, OpenAiAdapter, UnifiedTool};
use crate::store::SessionManager;
use crate::tools::registry::ToolRegistry;
use crate::jsonrpc::{get_param, write_notification, write_response};
use reqwest::blocking::Client;
use serde_json::json;
use tracing::{debug, trace};

pub(crate) fn handle_chat_send(
    request: &crate::jsonrpc::Request,
    session_manager: &SessionManager,
    tool_registry: &ToolRegistry,
    llm_config: &LlmConfig,
    client: &Client,
) -> Result<()> {
    let message_text = get_param(&request.params, "message")?;
    let session_id = get_param(&request.params, "sessionId")?;

    session_manager.add_message(session_id, "user", message_text)?;

    let session = session_manager.get_session(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
    let run_id = uuid::Uuid::new_v4().to_string();

    write_notification(&Notification::new("chat.stream", Some(json!({
        "sessionId": session_id, "runId": run_id, "kind": "started"
    }))))?;

    if llm_config.api_key.is_empty() {
        return Err(anyhow::anyhow!("API key not configured"));
    }

    let adapter: Box<dyn ApiAdapter> = match llm_config.api_protocol.as_str() {
        "anthropic" => Box::new(AnthropicAdapter),
        _ => Box::new(OpenAiAdapter),
    };
    let adapter = adapter.as_ref();

    let mut messages = session.messages.clone();

    let unified_tools: Vec<UnifiedTool> = tool_registry.list_specs().iter().map(|spec| {
        UnifiedTool {
            name: spec.function.name.clone(),
            description: spec.function.description.clone(),
            parameters: spec.function.parameters.clone(),
        }
    }).collect();

    let (final_content, final_thinking) = loop {
        let llm_msgs: Vec<LlmMessage> = messages.iter().map(|m| LlmMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: m.tool_calls.clone(),
            tool_call_id: m.tool_call_id.clone(),
            thinking: m.thinking.clone(),
        }).collect();

        let llm_req = LlmRequest {
            system: crate::system_prompt::build(tool_registry),
            model: llm_config.model.clone(),
            messages: llm_msgs,
            tools: unified_tools.clone(),
            thinking_enabled: llm_config.thinking_enabled,
        };

        let http = adapter.build(&llm_req, &llm_config.api_key, &llm_config.base_url)?;

        debug!("LLM call: url={} model={}", http.url, llm_config.model);
        trace!("LLM request body: {}", http.body);

        let mut resp = client.post(&http.url);
        for (k, v) in &http.headers {
            resp = resp.header(k.as_str(), v.as_str());
        }
        let resp = resp.body(http.body.clone()).send()
            .with_context(|| format!("Failed to reach {}", http.url))?;

        debug!("LLM response: status={}", resp.status());

        // Stream SSE line by line, send text deltas immediately
        use std::io::{BufRead, BufReader};
        let mut reader = BufReader::new(resp);
        let mut line = String::new();
        let mut body_bytes = Vec::new();

        while reader.read_line(&mut line)? > 0 {
            body_bytes.extend_from_slice(line.as_bytes());
            let trimmed = line.trim();

            if let Some(data) = trimmed.strip_prefix("data: ") {
                if data == "[DONE]" { line.clear(); continue; }
                if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                    for se in adapter.stream_events(&event) {
                        write_notification(&Notification::new("chat.stream", Some(json!({
                            "sessionId": session_id, "runId": run_id,
                            "kind": se.kind, "delta": se.delta,
                        }))))?;
                    }
                }
            }
            line.clear();
        }

        let body_str = String::from_utf8_lossy(&body_bytes);
        debug!("LLM response body: {} bytes", body_bytes.len());
        trace!("LLM response body: {}", body_str);

        let result = adapter.parse_stream(&body_str)?;

        if result.tool_calls.is_empty() {
            break (result.text, result.thinking);
        }

        debug!("Executing {} tool calls", result.tool_calls.len());

        session_manager.add_assistant_tool_calls(
            session_id,
            result.tool_calls.clone(),
            &result.text,
            result.thinking.as_deref(),
        )?;

        for tc in &result.tool_calls {
            debug!("Executing tool: {} id={} args={}", tc.function.name, tc.id, tc.function.arguments);

            let args_val: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

            write_notification(&Notification::new("chat.stream", Some(json!({
                "sessionId": session_id, "runId": run_id,
                "kind": "tool_call",
                "toolCallId": tc.id, "toolName": tc.function.name,
                "input": args_val,
            }))))?;

            let tool_result = match tool_registry.get(&tc.function.name) {
                Some(executor) => match executor.execute(args_val.clone()) {
                    Ok(output) => output,
                    Err(e) => format!("Tool error: {e}"),
                },
                None => format!("Unknown tool: {}", tc.function.name),
            };

            debug!("Tool result for {}: {:.200}", tc.function.name, tool_result);

            write_notification(&Notification::new("chat.stream", Some(json!({
                "sessionId": session_id, "runId": run_id,
                "kind": "tool_result",
                "toolCallId": tc.id, "toolName": tc.function.name,
                "output": tool_result,
            }))))?;

            session_manager.add_tool_result(session_id, &tc.id, &tool_result)?;
        }

        messages = session_manager.get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found after tool execution"))?
            .messages.clone();
    };

    let content = if final_content.is_empty() { "(no response)" } else { &final_content };
    session_manager.add_assistant_message(session_id, content, final_thinking.as_deref())?;

    write_notification(&Notification::new("chat.stream", Some(json!({
        "sessionId": session_id, "runId": run_id,
        "kind": "done"
    }))))?;

    let response = json!({
        "runId": run_id,
        "message": {
            "id": uuid::Uuid::new_v4().to_string(),
            "role": "assistant",
            "content": final_content,
            "timestamp": chrono::Utc::now().timestamp_millis()
        }
    });
    write_response(&Response::success(request.id.clone(), response))?;

    Ok(())
}
