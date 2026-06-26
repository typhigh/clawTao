//! chat.send handler — the agent turn loop.

use anyhow::{Context, Result};
use crate::config::LlmConfig;
use crate::jsonrpc::{Notification, Response};
use crate::llm::{ApiAdapter, AnthropicAdapter, LlmMessage, LlmRequest, OpenAiAdapter, UnifiedTool};
use crate::llm::types::LlmResponse;
use crate::store::{self, store_trait::SessionStore};
use crate::tools::registry::ToolRegistry;
use crate::jsonrpc::{get_param, write_notification, write_response};
use reqwest::blocking::Client;
use serde_json::json;
use tracing::{debug, trace};

/// Immutable context for a single turn.
struct TurnContext {
    session_id: String,
    run_id: String,
    system_prompt: String,
    tools: Vec<UnifiedTool>,
}

/// Entry point for a single chat turn.
pub(crate) fn run_turn(
    request: &crate::jsonrpc::Request,
    store: &dyn SessionStore,
    tool_registry: &ToolRegistry,
    llm_config: &LlmConfig,
    client: &Client,
) -> Result<()> {
    let message_text = get_param(&request.params, "message")?;
    let session_id = get_param(&request.params, "sessionId")?;

    store::add_message(store, session_id, "user", message_text)?;

    let session = store.get(session_id)?
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

    let tools: Vec<UnifiedTool> = tool_registry.list_specs().iter().map(|spec| UnifiedTool {
        name: spec.function.name.clone(),
        description: spec.function.description.clone(),
        parameters: spec.function.parameters.clone(),
    }).collect();

    let ctx = TurnContext {
        session_id: session_id.to_string(),
        run_id,
        system_prompt: crate::system_prompt::build(tool_registry),
        tools,
    };

    let mut messages = session.messages;

    // ── Agent loop ──────────────────────────────────────────────────

    let (final_content, final_thinking) = loop {
        let response = llm_step(adapter, client, llm_config, &messages, &ctx)?;

        if response.tool_calls.is_empty() {
            break (response.text, response.thinking);
        }

        store::add_assistant_tool_calls(
            store, &ctx.session_id,
            response.tool_calls.clone(),
            &response.text,
            response.thinking.as_deref(),
        )?;

        for tc in &response.tool_calls {
            debug!("Executing tool: {} id={} args={}", tc.function.name, tc.id, tc.function.arguments);

            let args_val: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

            write_notification(&Notification::new("chat.stream", Some(json!({
                "sessionId": ctx.session_id, "runId": ctx.run_id,
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
                "sessionId": ctx.session_id, "runId": ctx.run_id,
                "kind": "tool_result",
                "toolCallId": tc.id, "toolName": tc.function.name,
                "output": tool_result,
            }))))?;

            store::add_tool_result(store, &ctx.session_id, &tc.id, &tool_result)?;
        }

        messages = store.get(&ctx.session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found after tool execution"))?
            .messages;
    };

    // ── Finalize ─────────────────────────────────────────────────────

    let content = if final_content.is_empty() { "(no response)" } else { &final_content };
    store::add_assistant_message(store, &ctx.session_id, content, final_thinking.as_deref())?;

    write_notification(&Notification::new("chat.stream", Some(json!({
        "sessionId": ctx.session_id, "runId": ctx.run_id, "kind": "done"
    }))))?;

    write_response(&Response::success(request.id.clone(), json!({
        "runId": ctx.run_id,
        "message": {
            "id": uuid::Uuid::new_v4().to_string(),
            "role": "assistant",
            "content": final_content,
            "timestamp": chrono::Utc::now().timestamp_millis()
        }
    })))?;

    Ok(())
}

/// One LLM call: build request → HTTP → stream SSE → parse response.
fn llm_step(
    adapter: &dyn ApiAdapter,
    client: &Client,
    config: &LlmConfig,
    messages: &[store::Message],
    ctx: &TurnContext,
) -> Result<LlmResponse> {
    let llm_msgs: Vec<LlmMessage> = messages.iter().map(|m| LlmMessage {
        role: m.role.clone(),
        content: m.content.clone(),
        tool_calls: m.tool_calls.clone(),
        tool_call_id: m.tool_call_id.clone(),
        thinking: m.thinking.clone(),
    }).collect();

    let http = adapter.build(&LlmRequest {
        system: ctx.system_prompt.clone(),
        model: config.model.clone(),
        messages: llm_msgs,
        tools: ctx.tools.clone(),
        thinking_enabled: config.thinking_enabled,
    }, &config.api_key, &config.base_url)?;

    debug!("LLM call: url={} model={}", http.url, config.model);
    trace!("LLM request body: {}", http.body);

    let mut resp = client.post(&http.url);
    for (k, v) in &http.headers {
        resp = resp.header(k.as_str(), v.as_str());
    }
    let resp = resp.body(http.body.clone()).send()
        .with_context(|| format!("Failed to reach {}", http.url))?;

    debug!("LLM response: status={}", resp.status());

    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(resp);
    let mut line = String::new();
    let mut body_bytes = Vec::new();

    while reader.read_line(&mut line)? > 0 {
        body_bytes.extend_from_slice(line.as_bytes());
        if let Some(data) = line.trim().strip_prefix("data: ") {
            if data == "[DONE]" { line.clear(); continue; }
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(data) {
                for se in adapter.stream_events(&event) {
                    write_notification(&Notification::new("chat.stream", Some(json!({
                        "sessionId": ctx.session_id, "runId": ctx.run_id,
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

    adapter.parse_stream(&body_str)
}
