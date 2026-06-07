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
    })).unwrap();

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
    })).unwrap_err();

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
    })).unwrap_err();

    assert!(format!("{err}").contains("not found"));
    std::fs::remove_file(&tmp).ok();
}
