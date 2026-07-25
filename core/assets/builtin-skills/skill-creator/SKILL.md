---
name: skill-creator
description: "Create, install, and manage skills. Use when you need to install a skill from a remote source, create a new skill, or manage existing skills."
---

# Skill Creator

This skill covers everything about managing ClawTao skills.

## Skill directories

| Source | Location |
|--------|----------|
| Built-in | `~/.clawtao/skills/builtin/<name>/` |
| User-installed | `~/.clawtao/skills/installed/<name>/` |
| Project | `.clawtao/skills/<name>/` |

Priority when same name exists: **project > installed > builtin**.

## Install a skill from a Git repo

```bash
git clone <url> ~/.clawtao/skills/installed/<name>/
```

Verify: `cat ~/.clawtao/skills/installed/<name>/SKILL.md`

The skill will appear in `<available_skills>` on the next turn.

## Install a skill by writing its SKILL.md

If a skill doesn't have a public repo, create the directory and write SKILL.md:

```bash
mkdir -p ~/.clawtao/skills/installed/<name>/
```

Then use Write to create `~/.clawtao/skills/installed/<name>/SKILL.md` with valid YAML frontmatter.

## Create a skill from scratch

1. Pick a name: lowercase letters, digits, hyphens.
2. Create the directory: `mkdir -p ~/.clawtao/skills/installed/<name>/`
3. Create SKILL.md with this structure:

```markdown
---
name: my-skill
description: "What this skill does and when to use it."
---

# Skill Title

## Instructions
...
```

4. Optional subdirectories:
   - `scripts/` — executable code (Python, Bash, etc.) the agent can run
   - `references/` — documentation loaded on-demand via Read
   - `assets/` — templates, images, other files used in output

5. Write the body: include specific instructions, examples, commands, and references to scripts/ and references/ files.

## Remove a skill

```bash
rm -rf ~/.clawtao/skills/installed/<name>/
```

It will disappear from the next turn's `<available_skills>`.

## Validate frontmatter

Every SKILL.md must start with:

```markdown
---
name: <required>
description: <required>
---
```

Both `name` and `description` are required. The description should clearly state when the skill should be used — this is what the agent reads to decide whether to load the skill.
