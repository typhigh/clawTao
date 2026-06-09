use crate::store::{Message, Session};
use crate::store::sqlite_store::SqliteSessionStore;
use crate::store::store_trait::SessionStore;

fn make_store() -> SqliteSessionStore {
    let path = std::env::temp_dir().join(format!("clawtao_test_sqlite_{}.db", uuid::Uuid::new_v4()));
    SqliteSessionStore::new(path).unwrap()
}

fn msg(id: &str, role: &str, content: &str) -> Message {
    Message { id: id.into(), role: role.into(), content: content.into(), tool_calls: None, tool_call_id: None, timestamp: 1000 }
}

#[test]
fn create_and_get() {
    let mut store = make_store();
    let s = Session { id: "s1".into(), created_at: 1000, updated_at: 2000, messages: vec![], title: String::new() };
    store.create(&s).unwrap();
    let got = store.get("s1").unwrap().unwrap();
    assert_eq!(got.id, "s1");
}

#[test]
fn add_message_and_retrieve() {
    let mut store = make_store();
    store.create(&Session { id: "s1".into(), created_at: 1000, updated_at: 1000, messages: vec![], title: String::new() }).unwrap();
    store.add_message("s1", &msg("m1", "user", "hello")).unwrap();
    store.add_message("s1", &msg("m2", "assistant", "hi")).unwrap();
    let session = store.get("s1").unwrap().unwrap();
    assert_eq!(session.messages.len(), 2);
    assert_eq!(session.messages[0].role, "user");
}

#[test]
fn delete_cascades() {
    let mut store = make_store();
    store.create(&Session { id: "s1".into(), created_at: 1000, updated_at: 1000, messages: vec![], title: String::new() }).unwrap();
    store.add_message("s1", &msg("m1", "user", "hello")).unwrap();
    store.delete("s1").unwrap();
    assert!(store.get("s1").unwrap().is_none());
}

#[test]
fn list_returns_sessions() {
    let mut store = make_store();
    store.create(&Session { id: "a".into(), created_at: 1, updated_at: 1, messages: vec![], title: String::new() }).unwrap();
    store.create(&Session { id: "b".into(), created_at: 2, updated_at: 2, messages: vec![], title: String::new() }).unwrap();
    assert_eq!(store.list().unwrap().len(), 2);
}
