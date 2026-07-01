use super::spec::ToolSpec;
use std::fmt;
use std::sync::atomic::AtomicBool;

/// Reasons a tool call can fail.
/// `Execution` means the tool ran but produced an error.
/// `InvalidInput` means required parameters were missing or malformed.
#[derive(Debug)]
pub enum ToolError {
    Execution(String),
    InvalidInput(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Execution(msg) => write!(f, "tool execution failed: {msg}"),
            Self::InvalidInput(msg) => write!(f, "invalid tool input: {msg}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// Runtime contract for a tool that can be called by LLM.
pub trait ToolExecutor: Send + Sync {
    fn name(&self) -> &str;
    fn spec(&self) -> ToolSpec;

    /// Execute this tool with the arguments parsed from the LLM's tool_call.
    /// `input` is the JSON value of `tool_calls[].function.arguments`.
    /// `cancel` is set to true when the user interrupts the turn — tools
    /// should check it periodically and return "[interrupted by user]" promptly.
    fn execute(&self, input: serde_json::Value, cancel: &AtomicBool) -> Result<String, ToolError>;
}

#[cfg(test)]
#[path = "tests/executor_tests.rs"]
mod tests;
