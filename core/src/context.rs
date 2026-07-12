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
#[path = "tests/context_tests.rs"]
mod tests;
