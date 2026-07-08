mod bash;
mod edit;
mod grep;
mod read;
pub mod sandbox;
mod todo;
mod web_browser;
mod web_fetch;
mod write;

use super::registry::ToolRegistry;
use std::sync::Arc;

pub use bash::BashTool;
pub use edit::EditTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use sandbox::{SandboxConfig, SandboxMode, SandboxRules};
pub use todo::TodoWriteTool;
pub use web_browser::WebBrowserTool;
pub use web_fetch::WebFetchTool;
pub use write::WriteTool;

/// Register all built-in tools.
pub fn register_all(registry: &mut ToolRegistry, sandbox_cfg: SandboxConfig, bash_timeout_secs: Option<u64>) {
    registry.register(Arc::new(ReadTool));
    registry.register(Arc::new(WriteTool));
    registry.register(Arc::new(EditTool));
    registry.register(Arc::new(BashTool::new(sandbox_cfg, bash_timeout_secs)));
    registry.register(Arc::new(WebBrowserTool));
    registry.register(Arc::new(WebFetchTool));
    registry.register(Arc::new(GrepTool));
    registry.register(Arc::new(TodoWriteTool));
}
