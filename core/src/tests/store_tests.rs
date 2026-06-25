use crate::store::*;
use crate::store::json_store::JsonSessionStore;

fn make_session_manager() -> SessionManager {
    let dir = std::env::temp_dir().join(format!("clawtao_test_{}", uuid::Uuid::new_v4()));
    SessionManager::new(Box::new(JsonSessionStore::new(dir)))
}

#[test]
fn create_and_get_session() {
    let mgr = make_session_manager();
    let s = mgr.create_session().unwrap();
    let retrieved = mgr.get_session(&s.id).unwrap().unwrap();
    assert_eq!(retrieved.id, s.id);
}

#[test]
fn add_user_message() {
    let mgr = make_session_manager();
    let s = mgr.create_session().unwrap();
    mgr.add_message(&s.id, "user", "hello").unwrap();
    let session = mgr.get_session(&s.id).unwrap().unwrap();
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].role, "user");
}

#[test]
fn add_assistant_tool_calls() {
    let mgr = make_session_manager();
    let s = mgr.create_session().unwrap();
    mgr.add_assistant_tool_calls(&s.id, vec![ToolCall {
        id: "call_1".into(), call_type: "function".into(),
        function: ToolCallFunction { name: "Read".into(), arguments: r#"{"path":"README.md"}"#.into() },
    }], "", None).unwrap();
    let session = mgr.get_session(&s.id).unwrap().unwrap();
    assert_eq!(session.messages.len(), 1);
    assert!(session.messages[0].tool_calls.is_some());
}

#[test]
fn add_tool_result() {
    let mgr = make_session_manager();
    let s = mgr.create_session().unwrap();
    mgr.add_tool_result(&s.id, "call_1", "file content here").unwrap();
    let session = mgr.get_session(&s.id).unwrap().unwrap();
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].role, "tool");
    assert_eq!(session.messages[0].tool_call_id.as_deref(), Some("call_1"));
}

#[test]
fn session_list_empty() {
    let mgr = make_session_manager();
    assert!(mgr.list_sessions().unwrap().is_empty());
}
