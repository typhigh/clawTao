//! JSON-RPC method handlers and routing.

use crate::jsonrpc::{self, Request, Response};
use crate::store::{self, store_trait::SessionStore};
use anyhow::Result;
use serde_json::json;
use std::sync::Arc;

/// Route a JSON-RPC request to the appropriate handler.
pub fn route(
    request: &Request,
    store: &Arc<dyn SessionStore>,
) -> Result<()> {
    match request.method.as_str() {
        "session.list" => session_list(request, &**store),
        "session.create" => session_create(request, &**store),
        "session.get" => session_get(request, &**store),
        "session.delete" => session_delete(request, &**store),
        "ping" => ping(request),
        _ => not_found(request),
    }
}

/// Handle chat.interrupt: set the actor's cancel flag.
pub fn chat_interrupt(
    request: &Request,
    registry: &crate::session_actor::SessionRegistry,
) -> Result<()> {
    let sid = jsonrpc::get_param(&request.params, "sessionId")?;
    match registry.get_cancel(sid) {
        Some(cancel) => {
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
        }
        None => {
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
        }
    }
    Ok(())
}

// ── Session ──────────────────────────────────────────────────────────────

pub fn session_list(request: &Request, store: &dyn SessionStore) -> Result<()> {
    let result = serde_json::to_value(store.list().unwrap_or_default())?;
    jsonrpc::write_response(&Response::success(request.id.clone(), result))?;
    Ok(())
}

pub fn session_create(request: &Request, store: &dyn SessionStore) -> Result<()> {
    let s = store::new_session();
    store.create(&s)?;
    jsonrpc::write_response(&Response::success(request.id.clone(), serde_json::to_value(&s)?))?;
    Ok(())
}

pub fn session_get(request: &Request, store: &dyn SessionStore) -> Result<()> {
    let session_id = jsonrpc::get_param(&request.params, "sessionId")?;
    let session = store
        .get(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
    jsonrpc::write_response(&Response::success(request.id.clone(), serde_json::to_value(&session)?))?;
    Ok(())
}

pub fn session_delete(request: &Request, store: &dyn SessionStore) -> Result<()> {
    let session_id = jsonrpc::get_param(&request.params, "sessionId")?;
    store.delete(session_id)?;
    jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
    Ok(())
}

// ── Health ───────────────────────────────────────────────────────────────

pub fn ping(request: &Request) -> Result<()> {
    jsonrpc::write_response(&Response::success(
        request.id.clone(),
        json!({"status": "ok"}),
    ))?;
    Ok(())
}

// ── Error ────────────────────────────────────────────────────────────────

pub fn not_found(request: &Request) -> Result<()> {
    jsonrpc::write_response(&Response::error(
        request.id.clone(),
        -32601,
        format!("Method not found: {}", request.method),
    ))?;
    Ok(())
}
