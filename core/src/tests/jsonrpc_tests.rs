use super::*;

#[test]
fn serialize_request() {
    let req = Request {
        jsonrpc: "2.0".into(),
        id: Some(Value::Number(1.into())),
        method: "chat.send".into(),
        params: Some(serde_json::json!({"message": "hello"})),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.method, "chat.send");
    assert_eq!(parsed.id, Some(Value::Number(1.into())));
}

#[test]
fn serialize_response_success() {
    let resp = Response::success(Some(Value::Number(1.into())), serde_json::json!({"ok": true}));
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains(r#""result""#));
    assert!(!json.contains(r#""error""#));
}

#[test]
fn serialize_response_error() {
    let resp = Response::error(None, -32600, "Invalid Request");
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains(r#""error""#));
    assert!(!json.contains(r#""result""#));
}

#[test]
fn serialize_notification() {
    let notif = Notification::new("chat.text_delta", Some(serde_json::json!({"delta": "hi"})));
    let json = serde_json::to_string(&notif).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("id").is_none());
    assert_eq!(parsed["method"], "chat.text_delta");
}

#[test]
fn deserialize_request_without_params() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert_eq!(req.method, "ping");
    assert!(req.params.is_none());
}

#[test]
fn deserialize_response_both_result_and_error_none() {
    let json = r#"{"jsonrpc":"2.0","id":null}"#;
    let resp: Response = serde_json::from_str(json).unwrap();
    assert!(resp.result.is_none());
    assert!(resp.error.is_none());
}

#[test]
fn serialize_error_with_code() {
    let resp = Response::error_with_code(
        Some(Value::Number(1.into())),
        -32603,
        "Authentication failed",
        "UNAUTHORIZED",
        false,
    );
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains(r#""error_code":"UNAUTHORIZED""#));
    assert!(json.contains(r#""retryable":false"#));
    assert!(json.contains(r#""code":-32603"#));
    assert!(!json.contains(r#""result""#));
}

#[test]
fn deserialize_error_with_code_roundtrip() {
    let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"boom","error_code":"NETWORK_ERROR","retryable":true}}"#;
    let resp: Response = serde_json::from_str(json).unwrap();
    let err = resp.error.unwrap();
    assert_eq!(err.error_code.as_deref(), Some("NETWORK_ERROR"));
    assert_eq!(err.retryable, Some(true));
}

#[test]
fn deserialize_error_without_code_fields_backward_compat() {
    let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"old style"}}"#;
    let resp: Response = serde_json::from_str(json).unwrap();
    let err = resp.error.unwrap();
    assert_eq!(err.message, "old style");
    assert!(err.error_code.is_none());
    assert!(err.retryable.is_none());
}

#[test]
fn simple_error_omits_structured_fields() {
    let resp = Response::error(None, -32600, "Invalid Request");
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains(r#""code":-32600"#));
    assert!(!json.contains("error_code"));   // None, skipped
    assert!(!json.contains("retryable"));    // None, skipped
}
