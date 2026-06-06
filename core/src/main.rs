//! ClawTao Rust Backend - stdio JSON-RPC server
//!
//! Communicates with Electron via JSON-RPC 2.0 over stdin/stdout

mod jsonrpc;
mod session;

use anyhow::Result;
use jsonrpc::{Notification, Request, Response};
use reqwest::blocking::Client;
use serde_json::json;
use session::SessionManager;
use std::env;
use std::io::{self, BufRead, Write};
use tracing::{error, info, debug};
use tracing_subscriber::EnvFilter;

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

    // Ensure we have a default session
    if session_manager.list_sessions().is_empty() {
        session_manager.create_session();
    }

    // Initialize HTTP client
    let api_key = match env::var("OPENAI_API_KEY") {
        Ok(key) => {
            info!("OPENAI_API_KEY found");
            Some(key)
        }
        Err(_) => {
            error!("OPENAI_API_KEY not set");
            None
        }
    };

    let base_url = env::var("OPENAI_BASE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
    info!("OpenAI base URL: {}", base_url);

    let model = env::var("OPENAI_MODEL")
        .unwrap_or_else(|_| "gpt-4o".to_string());
    info!("OpenAI model: {}", model);

    let client = Client::new();

    // Main loop: read JSON-RPC requests from stdin
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    loop {
        let mut line = String::new();
        match handle.read_line(&mut line) {
            Ok(0) => {
                info!("Stdin closed, exiting");
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                match serde_json::from_str::<Request>(trimmed) {
                    Ok(request) => {
                        if let Err(e) = handle_request(
                            &request,
                            &mut session_manager,
                            api_key.as_deref(),
                            &base_url,
                            &model,
                            &client,
                        ) {
                            error!("Error handling request: {}", e);
                            let response = Response::error(
                                request.id,
                                -32603,
                                format!("Internal error: {}", e),
                            );
                            let _ = write_response(&response);
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse request: {}", e);
                        if let Ok(_notification) = serde_json::from_str::<Notification>(trimmed) {
                            continue;
                        }
                        let response = Response::error(None, -32700, "Parse error");
                        let _ = write_response(&response);
                    }
                }
            }
            Err(e) => {
                error!("Failed to read from stdin: {}", e);
                break;
            }
        }
    }
}

fn handle_request(
    request: &Request,
    session_manager: &mut SessionManager,
    api_key: Option<&str>,
    base_url: &str,
    model: &str,
    client: &Client,
) -> Result<()> {
    info!("Handling request: {}", request.method);

    match request.method.as_str() {
        "session.list" => {
            let sessions = session_manager.list_sessions();
            let result = serde_json::to_value(sessions)?;
            let response = Response::success(request.id.clone(), result);
            write_response(&response)?;
        }

        "session.create" => {
            let session = session_manager.create_session();
            let result = serde_json::to_value(&session)?;
            let response = Response::success(request.id.clone(), result);
            write_response(&response)?;
        }

        "session.get" => {
            let session_id = request
                .params
                .as_ref()
                .and_then(|p| p.get("sessionId"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing sessionId"))?;

            let session = session_manager
                .get_session(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
            let result = serde_json::to_value(&session)?;
            let response = Response::success(request.id.clone(), result);
            write_response(&response)?;
        }

        "chat.send" => {
            let message_text = request
                .params
                .as_ref()
                .and_then(|p| p.get("message"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

            let session_id = request
                .params
                .as_ref()
                .and_then(|p| p.get("sessionId"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing sessionId"))?;

            // Add user message to session
            session_manager.add_message(session_id, "user", message_text);

            // Get all messages for context
            let session = session_manager
                .get_session(session_id)
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

            let run_id = uuid::Uuid::new_v4().to_string();

            // Send start notification
            let start_notification = Notification::new(
                "chat.started",
                Some(json!({
                    "sessionId": session_id,
                    "runId": run_id
                })),
            );
            write_notification(&start_notification)?;

            let assistant_content = if let Some(key) = api_key {
                let mut full_response = String::new();

                let messages: Vec<_> = session
                    .messages
                    .iter()
                    .map(|m| json!({ "role": m.role, "content": m.content }))
                    .collect();

                let api_url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
                let body = json!({
                    "model": model,
                    "messages": messages,
                    "stream": true,
                });

                debug!(
                    "LLM request url=\"{api_url}\" model=\"{model}\" messages.count={}",
                    messages.len()
                );
                debug!("LLM request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());
                debug!(
                    "LLM auth: key.present={} key.len={}",
                    true,
                    key.len()
                );

                let mut resp = client
                    .post(&api_url)
                    .header("Authorization", format!("Bearer {}", key))
                    .header("Content-Type", "application/json")
                    .body(serde_json::to_string(&body)?)
                    .send()?;

                debug!(
                    "LLM response: status={} content-type={:?}",
                    resp.status(),
                    resp.headers().get("Content-Type").map(|v| v.to_str().unwrap_or("?"))
                );

                use std::io::Read;
                let mut body_bytes = Vec::new();
                resp.read_to_end(&mut body_bytes)?;
                let body_str = String::from_utf8_lossy(&body_bytes);

                debug!(
                    "LLM response body: {} bytes",
                    body_bytes.len()
                );

                // Parse SSE response
                let mut text_chunks = 0u32;
                for line in body_str.lines() {
                    match line.strip_prefix("data: ") {
                        None => {
                            // Skip empty or non-data lines
                            if !line.trim().is_empty() {
                                debug!("LLM SSE non-data line: \"{}\"", line);
                            }
                        }
                        Some(data) if data == "[DONE]" => {
                            debug!("LLM SSE stream end: [DONE]");
                        }
                        Some(data) => {
                            debug!("LLM SSE data line: {}", data);
                            match serde_json::from_str::<serde_json::Value>(data) {
                                Ok(event) => {
                                    if let Some(content) = event
                                        .get("choices")
                                        .and_then(|c| c.get(0))
                                        .and_then(|c| c.get("delta"))
                                        .and_then(|d| d.get("content"))
                                        .and_then(|c| c.as_str())
                                    {
                                        full_response.push_str(content);
                                        text_chunks += 1;

                                        let text_notification = Notification::new(
                                            "chat.text_delta",
                                            Some(json!({
                                                "sessionId": session_id,
                                                "runId": run_id,
                                                "delta": content
                                            })),
                                        );
                                        write_notification(&text_notification).ok();
                                    } else {
                                        // Log non-text delta events (finish_reason etc.)
                                        debug!("LLM SSE non-text event: {}", data);
                                    }
                                }
                                Err(e) => {
                                    debug!("LLM SSE parse error: \"{data}\" error={e:?}");
                                }
                            }
                        }
                    }
                }

                debug!(
                    "LLM done: text_chunks={} response_chars={}",
                    text_chunks,
                    full_response.len()
                );

                full_response
            } else {
                error!("LLM: OPENAI_API_KEY not set");
                "LLM client not initialized. Please set OPENAI_API_KEY environment variable.".to_string()
            };

            // Add assistant message to session
            session_manager.add_message(session_id, "assistant", &assistant_content);

            // Send done notification
            let done_notification = Notification::new(
                "chat.done",
                Some(json!({
                    "sessionId": session_id,
                    "runId": run_id
                })),
            );
            write_notification(&done_notification)?;

            // Send success response
            let result = json!({
                "runId": run_id,
                "message": {
                    "id": uuid::Uuid::new_v4().to_string(),
                    "role": "assistant",
                    "content": assistant_content,
                    "timestamp": chrono::Utc::now().timestamp_millis()
                }
            });
            let response = Response::success(request.id.clone(), result);
            write_response(&response)?;
        }

        "ping" => {
            let response = Response::success(request.id.clone(), json!({ "status": "ok" }));
            write_response(&response)?;
        }

        _ => {
            let response = Response::error(
                request.id.clone(),
                -32601,
                format!("Method not found: {}", request.method),
            );
            write_response(&response)?;
        }
    }

    Ok(())
}

fn write_response(response: &Response) -> io::Result<()> {
    let json =
        serde_json::to_string(response).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{}", json);
    io::stdout().flush()
}

fn write_notification(notification: &Notification) -> io::Result<()> {
    let json =
        serde_json::to_string(notification).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{}", json);
    io::stdout().flush()
}
