use super::types::{LlmRequest, LlmResponse};
use anyhow::Result;

/// Raw HTTP request built by an adapter.
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Protocol adapter: builds HTTP requests and parses SSE responses.
pub trait ApiAdapter: Send + Sync {
    fn build(&self, req: &LlmRequest, api_key: &str, base_url: &str) -> Result<HttpRequest>;
    fn parse_stream(&self, body: &str) -> Result<LlmResponse>;
}
