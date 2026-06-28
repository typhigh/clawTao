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
//! Short-lived RPCs (ping, session.*) are handled directly on the main thread.

mod chat;
mod handlers;
mod jsonrpc;
mod llm;
mod session_actor;
mod store;
mod sse;
mod tools;
mod system_prompt;

use jsonrpc::{Request, Response};
use session_actor::{actor_loop, process_run_wrapper, SessionMsg, SessionRegistry};
use store::json_store::JsonSessionStore;
use store::sqlite_store::SqliteSessionStore;
use std::io::{self, BufRead};
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        eprintln!("=== CLAWTAO PANIC ===\n{info}\n{backtrace}");
    }));

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .init();

    info!("ClawTao backend starting");

    let storage_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("clawtao");

    let store: Arc<dyn store::store_trait::SessionStore> = match std::env::var("SESSION_STORE").as_deref() {
        Ok("json") => Arc::new(JsonSessionStore::new(storage_path.join("sessions"))),
        _ => Arc::new(SqliteSessionStore::new(storage_path.join("sessions.db"))
            .expect("Failed to open SQLite store")),
    };

    if store.list().unwrap_or_default().is_empty() {
        store.create(&store::Session {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            updated_at: chrono::Utc::now().timestamp_millis(),
            messages: vec![],
            title: String::new(),
        }).ok();
    }

    let registry = SessionRegistry::new(Arc::clone(&store));

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
                            handle_chat_send(&request, &registry);
                        } else if request.method == "session.delete" {
                            let sid = jsonrpc::get_param_opt(&request.params, "sessionId");
                            if let Some(id) = sid { registry.remove(id); }
                            if let Err(e) = route(&request, &store) {
                                error!("Error handling request: {:#}", e);
                                let _ = jsonrpc::write_response(&Response::error(request.id, -32603, format!("Internal error: {:#}", e)));
                            }
                        } else if let Err(e) = route(&request, &store) {
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

fn handle_chat_send(request: &Request, registry: &SessionRegistry) {
    let session_id = match request.params.as_ref().and_then(|p| p.get("sessionId")).and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            let _ = jsonrpc::write_response(&Response::error(request.id.clone(), -32602, "Missing sessionId"));
            return;
        }
    };

    let store = Arc::clone(&registry.store);
    let params = request.params.clone().unwrap_or_default();
    let response_id = request.id.clone();
    let sid = session_id.clone();

    let tx = registry.get_or_spawn(&session_id, move |rx| {
        let store = Arc::clone(&store);
        let sid = sid.clone();
        std::thread::spawn(move || {
            actor_loop(rx, &sid, store, process_run_wrapper);
        })
    });

    if tx.send(SessionMsg::Run { params, response_id }).is_err() {
        error!("Failed to send to session actor {session_id} (channel closed)");
        let _ = jsonrpc::write_response(&Response::error(request.id.clone(), -32603, "Session actor has stopped"));
    }
}

use handlers::{not_found, ping, session_create, session_delete, session_get, session_list};

fn route(
    request: &Request,
    store: &Arc<dyn store::store_trait::SessionStore>,
) -> anyhow::Result<()> {
    match request.method.as_str() {
        "session.list" => session_list(request, &**store),
        "session.create" => session_create(request, &**store),
        "session.get" => session_get(request, &**store),
        "session.delete" => session_delete(request, &**store),
        "ping" => ping(request),
        _ => not_found(request),
    }
}
