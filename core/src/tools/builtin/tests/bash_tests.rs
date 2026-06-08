use super::*;
use crate::tools::executor::ToolError;

#[test]
fn bash_echo() {
    let tool = BashTool::new(vec![]);
    let result = tool.execute(serde_json::json!({"command": "echo hello"}));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("hello"));
}

#[test]
fn bash_missing_command() {
    let tool = BashTool::new(vec![]);
    let result = tool.execute(serde_json::json!({}));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::InvalidInput(_)));
}

#[test]
fn bash_exit_code() {
    let tool = BashTool::new(vec![]);
    let result = tool.execute(serde_json::json!({"command": "true"}));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("exit code"));
}

#[test]
fn bash_blocked_command() {
    let tool = BashTool::new(vec!["rm -rf /".into()]);
    let result = tool.execute(serde_json::json!({"command": "rm -rf / --no-preserve-root"}));
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("Blocked"));
}
