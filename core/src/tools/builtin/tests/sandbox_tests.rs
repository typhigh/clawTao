use crate::tools::builtin::sandbox::SandboxRules;

#[test]
fn rules_off_is_inactive() {
    let rules = SandboxRules::off();
    assert!(!rules.active);
    assert!(rules.workspace_dir.is_empty());
}

#[test]
fn rules_new_empty_dir_is_inactive() {
    let rules = SandboxRules::new("");
    assert!(!rules.active);
}

#[test]
fn rules_new_nonempty_dir_is_active() {
    let rules = SandboxRules::new("/tmp/test-ws");
    assert!(rules.active);
}

#[test]
fn path_absolute_inside_workspace_ok() {
    let rules = SandboxRules::new("/Users/test/myworkspace");
    assert!(rules.path_is_allowed("/Users/test/myworkspace/src/main.rs").is_ok());
    assert!(rules.path_is_allowed("/Users/test/myworkspace").is_ok());
}

#[test]
fn path_absolute_outside_workspace_err() {
    let rules = SandboxRules::new("/Users/test/myworkspace");
    assert!(rules.path_is_allowed("/etc/passwd").is_err());
}

#[test]
fn path_relative_inside_workspace_ok() {
    let rules = SandboxRules::new("/tmp/test-ws");
    assert!(rules.path_is_allowed("src/main.rs").is_ok());
}

#[test]
fn path_relative_dotdot_escapes_workspace_err() {
    let rules = SandboxRules::new("/tmp/test-ws");
    // "../other" should NOT be allowed because it resolves outside the workspace.
    assert!(rules.path_is_allowed("../other").is_err());
}

#[test]
fn path_when_inactive_always_ok() {
    let rules = SandboxRules::off();
    assert!(rules.path_is_allowed("/etc/passwd").is_ok());
    assert!(rules.path_is_allowed("../outside").is_ok());
}
