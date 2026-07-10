//! Context window management: token estimation, model matching, error detection,
//! and turn-boundary counting for automatic message compaction.

use crate::llm::types::UnifiedTool;
use crate::store::Message;

// ── Constants ────────────────────────────────────────────────────────────

/// Compact when estimated tokens exceed this fraction of the model's context window.
const COMPACTION_THRESHOLD_RATIO: f64 = 0.8;

/// Don't bother compacting sessions with fewer than this many messages.
const MIN_MESSAGES_TO_COMPACT: usize = 6;

/// Keep at least this many full conversation turns after compaction.
const MIN_RECENT_TURNS: usize = 3;

/// Estimated token cost per attached image (~7373 bytes / 4).
const TOKENS_PER_IMAGE: usize = 1844;

/// Truncate each message to this many chars when building the summarization input.
const MAX_CHARS_PER_MESSAGE_IN_SUMMARY: usize = 2000;

/// Fallback context window for unknown / custom providers.
const DEFAULT_CONTEXT_WINDOW: usize = 256_000;

// ── Public re-exports ────────────────────────────────────────────────────

pub const MIN_MSGS: usize = MIN_MESSAGES_TO_COMPACT;
pub const MIN_TURNS: usize = MIN_RECENT_TURNS;
pub const MAX_CHARS_PER_MSG: usize = MAX_CHARS_PER_MESSAGE_IN_SUMMARY;

// ── Token estimation ─────────────────────────────────────────────────────

/// Rough token estimate from text content.
///
/// Bilingual heuristic: English ~4 chars/token, CJK ~1.5 chars/token.
/// More accurate than `bytes / 4` for the default zh-CN locale.
pub fn estimate_tokens(text: &str) -> usize {
    let total = text.chars().count();
    let ascii = text.chars().filter(|c| c.is_ascii()).count();
    let non_ascii = total.saturating_sub(ascii);
    (ascii / 4).saturating_add(non_ascii.saturating_mul(2) / 3)
}

/// Estimate total tokens for a full LLM call from stored messages.
pub fn estimate_total_tokens_from_store(
    system_prompt: &str,
    messages: &[Message],
    tools: &[UnifiedTool],
    _model: &str,
) -> usize {
    let mut tokens = estimate_tokens(system_prompt);

    for m in messages {
        tokens = tokens.saturating_add(estimate_tokens(&m.content));

        if let Some(ref tcs) = m.tool_calls {
            for tc in tcs {
                tokens = tokens.saturating_add(estimate_tokens(&tc.function.name));
                tokens = tokens.saturating_add(estimate_tokens(&tc.function.arguments));
            }
        }

        if let Some(ref thinking) = m.thinking {
            tokens = tokens.saturating_add(estimate_tokens(thinking));
        }

        if let Some(ref paths) = m.image_paths {
            tokens = tokens.saturating_add(paths.len().saturating_mul(TOKENS_PER_IMAGE));
        }
    }

    // Tool definitions.
    for t in tools {
        tokens = tokens.saturating_add(estimate_tokens(&t.name));
        tokens = tokens.saturating_add(estimate_tokens(&t.description));
        let params = t.parameters.to_string();
        tokens = tokens.saturating_add(estimate_tokens(&params));
    }

    tokens
}

// ── Provider context window ──────────────────────────────────────────────

/// Look up the provider's context window size from its API base URL.
/// DeepSeek and MiniMax both support 1M tokens; custom providers default to 256K.
pub fn provider_context_window(base_url: &str) -> usize {
    let u = base_url.to_lowercase();
    if u.contains("deepseek") || u.contains("minimaxi") || u.contains("minimax") {
        1_000_000
    } else {
        DEFAULT_CONTEXT_WINDOW
    }
}

/// Token count at which proactive compaction fires.
pub fn compact_threshold(base_url: &str) -> usize {
    (provider_context_window(base_url) as f64 * COMPACTION_THRESHOLD_RATIO) as usize
}

// ── Error detection ──────────────────────────────────────────────────────

/// Detect whether an API error message indicates the context window was exceeded.
pub fn is_context_length_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    [
        "prompt is too long",
        "context_length_exceeded",
        "context window",
        "maximum context length",
        "too many tokens",
        "token limit",
        "exceeds the model's",
        "reduce the length",
        "input length",
        "context length",
        "maximum of",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

// ── Compaction helpers ───────────────────────────────────────────────────

/// Count how many messages from the **end** of the list constitute the last
/// `min_turns` conversation turns.  A turn starts with a `role: "user"` message.
pub fn count_turns_from_end(messages: &[Message], min_turns: usize) -> usize {
    let mut user_count = 0usize;
    for (idx, m) in messages.iter().enumerate().rev() {
        if m.role == "user" {
            user_count += 1;
            if user_count >= min_turns {
                return messages.len() - idx;
            }
        }
    }
    messages.len()
}

/// Build a compact text representation of messages for the summarization prompt.
pub fn build_conversation_text(messages: &[Message]) -> String {
    let mut out = String::with_capacity(messages.len() * 256);
    for m in messages {
        let role = &m.role;
        let content: String = m.content.chars().take(MAX_CHARS_PER_MESSAGE_IN_SUMMARY).collect();
        out.push_str(&format!("[{role}]: {content}"));

        if let Some(ref tcs) = m.tool_calls {
            for tc in tcs {
                let args: String = tc
                    .function
                    .arguments
                    .chars()
                    .take(MAX_CHARS_PER_MESSAGE_IN_SUMMARY / 2)
                    .collect();
                out.push_str(&format!("\n  [tool_call] {}: {}", tc.function.name, args));
            }
        }

        if let Some(ref thinking) = m.thinking {
            let t: String = thinking.chars().take(MAX_CHARS_PER_MESSAGE_IN_SUMMARY / 2).collect();
            out.push_str(&format!("\n  [thinking] {t}"));
        }

        out.push('\n');
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        crate::store::Message {
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
}
