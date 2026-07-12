use super::*;
use crate::store::Message;

// ── Token estimation ──────────────────────────────────────────────

#[test]
fn estimate_tokens_empty() {
    assert_eq!(estimate_tokens(""), 0);
}

#[test]
fn estimate_tokens_pure_english() {
    // 40 ASCII chars → 40/4 = 10 tokens
    assert_eq!(estimate_tokens("The quick brown fox jumps over the lazy dog"), 10);
}

#[test]
fn estimate_tokens_pure_chinese() {
    // 12 Chinese chars → 12*2/3 = 8 tokens
    assert_eq!(estimate_tokens("你好世界这是一条测试消息"), 8);
}

#[test]
fn estimate_tokens_mixed() {
    // "hello世界" — 5 ASCII + 2 CJK → 5/4 + 4/3 = 1 + 1 = 2
    assert_eq!(estimate_tokens("hello世界"), 2);
}

// ── Provider context window ───────────────────────────────────────

#[test]
fn provider_window_deepseek() {
    assert_eq!(provider_context_window("https://api.deepseek.com/anthropic"), 1_000_000);
}

#[test]
fn provider_window_minimax() {
    assert_eq!(provider_context_window("https://api.minimaxi.com/anthropic"), 1_000_000);
}

#[test]
fn provider_window_custom() {
    assert_eq!(provider_context_window("https://my-llm.example.com"), 256_000);
}

#[test]
fn compact_threshold_deepseek() {
    assert_eq!(compact_threshold("https://api.deepseek.com/anthropic"), 800_000);
}

#[test]
fn compact_threshold_custom() {
    assert_eq!(compact_threshold("https://unknown.example.com"), 204_800);
}

// ── Error detection ───────────────────────────────────────────────

#[test]
fn detects_context_length_errors() {
    assert!(is_context_length_error("prompt is too long for this model"));
    assert!(is_context_length_error("Error: context_length_exceeded"));
    assert!(is_context_length_error("This exceeds the model's maximum context length"));
    assert!(is_context_length_error("Your request has too many tokens"));
}

#[test]
fn does_not_detect_other_errors() {
    assert!(!is_context_length_error("invalid API key"));
    assert!(!is_context_length_error("rate limit exceeded"));
    assert!(!is_context_length_error(""));
}

#[test]
fn error_detection_case_insensitive() {
    assert!(is_context_length_error("CONTEXT_LENGTH_EXCEEDED"));
    assert!(is_context_length_error("Context Window Exceeded"));
}

// ── Turn counting ─────────────────────────────────────────────────

#[test]
fn count_turns_basic() {
    let msgs = vec![
        msg("user", "u0"),
        msg("assistant", "a0"),
        msg("user", "u1"),
        msg("assistant", "a1"),
        msg("user", "u2"),
    ];
    assert_eq!(count_turns_from_end(&msgs, 2), 3);
    assert_eq!(count_turns_from_end(&msgs, 3), 5);
    assert_eq!(count_turns_from_end(&msgs, 1), 1);
}

#[test]
fn count_turns_with_tool_messages() {
    let msgs = vec![
        msg("user", "u0"),
        msg("assistant", "a0"),
        msg("user", "u1"),
        msg("assistant", "tool_calls"),
        msg("tool", "result"),
        msg("assistant", "synthesis"),
        msg("user", "u2"),
    ];
    assert_eq!(count_turns_from_end(&msgs, 2), 5);
}

#[test]
fn count_turns_fewer_returns_all() {
    let msgs = vec![msg("user", "u0"), msg("assistant", "a0")];
    assert_eq!(count_turns_from_end(&msgs, 3), 2);
}

// ── Conversation text ─────────────────────────────────────────────

#[test]
fn build_conversation_text_basic() {
    let msgs = vec![msg("user", "hello"), msg("assistant", "hi there")];
    let text = build_conversation_text(&msgs);
    assert!(text.contains("[user]: hello"));
    assert!(text.contains("[assistant]: hi there"));
}

#[test]
fn build_conversation_text_truncates() {
    let long: String = std::iter::repeat('x').take(3000).collect();
    let msgs = vec![msg("user", &long)];
    let text = build_conversation_text(&msgs);
    let content_start = text.find("]: ").unwrap() + 3;
    let content = &text[content_start..].trim_end();
    assert!(content.len() <= MAX_CHARS_PER_MESSAGE_IN_SUMMARY);
}

// ── Helpers ───────────────────────────────────────────────────────

fn msg(role: &str, content: &str) -> Message {
    Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: role.to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_call_id: None,
        thinking: None,
        timestamp: 0,
        image_paths: None,
    }
}
