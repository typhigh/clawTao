//! ClawTao Rust Backend — JSON-RPC 2.0 server over stdio.
//!
//! Spawned by Electron as a child process. Communication via stdin/stdout,
//! one JSON-RPC 2.0 message per line. Stderr is reserved for tracing logs.
//!
//! Each active session runs in its own thread (actor). Chat requests are
//! dispatched via `SessionRegistry`; short-lived RPCs (ping, session.*)
//! are handled directly on the main thread.

mod chat;
mod context;
mod error;
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
use tracing_subscriber::{reload, EnvFilter};
use serde_json::json;

type FilterHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        eprintln!("=== CLAWTAO PANIC ===\n{info}\n{backtrace}");
    }));

    let initial_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = EnvFilter::try_new(&initial_level)
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let (filter_layer, reload_handle) = reload::Layer::new(env_filter);

    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt::Layer::default().with_writer(std::io::stderr))
        .init();

    let reload_handle = Arc::new(reload_handle);
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
            compacted_summary: None,
            compacted_message_id: None,
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
                    Ok(request) => dispatch(&request, &registry, &store, &reload_handle),
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
    filter: &FilterHandle,
) {
    if request.method == "chat.send" {
        session_actor::dispatch_chat_send(request, registry);
    } else if request.method == "session.compact" {
        session_actor::dispatch_session_compact(request, registry);
    } else if request.method == "chat.interrupt" {
        if let Err(e) = handlers::chat_interrupt(request, registry) {
            error!("Error handling request: {:#}", e);
            let _ = jsonrpc::write_response(&Response::error(request.id.clone(), -32603, format!("Internal error: {:#}", e)));
        }
    } else if request.method == "session.delete" {
        let sid = jsonrpc::get_param_opt(&request.params, "sessionId");
        if let Some(id) = sid { registry.remove(id); }
        try_route(request, store, filter);
    } else {
        try_route(request, store, filter);
    }
}

fn try_route(
    request: &Request,
    store: &Arc<dyn store::store_trait::SessionStore>,
    filter: &FilterHandle,
) {
    if request.method == "config.set_log_level" {
        let level = jsonrpc::get_param_opt(&request.params, "level").unwrap_or("info");
        match EnvFilter::try_new(level) {
            Ok(f) => {
                if filter.reload(f).is_ok() {
                    info!("Log level set to: {level}");
                    let _ = jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true, "level": level})));
                } else {
                    let _ = jsonrpc::write_response(&Response::error(request.id.clone(), -32000, "reload failed"));
                }
            }
            Err(e) => {
                let _ = jsonrpc::write_response(&Response::error(request.id.clone(), -32000, format!("invalid filter: {e}")));
            }
        }
        return;
    }
    if let Err(e) = handlers::route(request, store) {
        error!("Error handling request: {:#}", e);
        let _ = jsonrpc::write_response(&Response::error(
            request.id.clone(), -32603, format!("Internal error: {:#}", e),
        ));
    }
}
