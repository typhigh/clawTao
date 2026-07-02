use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use serde_json::json;
use std::sync::atomic::AtomicBool;

/// `update_todo` tool — ephemeral task list, per-turn only, not persisted.
pub struct UpdateTodoTool;

impl ToolExecutor for UpdateTodoTool {
    fn name(&self) -> &str {
        "update_todo"
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::new(
            "update_todo",
            "Create and update a task list for the current turn. Each item has a step description and a status: pending, in_progress, or completed. At most one step can be in_progress at a time.",
            json!({
                "type": "object",
                "properties": {
                    "explanation": { "type": "string", "description": "Brief explanation of the changes" },
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
