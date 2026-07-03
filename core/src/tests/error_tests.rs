use super::*;
use anyhow::Context;

#[test]
fn retryable_variants() {
    assert!(ChatError::Network { detail: "test".into() }.is_retryable());
    assert!(ChatError::Timeout { seconds: 30 }.is_retryable());
    assert!(ChatError::StreamDisconnected.is_retryable());
    assert!(ChatError::RateLimited { retry_after_secs: None }.is_retryable());
    assert!(ChatError::ServerOverloaded.is_retryable());
}

#[test]
fn non_retryable_variants() {
    assert!(!ChatError::BadRequest { detail: "test".into() }.is_retryable());
    assert!(!ChatError::Unauthorized { detail: "test".into() }.is_retryable());
    assert!(!ChatError::ContextExceeded.is_retryable());
    assert!(!ChatError::UsageLimitReached { detail: "test".into() }.is_retryable());
    assert!(!ChatError::Config { detail: "test".into() }.is_retryable());
    assert!(!ChatError::Session { detail: "test".into() }.is_retryable());
    assert!(!ChatError::MalformedResponse { detail: "test".into() }.is_retryable());
    assert!(!ChatError::Internal { detail: "test".into() }.is_retryable());
}

#[test]
fn codes_are_stable_strings() {
    assert_eq!(ChatError::Network { detail: "x".into() }.code(), "NETWORK_ERROR");
    assert_eq!(ChatError::Timeout { seconds: 1 }.code(), "TIMEOUT");
    assert_eq!(ChatError::StreamDisconnected.code(), "STREAM_DISCONNECTED");
    assert_eq!(ChatError::RateLimited { retry_after_secs: None }.code(), "RATE_LIMITED");
    assert_eq!(ChatError::ServerOverloaded.code(), "SERVER_OVERLOADED");
    assert_eq!(ChatError::BadRequest { detail: "x".into() }.code(), "BAD_REQUEST");
    assert_eq!(ChatError::Unauthorized { detail: "x".into() }.code(), "UNAUTHORIZED");
    assert_eq!(ChatError::ContextExceeded.code(), "CONTEXT_EXCEEDED");
    assert_eq!(ChatError::UsageLimitReached { detail: "x".into() }.code(), "USAGE_LIMIT_REACHED");
    assert_eq!(ChatError::Config { detail: "x".into() }.code(), "CONFIG_ERROR");
    assert_eq!(ChatError::Session { detail: "x".into() }.code(), "SESSION_ERROR");
    assert_eq!(ChatError::MalformedResponse { detail: "x".into() }.code(), "MALFORMED_RESPONSE");
    assert_eq!(ChatError::Internal { detail: "x".into() }.code(), "INTERNAL_ERROR");
}

#[test]
fn downcast_extracts_chat_error() {
    let ce = ChatError::Network { detail: "test".into() };
    let anyhow_err = anyhow::Error::new(ce);
    let extracted = downcast_chat_error(&anyhow_err);
    assert!(extracted.is_some());
    assert_eq!(extracted.unwrap().code(), "NETWORK_ERROR");
}

#[test]
fn downcast_through_context() {
    let ce = ChatError::Timeout { seconds: 30 };
    let result: anyhow::Result<()> = Err(anyhow::Error::new(ce));
    let anyhow_err = result.context("additional context").unwrap_err();
    let extracted = downcast_chat_error(&anyhow_err);
    assert!(extracted.is_some());
    assert_eq!(extracted.unwrap().code(), "TIMEOUT");
}

#[test]
fn user_messages_are_human_readable() {
    let msg = ChatError::Unauthorized { detail: "Invalid API key".into() }.user_message();
    assert!(msg.contains("API key"));
    assert!(msg.contains("Settings"));

    let msg = ChatError::Network { detail: "connection refused".into() }.user_message();
    assert!(msg.contains("connection refused"));
    assert!(msg.contains("try again"));
}
