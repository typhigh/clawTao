use super::*;

fn make_session_manager() -> SessionManager {
    let dir = std::env::temp_dir().join(format!("clawtao_test_{}", uuid::Uuid::new_v4()));
    SessionManager::new(dir)
}

#[test]
fn create_and_get_session() {
    let mut mgr = make_session_manager();
    let s = mgr.create_session();
    let retrieved = mgr.get_session(&s.id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().id, s.id);
}

#[test]
fn add_user_message() {
    let mut mgr = make_session_manager();
    let s = mgr.create_session();
    let msg = mgr.add_message(&s.id, "user", "hello");
    assert!(msg.is_some());
    let session = mgr.get_session(&s.id).unwrap();
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].role, "user");
    assert_eq!(session.messages[0].content, "hello");
}

#[test]
fn add_assistant_tool_calls() {
    let mut mgr = make_session_manager();
    let s = mgr.create_session();
    let tc = vec![ToolCall {
        id: "call_1".into(),
        call_type: "function".into(),
        function: ToolCallFunction {
            name: "Read".into(),
            arguments: r#"{"path":"README.md"}"#.into(),
        },
    }];
    let msg = mgr.add_assistant_tool_calls(&s.id, tc);
    assert!(msg.is_some());
    let session = mgr.get_session(&s.id).unwrap();
    assert_eq!(session.messages.len(), 1);
    assert!(session.messages[0].tool_calls.is_some());
}

#[test]
fn add_tool_result() {
    let mut mgr = make_session_manager();
    let s = mgr.create_session();
    let msg = mgr.add_tool_result(&s.id, "call_1", "file content here");
    assert!(msg.is_some());
    let session = mgr.get_session(&s.id).unwrap();
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].role, "tool");
    assert_eq!(session.messages[0].tool_call_id.as_deref(), Some("call_1"));
}

#[test]
fn message_to_llm_format_user() {
    let msg = Message {
        id: "1".into(), role: "user".into(), content: "hi".into(),
        tool_calls: None, tool_call_id: None, timestamp: 0,
    };
    let json = msg.to_llm_message();
    assert_eq!(json["role"], "user");
    assert_eq!(json["content"], "hi");
}

#[test]
fn message_to_llm_format_tool_result() {
    let msg = Message {
        id: "1".into(), role: "tool".into(), content: "result".into(),
        tool_calls: None, tool_call_id: Some("call_1".into()), timestamp: 0,
    };
    let json = msg.to_llm_message();
    assert_eq!(json["role"], "tool");
    assert_eq!(json["tool_call_id"], "call_1");
    assert_eq!(json["content"], "result");
}

#[test]
fn message_to_llm_format_assistant_with_tool_calls() {
    let msg = Message {
        id: "1".into(), role: "assistant".into(), content: "".into(),
        tool_calls: Some(vec![ToolCall {
            id: "call_1".into(), call_type: "function".into(),
            function: ToolCallFunction { name: "Read".into(), arguments: "{}".into() },
        }]),
        tool_call_id: None, timestamp: 0,
    };
    let json = msg.to_llm_message();
    assert_eq!(json["role"], "assistant");
    assert!(json["content"].is_null());
    assert!(json.get("tool_calls").is_some());
}

#[test]
fn session_list_empty() {
    let mgr = make_session_manager();
    assert!(mgr.list_sessions().is_empty());
}
