use super::types::{LlmRequest, LlmResponse};
use anyhow::Result;

/// Raw HTTP request built by an adapter.
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Result of extracting a single SSE event for real-time streaming.
/// `kind` is the notification kind (e.g. "text", "thinking").
/// `delta` is the text fragment to send to the UI.
#[derive(Debug)]
pub struct StreamEvent {
    pub kind: String,
    pub delta: String,
}

/// Protocol adapter: builds HTTP requests, parses accumulated SSE bodies,
/// and extracts per-event streaming deltas for real-time UI updates.
pub trait ApiAdapter: Send + Sync {
    fn build(&self, req: &LlmRequest, api_key: &str, base_url: &str) -> Result<HttpRequest>;
    fn parse_stream(&self, body: &str) -> Result<LlmResponse>;
    /// Extract streaming notification(s) from a single SSE `data:` line.
    /// Returns an empty Vec for events that don't need UI notification.
    fn stream_events(&self, event: &serde_json::Value) -> Vec<StreamEvent>;
}
