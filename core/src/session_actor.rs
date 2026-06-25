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
#[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn remove(&self, session_id: &str) {
        let handle = self.actors.lock().unwrap().remove(session_id);
        if let Some(h) = handle {
            let _ = h.tx.send(SessionMsg::Shutdown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::json_store::JsonSessionStore;
    use std::sync::Barrier;

    fn make_registry() -> SessionRegistry {
        let dir = std::env::temp_dir().join(format!("clawtao_test_actor_{}", uuid::Uuid::new_v4()));
        SessionRegistry::new(Arc::new(JsonSessionStore::new(dir)))
    }

    #[test]
    fn get_or_spawn_reuses_existing_actor() {
        let reg = make_registry();
        let spawned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s = Arc::clone(&spawned);

        let _tx1 = reg.get_or_spawn("s1", move |rx| {
            s.store(true, std::sync::atomic::Ordering::SeqCst);
            std::thread::spawn(move || {
                for msg in rx {
                    if matches!(msg, SessionMsg::Shutdown) { break; }
                }
            })
        });
        assert!(spawned.load(std::sync::atomic::Ordering::SeqCst));

        // Second call: factory should NOT run again.
        let spawned2 = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s2 = Arc::clone(&spawned2);
        let _tx2 = reg.get_or_spawn("s1", move |_rx| {
            s2.store(true, std::sync::atomic::Ordering::SeqCst);
            std::thread::spawn(|| {})
        });
        assert!(!spawned2.load(std::sync::atomic::Ordering::SeqCst));

        reg.remove("s1");
    }

    #[test]
    fn remove_shuts_down_actor() {
        let reg = make_registry();
        let barrier = Arc::new(Barrier::new(2));
        let b = Arc::clone(&barrier);
        let tx = reg.get_or_spawn("s1", move |rx| {
            std::thread::spawn(move || {
                b.wait(); // signal ready
                // Block until Shutdown arrives.
                assert!(matches!(rx.recv().unwrap(), SessionMsg::Shutdown));
            })
        });
        // Drop tx so remove's Shutdown reaches the actor (no other sender).
        drop(tx);
        barrier.wait(); // actor is ready
        reg.remove("s1");
        // If remove didn't work, the test would hang (actor never exits).
    }
}
