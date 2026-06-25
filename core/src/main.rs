//! ClawTao Rust Backend — JSON-RPC 2.0 server over stdio.
//!
//! ## Architecture
//!
//! This binary is spawned by Electron as a child process. All communication
//! goes through stdin (requests) and stdout (responses + notifications),
//! one JSON-RPC 2.0 message per line. Stderr is reserved for tracing logs.
//!
//! ## Concurrency model
//!
//! Each active session runs in its own thread (actor). Chat requests are
//! dispatched to the session's actor via an mpsc channel, so within a
//! session processing is serialised but different sessions run in parallel.
//! Short-lived RPCs (ping, session.*, config.*) are handled directly on the
//! main thread.

mod chat;
mod config;
mod handlers;
mod jsonrpc;
mod llm;
mod session_actor;
mod store;
mod sse;
mod tools;
mod system_prompt;

use config::LlmConfig;
use jsonrpc::{Request, Response};
use reqwest::blocking::Client;
use session_actor::{SessionMsg, SessionRegistry};
use store::json_store::JsonSessionStore;
use store::sqlite_store::SqliteSessionStore;
use std::io::{self, BufRead};
use std::sync::Arc;
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

    // Store is shared across all session actors + the main thread.
    let store: Arc<dyn store::store_trait::SessionStore> = match std::env::var("SESSION_STORE").as_deref() {
        Ok("json") => Arc::new(JsonSessionStore::new(storage_path.join("sessions"))),
        _ => Arc::new(SqliteSessionStore::new(storage_path.join("sessions.db"))
            .expect("Failed to open SQLite store")),
    };

    // Ensure at least one session exists.
    if store.list().unwrap_or_default().is_empty() {
        store.create(&store::Session {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            updated_at: chrono::Utc::now().timestamp_millis(),
            messages: vec![],
            title: String::new(),
        }).ok();
    }

    let tool_registry = {
        let mut tr = ToolRegistry::new();
        tools::builtin::register_all(&mut tr, llm_config.bash_blocked_commands.clone(), llm_config.bash_timeout_secs);
        tr
    };
    info!("Registered {} tools: {:?}", tool_registry.len(), tool_registry.names());

    let tool_registry = Arc::new(tool_registry);
    let registry = SessionRegistry::new(Arc::clone(&store));

    info!("LLM config: provider={} base_url={} model={}", llm_config.provider, llm_config.base_url, llm_config.model);

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
                        if request.method == "chat.send" {
                            handle_chat_send(&request, &registry, &tool_registry, &llm_config);
                        } else if request.method == "session.delete" {
                            // Clean up actor before deleting the session.
                            let sid = jsonrpc::get_param_opt(&request.params, "sessionId");
                            if let Some(id) = sid { registry.remove(id); }
                            if let Err(e) = route(&request, &store, &mut llm_config) {
                                error!("Error handling request: {:#}", e);
                                let _ = jsonrpc::write_response(&Response::error(request.id, -32603, format!("Internal error: {:#}", e)));
                            }
                        } else if let Err(e) = route(&request, &store, &mut llm_config) {
                            error!("Error handling request: {:#}", e);
                            let _ = jsonrpc::write_response(&Response::error(request.id, -32603, format!("Internal error: {:#}", e)));
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

/// Spawn or dispatch a chat.send to the appropriate session actor.
fn handle_chat_send(
    request: &Request,
    registry: &SessionRegistry,
    tool_registry: &Arc<ToolRegistry>,
    llm_config: &LlmConfig,
) {
    let session_id = match request.params.as_ref().and_then(|p| p.get("sessionId")).and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            let _ = jsonrpc::write_response(&Response::error(request.id.clone(), -32602, "Missing sessionId"));
            return;
        }
    };

    // Clone before the closure so it can be 'static.
    let store = Arc::clone(&registry.store);
    let tools = Arc::clone(tool_registry);
    let cfg = llm_config.clone();

    let sid = session_id.clone();
    let tx = registry.get_or_spawn(&session_id, move |rx| {
        let store = Arc::clone(&store);
        let tools = Arc::clone(&tools);
        let cfg = cfg.clone();
        let sid = sid.clone();
        std::thread::spawn(move || {
            actor_loop(rx, &sid, store, tools, cfg);
        })
    });

    if tx.send(SessionMsg::Run {
        params: request.params.clone().unwrap_or_default(),
        response_id: request.id.clone(),
    }).is_err() {
        error!("Failed to send to session actor {session_id} (channel closed)");
        let _ = jsonrpc::write_response(&Response::error(request.id.clone(), -32603, "Session actor has stopped"));
    }
}

/// Session actor main loop — processes one message at a time.
fn actor_loop(
    rx: std::sync::mpsc::Receiver<SessionMsg>,
    session_id: &str,
    store: Arc<dyn store::store_trait::SessionStore>,
    tool_registry: Arc<ToolRegistry>,
    llm_config: LlmConfig,
) {
    let session_manager = store::SessionManager::new_shared(store);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("Failed to build HTTP client");

    for msg in rx {
        match msg {
            SessionMsg::Run { params, response_id } => {
                let request = Request {
                    jsonrpc: "2.0".into(),
                    id: response_id,
                    method: "chat.send".into(),
                    params: Some(params),
                };
                if let Err(e) = chat::handle_chat_send(
                    &request,
                    &session_manager,
                    &tool_registry,
                    &llm_config,
                    &client,
                ) {
                    error!("chat error for session {session_id}: {e:#}");
                    let _ = jsonrpc::write_response(&Response::error(
                        request.id,
                        -32603,
                        format!("Internal error: {e:#}"),
                    ));
                }
            }
            SessionMsg::Shutdown => {
                info!("Actor for session {session_id} shutting down");
                break;
            }
        }
    }
}

use handlers::{
    config_get, config_inject_key, config_set, config_test_key, config_validate,
    not_found, ping, session_create, session_delete, session_get, session_list,
};

/// Route fast (non-chat) JSON-RPC requests. chat.send is handled directly
/// in the main loop via the session registry.
fn route(
    request: &Request,
    store: &Arc<dyn store::store_trait::SessionStore>,
    llm_config: &mut LlmConfig,
) -> anyhow::Result<()> {
    let session_manager = store::SessionManager::new_shared(Arc::clone(store));
    match request.method.as_str() {
        "session.list" => session_list(request, &session_manager),
        "session.create" => session_create(request, &session_manager),
        "session.get" => session_get(request, &session_manager),
        "session.delete" => session_delete(request, &session_manager),

        "config.get" => config_get(request, llm_config),
        "config.set" => config_set(request, llm_config),
        "config.injectKey" => config_inject_key(request, llm_config),
        "config.validate" => config_validate(request, llm_config),
        "config.testKey" => config_test_key(request, llm_config),

        "ping" => ping(request),

        _ => not_found(request),
    }
}
