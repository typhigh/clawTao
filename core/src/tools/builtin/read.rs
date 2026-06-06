use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;

pub struct ReadTool;

impl ToolExecutor for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "Read",
            "Read the contents of a file at the given path. Returns the file contents as a string.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to read"
                    }
                },
                "required": ["path"]
            }),
        )
    }

    fn execute(&self, input: serde_json::Value) -> Result<String, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'path'".into()))?;

        std::fs::read_to_string(path).map_err(|e| ToolError::Execution(format!("Read({path}): {e}")))
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
