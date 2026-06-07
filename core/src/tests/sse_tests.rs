use super::*;

#[test]
fn single_tool_single_chunk() {
    let body = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"call_1","type":"function","function":{"name":"Bash","arguments":"{}"},"index":0}]}}]}"#;
    let result = parse_sse_response(body);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].id, "call_1");
    assert_eq!(result.tool_calls[0].function.name, "Bash");
}

#[test]
fn single_tool_args_split() {
    // Chunk 1: id + name + partial args. Chunk 2: only args, no id/name.
    let body = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"tc1","type":"function","function":{"name":"Bash","arguments":"{\"path\": \"/tmp/a"},"index":0}]}}]}
data: {"choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":".txt\"}"},"index":0}]}}]}"#;
    let result = parse_sse_response(body);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].function.arguments, r#"{"path": "/tmp/a.txt"}"#);
}

#[test]
fn multiple_parallel_tools() {
    let body = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"t1","type":"function","function":{"name":"Read","arguments":"{}"},"index":0},{"id":"t2","type":"function","function":{"name":"Write","arguments":"{}"},"index":1}]}}]}"#;
    let result = parse_sse_response(body);
    assert_eq!(result.tool_calls.len(), 2);
    assert_eq!(result.tool_calls[0].function.name, "Read");
    assert_eq!(result.tool_calls[1].function.name, "Write");
}

#[test]
fn parallel_tools_one_split() {
    // tool index=0 complete, tool index=1 split across chunks
    let body = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"a","type":"function","function":{"name":"Read","arguments":"{}"},"index":0},{"id":"b","type":"function","function":{"name":"Bash","arguments":"{\"cmd\": \"f"},"index":1}]}}]}
data: {"choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":"ind\"}"},"index":1}]}}]}"#;
    let result = parse_sse_response(body);
    assert_eq!(result.tool_calls.len(), 2);
    assert_eq!(result.tool_calls[1].function.arguments, r#"{"cmd": "find"}"#);
}

#[test]
fn mixed_text_and_tool_calls() {
    let body = r#"data: {"choices":[{"index":0,"delta":{"content":"Let me check"}}]}
data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"c1","type":"function","function":{"name":"Read","arguments":"{}"},"index":0}]}}]}
data: {"choices":[{"index":0,"delta":{"content":" done!"}}]}"#;
    let result = parse_sse_response(body);
    assert_eq!(result.text, "Let me check done!");
    assert_eq!(result.tool_calls.len(), 1);
}

#[test]
fn empty_response() {
    let body = "";
    let result = parse_sse_response(body);
    assert!(result.text.is_empty());
    assert!(result.tool_calls.is_empty());
}

#[test]
fn text_only() {
    let body = r#"data: {"choices":[{"index":0,"delta":{"content":"Hello"}}]}
data: {"choices":[{"index":0,"delta":{"content":" world"}}]}
data: [DONE]"#;
    let result = parse_sse_response(body);
    assert_eq!(result.text, "Hello world");
    assert!(result.tool_calls.is_empty());
}

#[test]
fn invalid_args_json_dropped() {
    let body = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"bad","type":"function","function":{"name":"Bash","arguments":"not json"},"index":0}]}}]}"#;
    let result = parse_sse_response(body);
    assert!(result.tool_calls.is_empty());
}

#[test]
fn continuation_no_id_or_name() {
    let body = r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"id":"x","type":"function","function":{"name":"Bash","arguments":"{\"a\":\"1"},"index":0}]}}]}
data: {"choices":[{"index":0,"delta":{"tool_calls":[{"function":{"arguments":"2\"}"},"index":0}]}}]}"#;
    let result = parse_sse_response(body);
    assert_eq!(result.tool_calls.len(), 1);
    assert_eq!(result.tool_calls[0].id, "x");
    assert_eq!(result.tool_calls[0].function.name, "Bash");
    assert_eq!(result.tool_calls[0].function.arguments, r#"{"a":"12"}"#);
}
