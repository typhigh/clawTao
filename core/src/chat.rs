//! chat.send handler — the agent turn loop.

use anyhow::Result;
use crate::error::ChatError;
use crate::error::downcast_chat_error;
use crate::jsonrpc::{Notification, Response};
use crate::llm::{ApiAdapter, AnthropicAdapter, LlmMessage, LlmRequest, OpenAiAdapter, UnifiedTool};
use crate::llm::types::LlmResponse;
use crate::store::{self, store_trait::SessionStore};
use crate::tools::{self, registry::ToolRegistry};
use crate::jsonrpc::{get_param, write_notification, write_response};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{debug, trace, warn};

/// Immutable context for a single turn.
pub(crate) struct TurnContext {
    session_id: String,
    run_id: String,
    system_prompt: String,
    tools: Vec<UnifiedTool>,
    user_images: Option<Vec<crate::llm::types::ImageContent>>,
    sandbox_rules: crate::tools::builtin::SandboxRules,
}

/// Entry point for a single chat turn. Sets up the adapter, tool registry,
/// and context, then runs the state machine.
pub(crate) fn run_turn(
    request: &crate::jsonrpc::Request,
    store: &dyn SessionStore,
    client: &Client,
    cancel: &AtomicBool,
) -> Result<()> {
    let message_text = get_param(&request.params, "message")
        .map_err(|e| anyhow::anyhow!(ChatError::BadRequest { detail: e.to_string() }))?;
    let session_id = get_param(&request.params, "sessionId")
        .map_err(|e| anyhow::anyhow!(ChatError::BadRequest { detail: e.to_string() }))?;

    // Parse user-uploaded images from the request.
    let user_images = parse_user_images(&request.params);

    // Save user-uploaded images to workspace filesystem.
    let image_paths: Vec<String> = if let Some(ref imgs) = user_images {
        save_user_images(session_id, imgs)
    } else {
        vec![]
    };

    let config = request.params.as_ref()
        .and_then(|p| p.get("config"))
        .ok_or_else(|| anyhow::anyhow!(ChatError::Config { detail: "Missing config".into() }))?;

    let api_key = config["api_key"].as_str().filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!(ChatError::Config { detail: "API key not configured".into() }))?;
    let base_url = config["base_url"].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!(ChatError::Config { detail: "Missing config field: base_url".into() }))?;
    let model = config["model"].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!(ChatError::Config { detail: "Missing config field: model".into() }))?;
    let protocol = config["api_protocol"].as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!(ChatError::Config { detail: "Missing config field: api_protocol".into() }))?;
    let thinking_enabled = config["thinking_enabled"].as_bool().unwrap_or(false);
    let bash_timeout = config["bash_timeout_secs"].as_u64();

    // Read sandbox mode from config.
    let sandbox_mode = match config.get("sandbox_mode").and_then(|v| v.as_str()).unwrap_or("workspace_only") {
        "off" => tools::builtin::SandboxMode::Off,
        "strict" => tools::builtin::SandboxMode::Strict,
        _ => tools::builtin::SandboxMode::WorkspaceOnly,
    };

    // Store the user message first so subsequent store.get() includes it.
    if image_paths.is_empty() {
        store::add_message(store, session_id, "user", message_text)?;
    } else {
        store::add_user_message_with_images(store, session_id, message_text, image_paths)?;
    }

    let _session = store.get(session_id)?
        .ok_or_else(|| anyhow::anyhow!(ChatError::Session { detail: "Session not found".into() }))?;

    let workspace_dir = config.get("workspace_dir")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| workspace_dir_for_session(session_id));

    warn!("Turn sandbox: workspace_dir={}, mode={:?}",
          workspace_dir, sandbox_mode);
    let sandbox_cfg = tools::builtin::SandboxConfig::new(&workspace_dir, sandbox_mode);
    let sandbox_rules = if sandbox_cfg.is_active() {
        tools::builtin::SandboxRules::new(&sandbox_cfg.workspace_dir)
    } else {
        tools::builtin::SandboxRules::off()
    };
    let mut tool_registry = ToolRegistry::new();
    tools::builtin::register_all(&mut tool_registry, sandbox_cfg, bash_timeout);

    let run_id = uuid::Uuid::new_v4().to_string();

    write_notification(&Notification::new("chat.stream", Some(json!({
        "sessionId": session_id, "runId": run_id, "kind": "started"
    }))))?;

    let adapter: Box<dyn ApiAdapter> = match protocol {
        "anthropic" => Box::new(AnthropicAdapter),
        _ => Box::new(OpenAiAdapter),
    };

    let tools: Vec<UnifiedTool> = tool_registry.list_specs().iter().map(|spec| UnifiedTool {
        name: spec.function.name.clone(),
        description: spec.function.description.clone(),
        parameters: spec.function.parameters.clone(),
    }).collect();

    let ctx = TurnContext {
        session_id: session_id.to_string(),
        run_id,
        system_prompt: crate::system_prompt::build(&tool_registry, sandbox_rules.active.then_some(workspace_dir.as_str())),
        tools,
        user_images,
        sandbox_rules,
    };

    let (final_content, final_thinking) = run_state_machine(
        store, adapter.as_ref(), client,
        api_key, base_url, model, thinking_enabled,
        &ctx, &tool_registry, cancel,
        None, // production: write to stdout only
    )?;

    // ── Finalize ─────────────────────────────────────────────────────
    let content = if final_content.is_empty() { "(no response)" } else { &final_content };
    store::add_assistant_message(store, &ctx.session_id, content, final_thinking.as_deref())?;

    write_notification(&Notification::new("chat.stream", Some(json!({
        "sessionId": ctx.session_id, "runId": ctx.run_id, "kind": "done"
    }))))?;

    write_response(&Response::success(request.id.clone(), json!({
        "runId": ctx.run_id,
        "message": {
            "id": uuid::Uuid::new_v4().to_string(),
            "role": "assistant",
            "content": final_content,
            "timestamp": chrono::Utc::now().timestamp_millis()
        }
    })))?;

    Ok(())
}

// ── State machine ──────────────────────────────────────────────────

const MAX_RETRIES: u32 = 3;

enum TurnState {
    /// Waiting for the LLM to respond.
    Waiting { retry_count: u32 },
    /// Executing tool calls one by one.
    Tooling { response: LlmResponse, cursor: usize },
    /// Turn completed normally (text reply, no more tools).
    Done { text: String, thinking: Option<String> },
    /// User pressed cancel — preserve partial output.
    Interrupted { text: String, thinking: Option<String> },
    /// A retryable error occurred. Backoff, notify the frontend,
    /// then transition back to `Waiting`.
    Error { error: ChatError, retry_count: u32 },
    /// Non-retryable error, or retries exhausted. Terminal.
    Fatal { error: ChatError },
}

/// Exponential backoff with jitter for retryable errors.
fn backoff(attempt: u32) -> Duration {
    let ms = 200u64 * 2u64.pow(attempt.saturating_sub(1));
    // Simple jitter: ±20 %
    let jitter = (ms as f64 * 0.2) as u64;
    let low = ms.saturating_sub(jitter);
    let high = ms.saturating_add(jitter);
    // Deterministic for tests, still varied in practice.
    Duration::from_millis(if low < high { low + (high - low) / 2 } else { ms })
}

fn run_state_machine(
    store: &dyn SessionStore,
    adapter: &dyn ApiAdapter,
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    thinking_enabled: bool,
    ctx: &TurnContext,
    tool_registry: &ToolRegistry,
    cancel: &AtomicBool,
    mut notif_collector: Option<&mut Vec<Notification>>,
) -> Result<(String, Option<String>)> {
    let mut state = TurnState::Waiting { retry_count: 0 };
    let mut compaction_attempted = false;

    // Load the full session — store.messages is the complete history and
    // is never modified by compaction.
    let session = store.get(&ctx.session_id)?
        .ok_or_else(|| anyhow::anyhow!(ChatError::Session {
            detail: "Session not found".into(),
        }))?;
    let mut full_messages = session.messages.clone();

    let mut effective_msgs = match (&session.compacted_summary, &session.compacted_message_id) {
        (Some(summary), Some(id)) => build_effective_messages(&full_messages, id, summary),
        _ => full_messages.clone(),
    };

    // Inline helper — avoids closure ownership issues inside the loop.
    macro_rules! notify {
        ($n:expr) => {{
            let n = $n;
            if let Some(ref mut col) = notif_collector {
                col.push(n.clone());
            }
            write_notification(&n)?;
        }};
    }

    loop {
        state = match state {
            // ── Waiting: call the LLM ──────────────────────────────
            TurnState::Waiting { retry_count } => {
                // ── Proactive compaction ──────────────────────────
                if !compaction_attempted {
                    let est = crate::context::estimate_total_tokens_from_store(
                        &ctx.system_prompt, &effective_msgs, &ctx.tools, model,
                    );
                    if est > crate::context::compact_threshold(base_url) {
                        debug!(
                            "Compaction triggered: est {est} tokens > threshold ({} msgs)",
                            effective_msgs.len(),
                        );
                        notify!(Notification::new("chat.stream", Some(json!({
                            "sessionId": ctx.session_id, "runId": ctx.run_id,
                            "kind": "compacting",
                        }))));
                        match compact_session(
                            adapter, client, api_key, base_url, model,
                            store, &ctx.session_id, &full_messages,
                            crate::context::MIN_TURNS,
                        ) {
                            Ok((summary, last_id)) => {
                                effective_msgs = build_effective_messages(
                                    &full_messages, &last_id, &summary,
                                );
                                compaction_attempted = true;
                                notify!(Notification::new("chat.stream", Some(json!({
                                    "sessionId": ctx.session_id, "runId": ctx.run_id,
                                    "kind": "compacted",
                                    "messageCount": effective_msgs.len(),
                                    "warning": "Long threads may reduce model accuracy. Consider starting a new session for complex tasks.",
                                }))));
                            }
                            Err(e) => {
                                warn!("Proactive compaction failed: {e:?}");
                            }
                        }
                    }
                }

                match llm_step(adapter, client, api_key, base_url, model,
                                thinking_enabled, &effective_msgs, ctx, cancel) {
                    Ok(response) => {
                        // LLM succeeded — reset retry counter for the
                        // next round (tool → back to Waiting).
                        if cancel.load(Ordering::SeqCst) {
                            TurnState::Interrupted {
                                text: response.text,
                                thinking: response.thinking,
                            }
                        } else if response.tool_calls.is_empty() {
                            TurnState::Done {
                                text: response.text,
                                thinking: response.thinking,
                            }
                        } else {
                            store::add_assistant_tool_calls(
                                store, &ctx.session_id,
                                response.tool_calls.clone(),
                                &response.text,
                                response.thinking.as_deref(),
                            )?;
                            TurnState::Tooling { response, cursor: 0 }
                        }
                    }
                    Err(e) => {
                        let ce = downcast_chat_error(&e)
                            .cloned()
                            .unwrap_or_else(|| ChatError::Internal {
                                detail: format!("{e:#}"),
                            });

                        // ── Reactive compaction ────────────────────
                        if matches!(ce, ChatError::ContextExceeded) && !compaction_attempted {
                            debug!("Reactive compaction on ContextExceeded");
                            notify!(Notification::new("chat.stream", Some(json!({
                                "sessionId": ctx.session_id, "runId": ctx.run_id,
                                "kind": "compacting",
                            }))));
                            match compact_session(
                                adapter, client, api_key, base_url, model,
                                store, &ctx.session_id, &full_messages,
                                crate::context::MIN_TURNS,
                            ) {
                                Ok((summary, last_id)) => {
                                    effective_msgs = build_effective_messages(
                                        &full_messages, &last_id, &summary,
                                    );
                                    compaction_attempted = true;
                                    notify!(Notification::new("chat.stream", Some(json!({
                                        "sessionId": ctx.session_id, "runId": ctx.run_id,
                                        "kind": "compacted",
                                        "messageCount": effective_msgs.len(),
                                        "warning": "Long threads may reduce model accuracy. Consider starting a new session for complex tasks.",
                                    }))));
                                    TurnState::Waiting { retry_count: 0 }
                                }
                                Err(_) => {
                                    warn!("Reactive compaction failed");
                                    TurnState::Fatal { error: ce }
                                }
                            }
                        } else if ce.is_retryable() && retry_count < MAX_RETRIES {
                            TurnState::Error { error: ce, retry_count }
                        } else {
                            TurnState::Fatal { error: ce }
                        }
                    }
                }
            }

            // ── Tooling: execute tool calls sequentially ───────────
            TurnState::Tooling { response, cursor } => {
                if cursor >= response.tool_calls.len() {
                    let session = store.get(&ctx.session_id)?
                        .ok_or_else(|| anyhow::anyhow!(ChatError::Session {
                            detail: "Session not found after tool execution".into()
                        }))?;
                    full_messages = session.messages.clone();
                    effective_msgs = match (&session.compacted_summary, &session.compacted_message_id) {
                        (Some(s), Some(id)) => build_effective_messages(&full_messages, id, s),
                        _ => full_messages.clone(),
                    };
                    TurnState::Waiting { retry_count: 0 }
                } else {
                    let tc = &response.tool_calls[cursor];
                    debug!("Executing tool: {} id={} args={}",
                           tc.function.name, tc.id, tc.function.arguments);
                    let args_val: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(Value::Null);

                    notify!(Notification::new("chat.stream", Some(json!({
                        "sessionId": ctx.session_id, "runId": ctx.run_id,
                        "kind": "tool_call", "toolCallId": tc.id,
                        "toolName": tc.function.name, "input": args_val,
                    }))));

                    if tc.function.name == "TodoWrite" {
                        if let Some(todos) = args_val.get("todos") {
                            notify!(Notification::new("chat.stream", Some(json!({
                                "sessionId": ctx.session_id, "runId": ctx.run_id,
                                "kind": "todo", "todos": todos,
                            }))));
                        }
                    }

                    let tool_result = if cancel.load(Ordering::SeqCst) {
                        "[interrupted by user]".to_string()
                    } else if let Some(executor) = tool_registry.get(&tc.function.name) {
                        // ── Sandbox check ──────────────────────────
                        if let Err(msg) = executor.check_sandbox(&args_val, &ctx.sandbox_rules) {
                            format!("Sandbox denied: {msg}")
                        } else {
                            match executor.execute(args_val.clone(), cancel) {
                                Ok(output) => output,
                                Err(e) => {
                                    warn!("Tool {} failed: {e}", tc.function.name);
                                    format!("Tool error: {e}")
                                }
                            }
                        }
                    } else {
                        warn!("Unknown tool requested: {}", tc.function.name);
                        format!("Unknown tool: {}", tc.function.name)
                    };

                    debug!("Tool result for {}: {:.200}", tc.function.name, tool_result);
                    notify!(Notification::new("chat.stream", Some(json!({
                        "sessionId": ctx.session_id, "runId": ctx.run_id,
                        "kind": "tool_result", "toolCallId": tc.id,
                        "toolName": tc.function.name, "output": tool_result,
                    }))));
                    store::add_tool_result(store, &ctx.session_id, &tc.id, &tool_result)?;

                    TurnState::Tooling { response, cursor: cursor + 1 }
                }
            }

            // ── Error: retryable — backoff then retry ──────────────
            TurnState::Error { error, retry_count } => {
                let next = retry_count + 1;
                let delay = backoff(next);
                warn!("LLM error (attempt {}/{}): {} — retrying in {:?}",
                      next, MAX_RETRIES, error.user_message(), delay);
                notify!(Notification::new("chat.stream", Some(json!({
                    "sessionId": ctx.session_id, "runId": ctx.run_id,
                    "kind": "stream_error",
                    "errorCode": error.code(),
                    "message": format!("Reconnecting... {}/{}", next, MAX_RETRIES),
                    "retryable": true,
                }))));
                std::thread::sleep(delay);
                TurnState::Waiting { retry_count: next }
            }

            // ── Terminal states ────────────────────────────────────
            TurnState::Done { text, thinking } => break Ok((text, thinking)),
            TurnState::Interrupted { text, thinking } => break Ok((text, thinking)),
            TurnState::Fatal { error } => {
                break Err(anyhow::anyhow!(error));
            }
        };
    }
}

// ── One LLM call ───────────────────────────────────────────────────

fn llm_step(
    adapter: &dyn ApiAdapter,
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    thinking_enabled: bool,
    messages: &[store::Message],
    ctx: &TurnContext,
    cancel: &AtomicBool,
) -> Result<LlmResponse> {
    let mut llm_msgs: Vec<LlmMessage> = messages.iter().map(|m| {
        let (content, images) = if m.content.starts_with("[SCREENSHOT]\n") {
            let b64 = m.content.strip_prefix("[SCREENSHOT]\n").unwrap_or("");
            (
                "Screenshot captured.".to_string(),
                Some(vec![crate::llm::types::ImageContent {
                    base64: b64.to_string(),
                    media_type: "image/png".to_string(),
                }]),
            )
        } else {
            (m.content.clone(), None)
        };
        // Load historical images from filesystem paths.
        let mut imgs = images;
        if imgs.is_none() {
            if let Some(ref paths) = m.image_paths {
                let loaded = load_images_from_paths(paths);
                if !loaded.is_empty() { imgs = Some(loaded); }
            }
        }
        LlmMessage {
            role: m.role.clone(),
            content,
            images: imgs,
            tool_calls: m.tool_calls.clone(),
            tool_call_id: m.tool_call_id.clone(),
            thinking: m.thinking.clone(),
        }
    }).collect();

    // Attach user-uploaded images to the last user message of this turn.
    if let Some(ref imgs) = ctx.user_images {
        if let Some(last_user) = llm_msgs.iter_mut().rev().find(|m| m.role == "user") {
            match last_user.images {
                Some(ref mut existing) => existing.extend(imgs.iter().cloned()),
                None => last_user.images = Some(imgs.clone()),
            }
        }
    }

    let http = adapter.build(&LlmRequest {
        system: ctx.system_prompt.clone(),
        model: model.to_string(),
        messages: llm_msgs,
        tools: ctx.tools.clone(),
        thinking_enabled,
    }, api_key, base_url)?;

    debug!("LLM call: url={} model={}", http.url, model);
    trace!("LLM request body: {}", http.body);

    let mut resp = client.post(&http.url);
    for (k, v) in &http.headers {
        resp = resp.header(k.as_str(), v.as_str());
    }
    let resp = resp.body(http.body.clone()).send()
        .map_err(|e| map_network_error(e, &http.url))?;

    let status = resp.status();
    debug!("LLM response: status={}", status);

    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(resp);
    let mut line = String::new();
    let mut body_bytes = Vec::new();

    while reader.read_line(&mut line)
        .map_err(|_| anyhow::anyhow!(ChatError::StreamDisconnected))? > 0
    {
        body_bytes.extend_from_slice(line.as_bytes());
        if let Some(data) = line.trim().strip_prefix("data: ") {
            if data == "[DONE]" { line.clear(); continue; }
            if let Ok(event) = serde_json::from_str::<Value>(data) {
                // ── Inline SSE error detection ──────────────────────
                // Some providers send errors inside the SSE stream
                // (e.g. Anthropic "error" type, OpenAI {"error": {...}}).
                // Check before dispatching stream_events so the
                // turn fails fast instead of silently ignoring the error.
                if let Some(sse_err) = event.get("error") {
                    let msg = sse_err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown API error");
                    let err_type = sse_err.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let detail = if err_type.is_empty() {
                        msg.to_string()
                    } else {
                        format!("{err_type}: {msg}")
                    };
                    if crate::context::is_context_length_error(&detail) {
                        return Err(anyhow::anyhow!(ChatError::ContextExceeded));
                    }
                    return Err(anyhow::anyhow!(ChatError::BadRequest { detail }));
                }
                for se in adapter.stream_events(&event) {
                    write_notification(&Notification::new("chat.stream", Some(json!({
                        "sessionId": ctx.session_id, "runId": ctx.run_id,
                        "kind": se.kind, "delta": se.delta,
                    }))))?;
                }
            }
        }
        line.clear();
        if cancel.load(Ordering::SeqCst) { debug!("llm_step: SSE loop cancelled"); break; }
    }

    let body_str = String::from_utf8_lossy(&body_bytes);
    debug!("LLM response body: {} bytes", body_bytes.len());
    trace!("LLM response body: {}", body_str);

    // ── HTTP status check (after reading body) ────────────────────
    if !status.is_success() {
        return Err(anyhow::anyhow!(http_error_from_status(status.as_u16(), &body_str)));
    }

    adapter.parse_stream(&body_str)
}

// ── Context compaction ──────────────────────────────────────────────

/// Minimal LLM call for summarization — no streaming notifications, no
/// tools, no cancel check. Returns the model's text response.
fn summarize_conversation(
    adapter: &dyn ApiAdapter,
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    conversation_text: &str,
) -> Result<String> {
    let system =
        "You are a helpful assistant. Summarize conversations concisely and accurately.".to_string();
    let prompt = format!(
        "You are performing a CONTEXT CHECKPOINT COMPACTION. Create a handoff summary for another LLM that will resume the task.\n\n\
         Include:\n\
         - Current progress and key decisions made\n\
         - Important context, constraints, or user preferences\n\
         - What remains to be done (clear next steps)\n\
         - Any critical data, examples, or references needed to continue\n\n\
         Be concise, structured, and focused on helping the next LLM seamlessly continue the work.\n\n\
         Conversation:\n\
         {conversation_text}\n\n\
         Summary:"
    );

    let msg = LlmMessage {
        role: "user".into(),
        content: prompt,
        images: None,
        tool_calls: None,
        tool_call_id: None,
        thinking: None,
    };

    let req = LlmRequest {
        system,
        model: model.to_string(),
        messages: vec![msg],
        tools: vec![],
        thinking_enabled: false,
    };

    let http = adapter.build(&req, api_key, base_url)?;
    debug!("Compaction LLM call: url={} model={}", http.url, model);

    let mut resp = client.post(&http.url);
    for (k, v) in &http.headers {
        resp = resp.header(k.as_str(), v.as_str());
    }
    let resp = resp
        .body(http.body.clone())
        .send()
        .map_err(|e| map_network_error(e, &http.url))?;

    let status = resp.status();

    use std::io::{BufRead, BufReader};
    let mut reader = BufReader::new(resp);
    let mut line = String::new();
    let mut body_bytes = Vec::new();

    while reader
        .read_line(&mut line)
        .map_err(|_| anyhow::anyhow!(ChatError::StreamDisconnected))?
        > 0
    {
        body_bytes.extend_from_slice(line.as_bytes());
        if let Some(data) = line.trim().strip_prefix("data: ") {
            if data == "[DONE]" {
                line.clear();
                continue;
            }
            if let Ok(event) = serde_json::from_str::<Value>(data) {
                if let Some(sse_err) = event.get("error") {
                    let msg = sse_err
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown API error");
                    let err_type = sse_err
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let detail = if err_type.is_empty() {
                        msg.to_string()
                    } else {
                        format!("{err_type}: {msg}")
                    };
                    if crate::context::is_context_length_error(&detail) {
                        return Err(anyhow::anyhow!(ChatError::ContextExceeded));
                    }
                    return Err(anyhow::anyhow!(ChatError::BadRequest { detail }));
                }
            }
        }
        line.clear();
    }

    let body_str = String::from_utf8_lossy(&body_bytes);
    debug!("Compaction LLM response: {} bytes", body_bytes.len());

    if !status.is_success() {
        return Err(anyhow::anyhow!(http_error_from_status(
            status.as_u16(),
            &body_str,
        )));
    }

    let response = adapter.parse_stream(&body_str)?;
    Ok(response.text)
}

/// Generate a compaction summary for old messages and persist it as
/// session metadata.  Does **not** modify the messages table —
/// store.get() always returns the full history.
///
/// `min_turns_keep` controls how many recent conversation turns to keep
/// un-summarised.  Manual compaction uses 1 (aggressive), auto-compaction
/// uses `MIN_TURNS`.
///
/// Returns `(summary, last_compacted_message_id)`.
pub(crate) fn compact_session(
    adapter: &dyn ApiAdapter,
    client: &Client,
    api_key: &str,
    base_url: &str,
    model: &str,
    store: &dyn SessionStore,
    session_id: &str,
    messages: &[store::Message],
    min_turns_keep: usize,
) -> Result<(String, String)> {
    // ── Guards ──────────────────────────────────────────────────────
    if messages.len() < crate::context::MIN_MSGS {
        anyhow::bail!("too few messages to compact");
    }

    let keep_count = crate::context::count_turns_from_end(messages, min_turns_keep);
    if keep_count >= messages.len() {
        anyhow::bail!("all messages are recent — nothing to compact");
    }

    let to_summarize = &messages[..messages.len() - keep_count];
    if to_summarize.len() < 2 {
        anyhow::bail!("nothing worth summarising");
    }

    // ── Generate summary ────────────────────────────────────────────
    let mut skip: usize = 0; // 0 = first attempt uses all messages

    let summary = loop {
        let slice = &to_summarize[skip..];
        let conversation_text = crate::context::build_conversation_text(slice);
        debug!(
            "Compacting session {}: {} msgs (skip {skip}), conv_text {} chars",
            session_id,
            slice.len(),
            conversation_text.len(),
        );

        match summarize_conversation(
            adapter, client, api_key, base_url, model, &conversation_text,
        ) {
            Ok(s) => break s,
            Err(e) => {
                let is_ctx = downcast_chat_error(&e)
                    .is_some_and(|ce| matches!(ce, ChatError::ContextExceeded));
                if !is_ctx {
                    return Err(e);
                }
                // Exponential backoff: drop oldest messages 1, 2, 4, 8, ...
                skip = if skip == 0 { 1 } else { skip.saturating_mul(2) };
                if skip >= to_summarize.len() {
                    return Err(e);
                }
                warn!(
                    "Compaction summary hit context limit — retrying without oldest {skip} msgs"
                );
            }
        }
    };

    // ── Post-process summary ────────────────────────────────────────
    let summary = summary.trim().to_string();
    if summary.is_empty() {
        anyhow::bail!("compaction summary was empty");
    }
    let summary: String = summary
        .chars()
        .take(crate::context::MAX_CHARS_PER_MSG)
        .collect();

    let last_id = to_summarize.last().unwrap().id.clone();

    // Persist compaction metadata — does NOT touch messages.
    store.update_compaction(session_id, Some(&summary), Some(&last_id))?;

    debug!(
        "Compaction done: {} msgs → keeping last {keep_count}, last_compacted={last_id}",
        messages.len(),
    );

    Ok((summary, last_id))
}

/// Build the message list the LLM should see, given full store messages
/// and a compaction summary.  Merges the summary into the first kept
/// user message to satisfy the Anthropic alternating-role constraint.
pub(crate) fn build_effective_messages(
    messages: &[store::Message],
    compacted_id: &str,
    summary: &str,
) -> Vec<store::Message> {
    let pos = messages.iter().position(|m| m.id == compacted_id);
    let start = pos.map_or(0, |p| p + 1);
    if start >= messages.len() {
        return messages.to_vec();
    }
    let mut kept = messages[start..].to_vec();

    if let Some(first_user) = kept.iter().position(|m| m.role == "user") {
        kept[first_user].content = format!(
            "Another language model started to solve this problem and produced \
             a summary of its thinking process. Use this to build on the work \
             that has already been done and avoid duplicating work.\n\n\
             {summary}\n\n---\n\n{}",
            kept[first_user].content
        );
    } else {
        // Shouldn't happen — turn boundaries are user messages.
        let mut summary_msg = store::new_msg("user", summary);
        summary_msg.id = uuid::Uuid::new_v4().to_string();
        kept.insert(0, summary_msg);
    }
    kept
}

// ── Error mapping helpers ────────────────────────────────────────────

/// Map a `reqwest::Error` to a `ChatError`, distinguishing timeouts from
/// other network failures.
pub(crate) fn map_network_error(e: reqwest::Error, url: &str) -> anyhow::Error {
    if e.is_timeout() {
        anyhow::anyhow!(ChatError::Timeout { seconds: 0 /* unknown */ })
    } else if e.is_connect() {
        anyhow::anyhow!(ChatError::Network {
            detail: format!("Cannot connect to {url}: {e}")
        })
    } else if e.is_body() || e.is_decode() {
        anyhow::anyhow!(ChatError::StreamDisconnected)
    } else {
        anyhow::anyhow!(ChatError::Network {
            detail: format!("Request to {url} failed: {e}")
        })
    }
}

/// Map an HTTP status code + response body to a `ChatError`.
pub(crate) fn http_error_from_status(status: u16, body: &str) -> ChatError {
    let detail = extract_error_message(body).unwrap_or_else(|| {
        body.chars().take(500).collect::<String>()
    });

    // Detect context-length errors regardless of HTTP status.
    if crate::context::is_context_length_error(&detail) {
        return ChatError::ContextExceeded;
    }

    match status {
        400 => ChatError::BadRequest { detail },
        401 | 403 => ChatError::Unauthorized { detail },
        429 => ChatError::RateLimited { retry_after_secs: None },
        500 | 502 | 503 | 504 => ChatError::ServerOverloaded,
        _ => ChatError::BadRequest { detail: format!("HTTP {status}: {detail}") },
    }
}

/// Save user-uploaded images to `<cwd>/.clawtao/images/` and return the file paths.
fn save_user_images(session_id: &str, images: &[crate::llm::types::ImageContent]) -> Vec<String> {
    use base64::Engine;
    let dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(".clawtao").join("images");
    let _ = std::fs::create_dir_all(&dir);
    let mut paths = Vec::new();
    for img in images {
        let ext = match img.media_type.as_str() {
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => "png",
        };
        let fname = format!("{}_{}.{ext}", session_id, uuid::Uuid::new_v4());
        let path = dir.join(&fname);
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&img.base64) {
            if std::fs::write(&path, &bytes).is_ok() {
                paths.push(path.to_string_lossy().to_string());
            }
        }
    }
    paths
}

/// Load images from filesystem paths and encode as base64 for the LLM request.
fn load_images_from_paths(paths: &[String]) -> Vec<crate::llm::types::ImageContent> {
    use base64::Engine;
    let mut out = Vec::new();
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            let media_type = match std::path::Path::new(p).extension().and_then(|e| e.to_str()) {
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("gif") => "image/gif",
                Some("webp") => "image/webp",
                _ => "image/png",
            };
            out.push(crate::llm::types::ImageContent {
                base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
                media_type: media_type.to_string(),
            });
        }
    }
    out
}

/// Parse user-uploaded images from the JSON-RPC params.
fn parse_user_images(params: &Option<Value>) -> Option<Vec<crate::llm::types::ImageContent>> {
    let arr = params.as_ref()?.get("images")?.as_array()?;
    let mut out = Vec::new();
    for img in arr {
        let base64 = img.get("base64")?.as_str()?;
        let media_type = img.get("media_type")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");
        out.push(crate::llm::types::ImageContent {
            base64: base64.to_string(),
            media_type: media_type.to_string(),
        });
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Try to extract a human-readable error message from a JSON response body.
fn extract_error_message(body: &str) -> Option<String> {
    let val: Value = serde_json::from_str(body).ok()?;
    let error = val.get("error")?;
    // Prefer `error.message`, fall back to `error.type`
    if let Some(msg) = error.get("message").and_then(|v| v.as_str()) {
        let msg = msg.trim();
        if !msg.is_empty() {
            return Some(msg.to_string());
        }
    }
    if let Some(typ) = error.get("type").and_then(|v| v.as_str()) {
        let typ = typ.trim();
        if !typ.is_empty() {
            return Some(typ.to_string());
        }
    }
    None
}

/// Derive the default workspace directory for a session.
///
/// Uses `CLAWTAO_WORKSPACE_ROOT` env var if set, otherwise falls back to
/// `{data_local_dir}/clawtao/workspaces/{session_id}`.
fn workspace_dir_for_session(session_id: &str) -> String {
    let root = std::env::var("CLAWTAO_WORKSPACE_ROOT").unwrap_or_else(|_| {
        dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("clawtao")
            .join("workspaces")
            .to_string_lossy()
            .to_string()
    });
    std::path::Path::new(&root)
        .join(session_id)
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
#[path = "tests/chat_tests.rs"]
mod tests;
