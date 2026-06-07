use super::*;

#[test]
fn tool_error_invalid_input_display() {
    let err = ToolError::InvalidInput("missing path".into());
    assert!(format!("{err}").contains("invalid tool input"));
}

#[test]
fn tool_error_execution_display() {
    let err = ToolError::Execution("io error".into());
    assert!(format!("{err}").contains("tool execution failed"));
}
