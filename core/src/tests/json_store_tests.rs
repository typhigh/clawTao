use crate::store::{Message, Session};
use crate::store::json_store::JsonSessionStore;
use crate::store::store_trait::SessionStore;

fn make_store() -> JsonSessionStore {
    let dir = std::env::temp_dir().join(format!("clawtao_test_json_{}", uuid::Uuid::new_v4()));
    JsonSessionStore::new(dir)
}

fn msg(id: &str, role: &str, content: &str) -> Message {
    Message { id: id.into(), role: role.into(), content: content.into(), tool_calls: None, tool_call_id: None, thinking: None, timestamp: 1000, image_paths: None }
}

#[test]
fn create_and_get() {
    let store = make_store();
    let s = Session { id: "s1".into(), created_at: 1000, updated_at: 2000, messages: vec![], title: String::new() };
    store.create(&s).unwrap();
    let got = store.get("s1").unwrap().unwrap();
    assert_eq!(got.id, "s1");
}

#[test]
fn list_orders_by_updated_desc() {
    let store = make_store();
    store.create(&Session { id: "a".into(), created_at: 1, updated_at: 100, messages: vec![], title: String::new() }).unwrap();
    store.create(&Session { id: "b".into(), created_at: 2, updated_at: 200, messages: vec![], title: String::new() }).unwrap();
    let list = store.list().unwrap();
    assert_eq!(list[0].id, "b");
    assert_eq!(list[1].id, "a");
}

#[test]
fn add_message_append() {
    let store = make_store();
    store.create(&Session { id: "s1".into(), created_at: 1000, updated_at: 1000, messages: vec![], title: String::new() }).unwrap();
    store.add_message("s1", &msg("m1", "user", "hello")).unwrap();
    store.add_message("s1", &msg("m2", "assistant", "hi")).unwrap();
    let session = store.get("s1").unwrap().unwrap();
    assert_eq!(session.messages.len(), 2);
}

#[test]
fn delete_session() {
    let store = make_store();
    store.create(&Session { id: "s1".into(), created_at: 1000, updated_at: 1000, messages: vec![], title: String::new() }).unwrap();
    assert!(store.get("s1").unwrap().is_some());
    store.delete("s1").unwrap();
    assert!(store.get("s1").unwrap().is_none());
}

#[test]
fn get_nonexistent_returns_none() {
    let store = make_store();
    assert!(store.get("nope").unwrap().is_none());
}
