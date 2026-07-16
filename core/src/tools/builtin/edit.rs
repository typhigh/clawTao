use crate::tools::builtin::sandbox::SandboxRules;
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
            "Performs exact string replacement in a file. When `replace_all` is false (default), `old_string` must be unique. When true, replaces every occurrence.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact text to replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The text to replace it with"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "If true, replace all occurrences instead of requiring exactly one match (default: false)"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        )
    }

    fn check_sandbox(&self, input: &serde_json::Value, rules: &SandboxRules) -> Result<(), String> {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        // Edit reads the file before writing — check both policies.
        rules.read_path_is_allowed(path)?;
        rules.write_path_is_allowed(path)
    }

    fn execute(&self, input: serde_json::Value, _cancel: &std::sync::atomic::AtomicBool) -> Result<String, ToolError> {
        let path = input.get("path").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'path'".into()))?;

        let old_string = input.get("old_string").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'old_string'".into()))?;

        let new_string = input.get("new_string").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'new_string'".into()))?;

        let replace_all = input.get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let content = std::fs::read_to_string(path)
            .map_err(|e| ToolError::Execution(format!("Edit({path}): read failed: {e}")))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(ToolError::Execution(format!(
                "Edit({path}): old_string not found in file"
            )));
        }

        if !replace_all {
            if count > 1 {
                return Err(ToolError::Execution(format!(
                    "Edit({path}): old_string found {count} times (must be unique). Use replace_all: true to replace all."
                )));
            }
            let new_content = content.replacen(old_string, new_string, 1);
            std::fs::write(path, &new_content)
                .map_err(|e| ToolError::Execution(format!("Edit({path}): write failed: {e}")))?;
            Ok(format!("Successfully edited {path}"))
        } else {
            let new_content = content.replace(old_string, new_string);
            std::fs::write(path, &new_content)
                .map_err(|e| ToolError::Execution(format!("Edit({path}): write failed: {e}")))?;
            Ok(format!("Successfully edited {path}: {count} replacement(s)"))
        }
    }
}

#[cfg(test)]
#[path = "tests/edit_tests.rs"]
mod tests;
