use crate::tools::executor::{ToolError, ToolExecutor};
use crate::tools::spec::ToolSpec;
use super::ToolRegistry;
use serde_json::json;
use std::sync::Arc;

struct MockTool {
    name: &'static str,
    spec: ToolSpec,
}

impl ToolExecutor for MockTool {
    fn name(&self) -> &str { self.name }
    fn spec(&self) -> ToolSpec { self.spec.clone() }
    fn execute(&self, _input: serde_json::Value, _cancel: &std::sync::atomic::AtomicBool) -> Result<String, ToolError> {
        Ok("mock result".into())
    }
}

#[test]
fn register_and_get() {
    let mut reg = ToolRegistry::new();
    let tool = Arc::new(MockTool {
        name: "pwd",
        spec: ToolSpec::new("pwd", "print working dir", json!({"type": "object"})),
    });
    reg.register(tool);
    assert_eq!(reg.len(), 1);
    assert!(reg.get("pwd").is_some());
    assert!(reg.get("nonexistent").is_none());
}

#[test]
fn list_specs() {
    let mut reg = ToolRegistry::new();
    reg.register(Arc::new(MockTool {
        name: "t1",
        spec: ToolSpec::new("t1", "tool 1", json!({"type": "object"})),
    }));
    reg.register(Arc::new(MockTool {
        name: "t2",
        spec: ToolSpec::new("t2", "tool 2", json!({"type": "object"})),
    }));
    let specs = reg.list_specs();
    assert_eq!(specs.len(), 2);
}

#[test]
fn registry_starts_empty() {
    let reg = ToolRegistry::new();
    assert_eq!(reg.len(), 0);
    assert!(reg.list_specs().is_empty());
}
