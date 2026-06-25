//! Session actor model.
//!
//! Each active session has a dedicated thread (actor) that processes chat
//! messages sequentially via an mpsc channel. The session state lives on
//! the actor's stack — no Mutex needed for in-memory state.
//!
//! The store (persistence) is shared across all actors via `Arc<dyn SessionStore>`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};

use crate::store::store_trait::SessionStore;

/// Message sent to a session actor.
pub enum SessionMsg {
    /// Run a chat turn. The actor calls `handle_chat_send` internally.
    Run {
        params: serde_json::Value,
        response_id: Option<serde_json::Value>,
    },
    /// Gracefully shut down the actor (e.g. on session delete).
    Shutdown,
}

/// Handle to a running session actor.
pub struct ActorHandle {
    pub tx: mpsc::Sender<SessionMsg>,
}

/// Global registry of active session actors.
pub struct SessionRegistry {
    actors: Mutex<HashMap<String, ActorHandle>>,
    pub store: Arc<dyn SessionStore>,
}

impl SessionRegistry {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self { actors: Mutex::new(HashMap::new()), store }
    }

    /// Get or spawn the actor for `session_id`. Returns the sender.
    pub fn get_or_spawn(
        &self,
        session_id: &str,
        factory: impl FnOnce(mpsc::Receiver<SessionMsg>) -> std::thread::JoinHandle<()> + Send + 'static,
    ) -> mpsc::Sender<SessionMsg> {
        let mut actors = self.actors.lock().unwrap();
        if let Some(handle) = actors.get(session_id) {
            return handle.tx.clone();
        }
        let (tx, rx) = mpsc::channel();
        let _handle = factory(rx);
        actors.insert(session_id.to_string(), ActorHandle { tx: tx.clone() });
        tx
    }

    /// Remove and shut down the actor for `session_id`.
    pub fn remove(&self, session_id: &str) {
        let handle = self.actors.lock().unwrap().remove(session_id);
        if let Some(h) = handle {
            let _ = h.tx.send(SessionMsg::Shutdown);
        }
    }
}

#[cfg(test)]
#[path = "tests/session_actor_tests.rs"]
mod tests;
