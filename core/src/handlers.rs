//! JSON-RPC method handlers — all supported methods at a glance.

use crate::config::LlmConfig;
use crate::jsonrpc::{self, Request, Response};
use crate::store::{self, store_trait::SessionStore};
use anyhow::Result;
use serde_json::json;
use tracing::info;

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
    jsonrpc::write_response(&Response::success(
        request.id.clone(),
        serde_json::to_value(&session)?,
    ))?;
    Ok(())
}

pub fn session_delete(request: &Request, store: &dyn SessionStore) -> Result<()> {
    let session_id = jsonrpc::get_param(&request.params, "sessionId")?;
    store.delete(session_id)?;
    jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
    Ok(())
}

// ── Config ───────────────────────────────────────────────────────────────

pub fn config_get(request: &Request, cfg: &LlmConfig) -> Result<()> {
    jsonrpc::write_response(&Response::success(
        request.id.clone(),
        serde_json::to_value(cfg.masked())?,
    ))?;
    Ok(())
}

pub fn config_set(request: &Request, cfg: &mut LlmConfig) -> Result<()> {
    let new_config: LlmConfig = serde_json::from_value(
        request.params.clone().unwrap_or_default(),
    )
    .map_err(|e| anyhow::anyhow!("Invalid config: {e}"))?;
    new_config.save()?;
    *cfg = new_config;
    info!(
        "Config updated: provider={} model={}",
        cfg.provider, cfg.model
    );
    jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
    Ok(())
}

pub fn config_inject_key(request: &Request, cfg: &mut LlmConfig) -> Result<()> {
    let api_key = jsonrpc::get_param(&request.params, "api_key")?;
    cfg.api_key = api_key.to_string();
    info!("API key injected (length={})", cfg.api_key.len());
    jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
    Ok(())
}

pub fn config_validate(request: &Request, cfg: &LlmConfig) -> Result<()> {
    match cfg.validate() {
        Ok(()) => {
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?
        }
        Err(e) => jsonrpc::write_response(&Response::success(
            request.id.clone(),
            json!({"ok": false, "error": e}),
        ))?,
    }
    Ok(())
}

pub fn config_test_key(request: &Request, cfg: &LlmConfig) -> Result<()> {
    let api_key = request
        .params
        .as_ref()
        .and_then(|p| p.get("api_key"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&cfg.api_key);
    let base_url =
        jsonrpc::get_param(&request.params, "base_url").unwrap_or(&cfg.base_url);
    let model =
        jsonrpc::get_param(&request.params, "model").unwrap_or(&cfg.model);
    let api_protocol =
        jsonrpc::get_param(&request.params, "api_protocol").unwrap_or(&cfg.api_protocol);
    match LlmConfig::test_connection(base_url, model, api_key, api_protocol) {
        Ok(()) => {
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
        }
        Err(e) => jsonrpc::write_response(&Response::success(
            request.id.clone(),
            json!({"ok": false, "error": e}),
        ))?,
    }
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
