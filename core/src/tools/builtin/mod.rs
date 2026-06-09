mod bash;
mod edit;
mod read;
mod web_browser;
mod write;

use super::registry::ToolRegistry;
use std::sync::Arc;

pub use bash::BashTool;
pub use edit::EditTool;
pub use read::ReadTool;
pub use web_browser::WebBrowserTool;
pub use write::WriteTool;

/// Register all built-in tools.
pub fn register_all(registry: &mut ToolRegistry, bash_blocked_commands: Vec<String>) {
    registry.register(Arc::new(ReadTool));
    registry.register(Arc::new(WriteTool));
    registry.register(Arc::new(EditTool));
    registry.register(Arc::new(BashTool::new(bash_blocked_commands)));
    registry.register(Arc::new(WebBrowserTool));
}
