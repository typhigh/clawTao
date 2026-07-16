use std::sync::atomic::AtomicBool;
use super::*;
use crate::tools::executor::ToolError;
use crate::tools::builtin::sandbox::SandboxConfig;

#[test]
fn bash_echo() {
    let tool = BashTool::new(SandboxConfig::off(), Some(30));
    let result = tool.execute(serde_json::json!({"command": "echo hello"}), &AtomicBool::new(false));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("hello"));
}

#[test]
fn bash_missing_command() {
    let tool = BashTool::new(SandboxConfig::off(), Some(30));
    let result = tool.execute(serde_json::json!({}), &AtomicBool::new(false));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::InvalidInput(_)));
}

#[test]
fn bash_exit_code() {
    let tool = BashTool::new(SandboxConfig::off(), Some(30));
    let result = tool.execute(serde_json::json!({"command": "true"}), &AtomicBool::new(false));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("exit code"));
}

#[test]
fn bash_interrupted() {
    let tool = BashTool::new(SandboxConfig::off(), Some(30));
    let cancel = AtomicBool::new(true); // pre-set to true
    let result = tool.execute(serde_json::json!({"command": "sleep 10"}), &cancel);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "[interrupted by user]");
}

#[test]
fn sandbox_off_runs_directly() {
    let cfg = SandboxConfig::off();
    assert!(!cfg.is_active());
    let tool = BashTool::new(cfg, None);
    // Should not panic or error — runs without sandbox-exec wrapping.
    let result = tool.execute(serde_json::json!({"command": "echo sandbox-off"}), &AtomicBool::new(false));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("sandbox-off"));
}

#[test]
fn sandbox_active_config_is_active() {
    let cfg = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-sandbox".into()),
    };
    assert!(cfg.is_active());
}
