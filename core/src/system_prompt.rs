//! System prompt builder for the ClawTao agent.

use crate::tools::registry::ToolRegistry;

pub fn build(tool_registry: &ToolRegistry) -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    let mut lines: Vec<String> = Vec::new();

    // Identity + workspace
    lines.push(format!("You are ClawTao, a desktop AI agent. Working directory: {cwd}."));
    lines.push(String::new());

    // Tool list (auto-generated from registry)
    lines.push("Available tools:".to_string());
    for spec in tool_registry.list_specs() {
        lines.push(format!("- {}: {}", spec.function.name, spec.function.description));
    }

    // Usage hints
    lines.push(String::new());
    lines.push("Read before Edit (file may have changed). \
        Edit for small changes, Write for new files or full rewrites. \
        Use Grep instead of grep in Bash. \
        After code changes, run tests or build to verify.".to_string());

    // Tool selection hints
    lines.push(String::new());
    lines.push("WebFetch is for simple static pages (API responses, documentation, plain HTML). \
        For search engines, JS-heavy sites, or interactive browsing, use WebBrowser instead: \
        call search first, then snapshot to read the rendered page.".to_string());

    lines.join("\n")
}
