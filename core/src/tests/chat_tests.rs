use crate::chat::run_state_machine;
use crate::llm::adapter::{ApiAdapter, HttpRequest, StreamEvent};
use crate::llm::types::{LlmRequest, LlmResponse};
use crate::store::{self, store_trait::SessionStore, ToolCall, ToolCallFunction};
use crate::tools::{self, registry::ToolRegistry};
use reqwest::blocking::Client;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// A configurable mock adapter — returns pre-canned responses from parse_stream.
struct MockAdapter {
    responses: Mutex<Vec<LlmResponse>>,
    call_count: Mutex<usize>,
}

impl MockAdapter {
    fn new(responses: Vec<LlmResponse>) -> Self {
        Self { responses: Mutex::new(responses), call_count: Mutex::new(0) }
    }
}

impl ApiAdapter for MockAdapter {
    fn build(&self, _req: &LlmRequest, _api_key: &str, _base_url: &str) -> anyhow::Result<HttpRequest> {
        Ok(HttpRequest { url: "http://mock".into(), headers: vec![], body: "{}".into() })
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

#[test]
fn text_only_turn() {
    let store = make_store();
    let session = store::new_session();
    store.create(&session).unwrap();
    let adapter = MockAdapter::new(vec![text_response("Hello!")]);
    let client = Client::new();
    let cancel = AtomicBool::new(false);
    let tools = make_tool_registry();

    let ctx = super::TurnContext {
        session_id: session.id.clone(),
        run_id: "r1".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock", "gpt-4o", false,
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
    // First LLM call: tool request. Second: text reply.
    let adapter = MockAdapter::new(vec![
        tool_response("", vec![make_tool_call("t1", "Read", r#"{"path":"/x"}"#)]),
        text_response("Read result processed."),
    ]);
    let client = Client::new();
    let cancel = AtomicBool::new(false);
    let tools = make_tool_registry();

    let ctx = super::TurnContext {
        session_id: session.id.clone(),
        run_id: "r2".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, _thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock", "gpt-4o", false,
        vec![],
        &ctx, &tools, &cancel,
    ).unwrap();

    assert_eq!(text, "Read result processed.");

    // Verify tool call + result were persisted.
    let sess = store.get(&session.id).unwrap().unwrap();
    assert!(sess.messages.iter().any(|m| m.role == "tool"));
}

#[test]
fn cancel_after_llm_call_interrupts() {
    let store = make_store();
    let session = store::new_session();
    store.create(&session).unwrap();
    // LLM returns text, but cancel is set — should go to Interrupted.
    let adapter = MockAdapter::new(vec![text_response("partial response")]);
    let client = Client::new();
    let cancel = AtomicBool::new(true); // pre-set
    let tools = make_tool_registry();

    let ctx = super::TurnContext {
        session_id: session.id.clone(),
        run_id: "r3".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, _thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock", "gpt-4o", false,
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
    let read_tool = make_tool_call("t1", "Read", r#"{"path":"/x"}"#);
    let bash_tool = make_tool_call("t2", "Bash", r#"{"command":"echo hi"}"#);
    // LLM returns 2 tool calls. Cancel is pre-set → Evaluating transitions
    // directly to Interrupted, tools never execute.
    let adapter = MockAdapter::new(vec![tool_response("", vec![read_tool, bash_tool])]);
    let client = Client::new();
    let cancel = AtomicBool::new(true);
    let tools = make_tool_registry();

    let ctx = super::TurnContext {
        session_id: session.id.clone(),
        run_id: "r4".into(),
        system_prompt: String::new(),
        tools: vec![],
    };

    let (text, _thinking) = run_state_machine(
        &store, &adapter, &client,
        "sk-test", "http://mock", "gpt-4o", false,
        vec![],
        &ctx, &tools, &cancel,
    ).unwrap();

    // When cancel hits in Evaluating, partial text is preserved, tools are skipped.
    assert_eq!(text, "");

    let sess = store.get(&session.id).unwrap().unwrap();
    let tool_msgs: Vec<_> = sess.messages.iter().filter(|m| m.role == "tool").collect();
    // Tools were never executed because Evaluating → Interrupted skips Executing.
    assert_eq!(tool_msgs.len(), 0);
}
