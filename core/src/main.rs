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
mod handlers;
mod jsonrpc;
mod llm;
mod store;
mod sse;
mod tools;
mod system_prompt;

use config::LlmConfig;
use jsonrpc::{Request, Response};
use reqwest::blocking::Client;
use store::{json_store::JsonSessionStore, sqlite_store::SqliteSessionStore, SessionManager};
use std::io::{self, BufRead};
use tools::registry::ToolRegistry;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn main() {
    // Capture panic messages + backtrace to stderr so they are visible
    // in the Electron main-process console and crash logs.
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        eprintln!("=== CLAWTAO PANIC ===\n{info}\n{backtrace}");
    }));

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
    tools::builtin::register_all(&mut tool_registry, llm_config.bash_blocked_commands.clone(), llm_config.bash_timeout_secs);
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
                        if let Err(e) = route(
                            &request,
                            &mut session_manager,
                            &tool_registry,
                            &mut llm_config,
                            &client,
                        ) {
                            error!("Error handling request: {:#}", e);
                            let _ = jsonrpc::write_response(&Response::error(
                                request.id,
                                -32603,
                                format!("Internal error: {:#}", e),
                            ));
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse request: {e}");
                        if serde_json::from_str::<jsonrpc::Notification>(trimmed).is_ok() {
                            continue;
                        }
                        let _ = jsonrpc::write_response(&Response::error(None, -32700, "Parse error"));
                    }
                }
            }
            Err(e) => { error!("Failed to read from stdin: {e}"); break; }
        }
    }
}

use handlers::{
    chat_send, config_get, config_inject_key, config_set, config_test_key, config_validate,
    not_found, ping, session_create, session_delete, session_get, session_list,
};

/// Route a parsed JSON-RPC request to the appropriate handler.
///
/// Each handler function declares exactly the state it needs in its
/// signature — see [`handlers`] for the full list of supported methods.
fn route(
    request: &Request,
    session_manager: &mut SessionManager,
    tool_registry: &ToolRegistry,
    llm_config: &mut LlmConfig,
    client: &Client,
) -> anyhow::Result<()> {
    match request.method.as_str() {
        "session.list" => session_list(request, session_manager),
        "session.create" => session_create(request, session_manager),
        "session.get" => session_get(request, session_manager),
        "session.delete" => session_delete(request, session_manager),

        "config.get" => config_get(request, llm_config),
        "config.set" => config_set(request, llm_config),
        "config.injectKey" => config_inject_key(request, llm_config),
        "config.validate" => config_validate(request, llm_config),
        "config.testKey" => config_test_key(request, llm_config),

        "chat.send" => chat_send(request, session_manager, tool_registry, llm_config, client),

        "ping" => ping(request),

        _ => not_found(request),
    }
}
