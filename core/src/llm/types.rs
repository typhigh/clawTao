use crate::store::ToolCall;

/// Protocol-agnostic LLM request.
pub struct LlmRequest {
    pub system: String,
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<UnifiedTool>,
    pub thinking_enabled: bool,
}

/// Protocol-agnostic LLM response (accumulated from SSE stream).
#[derive(Debug)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    /// Accumulated thinking text. Persisted so it can be replayed in
    /// multi-turn conversations (providers require thinking blocks to be
    /// sent back unchanged on subsequent turns).
    pub thinking: Option<String>,
}

/// Internal message representation.
pub struct LlmMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    /// Thinking text for this assistant message, replayed back to the model.
    pub thinking: Option<String>,
}

/// Unified tool definition (serialized per-protocol at boundary).
#[derive(Clone)]
pub struct UnifiedTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
