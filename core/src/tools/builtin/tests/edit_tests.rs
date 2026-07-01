use std::sync::atomic::AtomicBool;
use super::*;

#[test]
fn edit_replace_single_occurrence() {
    let tool = EditTool;
    let tmp = std::env::temp_dir().join("clawtao_test_edit.txt");
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
    let tmp = std::env::temp_dir().join("clawtao_test_edit2.txt");
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
    let tmp = std::env::temp_dir().join("clawtao_test_edit3.txt");
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
    let tmp = std::env::temp_dir().join("clawtao_test_edit4.txt");
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
    let tmp = std::env::temp_dir().join("clawtao_test_edit5.txt");
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
