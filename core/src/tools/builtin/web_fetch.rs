use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;

pub struct WebFetchTool;

impl ToolExecutor for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "WebFetch",
            "Fetch content from a URL via HTTP GET and return the page text (HTML tags stripped). Use for quick page content retrieval without opening a browser.",
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The URL to fetch" }
                },
                "required": ["url"]
            }),
        )
    }

    fn execute(&self, input: serde_json::Value) -> Result<String, ToolError> {
        let url = input.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'url'".into()))?;

        let resp = reqwest::blocking::get(url)
            .map_err(|e| ToolError::Execution(format!("WebFetch {url}: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ToolError::Execution(format!("WebFetch HTTP {}: {}", status.as_u16(), url)));
        }

        let html = resp.text()
            .map_err(|e| ToolError::Execution(format!("WebFetch: {e}")))?;

        // Simple HTML-to-text: strip tags, decode entities, collapse whitespace
        let text = strip_html(&html);

        if text.trim().is_empty() {
            return Err(ToolError::Execution("Page returned empty content".into()));
        }

        Ok(text)
    }
}

fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut last_was_newline = false;

    // Iterate the lowercased copy to get byte indices that are valid for
    // slicing into `lower`. Track the original chars in parallel so we
    // output the real casing of the page text.
    let lower = html.to_lowercase();
    let mut html_chars = html.chars();
    for (i, lo_ch) in lower.char_indices() {
        let ch = html_chars.next().unwrap_or(lo_ch);
        if ch == '<' {
            in_tag = true;
            // Check if this is a <script> or <style> tag
            if lower[i..].starts_with("<script") {
                in_script = true;
            } else if lower[i..].starts_with("<style") {
                in_style = true;
            }
            continue;
        }
        if ch == '>' {
            in_tag = false;
            // Check if </script> or </style> just ended
            if in_script && lower[i.saturating_sub(8)..=i].contains("</script>") {
                in_script = false;
            }
            if in_style && lower[i.saturating_sub(7)..=i].contains("</style>") {
                in_style = false;
            }
            continue;
        }
        if in_tag || in_script || in_style {
            continue;
        }

        // Normalize whitespace
        if ch.is_whitespace() {
            if !last_was_newline {
                result.push(' ');
                last_was_newline = true;
            }
        } else {
            result.push(ch);
            last_was_newline = false;
        }
    }

    // Decode common HTML entities
    result = result.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Collapse multiple newlines
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }

    result.trim().to_string()
}

#[cfg(test)]
#[path = "tests/web_fetch_tests.rs"]
mod tests;
