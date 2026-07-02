//! chat.send handler — the agent turn loop.

use anyhow::{Context, Result};
use crate::jsonrpc::{Notification, Response};
use crate::llm::{ApiAdapter, AnthropicAdapter, LlmMessage, LlmRequest, OpenAiAdapter, UnifiedTool};
use crate::llm::types::LlmResponse;
use crate::store::{self, store_trait::SessionStore};
use crate::tools::{self, registry::ToolRegistry};
use crate::jsonrpc::{get_param, write_notification, write_response};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{debug, trace};

/// Immutable context for a single turn.
pub(crate) struct TurnContext {
    session_id: String,
    run_id: String,
    system_prompt: String,
    tools: Vec<UnifiedTool>,
}

/// Entry point for a single chat turn. Sets up the adapter, tool registry,
/// and context, then runs the state machine.
pub(crate) fn run_turn(
    request: &crate::jsonrpc::Request,
    store: &dyn SessionStore,
    client: &Client,
    cancel: &AtomicBool,
) -> Result<()> {
    let message_text = get_param(&request.params, "message")?;
    let session_id = get_param(&request.params, "sessionId")?;

    let config = request.params.as_ref()
        .and_then(|p| p.get("config"))
        .ok_or_else(|| anyhow::anyhow!("Missing config"))?;

    let api_key = config["api_key"].as_str().filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("API key not configured"))?;
    let base_url = config["base_url"].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing config field: base_url"))?;
    let model = config["model"].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing config field: model"))?;
    let protocol = config["api_protocol"].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing config field: api_protocol"))?;
    let thinking_enabled = config["thinking_enabled"].as_bool().unwrap_or(false);

    let blocked: Vec<String> = config["bash_blocked_commands"]
        .as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let bash_timeout = config["bash_timeout_secs"].as_u64();
    let mut tool_registry = ToolRegistry::new();
    tools::builtin::register_all(&mut tool_registry, blocked, bash_timeout);

    store::add_message(store, session_id, "user", message_text)?;

    let session = store.get(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
    let run_id = uuid::Uuid::new_v4().to_string();

    write_notification(&Notification::new("chat.stream", Some(json!({
        "sessionId": session_id, "runId": run_id, "kind": "started"
    }))))?;

    let adapter: Box<dyn ApiAdapter> = match protocol {
        "anthropic" => Box::new(AnthropicAdapter),
        _ => Box::new(OpenAiAdapter),
    };

    let tools: Vec<UnifiedTool> = tool_registry.list_specs().iter().map(|spec| UnifiedTool {
        name: spec.function.name.clone(),
        description: spec.function.description.clone(),
        parameters: spec.function.parameters.clone(),
    }).collect();

    let ctx = TurnContext {
        session_id: session_id.to_string(),
        run_id,
        system_prompt: crate::system_prompt::build(&tool_registry),
        tools,
    };

    let (final_content, final_thinking) = run_state_machine(
        store, adapter.as_ref(), client,
        api_key, base_url, model, thinking_enabled,
        session.messages, &ctx, &tool_registry, cancel,
    )?;

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

// ── State machine ──────────────────────────────────────────────────

enum TurnState {
    Sampling,
    Evaluating { response: LlmResponse },
    Executing { response: LlmResponse, cursor: usize },
    Interrupted { text: String, thinking: Option<String> },
    Finalizing { text: String, thinking: Option<String> },
}

fn run_state_machine(
    store: &dyn SessionStore,
    adapter: &dyn ApiAdapter,
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    thinking_enabled: bool,
    mut messages: Vec<store::Message>,
    ctx: &TurnContext,
    tool_registry: &ToolRegistry,
    cancel: &AtomicBool,
) -> Result<(String, Option<String>)> {
    let mut state = TurnState::Sampling;

    loop {
        state = match state {
            TurnState::Sampling => {
                let response = llm_step(adapter, client, api_key, base_url, model, thinking_enabled, &messages, ctx, cancel)?;
                TurnState::Evaluating { response }
            }
            TurnState::Evaluating { response } => {
                if cancel.load(Ordering::SeqCst) {
                    debug!("TurnState: Evaluating → Interrupted (cancel=true)");
                    TurnState::Interrupted { text: response.text, thinking: response.thinking }
                } else if response.tool_calls.is_empty() {
                    TurnState::Finalizing { text: response.text, thinking: response.thinking }
                } else {
                    store::add_assistant_tool_calls(
                        store, &ctx.session_id,
                        response.tool_calls.clone(),
                        &response.text,
                        response.thinking.as_deref(),
                    )?;
                    TurnState::Executing { response, cursor: 0 }
                }
            }
            TurnState::Executing { response, cursor } => {
                if cursor >= response.tool_calls.len() {
                    messages = store.get(&ctx.session_id)?
                        .ok_or_else(|| anyhow::anyhow!("Session not found after tool execution"))?
                        .messages;
                    TurnState::Sampling
                } else {
                    let tc = &response.tool_calls[cursor];
                    debug!("Executing tool: {} id={} args={}", tc.function.name, tc.id, tc.function.arguments);
                    let args_val: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);

                    write_notification(&Notification::new("chat.stream", Some(json!({
                        "sessionId": ctx.session_id, "runId": ctx.run_id,
                        "kind": "tool_call", "toolCallId": tc.id, "toolName": tc.function.name, "input": args_val,
                    }))))?;

                    // update_todo: send the todo list as a separate notification
                    // for the frontend to render inline.
                    if tc.function.name == "TodoWrite" {
                        if let Some(todos) = args_val.get("todos") {
                            write_notification(&Notification::new("chat.stream", Some(json!({
                                "sessionId": ctx.session_id, "runId": ctx.run_id,
                                "kind": "todo", "todos": todos,
                            }))))?;
                        }
                    }

                    let tool_result = if cancel.load(Ordering::SeqCst) {
                        "[interrupted by user]".to_string()
                    } else {
                        match tool_registry.get(&tc.function.name) {
                            Some(executor) => match executor.execute(args_val.clone(), cancel) {
                                Ok(output) => output,
                                Err(e) => format!("Tool error: {e}"),
                            },
                            None => format!("Unknown tool: {}", tc.function.name),
                        }
                    };

                    debug!("Tool result for {}: {:.200}", tc.function.name, tool_result);
                    write_notification(&Notification::new("chat.stream", Some(json!({
                        "sessionId": ctx.session_id, "runId": ctx.run_id,
                        "kind": "tool_result", "toolCallId": tc.id, "toolName": tc.function.name, "output": tool_result,
                    }))))?;
                    store::add_tool_result(store, &ctx.session_id, &tc.id, &tool_result)?;

                    TurnState::Executing { response, cursor: cursor + 1 }
                }
            }
            TurnState::Interrupted { text, thinking } => break Ok((text, thinking)),
            TurnState::Finalizing { text, thinking } => break Ok((text, thinking)),
        };
    }
}

// ── One LLM call ───────────────────────────────────────────────────

fn llm_step(
    adapter: &dyn ApiAdapter,
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    thinking_enabled: bool,
    messages: &[store::Message],
    ctx: &TurnContext,
    cancel: &AtomicBool,
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
        model: model.to_string(),
        messages: llm_msgs,
        tools: ctx.tools.clone(),
        thinking_enabled,
    }, api_key, base_url)?;

    debug!("LLM call: url={} model={}", http.url, model);
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
            if let Ok(event) = serde_json::from_str::<Value>(data) {
                for se in adapter.stream_events(&event) {
                    write_notification(&Notification::new("chat.stream", Some(json!({
                        "sessionId": ctx.session_id, "runId": ctx.run_id,
                        "kind": se.kind, "delta": se.delta,
                    }))))?;
                }
            }
        }
        line.clear();
        if cancel.load(Ordering::SeqCst) { debug!("llm_step: SSE loop cancelled"); break; }
    }

    let body_str = String::from_utf8_lossy(&body_bytes);
    debug!("LLM response body: {} bytes", body_bytes.len());
    trace!("LLM response body: {}", body_str);

    adapter.parse_stream(&body_str)
}

#[cfg(test)]
#[path = "tests/chat_tests.rs"]
mod tests;
