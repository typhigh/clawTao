use super::*;

#[test]
fn serialize_function_tool() {
    let spec = ToolSpec::new(
        "Read",
        "Read a file",
        serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}}),
    );
    let json = serde_json::to_string(&spec).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "function");
    assert_eq!(parsed["function"]["name"], "Read");
    assert_eq!(parsed["function"]["description"], "Read a file");
}

#[test]
fn serialize_has_flat_structure() {
    let spec = ToolSpec::new("Bash", "Run command", serde_json::json!({"type": "object"}));
    let json = serde_json::to_value(&spec).unwrap();
    // Outer: {"type": "function", "function": {...}}
    assert_eq!(json.get("type").and_then(|v| v.as_str()), Some("function"));
    assert!(json.get("function").is_some());
    assert_eq!(json["function"]["name"], "Bash");
}
