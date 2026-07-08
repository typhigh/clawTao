use crate::tools::builtin::sandbox::SandboxRules;
use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;

pub struct WriteTool;

impl ToolExecutor for WriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "Write",
            "Write content to a file at the given path. Creates the file if it doesn't exist, overwrites if it does.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        )
    }

    fn check_sandbox(&self, input: &serde_json::Value, rules: &SandboxRules) -> Result<(), String> {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        rules.path_is_allowed(path)
    }

    fn execute(&self, input: serde_json::Value, _cancel: &std::sync::atomic::AtomicBool) -> Result<String, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'path'".into()))?;

        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'content'".into()))?;

        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ToolError::Execution(format!("Write({path}): mkdir failed: {e}")))?;
        }

        std::fs::write(path, content)
            .map_err(|e| ToolError::Execution(format!("Write({path}): {e}")))?;

        Ok(format!("Successfully wrote {} bytes to {path}", content.len()))
    }
}

#[cfg(test)]
#[path = "tests/write_tests.rs"]
mod tests;
