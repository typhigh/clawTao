/// Mock-backed tests for `OpenAiAdapter::build` and `parse_stream`.
///
/// `build()` is tested by constructing `LlmRequest` values and verifying
/// the HTTP request body, URL, and headers.
///
/// `parse_stream()` delegates to `crate::sse::parse_sse_response`,
/// whose full coverage lives in `sse_tests.rs`.  We include a smoke
/// test here to confirm the delegation works end-to-end.
use crate::llm::adapter::ApiAdapter;
use crate::llm::openai::OpenAiAdapter;
use crate::llm::types::{LlmMessage, LlmRequest, UnifiedTool};
use crate::store::{ToolCall, ToolCallFunction};
use serde_json::json;

/// Helper: build a minimal `LlmRequest` with one user message.
fn basic_request() -> LlmRequest {
    LlmRequest {
        system: "You are helpful.".into(),
        model: "gpt-4o".into(),
        messages: vec![LlmMessage {
            role: "user".into(),
            content: "hello".into(),
            tool_calls: None,
            tool_call_id: None,
            thinking: None,
        }],
        tools: vec![],
        thinking_enabled: false,
    }
}

/// Helper: a typical `UnifiedTool` for tests.
fn read_tool() -> UnifiedTool {
    UnifiedTool {
        name: "Read".into(),
        description: "Read a file".into(),
        parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
    }
}

// ── build() ──────────────────────────────────────────────────────────────

#[test]
fn build_basic_request() {
    let adapter = OpenAiAdapter;
    let http = adapter
        .build(&basic_request(), "sk-test", "https://api.openai.com/v1")
        .expect("build should succeed");

    assert_eq!(http.url, "https://api.openai.com/v1/chat/completions");
    assert!(http.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer sk-test"));
    assert!(http.headers.iter().any(|(k, v)| k == "Content-Type" && v == "application/json"));

    let body: serde_json::Value =
        serde_json::from_str(&http.body).expect("body should be valid JSON");
    assert_eq!(body["model"], "gpt-4o");
    assert_eq!(body["stream"], true);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "You are helpful.");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "hello");
}

#[test]
fn build_url_trims_trailing_slash() {
    let adapter = OpenAiAdapter;
    let http = adapter
        .build(&basic_request(), "sk-test", "https://api.openai.com/v1/")
        .expect("build should succeed");
    assert_eq!(http.url, "https://api.openai.com/v1/chat/completions");
}

#[test]
fn build_maps_tool_role() {
    let adapter = OpenAiAdapter;
    let req = LlmRequest {
        messages: vec![LlmMessage {
            role: "tool".into(),
            content: "file contents here".into(),
            tool_calls: None,
            tool_call_id: Some("call_abc".into()),
            thinking: None,
        }],
        ..basic_request()
    };

    let http = adapter.build(&req, "sk-test", "https://api.openai.com/v1").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();

    let tool_msg = &body["messages"][1];
    assert_eq!(tool_msg["role"], "tool");
    assert_eq!(tool_msg["tool_call_id"], "call_abc");
    assert_eq!(tool_msg["content"], "file contents here");
}

#[test]
fn build_maps_assistant_with_tool_calls() {
    let adapter = OpenAiAdapter;
    let req = LlmRequest {
        messages: vec![LlmMessage {
            role: "assistant".into(),
            content: "Let me check".into(),
            tool_calls: Some(vec![ToolCall {
                id: "tc1".into(),
                call_type: "function".into(),
                function: ToolCallFunction {
                    name: "Read".into(),
                    arguments: r#"{"path": "/tmp/a.txt"}"#.into(),
                },
            }]),
            tool_call_id: None,
            thinking: None,
        }],
        ..basic_request()
    };

    let http = adapter.build(&req, "sk-test", "https://api.openai.com/v1").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();

    let asst_msg = &body["messages"][1];
    assert_eq!(asst_msg["role"], "assistant");
    assert_eq!(asst_msg["content"], serde_json::Value::Null);
    assert_eq!(asst_msg["tool_calls"][0]["id"], "tc1");
    assert_eq!(asst_msg["tool_calls"][0]["function"]["name"], "Read");
}

#[test]
fn build_includes_tools() {
    let adapter = OpenAiAdapter;
    let req = LlmRequest {
        tools: vec![read_tool()],
        ..basic_request()
    };

    let http = adapter.build(&req, "sk-test", "https://api.openai.com/v1").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();

    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "Read");
    assert_eq!(body["tools"][0]["function"]["description"], "Read a file");
    assert!(body["tools"][0]["function"]["parameters"].is_object());
}

#[test]
fn build_empty_tools() {
    let adapter = OpenAiAdapter;
    let http = adapter
        .build(&basic_request(), "sk-test", "https://api.openai.com/v1")
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();
    assert_eq!(body["tools"].as_array().unwrap().len(), 0);
}

#[test]
fn build_empty_system_prompt_is_still_present() {
    let adapter = OpenAiAdapter;
    let req = LlmRequest {
        system: String::new(),
        ..basic_request()
    };
    let http = adapter.build(&req, "sk-test", "https://api.openai.com/v1").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();
    // System message is always included, even if empty
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "");
}

// ── parse_stream() — smoke tests (delegates to sse::parse_sse_response) ─

#[test]
fn parse_stream_text_only() {
    let adapter = OpenAiAdapter;
    let body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"}}]}\n\
                data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" there\"}}]}\n\
                data: [DONE]\n";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.text, "Hi there");
    assert!(resp.tool_calls.is_empty());
}

#[test]
fn parse_stream_with_tool_call() {
    let adapter = OpenAiAdapter;
    let body = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"c1","type":"function","function":{"name":"Read","arguments":"{}"},"index":0}]}}]}"#;
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "c1");
    assert_eq!(resp.tool_calls[0].function.name, "Read");
}

#[test]
fn parse_stream_empty_body() {
    let adapter = OpenAiAdapter;
    let resp = adapter.parse_stream("").expect("parse should succeed");
    assert!(resp.text.is_empty());
    assert!(resp.tool_calls.is_empty());
}
