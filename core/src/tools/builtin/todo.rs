use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;
use std::sync::atomic::AtomicBool;

/// Ephemeral task list for the current turn. Not persisted.
pub struct TodoWriteTool;

impl ToolExecutor for TodoWriteTool {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "TodoWrite",
            "Create and update a prioritized task list for the current turn. \
             Use this to plan and track your progress on complex multi-step tasks. \
             Each item has a step description and a status. \
             At most one step should be in_progress at a time. \
             The list is ephemeral — it is not saved between turns.",
            json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "step": { "type": "string", "description": "Task description" },
                                "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                            },
                            "required": ["step", "status"]
                        }
                    }
                },
                "required": ["todos"]
            }),
        )
    }

    fn execute(&self, _input: serde_json::Value, _cancel: &AtomicBool) -> Result<String, ToolError> {
        Ok("ok".to_string())
    }
}
