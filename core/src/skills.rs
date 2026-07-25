//! Skill discovery and prompt formatting.
//!
//! Skills are directories containing a SKILL.md with YAML frontmatter.
//! They are discovered by scanning the filesystem — no DB, no hot-reload.
//!
//! Discovery order (within a scan_all call):
//!   1. builtin  (~/.clawtao/skills/builtin/)
//!   2. installed (~/.clawtao/skills/installed/)
//!   3. project  (.clawtao/skills/)
//!
//! Same-name skills: project > installed > builtin.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

/// Where a skill came from — used for display / debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    Builtin,
    Installed,
    Project,
}

/// A discovered skill — metadata extracted from SKILL.md frontmatter.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Absolute path to the SKILL.md file.
    pub path: PathBuf,
    /// Skill directory (for resolving relative paths to scripts/, references/ etc.).
    #[allow(dead_code)]
    pub base_dir: PathBuf,
    #[allow(dead_code)]
    pub source: SkillSource,
}

// ── Public API ──────────────────────────────────────────────────────────

/// Scan all skill sources and return deduplicated, sorted skills.
///
/// `clawtao_home` — typically `~/.clawtao`.
/// `workspace_dir` — current project root; project skills live under
///   `<workspace_dir>/.clawtao/skills/`.  `None` skips project skills.
pub fn scan_all(clawtao_home: &Path, workspace_dir: Option<&Path>) -> Vec<Skill> {
    let mut map: HashMap<String, Skill> = HashMap::new();

    // Lower priority first so later inserts overwrite.

    // 1. builtin
    for dir in list_subdirs(&clawtao_home.join("skills").join("builtin")) {
        if let Some(s) = scan_one(&dir, SkillSource::Builtin) {
            map.insert(s.name.clone(), s);
        }
    }

    // 2. installed
    for dir in list_subdirs(&clawtao_home.join("skills").join("installed")) {
        if let Some(s) = scan_one(&dir, SkillSource::Installed) {
            map.insert(s.name.clone(), s);
        }
    }

    // 3. project
    if let Some(ws) = workspace_dir {
        for dir in list_subdirs(&ws.join(".clawtao").join("skills")) {
            if let Some(s) = scan_one(&dir, SkillSource::Project) {
                map.insert(s.name.clone(), s);
            }
        }
    }

    let mut skills: Vec<Skill> = map.into_values().collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Install embedded built-in skills into `clawtao_home/skills/builtin/`.
/// Idempotent — files that already exist are not overwritten so user
/// customisations to built-ins survive restarts.
pub fn install_builtin_skills(clawtao_home: &Path) -> std::io::Result<()> {
    let builtin_dir = clawtao_home.join("skills").join("builtin");
    fs::create_dir_all(&builtin_dir)?;

    for (rel_path, content) in builtin_skill_assets() {
        let dest = builtin_dir.join(&rel_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if !dest.exists() {
            fs::write(&dest, content)?;
        }
    }
    Ok(())
}

/// Build the `<available_skills>` XML fragment for the system prompt.
/// Returns an empty string when there are no skills.
pub fn format_for_prompt(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut lines = vec![
        String::new(),
        "The following skills provide specialized instructions for specific tasks."
            .to_string(),
        "Use Read to load a skill file when the task matches its description."
            .to_string(),
        String::new(),
        "<available_skills>".to_string(),
    ];
    for s in skills {
        lines.push("  <skill>".to_string());
        lines.push(format!("    <name>{}</name>", s.name));
        lines.push(format!("    <description>{}</description>", s.description));
        lines.push(format!("    <location>{}</location>", s.path.display()));
        lines.push("  </skill>".to_string());
    }
    lines.push("</available_skills>".to_string());
    lines.join("\n")
}

// ── Internal helpers ────────────────────────────────────────────────────

/// Try to read `dir/SKILL.md` and parse its frontmatter.
fn scan_one(dir: &Path, source: SkillSource) -> Option<Skill> {
    let md_path = dir.join("SKILL.md");
    let content = fs::read_to_string(&md_path).ok()?;
    let fm = parse_frontmatter(&content)?;
    let name = fm.name.filter(|n| !n.is_empty())?;
    let description = fm.description.unwrap_or_default();
    let path = fs::canonicalize(&md_path).unwrap_or(md_path);
    let base_dir = path.parent()?.to_path_buf();
    Some(Skill { name, description, path, base_dir, source })
}

/// Tiny YAML frontmatter parser — only extracts top-level `name` and
/// `description` scalars.  Keeps dependency count low (no serde_yaml).
fn parse_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut name: Option<String> = None;
    let mut desc: Option<String> = None;
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("name:") {
            name = Some(trim_yaml_scalar(value));
        } else if let Some(value) = trimmed.strip_prefix("description:") {
            desc = Some(trim_yaml_scalar(value));
        }
    }
    Some(SkillFrontmatter { name, description: desc })
}

/// Strip quotes and whitespace from a YAML scalar value.
fn trim_yaml_scalar(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// List immediate subdirectories of `dir`, sorted, skipping dot-prefixed names.
fn list_subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return vec![] };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

/// Embedded built-in skill assets: (relative_path, content).
fn builtin_skill_assets() -> Vec<(String, &'static str)> {
    vec![
        ("skill-creator/SKILL.md".into(),
         include_str!("../assets/builtin-skills/skill-creator/SKILL.md")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_frontmatter() {
        let input = "---\nname: foo\ndescription: bar\n---\n\n# Body";
        let fm = parse_frontmatter(input).unwrap();
        assert_eq!(fm.name.as_deref(), Some("foo"));
        assert_eq!(fm.description.as_deref(), Some("bar"));
    }

    #[test]
    fn parse_quoted_description() {
        let input = "---\nname: test\ndescription: \"A skill for things\"\n---\nbody";
        let fm = parse_frontmatter(input).unwrap();
        assert_eq!(fm.description.as_deref(), Some("A skill for things"));
    }

    #[test]
    fn parse_missing_description() {
        let input = "---\nname: test\n---\nbody";
        let fm = parse_frontmatter(input).unwrap();
        assert_eq!(fm.name.as_deref(), Some("test"));
        assert_eq!(fm.description.as_deref(), None);
    }

    #[test]
    fn parse_missing_name_returns_none() {
        let input = "---\ndescription: test\n---\nbody";
        let fm = parse_frontmatter(input).unwrap();
        assert!(fm.name.is_none());
    }

    #[test]
    fn parse_no_frontmatter_returns_none() {
        assert!(parse_frontmatter("# Just a heading").is_none());
    }

    #[test]
    fn format_empty_skills() {
        assert_eq!(format_for_prompt(&[]), "");
    }

    #[test]
    fn format_includes_all_skills() {
        let skills = vec![
            Skill {
                name: "test".into(),
                description: "A test skill".into(),
                path: PathBuf::from("/home/skills/test/SKILL.md"),
                base_dir: PathBuf::from("/home/skills/test"),
                source: SkillSource::Installed,
            },
        ];
        let prompt = format_for_prompt(&skills);
        assert!(prompt.contains("<available_skills>"));
        assert!(prompt.contains("<name>test</name>"));
        assert!(prompt.contains("<description>A test skill</description>"));
    }
}
