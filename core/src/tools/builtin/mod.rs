mod bash;
mod read;
mod web_search;
mod write;

use super::registry::ToolRegistry;
use std::sync::Arc;

pub use bash::BashTool;
pub use read::ReadTool;
pub use web_search::WebSearchTool;
pub use write::WriteTool;

/// Register all built-in tools.
pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(Arc::new(ReadTool));
    registry.register(Arc::new(WriteTool));
    registry.register(Arc::new(BashTool));
    registry.register(Arc::new(WebSearchTool));
}
