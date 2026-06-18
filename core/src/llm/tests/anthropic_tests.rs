/// Mock-backed tests for `AnthropicAdapter::build` and `parse_stream`.
///
/// `build()` is tested by constructing `LlmRequest` values and inspecting
/// the HTTP request body, URL, and headers.
///
/// `parse_stream()` is tested with raw SSE strings covering every branch:
/// text deltas, tool_use streams (full input + incremental `partial_json`),
/// content_block_stop, error events, parallel tools, and edge cases.
use crate::llm::adapter::ApiAdapter;
use crate::llm::anthropic::AnthropicAdapter;
use crate::llm::types::{LlmMessage, LlmRequest, UnifiedTool};
use crate::store::{ToolCall, ToolCallFunction};
use serde_json::json;

/// Helper: minimal `LlmRequest` with one user message and no tools.
fn basic_request() -> LlmRequest {
    LlmRequest {
        system: "You are helpful.".into(),
        model: "claude-sonnet-4-6".into(),
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

fn read_tool() -> UnifiedTool {
    UnifiedTool {
        name: "Read".into(),
        description: "Read a file".into(),
        parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
    }
}

fn bash_tool() -> UnifiedTool {
    UnifiedTool {
        name: "Bash".into(),
        description: "Run a command".into(),
        parameters: json!({"type": "object", "properties": {"cmd": {"type": "string"}}, "required": ["cmd"]}),
    }
}

// ── build() ──────────────────────────────────────────────────────────────

#[test]
fn build_basic_user_message() {
    let adapter = AnthropicAdapter;
    let http = adapter
        .build(&basic_request(), "sk-ant-test", "https://api.anthropic.com")
        .expect("build should succeed");

    assert_eq!(http.url, "https://api.anthropic.com/v1/messages");

    // Headers
    assert!(http.headers.iter().any(|(k, v)| k == "x-api-key" && v == "sk-ant-test"));
    assert!(http.headers.iter().any(|(k, v)| k == "anthropic-version" && v == "2023-06-01"));

    let body: serde_json::Value =
        serde_json::from_str(&http.body).expect("body should be valid JSON");

    assert_eq!(body["model"], "claude-sonnet-4-6");
    assert_eq!(body["max_tokens"], 4096);
    assert_eq!(body["stream"], true);
    assert_eq!(body["system"], "You are helpful.");

    // Messages use Anthropic block format
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    assert_eq!(body["messages"][0]["content"][0]["text"], "hello");
}

#[test]
fn build_empty_system_omitted_from_body() {
    let adapter = AnthropicAdapter;
    let req = LlmRequest {
        system: String::new(),
        ..basic_request()
    };
    let http = adapter.build(&req, "sk-ant-test", "https://api.anthropic.com").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();
    // system key should be absent (not "" or null)
    assert!(body.get("system").is_none());
}

#[test]
fn build_url_trims_trailing_slash() {
    let adapter = AnthropicAdapter;
    let http = adapter
        .build(&basic_request(), "sk-ant-test", "https://api.anthropic.com/")
        .expect("build should succeed");
    assert_eq!(http.url, "https://api.anthropic.com/v1/messages");
}

#[test]
fn build_assistant_tool_calls_to_tool_use_blocks() {
    let adapter = AnthropicAdapter;
    let req = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "user".into(),
                content: "read /tmp/x".into(),
                tool_calls: None,
                tool_call_id: None,
                thinking: None,
            },
            LlmMessage {
                role: "assistant".into(),
                content: "I'll read that file".into(),
                tool_calls: Some(vec![ToolCall {
                    id: "toolu_01".into(),
                    call_type: "function".into(),
                    function: ToolCallFunction {
                        name: "Read".into(),
                        arguments: r#"{"path": "/tmp/x"}"#.into(),
                    },
                }]),
                tool_call_id: None,
                thinking: None,
            },
        ],
        ..basic_request()
    };

    let http = adapter.build(&req, "sk-ant-test", "https://api.anthropic.com").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();

    let asst = &body["messages"][1];
    assert_eq!(asst["role"], "assistant");
    // First block = text that preceded the tool_use
    assert_eq!(asst["content"][0]["type"], "text");
    assert_eq!(asst["content"][0]["text"], "I'll read that file");
    // Second block = tool_use
    assert_eq!(asst["content"][1]["type"], "tool_use");
    assert_eq!(asst["content"][1]["id"], "toolu_01");
    assert_eq!(asst["content"][1]["name"], "Read");
    assert_eq!(asst["content"][1]["input"]["path"], "/tmp/x");
}

#[test]
fn build_tool_result_to_tool_result_block() {
    let adapter = AnthropicAdapter;
    let req = LlmRequest {
        messages: vec![LlmMessage {
            role: "tool".into(),
            content: "file contents".into(),
            tool_calls: None,
            tool_call_id: Some("toolu_02".into()),
            thinking: None,
        }],
        ..basic_request()
    };

    let http = adapter.build(&req, "sk-ant-test", "https://api.anthropic.com").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();

    let tool_msg = &body["messages"][0];
    assert_eq!(tool_msg["role"], "user");
    assert_eq!(tool_msg["content"][0]["type"], "tool_result");
    assert_eq!(tool_msg["content"][0]["tool_use_id"], "toolu_02");
    assert_eq!(tool_msg["content"][0]["content"], "file contents");
}

#[test]
fn build_multiple_parallel_tool_calls() {
    let adapter = AnthropicAdapter;
    let req = LlmRequest {
        messages: vec![LlmMessage {
            role: "assistant".into(),
            content: "".into(),
            tool_calls: Some(vec![
                ToolCall {
                    id: "t1".into(),
                    call_type: "function".into(),
                    function: ToolCallFunction { name: "Read".into(), arguments: r#"{"path":"a"}"#.into() },
                },
                ToolCall {
                    id: "t2".into(),
                    call_type: "function".into(),
                    function: ToolCallFunction { name: "Bash".into(), arguments: r#"{"cmd":"ls"}"#.into() },
                },
            ]),
            tool_call_id: None,
            thinking: None,
        }],
        ..basic_request()
    };

    let http = adapter.build(&req, "sk-ant-test", "https://api.anthropic.com").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();

    let asst = &body["messages"][0];
    assert_eq!(asst["content"][0]["type"], "tool_use");
    assert_eq!(asst["content"][0]["id"], "t1");
    assert_eq!(asst["content"][1]["type"], "tool_use");
    assert_eq!(asst["content"][1]["id"], "t2");
}

#[test]
fn build_tools_with_input_schema() {
    let adapter = AnthropicAdapter;
    let req = LlmRequest {
        tools: vec![read_tool(), bash_tool()],
        ..basic_request()
    };

    let http = adapter.build(&req, "sk-ant-test", "https://api.anthropic.com").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();

    assert_eq!(body["tools"][0]["name"], "Read");
    assert_eq!(body["tools"][0]["description"], "Read a file");
    assert!(body["tools"][0]["input_schema"].is_object());
    assert_eq!(body["tools"][1]["name"], "Bash");
}

#[test]
fn build_no_tools_key_when_empty() {
    let adapter = AnthropicAdapter;
    let http = adapter
        .build(&basic_request(), "sk-ant-test", "https://api.anthropic.com")
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();
    assert!(body.get("tools").is_none());
}

#[test]
fn build_plain_user_message_block_format() {
    let adapter = AnthropicAdapter;
    let req = LlmRequest {
        messages: vec![LlmMessage {
            role: "user".into(),
            content: "plain text".into(),
            tool_calls: None,
            tool_call_id: None,
            thinking: None,
        }],
        ..basic_request()
    };
    let http = adapter.build(&req, "sk-ant-test", "https://api.anthropic.com").unwrap();
    let body: serde_json::Value = serde_json::from_str(&http.body).unwrap();
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    assert_eq!(body["messages"][0]["content"][0]["text"], "plain text");
}

// ── parse_stream() ───────────────────────────────────────────────────────

#[test]
fn parse_text_deltas() {
    let adapter = AnthropicAdapter;
    let body = "\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.text, "Hello world");
    assert!(resp.tool_calls.is_empty());
}

#[test]
fn parse_single_tool_use_with_complete_input() {
    let adapter = AnthropicAdapter;
    let body = "\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01\",\"name\":\"Read\",\"input\":{\"path\":\"/tmp/x\"}}}\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "toolu_01");
    assert_eq!(resp.tool_calls[0].function.name, "Read");
    assert_eq!(resp.tool_calls[0].function.arguments, r#"{"path":"/tmp/x"}"#);
    assert!(resp.text.is_empty());
}

#[test]
fn parse_tool_use_incremental_partial_json() {
    let adapter = AnthropicAdapter;
    // Use raw string to avoid double-escaping inside JSON string literals.
    // Fragments: {"cmd": "ls -la"}  split as  {"cmd": " / ls -la / "}
    let body = r##"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_02","name":"Bash","input":{}}}
event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"cmd\": \""}}
event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"ls -la"}}
event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"}"}}
event: content_block_stop
data: {"type":"content_block_stop","index":0}
"##;
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "toolu_02");
    assert_eq!(resp.tool_calls[0].function.name, "Bash");
    // Arguments should be valid JSON (the accumulated partial_json parts)
    let args: serde_json::Value =
        serde_json::from_str(&resp.tool_calls[0].function.arguments).expect("args should be valid JSON");
    assert_eq!(args["cmd"], "ls -la");
}

#[test]
fn parse_parallel_tools() {
    let adapter = AnthropicAdapter;
    let body = "\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Read\",\"input\":{\"path\":\"/a\"}}}\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"t2\",\"name\":\"Bash\",\"input\":{\"cmd\":\"ls\"}}}\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.tool_calls.len(), 2);
    assert_eq!(resp.tool_calls[0].id, "t1");
    assert_eq!(resp.tool_calls[0].function.name, "Read");
    assert_eq!(resp.tool_calls[1].id, "t2");
    assert_eq!(resp.tool_calls[1].function.name, "Bash");
}

#[test]
fn parse_text_and_tool_interleaved() {
    let adapter = AnthropicAdapter;
    let body = "\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Let me check.\"}}\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tc1\",\"name\":\"Read\",\"input\":{\"path\":\"/f\"}}}\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\" Done.\"}}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.text, "Let me check. Done.");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "tc1");
    assert_eq!(resp.tool_calls[0].function.name, "Read");
}

#[test]
fn parse_error_event() {
    let adapter = AnthropicAdapter;
    let body = "data: {\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"Invalid API key\"}}\n";
    let err = adapter.parse_stream(body).expect_err("should return error");
    assert!(
        err.to_string().contains("Invalid API key"),
        "expected 'Invalid API key' in: {}",
        err
    );
}

#[test]
fn parse_empty_body() {
    let adapter = AnthropicAdapter;
    let resp = adapter.parse_stream("").expect("parse should succeed");
    assert!(resp.text.is_empty());
    assert!(resp.tool_calls.is_empty());
}

#[test]
fn parse_blank_lines_skipped() {
    let adapter = AnthropicAdapter;
    let body = "\n\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\
\n\n";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.text, "ok");
}

#[test]
fn parse_tool_without_stop_event_flushes_on_end() {
    // If the stream ends without a content_block_stop, the pending tool
    // should be finalized (as long as its args are valid JSON).
    let adapter = AnthropicAdapter;
    let body = "\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tx1\",\"name\":\"Read\",\"input\":{\"path\":\"/f\"}}}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "tx1");
    assert_eq!(resp.tool_calls[0].function.name, "Read");
}

#[test]
fn parse_ignores_incomplete_tool_at_end() {
    // tool_use started with partial_json but never finished — args are
    // not valid JSON, so the tool should be dropped.
    let adapter = AnthropicAdapter;
    let body = "\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"bad\",\"name\":\"Bash\",\"input\":{}}}\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"cmd\\\":\\\"ls\"}}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert!(
        resp.tool_calls.is_empty(),
        "incomplete JSON args should be dropped"
    );
}

#[test]
fn parse_ignores_unknown_event_types() {
    let adapter = AnthropicAdapter;
    let body = "\
event: ping\ndata: {\"type\":\"ping\"}\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.text, "hi");
    assert!(resp.tool_calls.is_empty());
}

#[test]
fn parse_ignores_lines_without_data_prefix() {
    let adapter = AnthropicAdapter;
    // Lines that don't start with "data: " should be silently skipped.
    let body = "\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"A\"}}\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"B\"}}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert_eq!(resp.text, "AB");
}

#[test]
fn parse_tool_with_empty_id_ignored() {
    let adapter = AnthropicAdapter;
    let body = "\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"\",\"name\":\"Read\",\"input\":{\"path\":\"/x\"}}}\n\
";
    let resp = adapter.parse_stream(body).expect("parse should succeed");
    assert!(resp.tool_calls.is_empty());
}
