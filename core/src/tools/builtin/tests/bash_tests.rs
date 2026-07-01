use std::sync::atomic::AtomicBool;
use super::*;
use crate::tools::executor::ToolError;

#[test]
fn bash_echo() {
    let tool = BashTool::new(vec![], Some(30));
    let result = tool.execute(serde_json::json!({"command": "echo hello"}), &AtomicBool::new(false));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("hello"));
}

#[test]
fn bash_missing_command() {
    let tool = BashTool::new(vec![], Some(30));
    let result = tool.execute(serde_json::json!({}), &AtomicBool::new(false));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::InvalidInput(_)));
}

#[test]
fn bash_exit_code() {
    let tool = BashTool::new(vec![], Some(30));
    let result = tool.execute(serde_json::json!({"command": "true"}), &AtomicBool::new(false));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("exit code"));
}

#[test]
fn bash_blocked_command() {
    let tool = BashTool::new(vec!["rm -rf /".into()], Some(30));
    let result = tool.execute(serde_json::json!({"command": "rm -rf / --no-preserve-root"}), &AtomicBool::new(false));
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("Blocked"));
}

#[test]
fn bash_interrupted() {
    let tool = BashTool::new(vec![], Some(30));
    let cancel = AtomicBool::new(true); // pre-set to true
    let result = tool.execute(serde_json::json!({"command": "sleep 10"}), &cancel);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "[interrupted by user]");
}
