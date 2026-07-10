//! State machine unit tests.
//!
//! Tests cover every state transition, retry behaviour, and edge case.
//! The state machine calls `llm_step` which makes real HTTP requests.
//! We control the HTTP layer with tiny local servers ("sequence servers")
//! that return configurable status codes, and a `MockAdapter` whose
//! `parse_stream` returns pre-canned `LlmResponse` values.

use super::{run_state_machine, TurnContext};
use crate::llm::adapter::{ApiAdapter, HttpRequest, StreamEvent};
use crate::llm::types::{LlmRequest, LlmResponse};
use crate::store::{self, store_trait::SessionStore, ToolCall, ToolCallFunction};
use crate::tools::{self, registry::ToolRegistry};
use reqwest::blocking::Client;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

// ── Test infrastructure ───────────────────────────────────────────────

struct MockAdapter {
    responses: Mutex<Vec<LlmResponse>>,
    call_count: Mutex<usize>,
    url: String,
}

impl MockAdapter {
    fn new(responses: Vec<LlmResponse>, url: &str) -> Self {
        Self { responses: Mutex::new(responses), call_count: Mutex::new(0), url: url.to_string() }
    }
}

impl ApiAdapter for MockAdapter {
    fn build(&self, _req: &LlmRequest, _api_key: &str, _base_url: &str) -> anyhow::Result<HttpRequest> {
        Ok(HttpRequest { url: self.url.clone(), headers: vec![], body: "{}".into() })
    }
    fn parse_stream(&self, _body: &str) -> anyhow::Result<LlmResponse> {
        let mut count = self.call_count.lock().unwrap();
        let i = *count;
        *count += 1;
        Ok(self.responses.lock().unwrap()[i].clone())
    }
    fn stream_events(&self, _event: &serde_json::Value) -> Vec<StreamEvent> { vec![] }
}

fn text(text: &str) -> LlmResponse {
    LlmResponse { text: text.into(), tool_calls: vec![], thinking: None }
}

fn tool(text: &str, calls: Vec<ToolCall>) -> LlmResponse {
    LlmResponse { text: text.into(), tool_calls: calls, thinking: None }
}

fn tc(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.into(), call_type: "function".into(),
        function: ToolCallFunction { name: name.into(), arguments: args.into() },
    }
}

fn store() -> impl SessionStore {
    use crate::store::json_store::JsonSessionStore;
    JsonSessionStore::new(
        std::env::temp_dir().join(format!("clawtao_test_{}", uuid::Uuid::new_v4())),
    )
}

fn tools() -> ToolRegistry {
    let mut tr = ToolRegistry::new();
    tools::builtin::register_all(&mut tr, tools::builtin::SandboxConfig::off(), Some(30));
    tr
}

fn ctx(sid: &str, rid: &str) -> TurnContext {
    TurnContext { session_id: sid.into(), run_id: rid.into(), system_prompt: String::new(), tools: vec![], user_images: None, sandbox_rules: crate::tools::builtin::SandboxRules::off() }
}

/// Start an HTTP server that returns a **sequence** of status lines.
/// Each connection consumes the next status; the last repeats forever
/// so the server never closes and tests can run in parallel.
fn sequence_server(statuses: &[&str]) -> String {
    let list: Vec<String> = statuses.iter().map(|s| s.to_string()).collect();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{port}");
    std::thread::spawn(move || {
        let mut idx = 0usize;
        for stream in listener.incoming() {
            if let Ok(mut s) = stream {
                let status = &list[idx.min(list.len().saturating_sub(1))];
                let _ = s.write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes(),
                );
                let _ = s.flush();
                idx = idx.saturating_add(1);
            }
        }
    });
    url
}

/// Shorthand: server that always returns the same status (infinite connections).
fn repeat_server(status: &str) -> String {
    sequence_server(&[status])
}

fn run(
    store: &dyn SessionStore,
    adapter: &dyn ApiAdapter,
    cancel: &AtomicBool,
    sid: &str,
    rid: &str,
) -> Result<(String, Option<String>), anyhow::Error> {
    run_state_machine(
        store, adapter, &Client::new(),
        "sk-test", "http://mock-base", "gpt-4o", false,
        &ctx(sid, rid), &tools(), cancel, None,
    )
}

/// Like `run` but collects every notification sent by the state machine.
fn run_collect(
    store: &dyn SessionStore,
    adapter: &dyn ApiAdapter,
    cancel: &AtomicBool,
    sid: &str,
    rid: &str,
) -> (Result<(String, Option<String>), anyhow::Error>, Vec<crate::jsonrpc::Notification>) {
    let mut notifications = Vec::new();
    let r = run_state_machine(
        store, adapter, &Client::new(),
        "sk-test", "http://mock-base", "gpt-4o", false,
        &ctx(sid, rid), &tools(), cancel,
        Some(&mut notifications),
    );
    (r, notifications)
}

// ── Waiting → Done ────────────────────────────────────────────────────

#[test]
fn waiting_to_done_text_only() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("200 OK");
    let a = MockAdapter::new(vec![text("hello")], &url);
    let (out, _) = run(&s, &a, &AtomicBool::new(false), &sess.id, "r1").unwrap();
    assert_eq!(out, "hello");
}

// ── Waiting → Tooling → Waiting → Done ───────────────────────────────

#[test]
fn full_cycle_tool_then_text() {
    let s = store();
    let sess = s.create_session_for_test();
    // 2 LLM calls: tool request + text follow-up
    let url = repeat_server("200 OK");
    let a = MockAdapter::new(
        vec![tool("", vec![tc("t1", "Read", r#"{"path":"/x"}"#)]), text("done")],
        &url,
    );
    let (out, _) = run(&s, &a, &AtomicBool::new(false), &sess.id, "r2").unwrap();
    assert_eq!(out, "done");
    assert!(s.get(&sess.id).unwrap().unwrap().messages.iter().any(|m| m.role == "tool"));
}

// ── Waiting → Interrupted ─────────────────────────────────────────────

#[test]
fn cancel_before_tools_goes_to_interrupted() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("200 OK");
    let cancel = AtomicBool::new(true);
    let (out, _) = run(&s, &MockAdapter::new(vec![text("partial")], &url), &cancel, &sess.id, "r3").unwrap();
    assert_eq!(out, "partial");
}

#[test]
fn cancel_during_tools_skips_remaining() {
    let s = store();
    let sess = s.create_session_for_test();
    // Enough connections for possible retries from port-reuse on macOS.
    let url = repeat_server("200 OK");
    let cancel = AtomicBool::new(true);
    let a = MockAdapter::new(
        vec![tool("", vec![tc("a", "Read", r#"{"path":"/x"}"#), tc("b", "Bash", r#"{"command":"echo"}"#)])],
        &url,
    );
    // Cancel before tools → Interrupted, tools never run.
    let (out, _) = run(&s, &a, &cancel, &sess.id, "r4").unwrap();
    assert_eq!(out, "");
    let msgs = &s.get(&sess.id).unwrap().unwrap().messages;
    assert_eq!(msgs.iter().filter(|m| m.role == "tool").count(), 0);
}

// ── Waiting → Error → Waiting → Done  (retry recovers) ───────────────

#[test]
fn retryable_error_recovers_on_retry() {
    let s = store();
    let sess = s.create_session_for_test();
    // One 503, then 200.
    let url = sequence_server(&["503 Service Unavailable", "200 OK"]);
    let (out, _) = run(&s, &MockAdapter::new(vec![text("recovered")], &url), &AtomicBool::new(false), &sess.id, "r5").unwrap();
    assert_eq!(out, "recovered");
}

#[test]
fn retry_recovers_after_two_failures() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = sequence_server(&["503 Service Unavailable", "503 Service Unavailable", "200 OK"]);
    let (out, _) = run(&s, &MockAdapter::new(vec![text("ok")], &url), &AtomicBool::new(false), &sess.id, "r6").unwrap();
    assert_eq!(out, "ok");
}

// ── Waiting → Error × 3 → Fatal  (retries exhausted) ─────────────────

#[test]
fn retries_exhausted_becomes_fatal() {
    let s = store();
    let sess = s.create_session_for_test();
    // 4 × 503: initial + 3 retries
    let url = repeat_server("503 Service Unavailable");
    let r = run(&s, &MockAdapter::new(vec![text("nope")], &url), &AtomicBool::new(false), &sess.id, "r7");
    assert!(r.is_err());
    assert!(format!("{}", r.unwrap_err()).contains("capacity"));
}

// ── Waiting → Fatal  (non-retryable, no backoff) ──────────────────────

#[test]
fn non_retryable_skips_retries() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("401 Unauthorized");
    let r = run(&s, &MockAdapter::new(vec![text("x")], &url), &AtomicBool::new(false), &sess.id, "r8");
    assert!(r.is_err());
    let msg = format!("{}", r.unwrap_err());
    assert!(msg.contains("API key"), "expected UNAUTHORIZED, got: {msg}");
}

#[test]
fn bad_request_is_fatal() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("400 Bad Request");
    let r = run(&s, &MockAdapter::new(vec![text("x")], &url), &AtomicBool::new(false), &sess.id, "r9");
    assert!(r.is_err());
}

// ── Retry-counter resets after a successful call ──────────────────────

#[test]
fn retry_counter_resets_after_success() {
    // Fail once → recover → tool → fail again → gets fresh 3 retries.
    let s = store();
    let sess = s.create_session_for_test();
    // Sequence: 503 → retry → 200 (tool) → 503 → retry → 503 → retry → 200
    let url = sequence_server(&[
        "503 Service Unavailable",  // attempt 1
        "200 OK",                    // attempt 2 (retry) → tool
        "503 Service Unavailable",  // attempt 1 (reset)
        "200 OK",                    // attempt 2 (retry) → text
    ]);
    let a = MockAdapter::new(
        vec![tool("", vec![tc("t1", "Read", r#"{"path":"/x"}"#)]), text("finally")],
        &url,
    );
    let (out, _) = run(&s, &a, &AtomicBool::new(false), &sess.id, "r10").unwrap();
    assert_eq!(out, "finally");
}

// ── Notification content verification ─────────────────────────────────

#[test]
fn retry_sends_stream_error_notifications_with_correct_fields() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = sequence_server(&[
        "503 Service Unavailable",
        "503 Service Unavailable",
        "200 OK",
    ]);
    let (result, notes) = run_collect(
        &s, &MockAdapter::new(vec![text("ok2")], &url),
        &AtomicBool::new(false), &sess.id, "r11",
    );
    let (out, _) = result.unwrap();
    assert_eq!(out, "ok2");

    // Extract stream_error notifications.
    let errors: Vec<_> = notes.iter().filter(|n| {
        n.params.as_ref().and_then(|p| p.get("kind")).and_then(|v| v.as_str()) == Some("stream_error")
    }).collect();
    assert_eq!(errors.len(), 2, "expected 2 stream_error notifications, got {errors:?}");

    // First error: "Reconnecting... 1/3"
    let p1 = errors[0].params.as_ref().unwrap();
    assert_eq!(p1["errorCode"], "SERVER_OVERLOADED");
    assert_eq!(p1["retryable"], true);
    assert!(p1["message"].as_str().unwrap().contains("1/3"));

    // Second error: "Reconnecting... 2/3"
    let p2 = errors[1].params.as_ref().unwrap();
    assert!(p2["message"].as_str().unwrap().contains("2/3"));
}

#[test]
fn normal_turn_sends_tool_call_and_result_notifications() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("200 OK");
    let (result, notes) = run_collect(
        &s,
        &MockAdapter::new(
            vec![tool("", vec![tc("t1", "Bash", r#"{"command":"echo hi"}"#)]), text("done")],
            &url,
        ),
        &AtomicBool::new(false), &sess.id, "r12",
    );
    let (out, _) = result.unwrap();
    assert_eq!(out, "done");

    let kinds: Vec<_> = notes.iter().filter_map(|n| {
        n.params.as_ref().and_then(|p| p.get("kind")).and_then(|v| v.as_str())
    }).collect();
    assert!(kinds.contains(&"tool_call"), "expected tool_call, got {kinds:?}");
    assert!(kinds.contains(&"tool_result"), "expected tool_result, got {kinds:?}");
}

#[test]
fn normal_turn_no_tools_sends_no_tool_notifications() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("200 OK");
    let (result, notes) = run_collect(
        &s, &MockAdapter::new(vec![text("hello")], &url),
        &AtomicBool::new(false), &sess.id, "r13",
    );
    let (out, _) = result.unwrap();
    assert_eq!(out, "hello");
    let kinds: Vec<_> = notes.iter().filter_map(|n| {
        n.params.as_ref().and_then(|p| p.get("kind")).and_then(|v| v.as_str())
    }).collect();
    assert!(!kinds.contains(&"tool_call"));
    assert!(!kinds.contains(&"tool_result"));
    assert!(!kinds.contains(&"stream_error"));
}

#[test]
fn fatal_error_sends_no_stream_error_notification() {
    // Non-retryable errors go directly to Fatal — no stream_error events.
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("401 Unauthorized");
    let (r, notes) = run_collect(
        &s, &MockAdapter::new(vec![text("x")], &url),
        &AtomicBool::new(false), &sess.id, "r14",
    );
    assert!(r.is_err());
    let has_stream_error = notes.iter().any(|n| {
        n.params.as_ref().and_then(|p| p.get("kind")).and_then(|v| v.as_str()) == Some("stream_error")
    });
    assert!(!has_stream_error, "Fatal should not emit stream_error");
}

// ── Max retries = 3: boundary test ────────────────────────────────────

#[test]
fn exactly_three_retries_then_fatal() {
    let s = store();
    let sess = s.create_session_for_test();
    // 4 attempts total: 1 initial + 3 retries. All must fail.
    let url = repeat_server("503 Service Unavailable");
    let r = run(&s, &MockAdapter::new(vec![text("x")], &url), &AtomicBool::new(false), &sess.id, "r12");
    assert!(r.is_err());
}

#[test]
fn succeeds_on_last_retry_attempt() {
    // 3rd retry succeeds (4th attempt total).
    let s = store();
    let sess = s.create_session_for_test();
    let url = sequence_server(&[
        "503 Service Unavailable",
        "503 Service Unavailable",
        "503 Service Unavailable",
        "200 OK",
    ]);
    let (out, _) = run(&s, &MockAdapter::new(vec![text("last chance")], &url), &AtomicBool::new(false), &sess.id, "r13").unwrap();
    assert_eq!(out, "last chance");
}

// ── Tooling state edge cases ──────────────────────────────────────────

#[test]
fn tooling_multiple_tools_executed_sequentially() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = repeat_server("200 OK");
    let a = MockAdapter::new(
        vec![
            tool("", vec![
                tc("a", "Bash", r#"{"command":"echo one"}"#),
                tc("b", "Bash", r#"{"command":"echo two"}"#),
            ]),
            text("all done"),
        ],
        &url,
    );
    let (out, _) = run(&s, &a, &AtomicBool::new(false), &sess.id, "r14").unwrap();
    assert_eq!(out, "all done");
    let msgs = &s.get(&sess.id).unwrap().unwrap().messages;
    let tool_count = msgs.iter().filter(|m| m.role == "tool").count();
    assert_eq!(tool_count, 2);
}

// ── HTTP status mapping under state machine ───────────────────────────

#[test]
fn rate_limited_is_retryable_from_waiting() {
    // 429 is retryable → should go Error → Waiting, not Fatal.
    let s = store();
    let sess = s.create_session_for_test();
    let url = sequence_server(&["429 Too Many Requests", "200 OK"]);
    let (out, _) = run(&s, &MockAdapter::new(vec![text("ok429")], &url), &AtomicBool::new(false), &sess.id, "r15").unwrap();
    assert_eq!(out, "ok429");
}

#[test]
fn server_overloaded_is_retryable() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = sequence_server(&["500 Internal Server Error", "200 OK"]);
    let (out, _) = run(&s, &MockAdapter::new(vec![text("ok500")], &url), &AtomicBool::new(false), &sess.id, "r16").unwrap();
    assert_eq!(out, "ok500");
}

#[test]
fn http_502_is_treated_as_overloaded() {
    let s = store();
    let sess = s.create_session_for_test();
    let url = sequence_server(&["502 Bad Gateway", "200 OK"]);
    let (out, _) = run(&s, &MockAdapter::new(vec![text("ok502")], &url), &AtomicBool::new(false), &sess.id, "r17").unwrap();
    assert_eq!(out, "ok502");
}

#[test]
fn config_error_no_api_key_is_fatal() {
    // This goes through run_turn's config check, not the state machine.
    // Already tested indirectly — no API key → Config error → Fatal.
    // We test the HTTP mapping helper directly here.
    let err = super::http_error_from_status(401, r#"{"error":{"message":"Invalid API key"}}"#);
    assert_eq!(err.code(), "UNAUTHORIZED");
    assert!(!err.is_retryable());
}

// ── Store helpers ─────────────────────────────────────────────────────

/// Extension trait so tests can create a session without the full
/// `run_turn` entry-point.
trait TestStore {
    fn create_session_for_test(&self) -> store::Session;
}

impl<T: SessionStore> TestStore for T {
    fn create_session_for_test(&self) -> store::Session {
        let sess = store::Session {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            updated_at: chrono::Utc::now().timestamp_millis(),
            messages: vec![],
            title: String::new(),
            compacted_summary: None,
            compacted_message_id: None,
        };
        self.create(&sess).unwrap();
        sess
    }
}

// ── HTTP error mapping helpers (unit tests, no state machine) ────────

#[test]
fn http_error_401_maps_to_unauthorized() {
    let err = super::http_error_from_status(401, r#"{"error":{"message":"Invalid API key"}}"#);
    assert_eq!(err.code(), "UNAUTHORIZED");
    assert!(!err.is_retryable());
    assert!(err.user_message().contains("API key"));
}

#[test]
fn http_error_429_maps_to_rate_limited() {
    let err = super::http_error_from_status(429, "");
    assert_eq!(err.code(), "RATE_LIMITED");
    assert!(err.is_retryable());
}

#[test]
fn http_error_503_maps_to_server_overloaded() {
    let err = super::http_error_from_status(503, "");
    assert_eq!(err.code(), "SERVER_OVERLOADED");
    assert!(err.is_retryable());
}

#[test]
fn http_error_extracts_message_from_json_body() {
    let err = super::http_error_from_status(400, r#"{"error":{"message":"Model not found"}}"#);
    assert!(err.user_message().contains("Model not found"));
}

// ── TodoWrite tool executor ────────────────────────────────────────────

#[test]
fn todo_write_valid_input_returns_ok() {
    use crate::tools::builtin::TodoWriteTool;
    use crate::tools::executor::ToolExecutor;
    let tool = TodoWriteTool;
    let input = serde_json::json!({"todos": [{"step": "do A", "status": "pending"}]});
    let result = tool.execute(input, &AtomicBool::new(false));
    assert_eq!(result.unwrap(), "ok");
}

#[test]
fn todo_write_spec_requires_todos_field() {
    use crate::tools::builtin::TodoWriteTool;
    use crate::tools::executor::ToolExecutor;
    let tool = TodoWriteTool;
    let spec = tool.spec();
    let params = &spec.function.parameters;
    assert!(params["required"].as_array().unwrap().iter().any(|v| v == "todos"));
    assert!(params["properties"]["todos"]["type"] == "array");
}

#[test]
fn todo_write_spec_items_have_step_and_status() {
    use crate::tools::builtin::TodoWriteTool;
    use crate::tools::executor::ToolExecutor;
    let tool = TodoWriteTool;
    let spec = tool.spec();
    let item = &spec.function.parameters["properties"]["todos"]["items"];
    let req: Vec<_> = item["required"].as_array().unwrap()
        .iter().map(|v| v.as_str().unwrap()).collect();
    assert!(req.contains(&"step"));
    assert!(req.contains(&"status"));
    let enums: Vec<_> = item["properties"]["status"]["enum"].as_array().unwrap()
        .iter().map(|v| v.as_str().unwrap()).collect();
    assert!(enums.contains(&"pending"));
    assert!(enums.contains(&"in_progress"));
    assert!(enums.contains(&"completed"));
}

// ── WebBrowser tool executor ──────────────────────────────────────────
// Note: execute() depends on a running browser-server, which isn't
// available in all environments. Spec-level tests are deterministic.

#[test]
fn web_browser_spec_requires_action_field() {
    use crate::tools::builtin::WebBrowserTool;
    use crate::tools::executor::ToolExecutor;
    let tool = WebBrowserTool;
    let spec = tool.spec();
    let required = spec.function.parameters["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "action"));
}

#[test]
fn web_browser_spec_has_all_actions() {
    use crate::tools::builtin::WebBrowserTool;
    use crate::tools::executor::ToolExecutor;
    let tool = WebBrowserTool;
    let spec = tool.spec();
    let actions: Vec<_> = spec.function.parameters["properties"]["action"]["enum"]
        .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(actions.contains(&"start"));
    assert!(actions.contains(&"stop"));
    assert!(actions.contains(&"navigate"));
    assert!(actions.contains(&"search"));
    assert!(actions.contains(&"snapshot"));
    assert!(actions.contains(&"screenshot"));
    assert!(actions.contains(&"click"));
    assert!(actions.contains(&"type"));
    assert!(actions.contains(&"tabs"));
    assert!(actions.contains(&"newTab"));
}

#[test]
fn web_browser_name_matches_registry_key() {
    use crate::tools::builtin::WebBrowserTool;
    use crate::tools::executor::ToolExecutor;
    let tool = WebBrowserTool;
    assert_eq!(tool.name(), "WebBrowser");
}

#[test]
fn todo_write_name_matches_registry_key() {
    use crate::tools::builtin::TodoWriteTool;
    use crate::tools::executor::ToolExecutor;
    let tool = TodoWriteTool;
    assert_eq!(tool.name(), "TodoWrite");
}
