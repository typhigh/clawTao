//! System prompt builder for the ClawTao agent.

use crate::tools::registry::ToolRegistry;
use crate::skills::{Skill, format_for_prompt};

pub fn build(
    tool_registry: &ToolRegistry,
    workspace_dir: Option<&str>,
    skills: &[Skill],
    injected_skills: &[(String, String)],
) -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    let mut lines: Vec<String> = Vec::new();

    // Identity + working directory
    lines.push(format!("You are ClawTao, a desktop AI agent. Working directory: {cwd}."));
    if let Some(ws) = workspace_dir {
        if !ws.is_empty() {
            lines.push(format!("Sandbox is active: Bash commands are restricted to write only in {ws}. \
                Use this path for all file operations."));
        }
    }
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

    // Skills catalog (name + description for all discovered skills)
    lines.push(format_for_prompt(skills));

    // Injected skill bodies (full content for @skill-name mentions)
    for (name, body) in injected_skills {
        lines.push(format!(
            "\nThe user explicitly referenced the @{name} skill. \
             Its full content is loaded below. Follow its instructions.\n\n{body}"
        ));
    }

    lines.join("\n")
}
