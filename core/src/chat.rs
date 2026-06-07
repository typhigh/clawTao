//! chat.send handler — the core LLM interaction loop.

use anyhow::Result;
use crate::config::LlmConfig;
use crate::jsonrpc::{Notification, Response};
use crate::session::SessionManager;
use crate::sse::parse_sse_response;
use crate::tools::registry::ToolRegistry;
use crate::{get_param, write_notification, write_response};
use reqwest::blocking::Client;
use serde_json::json;
use tracing::{debug, trace};

const MAX_TOOL_ROUNDS: usize = 10;

pub(crate) fn handle_chat_send(
    request: &crate::jsonrpc::Request,
    session_manager: &mut SessionManager,
    tool_registry: &ToolRegistry,
    llm_config: &LlmConfig,
    client: &Client,
) -> Result<()> {
    let message_text = get_param(&request.params, "message")?;
    let session_id = get_param(&request.params, "sessionId")?;

    session_manager.add_message(session_id, "user", message_text);

    let session = session_manager.get_session(session_id)
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
    let run_id = uuid::Uuid::new_v4().to_string();

    write_notification(&Notification::new("chat.started", Some(json!({
        "sessionId": session_id, "runId": run_id
    }))))?;

    if llm_config.api_key.is_empty() {
        return Err(anyhow::anyhow!("API key not configured"));
    }
    let api_url = format!("{}/chat/completions", llm_config.base_url.trim_end_matches('/'));

    let mut messages = session.messages.clone();
    let mut final_content = String::new();

    for round in 0..MAX_TOOL_ROUNDS {
        let mut api_messages: Vec<serde_json::Value> = messages.iter().map(|m| m.to_llm_message()).collect();
        api_messages.insert(0, json!({
            "role": "system",
            "content": "You are ClawTao, a helpful AI assistant with tool calling capabilities."
        }));

        let tools_specs: Vec<serde_json::Value> = tool_registry.list_specs()
            .iter()
            .filter_map(|s| serde_json::to_value(s).ok())
            .collect();

        let body = json!({
            "model": llm_config.model,
            "messages": api_messages,
            "stream": true,
            "tools": tools_specs,
        });

        debug!("LLM round {round}: url={api_url} model={} msgs={} tools={}", llm_config.model, api_messages.len(), tools_specs.len());
        trace!("LLM request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());

        let mut resp = client.post(&api_url)
            .header("Authorization", format!("Bearer {}", llm_config.api_key))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&body)?)
            .send()?;

        debug!("LLM response: status={} bytes={}", resp.status(), resp.content_length().unwrap_or(0));

        use std::io::Read;
        let mut body_bytes = Vec::new();
        resp.read_to_end(&mut body_bytes)?;
        let body_str = String::from_utf8_lossy(&body_bytes);

        debug!("LLM response body: {} bytes", body_bytes.len());
        trace!("LLM response body: {}", body_str);

        let result = parse_sse_response(&body_str);

        if !result.text.is_empty() {
            write_notification(&Notification::new("chat.text_delta", Some(json!({
                "sessionId": session_id, "runId": run_id, "delta": result.text
            }))))?;
        }

        let round_text = result.text;
        let round_tool_calls = result.tool_calls;

        if round_tool_calls.is_empty() {
            final_content = round_text;
            break;
        }

        debug!("Round {round}: executing {} tool calls", round_tool_calls.len());

        let tc_clone = round_tool_calls.clone();
        session_manager.add_assistant_tool_calls(session_id, tc_clone);

        for tc in &round_tool_calls {
            debug!("Executing tool: {} id={} args={}", tc.function.name, tc.id, tc.function.arguments);

            let args_val: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

            write_notification(&Notification::new("chat.tool_started", Some(json!({
                "sessionId": session_id, "runId": run_id,
                "toolCallId": tc.id, "toolName": tc.function.name,
                "toolInput": args_val,
            }))))?;

            let tool_result = match tool_registry.get(&tc.function.name) {
                Some(executor) => match executor.execute(args_val.clone()) {
                    Ok(output) => output,
                    Err(e) => format!("Tool error: {e}"),
                },
                None => format!("Unknown tool: {}", tc.function.name),
            };

            debug!("Tool result for {}: {:.200}", tc.function.name, tool_result);

            write_notification(&Notification::new("chat.tool_result", Some(json!({
                "sessionId": session_id, "runId": run_id,
                "toolCallId": tc.id, "toolName": tc.function.name,
                "result": tool_result,
            }))))?;

            session_manager.add_tool_result(session_id, &tc.id, &tool_result);
        }

        messages = session_manager.get_session(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found after tool execution"))?
            .messages.clone();
    }

    if !final_content.is_empty() {
        session_manager.add_message(session_id, "assistant", &final_content);
    } else {
        session_manager.add_message(session_id, "assistant", "(no response)");
    }

    write_notification(&Notification::new("chat.done", Some(json!({
        "sessionId": session_id, "runId": run_id
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
