use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct BashTool {
    blocked_commands: Vec<String>,
    timeout: Option<Duration>,
}

impl BashTool {
    pub fn new(blocked_commands: Vec<String>, timeout_secs: Option<u64>) -> Self {
        Self { blocked_commands, timeout: timeout_secs.map(Duration::from_secs) }
    }
}

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
                    "command": { "type": "string", "description": "The shell command to execute" },
                    "cwd": { "type": "string", "description": "Working directory for the command (optional)" }
                },
                "required": ["command"]
            }),
        )
    }

    fn execute(&self, input: serde_json::Value, cancel: &AtomicBool) -> Result<String, ToolError> {
        let command = input.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'command'".into()))?;

        for blocked in &self.blocked_commands {
            if command.contains(blocked) {
                return Err(ToolError::Execution(format!(
                    "Blocked: '{command}' matches blocked pattern '{blocked}'"
                )));
            }
        }

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

        let Some(timeout) = self.timeout else {
            return format_output(cmd.output().map_err(|e| ToolError::Execution(format!("Bash: {e}")))?);
        };
        let mut child = cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::Execution(format!("Bash: {e}")))?;
        let start = std::time::Instant::now();
        let output = loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill(); let _ = child.wait();
                return Ok("[interrupted by user]".to_string());
            }
            match child.try_wait() {
                Ok(Some(_)) => break child.wait_with_output().map_err(|e| ToolError::Execution(format!("Bash: {e}")))?,
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill(); let _ = child.wait();
                    return Err(ToolError::Execution(format!("Timed out after {}s", timeout.as_secs())));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(ToolError::Execution(format!("Bash: {e}"))),
            }
        };

        format_output(output)
    }
}

fn format_output(output: std::process::Output) -> Result<String, ToolError> {
    let mut result = String::new();
    if !output.stdout.is_empty() { result.push_str(&format!("stdout:\n{}", String::from_utf8_lossy(&output.stdout))); }
    if !output.stderr.is_empty() {
        if !result.is_empty() { result.push('\n'); }
        result.push_str(&format!("stderr:\n{}", String::from_utf8_lossy(&output.stderr)));
    }
    if result.is_empty() { result.push_str(&format!("(exit code: {})", output.status.code().unwrap_or(-1))); }
    Ok(result)
}

#[cfg(test)]
#[path = "tests/bash_tests.rs"]
mod tests;
