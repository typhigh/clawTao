//! JSON-RPC method handlers and routing.

use crate::jsonrpc::{self, Request, Response};
use crate::store::{self, store_trait::SessionStore};
use crate::tools::registry::ToolRegistry;
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
        "session.context_stats" => session_context_stats(request, &**store),
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
    tracing::trace!("chat.interrupt: session={sid}");
    match registry.get_cancel(sid) {
        Some(cancel) => {
            cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            tracing::trace!("chat.interrupt: cancel flag set for session={sid}");
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({"ok": true})))?;
        }
        None => {
            tracing::trace!("chat.interrupt: no actor found for session={sid}");
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

// ── Compaction ────────────────────────────────────────────────────────────

/// Manual compaction handler. Runs on the session actor thread so store
/// access is serialised with chat.send.
pub(crate) fn session_compact(
    request: &Request,
    store: &dyn SessionStore,
    client: &reqwest::blocking::Client,
) -> Result<()> {
    let session_id = jsonrpc::get_param(&request.params, "sessionId")?;

    let config = request.params.as_ref()
        .and_then(|p| p.get("config"))
        .ok_or_else(|| anyhow::anyhow!("Missing config"))?;

    let api_key = config["api_key"].as_str().filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("API key not configured"))?;
    let base_url = config["base_url"].as_str().filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing base_url"))?;
    let model = config["model"].as_str().filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing model"))?;
    let protocol = config["api_protocol"].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing api_protocol"))?;

    let session = store.get(session_id)?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

    if session.messages.len() < crate::context::MIN_MSGS {
        jsonrpc::write_response(&Response::success(request.id.clone(), json!({
            "compacted": false,
            "reason": "too few messages to compact",
        })))?;
        return Ok(());
    }

    // ── Estimate "before" tokens (what the LLM currently sees) ──────
    let before_tokens = if let (Some(ref summary), Some(ref compacted_id)) =
        (&session.compacted_summary, &session.compacted_message_id)
    {
        let effective = crate::chat::build_effective_messages(
            &session.messages, compacted_id, summary,
        );
        crate::context::estimate_total_tokens_from_store("", &effective, &[], "")
    } else {
        crate::context::estimate_total_tokens_from_store("", &session.messages, &[], "")
    };

    let adapter: Box<dyn crate::llm::ApiAdapter> = match protocol {
        "anthropic" => Box::new(crate::llm::AnthropicAdapter),
        _ => Box::new(crate::llm::OpenAiAdapter),
    };

    // Manual compaction is more aggressive than automatic: keep only
    // the last turn so the user sees a noticeable reduction.
    const MANUAL_KEEP_TURNS: usize = 1;

    match crate::chat::compact_session(
        adapter.as_ref(), client, api_key, base_url, model,
        store, session_id, &session.messages,
        MANUAL_KEEP_TURNS,
    ) {
        Ok((_summary, _last_id)) => {
            // Re-read session to get the persisted compacted_summary.
            let session = store.get(session_id)?
                .ok_or_else(|| anyhow::anyhow!("Session vanished after compaction"))?;
            let after_tokens = {
                let effective = crate::chat::build_effective_messages(
                    &session.messages,
                    session.compacted_message_id.as_ref().unwrap(),
                    session.compacted_summary.as_ref().unwrap(),
                );
                crate::context::estimate_total_tokens_from_store("", &effective, &[], "")
            };
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({
                "compacted": true,
                "beforeTokens": before_tokens,
                "afterTokens": after_tokens,
            })))?;
        }
        Err(e) => {
            let detail = format!("{e:#}");
            tracing::warn!("Manual compaction failed: {detail}");
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({
                "compacted": false,
                "reason": detail,
            })))?;
        }
    }
    Ok(())
}

// ── Context stats ────────────────────────────────────────────────────────

/// Lightweight context-window usage snapshot for the per-session UI.
///
/// Returns the breakdown the frontend needs to render the 10×10 context grid:
///  - `systemTokens`: tokens consumed by the system prompt + tool definitions.
///    Constant per provider until tools are added.
///  - `messageTokens`: tokens consumed by the (effective) message history,
///    i.e. anything the LLM would see on the next call.
///  - `contextWindow`: the provider's max context window.
///
/// The frontend fills cells in a 10×10 grid (= 100 cells = 1% per cell):
///  - `systemTokens / contextWindow` → dark-gray cells (system-prompt slice)
///  - `messageTokens / contextWindow` → light-gray cells (history slice)
///  - everything else stays white.
pub(crate) fn session_context_stats(
    request: &Request,
    store: &dyn SessionStore,
) -> Result<()> {
    let session_id = jsonrpc::get_param(&request.params, "sessionId")?;

    // base_url is required for the per-provider context-window lookup.
    // If the caller didn't supply one (renderer shortcut, ad-hoc shell
    // query, …) fall back to a conservative 256K window so the UI still
    // shows a useful proportion rather than crashing.
    let base_url = jsonrpc::get_param_opt(&request.params, "base_url")
        .unwrap_or("");

    let model = jsonrpc::get_param_opt(&request.params, "model").unwrap_or("");

    let session = match store.get(session_id)? {
        Some(s) => s,
        None => {
            jsonrpc::write_response(&Response::success(request.id.clone(), json!({
                "systemTokens": 0,
                "messageTokens": 0,
                "contextWindow": crate::context::provider_context_window(base_url),
                "compacted": false,
            })))?;
            return Ok(());
        }
    };

    // Use the same "effective" message list the LLM would actually see —
    // respects any prior compaction summary.
    let (effective, compacted) = match (
        session.compacted_summary.as_ref(),
        session.compacted_message_id.as_ref(),
    ) {
        (Some(summary), Some(compacted_id)) => {
            let msgs = crate::chat::build_effective_messages(
                &session.messages, compacted_id, summary,
            );
            (msgs, true)
        }
        _ => (session.messages.clone(), false),
    };

    // System tokens = the system prompt (built with the schema-only tool
    // registry, since the actual turn uses the same tool set) plus the
    // tool-definition tokens the LLM sees on every call.
    //
    // We rebuild the system prompt here for parity with `chat.rs:124` so
    // the displayed number matches what the next turn will actually send.
    let workspace_dir = jsonrpc::get_param_opt(&request.params, "workspace_dir");
    let tool_registry = build_schema_registry();
    let system_prompt = crate::system_prompt::build(
        &tool_registry, workspace_dir.filter(|s| !s.is_empty()),
    );
    let tool_defs: Vec<crate::llm::UnifiedTool> = tool_registry.list_specs().iter()
        .map(|spec| crate::llm::UnifiedTool {
            name: spec.function.name.clone(),
            description: spec.function.description.clone(),
            parameters: spec.function.parameters.clone(),
        })
        .collect();
    let system_tokens = crate::context::estimate_total_tokens_from_store(
        &system_prompt, &[], &tool_defs, model,
    );
    let message_tokens = crate::context::estimate_total_tokens_from_store(
        "", &effective, &[], model,
    );
    let context_window = crate::context::provider_context_window(base_url);

    jsonrpc::write_response(&Response::success(request.id.clone(), json!({
        "systemTokens": system_tokens,
        "messageTokens": message_tokens,
        "contextWindow": context_window,
        "compacted": compacted,
    })))?;
    Ok(())
}

/// Build a stateless tool registry containing the schema-only tool
/// definitions used for token estimation. Re-creating this on every
/// `context_stats` call is fine — `register_all` is cheap and the
/// tools have no per-instance state we'd care about for token math.
fn build_schema_registry() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    crate::tools::builtin::register_all(
        &mut reg,
        // Sandbox mode is irrelevant to token estimation; off is the
        // safest default and keeps the schema stable.
        crate::tools::builtin::SandboxConfig::off(),
        None,
    );
    reg
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

#[cfg(test)]
#[path = "tests/context_stats_tests.rs"]
mod context_stats_tests;
