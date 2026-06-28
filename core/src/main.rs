//! ClawTao Rust Backend — JSON-RPC 2.0 server over stdio.
//!
//! Spawned by Electron as a child process. Communication via stdin/stdout,
//! one JSON-RPC 2.0 message per line. Stderr is reserved for tracing logs.
//!
//! Each active session runs in its own thread (actor). Chat requests are
//! dispatched via `SessionRegistry`; short-lived RPCs (ping, session.*)
//! are handled directly on the main thread.

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
use session_actor::SessionRegistry;
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
                    Ok(request) => dispatch(&request, &registry, &store),
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

fn dispatch(
    request: &Request,
    registry: &SessionRegistry,
    store: &Arc<dyn store::store_trait::SessionStore>,
) {
    if request.method == "chat.send" {
        session_actor::dispatch_chat_send(request, registry);
    } else if request.method == "session.delete" {
        let sid = jsonrpc::get_param_opt(&request.params, "sessionId");
        if let Some(id) = sid { registry.remove(id); }
        try_route(request, store);
    } else {
        try_route(request, store);
    }
}

fn try_route(
    request: &Request,
    store: &Arc<dyn store::store_trait::SessionStore>,
) {
    if let Err(e) = handlers::route(request, store) {
        error!("Error handling request: {:#}", e);
        let _ = jsonrpc::write_response(&Response::error(
            request.id.clone(), -32603, format!("Internal error: {:#}", e),
        ));
    }
}
