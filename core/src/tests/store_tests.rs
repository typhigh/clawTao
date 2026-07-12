use crate::store;
use crate::store::json_store::JsonSessionStore;
use crate::store::store_trait::SessionStore;

fn test_temp_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_store() -> JsonSessionStore {
    let dir = test_temp_dir();
    JsonSessionStore::new(dir)
}

#[test]
fn create_and_get_session() {
    let s = make_store();
    let session = store::new_session();
    s.create(&session).unwrap();
    let retrieved = s.get(&session.id).unwrap().unwrap();
    assert_eq!(retrieved.id, session.id);
}

#[test]
fn add_user_message() {
    let s = make_store();
    let session = store::new_session();
    s.create(&session).unwrap();
    store::add_message(&s, &session.id, "user", "hello").unwrap();
    let got = s.get(&session.id).unwrap().unwrap();
    assert_eq!(got.messages.len(), 1);
    assert_eq!(got.messages[0].role, "user");
}

#[test]
fn add_assistant_tool_calls() {
    let s = make_store();
    let session = store::new_session();
    s.create(&session).unwrap();
    store::add_assistant_tool_calls(&s, &session.id, vec![store::ToolCall {
        id: "call_1".into(), call_type: "function".into(),
        function: store::ToolCallFunction { name: "Read".into(), arguments: r#"{"path":"README.md"}"#.into() },
    }], "", None).unwrap();
    let got = s.get(&session.id).unwrap().unwrap();
    assert_eq!(got.messages.len(), 1);
    assert!(got.messages[0].tool_calls.is_some());
}

#[test]
fn add_tool_result() {
    let s = make_store();
    let session = store::new_session();
    s.create(&session).unwrap();
    store::add_tool_result(&s, &session.id, "call_1", "file content here").unwrap();
    let got = s.get(&session.id).unwrap().unwrap();
    assert_eq!(got.messages.len(), 1);
    assert_eq!(got.messages[0].role, "tool");
    assert_eq!(got.messages[0].tool_call_id.as_deref(), Some("call_1"));
}

#[test]
fn session_list_empty() {
    let s = make_store();
    assert!(s.list().unwrap().is_empty());
}
