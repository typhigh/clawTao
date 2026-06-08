use super::{Message, Session};
use anyhow::Result;

/// Storage abstraction for session persistence.
/// Implementations: JsonSessionStore (JSONL append), SqliteSessionStore.
#[allow(dead_code)]
pub trait SessionStore: Send + Sync {
    fn create(&mut self, session: &Session) -> Result<()>;
    fn get(&self, id: &str) -> Result<Option<Session>>;
    fn list(&self) -> Result<Vec<Session>>;
    fn add_message(&mut self, session_id: &str, msg: &Message) -> Result<()>;
    fn delete(&mut self, id: &str) -> Result<()>;
}
