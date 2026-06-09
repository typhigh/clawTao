use crate::store::ToolCall;

/// Protocol-agnostic LLM request.
pub struct LlmRequest {
    pub system: String,
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<UnifiedTool>,
}

/// Protocol-agnostic LLM response (accumulated from SSE stream).
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
}

/// Internal message representation.
pub struct LlmMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

/// Unified tool definition (serialized per-protocol at boundary).
#[derive(Clone)]
pub struct UnifiedTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
