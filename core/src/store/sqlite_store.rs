use super::{Message, Session};
use super::store_trait::SessionStore;
use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct SqliteSessionStore {
    conn: Mutex<Connection>,
}

impl SqliteSessionStore {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                title TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                tool_calls TEXT,
                tool_call_id TEXT,
                thinking TEXT,
                timestamp INTEGER NOT NULL,
                image_paths TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);"
        )?;
        // Migration: add image_paths to old DBs that don't have it.
        conn.execute_batch("ALTER TABLE messages ADD COLUMN image_paths TEXT;").ok();
        Ok(Self { conn: Mutex::new(conn) })
    }
}

impl SessionStore for SqliteSessionStore {
    fn create(&self, session: &Session) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (id, created_at, updated_at, title) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session.id, session.created_at, session.updated_at, session.title],
        )?;
        for msg in &session.messages {
            add_message_inner(&conn, &session.id, msg)?;
        }
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, created_at, updated_at, title FROM sessions WHERE id = ?1")?;
        let session = stmt.query_row(rusqlite::params![id], |row| {
            Ok(Session { id: row.get(0)?, created_at: row.get(1)?, updated_at: row.get(2)?, title: row.get(3)?, messages: Vec::new() })
        }).ok();
        let Some(mut session) = session else { return Ok(None) };
        let mut msg_stmt = conn.prepare(
            "SELECT id, role, content, tool_calls, tool_call_id, thinking, timestamp, image_paths
             FROM messages WHERE session_id = ?1 ORDER BY timestamp"
        )?;
        let msgs = msg_stmt.query_map(rusqlite::params![id], |row| {
            let tool_calls: Option<String> = row.get(3)?;
            let image_paths: Option<String> = row.get(7)?;
            Ok(Message {
                id: row.get(0)?, role: row.get(1)?, content: row.get(2)?,
                tool_calls: tool_calls.and_then(|s| serde_json::from_str(&s).ok()),
                tool_call_id: row.get(4)?, thinking: row.get(5)?, timestamp: row.get(6)?,
                image_paths: image_paths.and_then(|s| serde_json::from_str(&s).ok()),
            })
        })?;
        for msg in msgs { session.messages.push(msg?); }
        Ok(Some(session))
    }

    fn list(&self) -> Result<Vec<Session>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, created_at, updated_at, title FROM sessions ORDER BY updated_at DESC")?;
        let sessions = stmt.query_map([], |row| {
            Ok(Session { id: row.get(0)?, created_at: row.get(1)?, updated_at: row.get(2)?, title: row.get(3)?, messages: Vec::new() })
        })?;
        let mut result = Vec::new();
        for s in sessions { result.push(s?); }
        Ok(result)
    }

    fn add_message(&self, session_id: &str, msg: &Message) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        add_message_inner(&conn, session_id, msg)
    }

    fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM messages WHERE session_id = ?1", rusqlite::params![id])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }
}

fn add_message_inner(conn: &Connection, session_id: &str, msg: &Message) -> Result<()> {
    let tool_calls_json = msg.tool_calls.as_ref().and_then(|tc| serde_json::to_string(tc).ok());
    // Set title from first message content (chars for UTF-8 safety)
    let title_preview: String = msg.content.chars().take(50).collect();
    conn.execute(
        "UPDATE sessions SET title = CASE WHEN title = '' THEN ?2 ELSE title END WHERE id = ?1",
        rusqlite::params![session_id, title_preview],
    )?;
    let image_paths_json = msg.image_paths.as_ref().and_then(|p| serde_json::to_string(p).ok());
    conn.execute(
        "INSERT INTO messages (id, session_id, role, content, tool_calls, tool_call_id, thinking, timestamp, image_paths)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![msg.id, session_id, msg.role, msg.content, tool_calls_json, msg.tool_call_id, msg.thinking, msg.timestamp, image_paths_json],
    )?;
    conn.execute(
        "UPDATE sessions SET updated_at = MAX(updated_at, ?1) WHERE id = ?2",
        rusqlite::params![msg.timestamp, session_id],
    )?;
    Ok(())
}
