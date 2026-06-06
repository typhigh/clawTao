use serde::Serialize;

/// ToolSpec sent to LLM as part of the `tools` array.
/// Serializes to OpenAI function-calling format:
/// `{"type": "function", "function": {"name": "...", "description": "...", "parameters": {...}}}`
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolSpec {
    /// Always "function" for now
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Nested function definition
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            tool_type: "function".into(),
            function: ToolFunction {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
