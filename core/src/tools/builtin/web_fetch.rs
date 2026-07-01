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

    fn execute(&self, input: serde_json::Value, _cancel: &std::sync::atomic::AtomicBool) -> Result<String, ToolError> {
        let url = input.get("url").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'url'".into()))?;

        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; ClawTao/0.1)")
            .build()
            .map_err(|e| ToolError::Execution(format!("WebFetch: {e}")))?;

        let resp = client.get(url).send()
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

/// HTML-to-text via a real parser (`html5ever` / `scraper`).
///
/// All entity decoding, tag matching, and UTF-8 handling is done by the
/// parser — no hand-rolled state machine, no byte slicing.
fn strip_html(html: &str) -> String {
    let doc = scraper::Html::parse_document(html);

    let mut text = String::with_capacity(html.len());
    // Walk the DOM tree from <html> down, skipping <script> and <style> subtrees.
    let root = doc.root_element();
    collect_text(&root, &mut text);

    // Normalise whitespace: collapse runs of space/newline into a single space.
    let mut result = String::with_capacity(text.len());
    let mut in_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !in_space {
                result.push(' ');
                in_space = true;
            }
        } else {
            result.push(ch);
            in_space = false;
        }
    }

    result.trim().to_string()
}

/// Recursively collect text from element children, skipping `<script>` and `<style>`.
fn collect_text(element: &scraper::ElementRef<'_>, out: &mut String) {
    for child in element.children() {
        if let Some(el) = child.value().as_element() {
            let name = el.name();
            if name == "script" || name == "style" {
                continue; // skip the entire subtree
            }
        }
        if let Some(t) = child.value().as_text() {
            out.push_str(t);
        }
        if let Some(el_ref) = scraper::ElementRef::wrap(child) {
            collect_text(&el_ref, out);
        }
    }
}

#[cfg(test)]
#[path = "tests/web_fetch_tests.rs"]
mod tests;
