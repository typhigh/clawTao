use super::spec::ToolSpec;
use crate::tools::builtin::sandbox::SandboxRules;
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
    fn execute(&self, input: serde_json::Value, cancel: &AtomicBool) -> Result<String, ToolError>;

    /// Check whether this tool invocation violates sandbox rules.
    /// Default: no restriction. Override in tools that write files.
    fn check_sandbox(&self, _input: &serde_json::Value, _rules: &SandboxRules) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/executor_tests.rs"]
mod tests;
