//! Tests for the building blocks of the session.context_stats handler.
//!
//! Verifies the system/message token split, the context-window lookup,
//! and that the handler's inputs (store round-trip, system-prompt builder,
//! schema-only tool registry) behave correctly. The full JSON-RPC write
//! path is exercised by the integration harness; here we cover the parts
//! that a regression would break silently.

use crate::store::json_store::JsonSessionStore;
use crate::store::store_trait::SessionStore;
use crate::store::{Message, Session};

fn temp_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn msg(id: &str, role: &str, content: &str) -> Message {
    Message {
        id: id.into(),
        role: role.into(),
        content: content.into(),
        tool_calls: None,
        tool_call_id: None,
        thinking: None,
        timestamp: 1000,
        image_paths: None,
    }
}

fn build_session(messages: Vec<Message>, compacted_summary: Option<String>, compacted_id: Option<String>) -> Session {
    Session {
        id: "s1".into(),
        created_at: 1000,
        updated_at: 2000,
        messages,
        title: String::new(),
        compacted_summary,
        compacted_message_id: compacted_id,
    }
}

// ── Context-window lookup ──────────────────────────────────────────

#[test]
fn context_window_matches_provider() {
    use crate::context::provider_context_window;
    assert_eq!(provider_context_window("https://api.deepseek.com/anthropic"), 1_000_000);
    assert_eq!(provider_context_window("https://api.minimaxi.com/anthropic"), 1_000_000);
    assert_eq!(provider_context_window("https://unknown.example.com"), 256_000);
}

// ── System prompt reflects workspace ──────────────────────────────

#[test]
fn system_prompt_includes_workspace_when_set() {
    use crate::system_prompt::build;
    use crate::tools::registry::ToolRegistry;
    let mut reg = ToolRegistry::new();
    crate::tools::builtin::register_all(
        &mut reg,
        crate::tools::builtin::SandboxConfig::off(),
        None,
    );
    let with_ws = build(&reg, Some("/tmp/sandbox"), &[], &[]);
    assert!(with_ws.contains("/tmp/sandbox"));
    let without_ws = build(&reg, None, &[], &[]);
    assert!(!without_ws.contains("Sandbox is active"));
}

#[test]
fn estimate_total_with_workspace_grows_vs_without() {
    use crate::context::estimate_total_tokens_from_store;
    use crate::tools::builtin::SandboxConfig;
    use crate::tools::registry::ToolRegistry;

    fn build_with(workspace: Option<&str>) -> String {
        let mut reg = ToolRegistry::new();
        crate::tools::builtin::register_all(&mut reg, SandboxConfig::off(), None);
        crate::system_prompt::build(&reg, workspace, &[], &[])
    }

    fn tools() -> Vec<crate::llm::UnifiedTool> {
        let mut reg = crate::tools::registry::ToolRegistry::new();
        crate::tools::builtin::register_all(&mut reg, SandboxConfig::off(), None);
        reg.list_specs().iter().map(|s| crate::llm::UnifiedTool {
            name: s.function.name.clone(),
            description: s.function.description.clone(),
            parameters: s.function.parameters.clone(),
        }).collect()
    }

    let s_with = build_with(Some("/tmp/foo"));
    let s_without = build_with(None);
    let t = tools();
    let n_with = estimate_total_tokens_from_store(&s_with, &[], &t, "");
    let n_without = estimate_total_tokens_from_store(&s_without, &[], &t, "");
    // Workspace adds the "Sandbox is active: ..." line — strictly more tokens.
    assert!(n_with > n_without, "expected with-workspace tokens > without, got {n_with} vs {n_without}");
}

// ── Schema-only registry shape ────────────────────────────────────

#[test]
fn schema_registry_has_eight_tools() {
    let mut reg = crate::tools::registry::ToolRegistry::new();
    crate::tools::builtin::register_all(
        &mut reg,
        crate::tools::builtin::SandboxConfig::off(),
        None,
    );
    let names: Vec<&str> = reg.names().into_iter().collect();
    assert_eq!(names.len(), 8, "expected 8 built-in tools, got {names:?}");
}

// ── Store round-trip (handler input) ──────────────────────────────

#[test]
fn store_round_trip_preserves_messages_for_handler_input() {
    let dir = temp_dir();
    let store = JsonSessionStore::new(dir);
    let session = build_session(
        vec![msg("u1", "user", "hello"), msg("a1", "assistant", "hi back")],
        None,
        None,
    );
    store.create(&session).unwrap();

    let loaded = store.get("s1").unwrap().unwrap();
    assert_eq!(loaded.messages.len(), 2);
    assert_eq!(loaded.messages[0].content, "hello");
    assert_eq!(loaded.messages[1].content, "hi back");
}

#[test]
fn store_round_trip_preserves_compaction_metadata() {
    let dir = temp_dir();
    let store = JsonSessionStore::new(dir);
    let session = build_session(
        vec![msg("u1", "user", "hi")],
        Some("summary text".into()),
        Some("u1".into()),
    );
    store.create(&session).unwrap();

    let loaded = store.get("s1").unwrap().unwrap();
    assert_eq!(loaded.compacted_summary.as_deref(), Some("summary text"));
    assert_eq!(loaded.compacted_message_id.as_deref(), Some("u1"));
}

// ── Effective-messages accounting (handler computes against this) ─

#[test]
fn effective_messages_drops_compacted_prefix() {
    use crate::chat::build_effective_messages;
    let messages = vec![
        msg("u1", "user", "old 1"),
        msg("a1", "assistant", "old 2"),
        msg("u2", "user", "new 1"),
        msg("a2", "assistant", "new 2"),
    ];
    // Compact everything up to and including "u1" — only messages after
    // "u1" remain in the effective list.
    let effective = build_effective_messages(&messages, "u1", "summary");
    assert_eq!(effective.len(), 3);
    assert_eq!(effective[0].id, "a1"); // messages after compacted_id
    // First kept user message gets the summary prepended.
    let u2_idx = effective.iter().position(|m| m.id == "u2").unwrap();
    assert!(effective[u2_idx].content.starts_with("Another language model"));
    assert!(effective[u2_idx].content.contains("summary"));
    assert!(effective[u2_idx].content.contains("new 1"));
}