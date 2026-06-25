use super::{Message, Session};
use anyhow::Result;

/// Storage abstraction for session persistence.
/// Implementations: JsonSessionStore (JSONL append), SqliteSessionStore.
pub trait SessionStore: Send + Sync {
    fn create(&self, session: &Session) -> Result<()>;
    fn get(&self, id: &str) -> Result<Option<Session>>;
    fn list(&self) -> Result<Vec<Session>>;
    fn add_message(&self, session_id: &str, msg: &Message) -> Result<()>;
    fn delete(&self, id: &str) -> Result<()>;
}
