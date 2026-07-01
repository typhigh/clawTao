use std::sync::atomic::AtomicBool;
use super::*;
use crate::tools::executor::ToolError;

#[test]
fn write_and_read_back() {
    let tool = WriteTool;
    let tmp = std::env::temp_dir().join("clawtao_test_write.txt");
    let result = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "content": "test content"
    }), &AtomicBool::new(false));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("bytes"));

    let read_back = std::fs::read_to_string(&tmp).unwrap();
    assert_eq!(read_back, "test content");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn write_missing_content() {
    let tool = WriteTool;
    let result = tool.execute(serde_json::json!({"path": "/tmp/test.txt"}), &AtomicBool::new(false));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::InvalidInput(_)));
}
