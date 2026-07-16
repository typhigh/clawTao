use crate::tools::builtin::sandbox::SandboxRules;
use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use regex::Regex;
use serde_json::json;
use std::fs;
use std::path::Path;

const MAX_MATCHES: usize = 100;
const MAX_LINE_LEN: usize = 500;

pub struct GrepTool;

impl ToolExecutor for GrepTool {
    fn name(&self) -> &str { "Grep" }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "Grep",
            "Search file contents using regex patterns. Returns matching files with line numbers and content. Faster and more structured than using Bash grep.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "The regex pattern to search for"},
                    "path":    {"type": "string", "description": "Directory or file to search. When read policy is Restricted, must be inside the configured workspace. Defaults to current directory."},
                    "include": {"type": "string", "description": "File glob to limit search (e.g. \"*.rs\", \"*.ts\")"}
                },
                "required": ["pattern"]
            }),
        )
    }

    fn check_sandbox(&self, input: &serde_json::Value, rules: &SandboxRules) -> Result<(), String> {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        rules.read_path_is_allowed(path)
    }

    fn execute(&self, input: serde_json::Value, _cancel: &std::sync::atomic::AtomicBool) -> Result<String, ToolError> {
        let pattern = input.get("pattern").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'pattern'".into()))?;
        let search_path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let include = input.get("include").and_then(|v| v.as_str());

        let re = Regex::new(pattern)
            .map_err(|e| ToolError::InvalidInput(format!("invalid regex: {e}")))?;

        let mut results: Vec<(String, usize, String)> = Vec::new();
        let search_root = Path::new(search_path);

        if search_root.is_file() {
            search_file(search_root, &re, &mut results);
        } else if search_root.is_dir() {
            search_dir(search_root, &re, include, &mut results);
        } else {
            return Err(ToolError::Execution(format!("path not found: {search_path}")));
        }

        if results.is_empty() {
            return Ok(format!("No matches found for pattern: {pattern}"));
        }

        let total = results.len();
        let truncated = total > MAX_MATCHES;
        if truncated { results.truncate(MAX_MATCHES); }

        let mut out = vec![format!("Found {total} match{} for \"{pattern}\"{}",
            if total == 1 { "" } else { "es" },
            if truncated { format!(" (showing first {MAX_MATCHES})") } else { String::new() }
        )];
        let mut last_file = String::new();
        for (file, line, text) in &results {
            if last_file != *file {
                if !last_file.is_empty() { out.push(String::new()); }
                last_file = file.clone();
                out.push(format!("{file}:"));
            }
            let display = if text.len() > MAX_LINE_LEN { format!("{}...", &text[..MAX_LINE_LEN]) } else { text.clone() };
            out.push(format!("  {line}: {display}"));
        }
        if truncated {
            out.push(String::new());
            out.push(format!("(Truncated: {total} total, showing {MAX_MATCHES})"));
        }
        Ok(out.join("\n"))
    }
}

fn search_dir(dir: &Path, re: &Regex, include: Option<&str>, results: &mut Vec<(String, usize, String)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || name == "node_modules" || name == "target" { continue; }
            search_dir(&path, re, include, results);
        } else if path.is_file() {
            if let Some(glob) = include {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !glob_match(glob, &name) { continue; }
            }
            search_file(&path, re, results);
        }
    }
}

fn search_file(path: &Path, re: &Regex, results: &mut Vec<(String, usize, String)>) {
    let Ok(content) = fs::read_to_string(path) else { return };
    for (i, line) in content.lines().enumerate() {
        if re.is_match(line) {
            results.push((path.to_string_lossy().to_string(), i + 1, line.trim().to_string()));
        }
    }
}

/// Simple glob match: "*.rs" matches "main.rs", "*.{ts,tsx}" matches "foo.ts"
fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" || pattern == "*.*" { return true; }
    if let Some(exts) = pattern.strip_prefix("*.{").and_then(|s| s.strip_suffix('}')) {
        return exts.split(',').any(|ext| name.ends_with(&format!(".{ext}")));
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        return name.ends_with(&format!(".{ext}"));
    }
    name.contains(pattern.trim_matches('*'))
}

#[cfg(test)]
#[path = "tests/grep_tests.rs"]
mod tests;
