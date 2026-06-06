use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;

pub struct WebSearchTool;

impl ToolExecutor for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "WebSearch",
            "Search the web for information. Returns search results.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    }
                },
                "required": ["query"]
            }),
        )
    }

    fn execute(&self, _input: serde_json::Value) -> Result<String, ToolError> {
        // TODO: implement real web search via an external API
        Ok("WebSearch is not yet implemented. Please use other tools to find information.".to_string())
    }
}

#[cfg(test)]
#[path = "web_search_tests.rs"]
mod tests;
