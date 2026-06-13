use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;

fn browser_server_url() -> Result<String, ToolError> {
    let port_file = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("clawtao")
        .join("browser-port");
    let port = std::fs::read_to_string(&port_file)
        .map_err(|e| ToolError::Execution(format!("Cannot read browser port file ({port_file:?}): {e}. Start the browser server with: node core/scripts/browser-server.mjs")))?
        .trim()
        .to_string();
    if port.is_empty() {
        return Err(ToolError::Execution("Browser port file is empty. Start the browser server with: node core/scripts/browser-server.mjs".into()));
    }
    Ok(format!("http://127.0.0.1:{port}"))
}

pub struct WebBrowserTool;

impl ToolExecutor for WebBrowserTool {
    fn name(&self) -> &str {
        "WebBrowser"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new("WebBrowser",
            "Control a visible Chromium browser. Actions: start, stop, navigate, snapshot, screenshot, click, type, evaluate, tabs, newTab.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["start","stop","navigate","search","snapshot","screenshot","click","type","evaluate","tabs","newTab"] },
                    "url":    { "type": "string", "description": "URL or search keywords" },
                    "selector": { "type": "string", "description": "CSS selector for click/type/evaluate" },
                    "text":   { "type": "string", "description": "Text to type" }
                },
                "required": ["action"]
            }),
        )
    }

    fn execute(&self, input: serde_json::Value) -> Result<String, ToolError> {
        let client = reqwest::blocking::Client::new();
        let resp = client.post(browser_server_url()?)
            .json(&input)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .map_err(|e| ToolError::Execution(format!("Browser server not reachable: {e}. Start it with: node core/scripts/browser-server.mjs")))?;

        let result: serde_json::Value = resp.json().map_err(|e| ToolError::Execution(format!("Parse error: {e}")))?;

        if result.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
                Ok(text.to_string())
            } else if let Some(title) = result.get("title").and_then(|v| v.as_str()) {
                Ok(format!("Opened: {title} ({})", result["url"].as_str().unwrap_or("")))
            } else if let Some(msg) = result.get("message").and_then(|v| v.as_str()) {
                Ok(msg.to_string())
            } else {
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
        } else {
            Err(ToolError::Execution(result["error"].as_str().unwrap_or("unknown error").to_string()))
        }
    }
}
