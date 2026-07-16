use super::executor::ToolExecutor;
use super::spec::ToolSpec;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Central registry for all available tools.
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: BTreeMap::new() }
    }

    pub fn register(&mut self, tool: Arc<dyn ToolExecutor>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn ToolExecutor>> {
        self.tools.get(name)
    }

    /// Remove a tool from the registry.  No-op if absent.
    pub fn unregister(&mut self, name: &str) -> bool {
        self.tools.remove(name).is_some()
    }

    /// All tool specs for sending to LLM (stable order by name).
    pub fn list_specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    /// Number of registered tools.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Tool names for logging.
    #[allow(dead_code)]
    pub fn names(&self) -> Vec<&str> {
        self.tools.values().map(|t| t.name()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "tests/registry_tests.rs"]
mod tests;
