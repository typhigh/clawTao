use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::warn;

use super::sandbox::{SandboxConfig, SandboxProfile};

pub struct BashTool {
    sandbox: SandboxConfig,
    timeout: Option<Duration>,
}

impl BashTool {
    pub fn new(sandbox: SandboxConfig, timeout_secs: Option<u64>) -> Self {
        Self { sandbox, timeout: timeout_secs.map(Duration::from_secs) }
    }
}

impl ToolExecutor for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn spec(&self) -> ToolSpec {
        let mut desc = String::from(
            "Execute a shell command and return its stdout and stderr. \
             Use for file operations, git, build commands, etc."
        );
        if self.sandbox.is_active() {
            desc.push_str(" Commands are sandboxed to the workspace directory via macOS Seatbelt.");
        }

        ToolSpec::new("Bash", &desc, json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" },
                "cwd": { "type": "string", "description": "Working directory for the command (optional)" }
            },
            "required": ["command"]
        }))
    }

    fn execute(&self, input: serde_json::Value, cancel: &AtomicBool) -> Result<String, ToolError> {
        let command = input.get("command").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'command'".into()))?;

        let cmd_cwd = input.get("cwd").and_then(|v| v.as_str());

        // ── Build the command: optionally wrap with sandbox-exec ──────────
        let mut cmd = if self.sandbox.is_active() {
            let profile = SandboxProfile::generate(&self.sandbox);
            warn!("Sandbox: workspace={}, mode={:?}",
                  self.sandbox.workspace_dir, self.sandbox.mode);
            match SandboxProfile::wrap_command(&self.sandbox, command) {
                Some(c) => {
                    warn!("Sandbox cmd: {:?}", c);
                    c
                }
                None => {
                    // sandbox-exec not available — fall back to direct exec
                    let mut c = Command::new("sh");
                    c.args(["-c", command]);
                    c
                }
            }
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        // Default to workspace dir when sandboxed so basic commands (pwd, ls) work.
        if let Some(cwd) = cmd_cwd {
            cmd.current_dir(cwd);
        } else if self.sandbox.is_active() {
            cmd.current_dir(&self.sandbox.workspace_dir);
        }

        // ── No timeout: simple output ─────────────────────────────────────
        let Some(timeout) = self.timeout else {
            return format_output(cmd.output().map_err(|e| ToolError::Execution(format!("Bash: {e}")))?);
        };

        // ── Timeout-aware: spawn + poll ───────────────────────────────────
        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::Execution(format!("Bash: {e}")))?;

        let start = Instant::now();
        loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok("[interrupted by user]".to_string());
            }
            match child.try_wait() {
                Ok(Some(_)) => {
                    let output = child
                        .wait_with_output()
                        .map_err(|e| ToolError::Execution(format!("Bash: {e}")))?;
                    break format_output(output);
                }
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ToolError::Execution(format!(
                        "Timed out after {}s",
                        timeout.as_secs()
                    )));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(ToolError::Execution(format!("Bash: {e}"))),
            }
        }
    }
}

fn format_output(output: std::process::Output) -> Result<String, ToolError> {
    let exit = output.status.code();
    let signalled = exit.is_none();
    let mut result = String::new();
    if !output.stdout.is_empty() {
        result.push_str(&format!(
            "stdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    if !output.stderr.is_empty() {
        if !result.is_empty() { result.push('\n'); }
        result.push_str(&format!(
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    if result.is_empty() {
        if signalled {
            result.push_str("(killed by sandbox: the command tried to access a restricted path or resource. \
                Check the workspace directory exists and is writable.)");
        } else {
            result.push_str(&format!("(exit code: {})", exit.unwrap_or(-1)));
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "tests/bash_tests.rs"]
mod tests;
