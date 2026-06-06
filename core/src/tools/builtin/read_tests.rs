use super::*;
use crate::tools::executor::ToolError;

#[test]
fn read_existing_file() {
    let tool = ReadTool;
    let tmp = std::env::temp_dir().join("clawtao_test_read.txt");
    std::fs::write(&tmp, "hello world").unwrap();
    let result = tool.execute(serde_json::json!({"path": tmp.to_str().unwrap()}));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "hello world");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn read_missing_file() {
    let tool = ReadTool;
    let result = tool.execute(serde_json::json!({"path": "/nonexistent/file"}));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::Execution(_)));
}

#[test]
fn read_missing_param() {
    let tool = ReadTool;
    let result = tool.execute(serde_json::json!({}));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::InvalidInput(_)));
}
