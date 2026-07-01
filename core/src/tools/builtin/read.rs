use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;

pub struct ReadTool;

impl ToolExecutor for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "Read",
            "Reads a file from the local filesystem. You can optionally specify an offset and limit for paginated reading.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute or relative path to the file to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start reading from (default: 1)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read. If omitted, reads the entire file."
                    }
                },
                "required": ["path"]
            }),
        )
    }

    fn execute(&self, input: serde_json::Value, _cancel: &std::sync::atomic::AtomicBool) -> Result<String, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing or invalid 'path'".into()))?;

        let offset = input
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|n| n.max(1) as usize)
            .unwrap_or(1);

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let content = std::fs::read_to_string(path)
            .map_err(|e| ToolError::Execution(format!("Read({path}): {e}")))?;

        // Fast path: no offset/limit specified, return raw content as-is
        if offset == 1 && limit.is_none() {
            return Ok(content);
        }

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        if offset > total_lines {
            return Ok(format!(
                "File has {total_lines} lines, offset {offset} is out of range."
            ));
        }

        let start_idx = offset - 1;
        let end_idx = match limit {
            Some(lim) => (start_idx + lim).min(total_lines),
            None => total_lines,
        };
        let selected = &lines[start_idx..end_idx];

        let mut result = selected.join("\n");

        if end_idx < total_lines {
            result.push_str(&format!(
                "\n\n(Truncated: {total_lines} total lines, showing lines {offset}-{end_idx})"
            ));
        }

        Ok(result)
    }
}

#[cfg(test)]
#[path = "tests/read_tests.rs"]
mod tests;
