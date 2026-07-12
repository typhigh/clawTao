use std::sync::atomic::AtomicBool;
use super::*;
use crate::tools::executor::ToolError;

fn test_temp_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target").join("tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tmp_file(name: &str) -> std::path::PathBuf {
    test_temp_dir().join(name)
}

#[test]
fn read_existing_file() {
    let tool = ReadTool;
    let tmp = tmp_file("test_read.txt");
    std::fs::write(&tmp, "hello world").unwrap();
    let result = tool.execute(serde_json::json!({"path": tmp.to_str().unwrap()}), &AtomicBool::new(false));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "hello world");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn read_missing_file() {
    let tool = ReadTool;
    let result = tool.execute(serde_json::json!({"path": "/nonexistent/file"}), &AtomicBool::new(false));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::Execution(_)));
}

#[test]
fn read_missing_param() {
    let tool = ReadTool;
    let result = tool.execute(serde_json::json!({}), &AtomicBool::new(false));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ToolError::InvalidInput(_)));
}

#[test]
fn read_with_offset() {
    let tool = ReadTool;
    let tmp = tmp_file("test_read_offset.txt");
    let content = "line1\nline2\nline3\nline4\nline5";
    std::fs::write(&tmp, content).unwrap();

    // offset=3 should start from line3
    let result = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "offset": 3
    }), &AtomicBool::new(false)).unwrap();
    assert_eq!(result, "line3\nline4\nline5");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn read_with_limit() {
    let tool = ReadTool;
    let tmp = tmp_file("test_read_limit.txt");
    let content = "line1\nline2\nline3\nline4\nline5";
    std::fs::write(&tmp, content).unwrap();

    // limit=2 should return only first 2 lines with truncation notice
    let result = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "limit": 2
    }), &AtomicBool::new(false)).unwrap();
    assert!(result.starts_with("line1\nline2"));
    assert!(result.contains("Truncated"));
    assert!(result.contains("5 total lines"));
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn read_with_offset_and_limit() {
    let tool = ReadTool;
    let tmp = tmp_file("test_read_offlim.txt");
    let content = "a\nb\nc\nd\ne";
    std::fs::write(&tmp, content).unwrap();

    // offset=2, limit=2 should return lines 2-3 with truncation
    let result = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "offset": 2,
        "limit": 2
    }), &AtomicBool::new(false)).unwrap();
    assert!(result.starts_with("b\nc"));
    assert!(result.contains("Truncated"));
    assert!(result.contains("lines 2-3"));
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn read_offset_out_of_range() {
    let tool = ReadTool;
    let tmp = tmp_file("test_read_oob.txt");
    std::fs::write(&tmp, "only\nthree\nlines").unwrap();

    let result = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "offset": 10
    }), &AtomicBool::new(false)).unwrap();
    assert!(result.contains("offset 10 is out of range"));
    std::fs::remove_file(&tmp).ok();
}
