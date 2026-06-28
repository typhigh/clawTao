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
use reqwest::blocking::Client;
use serde_json::Value;
use tracing::{error, info};

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

/// Session actor loop. Processes Run messages sequentially, calling `process`
/// for each one. The `process` function is `process_run_wrapper` in production,
/// or a counter in tests.
pub(crate) fn actor_loop(
    rx: mpsc::Receiver<SessionMsg>,
    session_id: &str,
    store: Arc<dyn SessionStore>,
    process: impl Fn(&Client, &dyn SessionStore, Value, Option<Value>) + Send + 'static,
) {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .expect("Failed to build HTTP client");

    for msg in rx {
        match msg {
            SessionMsg::Run { params, response_id } => {
                process(&client, &*store, params, response_id);
            }
            SessionMsg::Shutdown => {
                info!("Actor for session {session_id} shutting down");
                break;
            }
        }
    }
}

/// Production processor: delegates to chat::run_turn.
pub(crate) fn process_run_wrapper(
    client: &Client,
    store: &dyn SessionStore,
    params: Value,
    response_id: Option<Value>,
) {
    let request = crate::jsonrpc::Request {
        jsonrpc: "2.0".into(),
        id: response_id,
        method: "chat.send".into(),
        params: Some(params),
    };
    if let Err(e) = crate::chat::run_turn(&request, store, client) {
        error!("chat error: {e:#}");
        let _ = crate::jsonrpc::write_response(&crate::jsonrpc::Response::error(
            request.id, -32603, format!("Internal error: {e:#}"),
        ));
    }
}

#[cfg(test)]
#[path = "tests/session_actor_tests.rs"]
mod tests;
