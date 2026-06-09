//! chat.send handler — the core LLM interaction loop.

use anyhow::Result;
use crate::config::LlmConfig;
use crate::jsonrpc::{Notification, Response};
use crate::llm::{ApiAdapter, AnthropicAdapter, LlmMessage, LlmRequest, OpenAiAdapter, UnifiedTool};
use crate::store::SessionManager;
use crate::tools::registry::ToolRegistry;
use crate::{get_param, write_notification, write_response};
use reqwest::blocking::Client;
use serde_json::json;
use tracing::{debug, trace};

pub(crate) fn handle_chat_send(
    request: &crate::jsonrpc::Request,
    session_manager: &mut SessionManager,
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

    write_notification(&Notification::new("chat.started", Some(json!({
        "sessionId": session_id, "runId": run_id
    }))))?;

    if llm_config.api_key.is_empty() {
        return Err(anyhow::anyhow!("API key not configured"));
    }

    let adapter: &dyn ApiAdapter = match llm_config.api_protocol.as_str() {
        "anthropic" => &AnthropicAdapter,
        _ => &OpenAiAdapter,
    };

    let mut messages = session.messages.clone();

    let unified_tools: Vec<UnifiedTool> = tool_registry.list_specs().iter().map(|s| {
        let f = serde_json::to_value(s).unwrap_or_default();
        let func = &f["function"];
        UnifiedTool {
            name: func["name"].as_str().unwrap_or("").into(),
            description: func["description"].as_str().unwrap_or("").into(),
            parameters: func["parameters"].clone(),
        }
    }).collect();

    let final_content = loop {
        let llm_msgs: Vec<LlmMessage> = messages.iter().map(|m| LlmMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: m.tool_calls.clone(),
            tool_call_id: m.tool_call_id.clone(),
        }).collect();

        let llm_req = LlmRequest {
            system: "You are ClawTao, a helpful AI assistant with tool calling capabilities.".into(),
            model: llm_config.model.clone(),
            messages: llm_msgs,
            tools: unified_tools.clone(),
        };

        let http = adapter.build(&llm_req, &llm_config.api_key, &llm_config.base_url)?;

        debug!("LLM call: url={} model={}", http.url, llm_config.model);
        trace!("LLM request body: {}", http.body);

        let mut resp = client.post(&http.url);
        for (k, v) in &http.headers {
            resp = resp.header(k.as_str(), v.as_str());
        }
        let mut resp = resp.body(http.body.clone()).send()?;

        debug!("LLM response: status={}", resp.status());

        use std::io::Read;
        let mut body_bytes = Vec::new();
        resp.read_to_end(&mut body_bytes)?;
        let body_str = String::from_utf8_lossy(&body_bytes);

        debug!("LLM response body: {} bytes", body_bytes.len());
        trace!("LLM response body: {}", body_str);

        let result = adapter.parse_stream(&body_str)?;

        if !result.text.is_empty() {
            write_notification(&Notification::new("chat.text_delta", Some(json!({
                "sessionId": session_id, "runId": run_id, "delta": result.text
            }))))?;
        }

        if result.tool_calls.is_empty() {
            break result.text;
        }

        debug!("Executing {} tool calls", result.tool_calls.len());

        session_manager.add_assistant_tool_calls(session_id, result.tool_calls.clone())?;

        for tc in &result.tool_calls {
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

            session_manager.add_tool_result(session_id, &tc.id, &tool_result)?;
        }

        messages = session_manager.get_session(session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found after tool execution"))?
            .messages.clone();
    };

    if final_content.is_empty() {
        session_manager.add_message(session_id, "assistant", "(no response)")?;
    } else {
        session_manager.add_message(session_id, "assistant", &final_content)?;
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
