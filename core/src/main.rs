//! ClawTao Rust Backend — JSON-RPC 2.0 server over stdio.
//!
//! ## Architecture
//!
//! This binary is spawned by Electron as a child process. All communication
//! goes through stdin (requests) and stdout (responses + notifications),
//! one JSON-RPC 2.0 message per line. Stderr is reserved for tracing logs.
//!
//! ## Thread safety
//!
//! The current implementation uses a blocking reqwest client and processes
//! one request at a time (single-threaded event loop). This is deliberate:
//! Session state is mutable and not yet behind a lock.

mod chat;
mod config;
mod jsonrpc;
mod llm;
mod store;
mod sse;
mod tools;

use anyhow::Result;
use chat::handle_chat_send;
use config::LlmConfig;
use jsonrpc::{Notification, Request, Response};
use reqwest::blocking::Client;
use serde_json::json;
use store::{SessionManager, json_store::JsonSessionStore, sqlite_store::SqliteSessionStore};
use std::io::{self, BufRead, Write};
use tools::registry::ToolRegistry;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

fn main() {
    // Load config first to get log_level, then init tracing
    let mut llm_config = LlmConfig::load();
    let log_level = llm_config.effective_log_level();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(&log_level))
        )
        .init();

    if std::env::var("RUST_LOG").is_ok() {
        info!("ClawTao backend starting (log_level={log_level}, source=RUST_LOG env)");
    } else {
        info!("ClawTao backend starting (log_level={log_level}, source=config.json)");
    }

    let storage_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("clawtao");

    // Default to SQLite, "json" env var for JSONL fallback
    let store: Box<dyn crate::store::store_trait::SessionStore> = match std::env::var("SESSION_STORE").as_deref() {
        Ok("json") => Box::new(JsonSessionStore::new(storage_path.join("sessions"))),
        _ => Box::new(SqliteSessionStore::new(storage_path.join("sessions.db"))
            .expect("Failed to open SQLite store")),
    };
    let mut session_manager: SessionManager = SessionManager::new(store);

    if session_manager.list_sessions().unwrap_or_default().is_empty() {
        session_manager.create_session().ok();
    }

    let mut tool_registry = ToolRegistry::new();
    tools::builtin::register_all(&mut tool_registry, llm_config.bash_blocked_commands.clone());
    info!("Registered {} tools: {:?}", tool_registry.len(), tool_registry.names());

    info!("LLM config: provider={} base_url={} model={}", llm_config.provider, llm_config.base_url, llm_config.model);
    let client = Client::new();

    // Main event loop
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
                            &mut llm_config,
                            &client,
                        ) {
                            error!("Error handling request: {}", e);
                            let _ = write_response(&Response::error(request.id, -32603, format!("Internal error: {e}")));
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

fn handle_request(
    request: &Request,
    session_manager: &mut SessionManager,
    tool_registry: &ToolRegistry,
    llm_config: &mut LlmConfig,
    client: &Client,
) -> Result<()> {
    debug!("{}", request.method);

    match request.method.as_str() {
        "session.list" => {
            let result = serde_json::to_value(session_manager.list_sessions().unwrap_or_default())?;
            write_response(&Response::success(request.id.clone(), result))?;
        }
        "session.create" => {
            let result = serde_json::to_value(session_manager.create_session()?)?;
            write_response(&Response::success(request.id.clone(), result))?;
        }
        "session.get" => {
            let session_id = get_param(&request.params, "sessionId")?;
            let session = session_manager.get_session(session_id)?
                .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
            write_response(&Response::success(request.id.clone(), serde_json::to_value(&session)?))?;
        }
        "session.delete" => {
            let session_id = get_param(&request.params, "sessionId")?;
            session_manager.delete_session(session_id)?;
            write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
        }

        "config.get" => {
            write_response(&Response::success(request.id.clone(), serde_json::to_value(llm_config.masked())?))?;
        }
        "config.set" => {
            let new_config: LlmConfig = serde_json::from_value(request.params.clone().unwrap_or_default())
                .map_err(|e| anyhow::anyhow!("Invalid config: {e}"))?;
            new_config.save()?;
            *llm_config = new_config;
            info!("Config updated: provider={} model={}", llm_config.provider, llm_config.model);
            write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
        }
        "config.injectKey" => {
            let api_key = get_param(&request.params, "api_key")?;
            llm_config.api_key = api_key.to_string();
            info!("API key injected (length={})", llm_config.api_key.len());
            write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
        }
        "config.validate" => {
            match llm_config.validate() {
                Ok(()) => write_response(&Response::success(request.id.clone(), json!({"ok": true})))?,
                Err(e) => write_response(&Response::success(request.id.clone(), json!({"ok": false, "error": e})))?,
            }
        }
        "config.testKey" => {
            let api_key = get_param(&request.params, "api_key")?;
            let base_url = get_param(&request.params, "base_url").unwrap_or(&llm_config.base_url);
            let model = get_param(&request.params, "model").unwrap_or(&llm_config.model);
            match LlmConfig::test_connection(base_url, model, api_key) {
                Ok(()) => write_response(&Response::success(request.id.clone(), json!({"ok": true})))?,
                Err(e) => write_response(&Response::success(request.id.clone(), json!({"ok": false, "error": e})))?,
            }
        }

        "chat.send" => handle_chat_send(request, session_manager, tool_registry, llm_config, client)?,

        "ping" => {
            write_response(&Response::success(request.id.clone(), json!({"status":"ok"})))?;
        }

        _ => {
            write_response(&Response::error(request.id.clone(), -32601, format!("Method not found: {}", request.method)))?;
        }
    }

    Ok(())
}

pub(crate) fn get_param<'a>(params: &'a Option<serde_json::Value>, key: &str) -> Result<&'a str> {
    params.as_ref()
        .and_then(|p| p.get(key))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing parameter: {key}"))
}

pub(crate) fn write_response(response: &Response) -> io::Result<()> {
    let json = serde_json::to_string(response).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{json}");
    io::stdout().flush()
}

pub(crate) fn write_notification(notification: &Notification) -> io::Result<()> {
    let json = serde_json::to_string(notification).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    println!("{json}");
    io::stdout().flush()
}
