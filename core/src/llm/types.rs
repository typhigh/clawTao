use crate::store::ToolCall;

/// Image content attached to a message (user upload or tool output).
#[derive(Debug, Clone)]
pub struct ImageContent {
    pub base64: String,
    pub media_type: String, // "image/png" | "image/jpeg" | "image/gif" | "image/webp"
}

/// Protocol-agnostic LLM request.
pub struct LlmRequest {
    pub system: String,
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<UnifiedTool>,
    pub thinking_enabled: bool,
}

/// Protocol-agnostic LLM response (accumulated from SSE stream).
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    /// Accumulated thinking text.
    pub thinking: Option<String>,
}

/// Internal message representation.
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
    pub images: Option<Vec<ImageContent>>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub thinking: Option<String>,
}

/// Unified tool definition (serialized per-protocol at boundary).
#[derive(Clone)]
pub struct UnifiedTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
