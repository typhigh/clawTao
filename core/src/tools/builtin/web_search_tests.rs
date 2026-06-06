use super::*;

#[test]
fn web_search_placeholder() {
    let tool = WebSearchTool;
    let result = tool.execute(serde_json::json!({"query": "test"}));
    assert!(result.is_ok());
    assert!(result.unwrap().contains("not yet implemented"));
}
