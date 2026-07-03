use super::{run_state_machine, TurnContext};
use crate::llm::adapter::{ApiAdapter, HttpRequest, StreamEvent};
use crate::llm::types::{LlmRequest, LlmResponse};
use crate::store::{self, store_trait::SessionStore, ToolCall, ToolCallFunction};
use crate::tools::{self, registry::ToolRegistry};
use reqwest::blocking::Client;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// A configurable mock adapter — returns pre-canned responses from parse_stream.
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

fn text_response(text: &str) -> LlmResponse {
    LlmResponse { text: text.into(), tool_calls: vec![], thinking: None }
}

fn tool_response(text: &str, tool_calls: Vec<ToolCall>) -> LlmResponse {
    LlmResponse { text: text.into(), tool_calls, thinking: None }
}

fn make_tool_call(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        call_type: "function".into(),
        function: ToolCallFunction { name: name.into(), arguments: args.into() },
    }
}

fn make_store() -> impl SessionStore {
    use crate::store::json_store::JsonSessionStore;
    let dir = std::env::temp_dir().join(format!("clawtao_test_sm_{}", uuid::Uuid::new_v4()));
    JsonSessionStore::new(dir)
}

fn make_tool_registry() -> ToolRegistry {
    let mut tr = ToolRegistry::new();
    tools::builtin::register_all(&mut tr, vec![], Some(30));
    tr
}

/// Start a tiny HTTP server on a random port that replies 200 OK with an
/// empty body. Returns the URL to connect to. The server handles up to
/// `max_requests` before shutting down.
fn start_mock_http_n(max_requests: usize) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{port}");
    std::thread::spawn(move || {
        for stream in listener.incoming().take(max_requests) {
            if let Ok(mut stream) = stream {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                let _ = stream.flush();
            }
        }
    });
    url
}

/// Start a tiny HTTP server that replies 200 OK for a single request.
fn start_mock_http() -> String {
    start_mock_http_n(1)
}

#[test]
fn text_only_turn() {
    let store = make_store();
    let session = store::new_session();
    store.create(&session).unwrap();
    let url = start_mock_http();
    let adapter = MockAdapter::new(vec![text_response("Hello!")], &url);
    let client = Client::new();
    let cancel = AtomicBool::new(false);
    let tools = make_tool_registry();

    let ctx = TurnContext {
        session_id: session.id.clone(),
        run_id: "r1".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock-base", "gpt-4o", false,
        vec![],
        &ctx, &tools, &cancel,
    ).unwrap();

    assert_eq!(text, "Hello!");
    assert!(thinking.is_none());
}

#[test]
fn tool_call_then_text() {
    let store = make_store();
    let session = store::new_session();
    store.create(&session).unwrap();
    let url = start_mock_http_n(2);  // two LLM calls: tool + text
    let adapter = MockAdapter::new(vec![
        tool_response("", vec![make_tool_call("t1", "Read", r#"{"path":"/x"}"#)]),
        text_response("Read result processed."),
    ], &url);
    let client = Client::new();
    let cancel = AtomicBool::new(false);
    let tools = make_tool_registry();

    let ctx = TurnContext {
        session_id: session.id.clone(),
        run_id: "r2".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, _thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock-base", "gpt-4o", false,
        vec![],
        &ctx, &tools, &cancel,
    ).unwrap();

    assert_eq!(text, "Read result processed.");

    let sess = store.get(&session.id).unwrap().unwrap();
    assert!(sess.messages.iter().any(|m| m.role == "tool"));
}

#[test]
fn cancel_after_llm_call_interrupts() {
    let store = make_store();
    let session = store::new_session();
    store.create(&session).unwrap();
    let url = start_mock_http();
    let adapter = MockAdapter::new(vec![text_response("partial response")], &url);
    let client = Client::new();
    let cancel = AtomicBool::new(true); // pre-set
    let tools = make_tool_registry();

    let ctx = TurnContext {
        session_id: session.id.clone(),
        run_id: "r3".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, _thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock-base", "gpt-4o", false,
        vec![],
        &ctx, &tools, &cancel,
    ).unwrap();

    assert_eq!(text, "partial response");
}

#[test]
fn cancel_during_tool_execution_marks_remaining() {
    let store = make_store();
    let session = store::new_session();
    store.create(&session).unwrap();
    let url = start_mock_http();
    let read_tool = make_tool_call("t1", "Read", r#"{"path":"/x"}"#);
    let bash_tool = make_tool_call("t2", "Bash", r#"{"command":"echo hi"}"#);
    let adapter = MockAdapter::new(vec![tool_response("", vec![read_tool, bash_tool])], &url);
    let client = Client::new();
    let cancel = AtomicBool::new(true);
    let tools = make_tool_registry();

    let ctx = TurnContext {
        session_id: session.id.clone(),
        run_id: "r4".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, _thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock-base", "gpt-4o", false,
        vec![],
        &ctx, &tools, &cancel,
    ).unwrap();

    assert_eq!(text, "");

    let sess = store.get(&session.id).unwrap().unwrap();
    let tool_msgs: Vec<_> = sess.messages.iter().filter(|m| m.role == "tool").collect();
    assert_eq!(tool_msgs.len(), 0);
}

#[test]
fn llm_http_5xx_maps_to_server_overloaded() {
    // Start a server that returns 503.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{port}");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let _ = stream.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                let _ = stream.flush();
            }
            break;
        }
    });

    let store = make_store();
    let session = store::new_session();
    store.create(&session).unwrap();
    let adapter = MockAdapter::new(vec![text_response("should not reach")], &url);
    let client = Client::new();
    let cancel = AtomicBool::new(false);
    let tools = make_tool_registry();

    let ctx = TurnContext {
        session_id: session.id.clone(),
        run_id: "r5".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let result = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock-base", "gpt-4o", false,
        vec![],
        &ctx, &tools, &cancel,
    );
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("capacity") || err_msg.contains("ServerOverloaded"),
        "expected server overloaded message, got: {err_msg}");
}

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

