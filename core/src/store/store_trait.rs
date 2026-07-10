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

    /// Persist compaction metadata. Does NOT modify the messages table.
    /// Pass `None` to clear compaction state.
    fn update_compaction(
        &self,
        session_id: &str,
        summary: Option<&str>,
        last_msg_id: Option<&str>,
    ) -> Result<()>;
}
