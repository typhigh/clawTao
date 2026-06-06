//! ClawTao Rust Backend - stdio JSON-RPC server
//!
//! Communicates with Electron via JSON-RPC 2.0 over stdin/stdout

mod jsonrpc;
mod session;
mod tools;

use anyhow::Result;
use jsonrpc::{Notification, Request, Response};
use reqwest::blocking::Client;
use serde_json::json;
use session::{SessionManager, ToolCall};
use std::env;
use std::io::{self, BufRead, Write};
use tools::registry::ToolRegistry;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

const MAX_TOOL_ROUNDS: usize = 10;

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env().add_directive("debug".parse().unwrap()))
        .init();

    info!("ClawTao backend starting...");

    // Initialize session manager
    let storage_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("clawtao")
        .join("sessions");
    let mut session_manager = SessionManager::new(storage_path);

    if session_manager.list_sessions().is_empty() {
        session_manager.create_session();
    }

    // Initialize ToolRegistry
    let mut tool_registry = ToolRegistry::new();
    tools::builtin::register_all(&mut tool_registry);
    info!("Registered {} tools", tool_registry.len());

    // Config
    let api_key = env::var("OPENAI_API_KEY").ok();
    if api_key.is_none() {
        error!("OPENAI_API_KEY not set");
    }

    let base_url = env::var("OPENAI_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
    info!("OpenAI base URL: {}", base_url);

    let model = env::var("OPENAI_MODEL")
        .unwrap_or_else(|_| "gpt-4o".to_string());
    info!("OpenAI model: {}", model);

    let client = Client::new();

    // Main loop
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    loop {
        let mut line = String::new();
        match handle.read_line(&mut line) {
            Ok(0) => { info!("Stdin closed, exiting"); break; }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }

                match serde_json::from_str::<Request>(trimmed) {
                    Ok(request) => {
                        if let Err(e) = handle_request(
                            &request,
                            &mut session_manager,
                            &tool_registry,
                            api_key.as_deref(),
                            &base_url,
                            &model,
                            &client,
                        ) {
                            error!("Error handling request: {}", e);
                            let response = Response::error(request.id, -32603, format!("Internal error: {e}"));
                            let _ = write_response(&response);
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse request: {e}");
                        if serde_json::from_str::<Notification>(trimmed).is_ok() { continue; }
                        let _ = write_response(&Response::error(None, -32700, "Parse error"));
                    }
                }
            }
            Err(e) => { error!("Failed to read from stdin: {e}"); break; }
        }
    }
}

/// Parsed result from LLM SSE response.
struct SseResult {
    text: String,
    tool_calls: Vec<ToolCall>,
}

/// Parse OpenAI chat completions SSE response body.
/// Returns accumulated text and finalized tool calls.
fn parse_sse_response(body_str: &str) -> SseResult {
    let mut text = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut pending_tools: Vec<(String, String, String)> = Vec::new(); // (id, name, args_json)

    for line in body_str.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" { continue; }
            let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else { continue; };
            let delta = event.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta"));

            // Text content
            if let Some(content) = delta.and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                text.push_str(content);
            }

            // Tool calls (may be split across chunks; keyed by `index`)
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

    // Finalize
    for (id, name, args) in pending_tools {
        if id.is_empty() || name.is_empty() { continue; }
        if serde_json::from_str::<serde_json::Value>(&args).is_ok() {
            tool_calls.push(ToolCall {
                id,
                call_type: "function".to_string(),
                function: session::ToolCallFunction { name, arguments: args },
            });
        }
    }

    SseResult { text, tool_calls }
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod sse_tests;

fn handle_request(
    request: &Request,
    session_manager: &mut SessionManager,
    tool_registry: &ToolRegistry,
    api_key: Option<&str>,
    base_url: &str,
    model: &str,
    client: &Client,
) -> Result<()> {
    info!("Handling request: {}", request.method);

    match request.method.as_str() {
        "session.list" => {
            let result = serde_json::to_value(session_manager.list_sessions())?;
            write_response(&Response::success(request.id.clone(), result))?;
        }
        "session.create" => {
            let result = serde_json::to_value(session_manager.create_session())?;
            write_response(&Response::success(request.id.clone(), result))?;
        }
        "session.get" => {
            let session_id = get_param(&request.params, "sessionId")?;
            let session = session_manager.get_session(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
            write_response(&Response::success(request.id.clone(), serde_json::to_value(&session)?))?;
        }

        "chat.send" => handle_chat_send(request, session_manager, tool_registry, api_key, base_url, model, client)?,

        "ping" => {
            write_response(&Response::success(request.id.clone(), json!({"status":"ok"})))?;
        }

        _ => {
            write_response(&Response::error(request.id.clone(), -32601, format!("Method not found: {}", request.method)))?;
        }
    }

    Ok(())
}

fn handle_chat_send(
    request: &Request,
    session_manager: &mut SessionManager,
    tool_registry: &ToolRegistry,
    api_key: Option<&str>,
    base_url: &str,
    model: &str,
    client: &Client,
) -> Result<()> {
    let message_text = get_param(&request.params, "message")?;
    let session_id = get_param(&request.params, "sessionId")?;

    session_manager.add_message(session_id, "user", message_text);

    let session = session_manager.get_session(session_id)
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
    let run_id = uuid::Uuid::new_v4().to_string();

    // Send start notification
    write_notification(&Notification::new("chat.started", Some(json!({
        "sessionId": session_id, "runId": run_id
    }))))?;

    let key = api_key.ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
    let api_url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    // Multi-turn tool loop
    let mut messages = session.messages.clone();
    let mut final_content = String::new();

    for round in 0..MAX_TOOL_ROUNDS {
        // Build API messages
        let mut api_messages: Vec<serde_json::Value> = messages.iter().map(|m| m.to_llm_message()).collect();

        // Inject system message at start
        api_messages.insert(0, json!({
            "role": "system",
            "content": "You are ClawTao, a helpful AI assistant with tool calling capabilities."
        }));

        // Build request body with tools
        let tools_specs: Vec<serde_json::Value> = tool_registry.list_specs()
            .iter()
            .filter_map(|s| serde_json::to_value(s).ok())
            .collect();

        let body = json!({
            "model": model,
            "messages": api_messages,
            "stream": true,
            "tools": tools_specs,
        });

        debug!("LLM round {round}: url={api_url} model={model} msgs={} tools={}", api_messages.len(), tools_specs.len());
        debug!("LLM request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());

        let mut resp = client.post(&api_url)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&body)?)
            .send()?;

        debug!("LLM response: status={}", resp.status());

        // Read response body
        use std::io::Read;
        let mut body_bytes = Vec::new();
        resp.read_to_end(&mut body_bytes)?;
        let body_str = String::from_utf8_lossy(&body_bytes);

        debug!("LLM response body ({} bytes): {}", body_bytes.len(), body_str);

        let result = parse_sse_response(&body_str);

        // Forward text deltas to UI
        // Note: since we use blocking read_to_end(), text isn't truly streamed.
        // We send the full accumulated text as one delta for now.
        if !result.text.is_empty() {
            write_notification(&Notification::new("chat.text_delta", Some(json!({
                "sessionId": session_id, "runId": run_id, "delta": result.text
            }))))?;
        }

        let round_text = result.text;
        let round_tool_calls = result.tool_calls;

        if round_tool_calls.is_empty() {
            // No tool calls, this is the final response
            final_content = round_text;
            break;
        }

        // Execute tool calls
        debug!("Round {round}: executing {} tool calls", round_tool_calls.len());

        // Save assistant tool_calls to session
        let tc_clone = round_tool_calls.clone();
        session_manager.add_assistant_tool_calls(session_id, tc_clone);

        // Execute each tool
        for tc in &round_tool_calls {
            debug!("Executing tool: {} id={} args={}", tc.function.name, tc.id, tc.function.arguments);

            let args_val: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

            // Notify UI about tool call
            write_notification(&Notification::new("chat.tool_started", Some(json!({
                "sessionId": session_id, "runId": run_id,
                "toolCallId": tc.id, "toolName": tc.function.name,
                "toolInput": args_val,
            }))))?;

            let result = match tool_registry.get(&tc.function.name) {
                Some(executor) => match executor.execute(args_val.clone()) {
                    Ok(output) => output,
                    Err(e) => format!("Tool error: {e}"),
                },
                None => format!("Unknown tool: {}", tc.function.name),
            };

            debug!("Tool result for {}: {:.200}", tc.function.name, result);

            // Notify UI about tool result
            write_notification(&Notification::new("chat.tool_result", Some(json!({
                "sessionId": session_id, "runId": run_id,
                "toolCallId": tc.id, "toolName": tc.function.name,
                "result": result,
            }))))?;

            // Add tool result to session
            session_manager.add_tool_result(session_id, &tc.id, &result);
        }

        // Reload messages after tool execution
        messages = session_manager.get_session(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found after tool execution"))?
            .messages
            .clone();

        // Continue loop - send back to LLM with tool results
    }

    if !final_content.is_empty() {
        session_manager.add_message(session_id, "assistant", &final_content);
    } else {
        session_manager.add_message(session_id, "assistant", "(no response)");
    }

    // Send done notification
    write_notification(&Notification::new("chat.done", Some(json!({
        "sessionId": session_id, "runId": run_id
    }))))?;

    let result = json!({
        "runId": run_id,
        "message": {
            "id": uuid::Uuid::new_v4().to_string(),
            "role": "assistant",
            "content": final_content,
            "timestamp": chrono::Utc::now().timestamp_millis()
        }
    });
    write_response(&Response::success(request.id.clone(), result))?;

    Ok(())
}

fn get_param<'a>(params: &'a Option<serde_json::Value>, key: &str) -> Result<&'a str> {
    params.as_ref()
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing parameter: {key}"))
}

fn write_response(response: &Response) -> io::Result<()> {
    let json = serde_json::to_string(response).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{json}");
    io::stdout().flush()
}

fn write_notification(notification: &Notification) -> io::Result<()> {
    let json = serde_json::to_string(notification).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{json}");
    io::stdout().flush()
}
