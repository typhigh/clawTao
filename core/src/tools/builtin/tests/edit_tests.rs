use std::sync::atomic::AtomicBool;
use super::*;

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
fn edit_replace_single_occurrence() {
    let tool = EditTool;
    let tmp = tmp_file("test_edit.txt");
    std::fs::write(&tmp, "hello world").unwrap();

    tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "old_string": "hello",
        "new_string": "hi"
    }), &AtomicBool::new(false)).unwrap();

    assert_eq!(std::fs::read_to_string(&tmp).unwrap(), "hi world");
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn edit_multiple_occurrences_fails() {
    let tool = EditTool;
    let tmp = tmp_file("test_edit2.txt");
    std::fs::write(&tmp, "aa bb aa").unwrap();

    let err = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "old_string": "aa",
        "new_string": "cc"
    }), &AtomicBool::new(false)).unwrap_err();

    assert!(format!("{err}").contains("2 times"));
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn edit_not_found_fails() {
    let tool = EditTool;
    let tmp = tmp_file("test_edit3.txt");
    std::fs::write(&tmp, "hello").unwrap();

    let err = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "old_string": "xyz",
        "new_string": "abc"
    }), &AtomicBool::new(false)).unwrap_err();

    assert!(format!("{err}").contains("not found"));
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn edit_replace_all() {
    let tool = EditTool;
    let tmp = tmp_file("test_edit4.txt");
    std::fs::write(&tmp, "aa bb aa cc aa").unwrap();

    let result = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "old_string": "aa",
        "new_string": "xx",
        "replace_all": true
    }), &AtomicBool::new(false)).unwrap();

    assert_eq!(std::fs::read_to_string(&tmp).unwrap(), "xx bb xx cc xx");
    assert!(result.contains("3 replacement(s)"));
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn edit_replace_all_not_found_fails() {
    let tool = EditTool;
    let tmp = tmp_file("test_edit5.txt");
    std::fs::write(&tmp, "hello").unwrap();

    let err = tool.execute(serde_json::json!({
        "path": tmp.to_str().unwrap(),
        "old_string": "xyz",
        "new_string": "abc",
        "replace_all": true
    }), &AtomicBool::new(false)).unwrap_err();

    assert!(format!("{err}").contains("not found"));
    std::fs::remove_file(&tmp).ok();
}
