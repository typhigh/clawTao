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
