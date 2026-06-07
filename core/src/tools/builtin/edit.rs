use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;

pub struct EditTool;

impl ToolExecutor for EditTool {
    fn name(&self) -> &str {
        "Edit"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "Edit",
            "Perform exact string replacements in a file. If `old_string` is not unique in the file, the edit fails (no partial replacements).",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact text to replace (must be unique in the file)"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The text to replace it with"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        )
    }

    fn execute(&self, input: serde_json::Value) -> Result<String, ToolError> {
        let path = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'path'".into()))?;

        let old_string = input.get("old_string").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'old_string'".into()))?;

        let new_string = input.get("new_string").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'new_string'".into()))?;

        let content = std::fs::read_to_string(path)
            .map_err(|e| ToolError::Execution(format!("Edit({path}): read failed: {e}")))?;

        // Count occurrences — must be exactly 1
        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(ToolError::Execution(format!(
                "Edit({path}): old_string not found in file"
            )));
        }
        if count > 1 {
            return Err(ToolError::Execution(format!(
                "Edit({path}): old_string found {count} times (must be unique)"
            )));
        }

        let new_content = content.replacen(old_string, new_string, 1);
        std::fs::write(path, &new_content)
            .map_err(|e| ToolError::Execution(format!("Edit({path}): write failed: {e}")))?;

        Ok(format!("Successfully edited {path}"))
    }
}

#[cfg(test)]
#[path = "tests/edit_tests.rs"]
mod tests;
