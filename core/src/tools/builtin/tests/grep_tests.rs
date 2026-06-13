use super::*;

#[test]
fn grep_finds_matches() {
    let tool = GrepTool;
    let dir = std::env::temp_dir().join("clawtao_test_grep");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), "hello world\nfoo bar").unwrap();
    std::fs::write(dir.join("b.txt"), "goodbye\nhello again").unwrap();

    let result = tool.execute(serde_json::json!({
        "pattern": "hello",
        "path": dir.to_str().unwrap(),
    })).unwrap();
    assert!(result.contains("a.txt"));
    assert!(result.contains("b.txt"));
    assert!(result.contains("hello"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_no_matches() {
    let tool = GrepTool;
    let dir = std::env::temp_dir().join("clawtao_test_grep2");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), "foo bar").unwrap();

    let result = tool.execute(serde_json::json!({
        "pattern": "xyz123",
        "path": dir.to_str().unwrap(),
    })).unwrap();
    assert!(result.contains("No matches"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_with_include() {
    let tool = GrepTool;
    let dir = std::env::temp_dir().join("clawtao_test_grep3");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn main() { hello }").unwrap();
    std::fs::write(dir.join("b.ts"), "const x = 'hello'").unwrap();

    let result = tool.execute(serde_json::json!({
        "pattern": "hello",
        "path": dir.to_str().unwrap(),
        "include": "*.rs",
    })).unwrap();
    assert!(result.contains("a.rs"));
    assert!(!result.contains("b.ts"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_invalid_regex() {
    let tool = GrepTool;
    let result = tool.execute(serde_json::json!({"pattern": "[invalid"}));
    assert!(result.is_err());
}

#[test]
fn grep_skips_hidden_dirs() {
    let tool = GrepTool;
    let dir = std::env::temp_dir().join("clawtao_test_grep5");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("node_modules")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join(".git/a.txt"), "hello").unwrap();
    std::fs::write(dir.join("node_modules/b.txt"), "hello").unwrap();
    std::fs::write(dir.join("src/c.txt"), "hello").unwrap();

    let result = tool.execute(serde_json::json!({
        "pattern": "hello", "path": dir.to_str().unwrap(),
    })).unwrap();
    assert!(result.contains("src/c.txt"));
    assert!(!result.contains(".git"));
    assert!(!result.contains("node_modules"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_on_single_file() {
    let tool = GrepTool;
    let dir = std::env::temp_dir().join("clawtao_test_grep6");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.rs");
    std::fs::write(&file, "fn main() {}\n// TODO: fix\n").unwrap();

    let result = tool.execute(serde_json::json!({
        "pattern": "TODO", "path": file.to_str().unwrap(),
    })).unwrap();
    assert!(result.contains("TODO"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_path_not_found() {
    let tool = GrepTool;
    let result = tool.execute(serde_json::json!({
        "pattern": "x", "path": "/nonexistent/path"
    }));
    assert!(result.is_err());
}

#[test]
fn grep_truncation_on_many_matches() {
    let tool = GrepTool;
    let dir = std::env::temp_dir().join("clawtao_test_grep7");
    std::fs::create_dir_all(&dir).unwrap();
    let mut content = String::new();
    for i in 0..120 { content.push_str(&format!("line{i} match\n")); }
    std::fs::write(dir.join("big.txt"), &content).unwrap();

    let result = tool.execute(serde_json::json!({
        "pattern": "match", "path": dir.to_str().unwrap(),
    })).unwrap();
    assert!(result.contains("Found 120 matches"));
    assert!(result.contains("showing first 100"));
    std::fs::remove_dir_all(&dir).ok();
}
