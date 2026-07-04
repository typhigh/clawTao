//! Structured error types for ClawTao.
//!
//! Every chat-related error is a `ChatError` variant that carries:
//! - an error code   (stable string the frontend can switch on)
//! - a retryable flag (so the caller can decide whether to retry)
//! - a user-facing message
//!
//! The outer functions still return `anyhow::Result`, but the error
//! chain always contains a `ChatError` that can be downcast for
//! structured JSON-RPC error responses.

use std::fmt;

/// Unified error type for the chat state machine.
///
/// Every variant maps to a stable `errorCode` string that the
/// frontend uses to choose between "show retry button" / "prompt
/// for API key" / "display fatal error" etc.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ChatError {
    // ── Network / transport (retryable) ──────────────────────────
    /// DNS, TCP connect, TLS handshake, or connection reset.
    Network { detail: String },
    /// Request timed out before the server responded.
    Timeout { seconds: u64 },
    /// SSE stream was interrupted before `response.completed`.
    StreamDisconnected,

    // ── API-level (retryable) ────────────────────────────────────
    /// 429 Too Many Requests.
    RateLimited { retry_after_secs: Option<u64> },
    /// 503 Service Unavailable / server overloaded.
    ServerOverloaded,

    // ── API-level (non-retryable) ────────────────────────────────
    /// 400 Bad Request — the request payload was rejected.
    BadRequest { detail: String },
    /// 401 Unauthorized / 403 Forbidden.
    Unauthorized { detail: String },
    /// The model's context window was exceeded.
    ContextExceeded,
    /// Quota or usage limit reached; the user needs to upgrade / wait.
    UsageLimitReached { detail: String },

    // ── Local errors (non-retryable) ────────────────────────────
    /// Missing or invalid configuration.
    Config { detail: String },
    /// Session lookup / store failure.
    Session { detail: String },
    /// LLM returned a response we could not parse.
    MalformedResponse { detail: String },

    // ── Internal ─────────────────────────────────────────────────
    /// Catch-all for unexpected internal errors.
    Internal { detail: String },
}

impl ChatError {
    /// Stable snake_case error code for the JSON-RPC `error.data.errorCode` field.
    pub fn code(&self) -> &'static str {
        match self {
            ChatError::Network { .. } => "NETWORK_ERROR",
            ChatError::Timeout { .. } => "TIMEOUT",
            ChatError::StreamDisconnected => "STREAM_DISCONNECTED",
            ChatError::RateLimited { .. } => "RATE_LIMITED",
            ChatError::ServerOverloaded => "SERVER_OVERLOADED",
            ChatError::BadRequest { .. } => "BAD_REQUEST",
            ChatError::Unauthorized { .. } => "UNAUTHORIZED",
            ChatError::ContextExceeded => "CONTEXT_EXCEEDED",
            ChatError::UsageLimitReached { .. } => "USAGE_LIMIT_REACHED",
            ChatError::Config { .. } => "CONFIG_ERROR",
            ChatError::Session { .. } => "SESSION_ERROR",
            ChatError::MalformedResponse { .. } => "MALFORMED_RESPONSE",
            ChatError::Internal { .. } => "INTERNAL_ERROR",
        }
    }

    /// Whether the caller should consider automatically retrying.
    pub fn is_retryable(&self) -> bool {
        match self {
            ChatError::Network { .. }
            | ChatError::Timeout { .. }
            | ChatError::StreamDisconnected
            | ChatError::RateLimited { .. }
            | ChatError::ServerOverloaded => true,

            ChatError::BadRequest { .. }
            | ChatError::Unauthorized { .. }
            | ChatError::ContextExceeded
            | ChatError::UsageLimitReached { .. }
            | ChatError::Config { .. }
            | ChatError::Session { .. }
            | ChatError::MalformedResponse { .. }
            | ChatError::Internal { .. } => false,
        }
    }

    /// A short, user-readable message (no internal stack traces).
    pub fn user_message(&self) -> String {
        match self {
            ChatError::Network { detail } => {
                format!("Network error: {detail}. Check your connection and try again.")
            }
            ChatError::Timeout { seconds } => {
                format!("Request timed out after {seconds}s. The model may be busy; try again.")
            }
            ChatError::StreamDisconnected => {
                "Response stream was interrupted. Please try again.".to_string()
            }
            ChatError::RateLimited { retry_after_secs } => {
                if let Some(s) = retry_after_secs {
                    format!("Rate limited. Retry after {s}s.")
                } else {
                    "Rate limited. Please wait and try again.".to_string()
                }
            }
            ChatError::ServerOverloaded => {
                "The model is at capacity. Try a different model or wait a moment.".to_string()
            }
            ChatError::BadRequest { detail } => {
                format!("Bad request: {detail}")
            }
            ChatError::Unauthorized { detail } => {
                format!("Authentication failed: {detail}. Check your API key in Settings.")
            }
            ChatError::ContextExceeded => {
                "Context window exceeded. Start a new session or clear earlier messages.".to_string()
            }
            ChatError::UsageLimitReached { detail } => {
                format!("Usage limit reached: {detail}")
            }
            ChatError::Config { detail } => {
                format!("Configuration error: {detail}. Check your Settings.")
            }
            ChatError::Session { detail } => {
                format!("Session error: {detail}")
            }
            ChatError::MalformedResponse { detail } => {
                format!("Unexpected response from model: {detail}")
            }
            ChatError::Internal { detail } => {
                format!("Internal error: {detail}")
            }
        }
    }
}

impl fmt::Display for ChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for ChatError {}

/// Helper: try to extract the innermost `ChatError` from an `anyhow::Error` chain.
///
/// Walks the chain of sources (including the top-level error itself) and returns
/// the first `ChatError` found.
pub fn downcast_chat_error(e: &anyhow::Error) -> Option<&ChatError> {
    // Check the top-level error first (anyhow wraps the original error).
    if let Some(ce) = e.downcast_ref::<ChatError>() {
        return Some(ce);
    }
    // Walk the source chain.
    let mut source = e.source();
    while let Some(s) = source {
        if let Some(ce) = s.downcast_ref::<ChatError>() {
            return Some(ce);
        }
        source = s.source();
    }
    None
}

#[cfg(test)]
#[path = "tests/error_tests.rs"]
mod tests;
