use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;
use std::process::Command;

pub struct BashTool;

impl ToolExecutor for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "Bash",
            "Execute a shell command and return its stdout and stderr. Use for file operations, git, build commands, etc.",
            json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory for the command (optional)"
                    }
                },
                "required": ["command"]
            }),
        )
    }

    fn execute(&self, input: serde_json::Value) -> Result<String, ToolError> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'command'".into()))?;

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        if let Some(cwd) = input.get("cwd").and_then(|v| v.as_str()) {
            cmd.current_dir(cwd);
        }

        let output = cmd
            .output()
            .map_err(|e| ToolError::Execution(format!("Bash: failed to execute: {e}")))?;

        let mut result = String::new();
        if !output.stdout.is_empty() {
            result.push_str(&format!("stdout:\n{}", String::from_utf8_lossy(&output.stdout)));
        }
        if !output.stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("stderr:\n{}", String::from_utf8_lossy(&output.stderr)));
        }
        if result.is_empty() {
            result.push_str(&format!("(exit code: {})", output.status.code().unwrap_or(-1)));
        }

        Ok(result)
    }
}

#[cfg(test)]
#[path = "bash_tests.rs"]
mod tests;
