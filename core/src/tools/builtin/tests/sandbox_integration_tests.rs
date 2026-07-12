use super::*;

#[test]
fn generate_off_does_nothing() {
    let config = SandboxConfig::off();
    assert!(!config.is_active());
    assert!(SandboxProfile::wrap_command(&config, "ls").is_none());
}

#[test]
fn generate_workspace_only_includes_workspace() {
    let config = SandboxConfig::new("/tmp/test-ws", SandboxMode::WorkspaceOnly);
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(sbpl.contains("/tmp/test-ws"));
    assert!(sbpl.contains("(allow network-outbound)"));
}

#[test]
fn generate_strict_blocks_network() {
    let config = SandboxConfig::new("/tmp/test-ws", SandboxMode::Strict);
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(sbpl.contains("/tmp/test-ws"));
    assert!(!sbpl.contains("(allow network-outbound)"), "strict mode should deny full network; got:\n{sbpl}");
}

#[test]
fn generate_replaces_home() {
    let config = SandboxConfig::new("/tmp/test-ws", SandboxMode::WorkspaceOnly);
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(!sbpl.contains("{{HOME}}"));
    assert!(!sbpl.contains("{{WORKSPACE_DIR}}"));
}

#[test]
fn sandbox_mode_clone_and_eq() {
    let a = SandboxMode::WorkspaceOnly;
    assert_eq!(a.clone(), SandboxMode::WorkspaceOnly);
    assert_ne!(a, SandboxMode::Off);
}

// ── Integration tests: real sandbox-exec ─────────────────────────

/// Check if sandbox-exec can actually run (not just exist).
/// Nested sandboxing may be blocked when the test process itself is sandboxed.
fn sandbox_exec_works() -> bool {
    std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg("(version 1) (allow default)")
        .arg("--").arg("/usr/bin/true")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_allows_write_in_workspace() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available (may be running under sandbox)");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    // Canonicalize to resolve /tmp → /private/tmp symlink on macOS.
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig::new(&tmp.to_string_lossy(), SandboxMode::WorkspaceOnly);
    let profile = SandboxProfile::generate(&config);

    let target = tmp.join("test.txt");
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg(format!("echo ok > {}", target.display()))
        .output().unwrap();

    assert!(
        output.status.success(),
        "sandbox-exec failed on allowed write (exit {:?}):\n  SBPL workspace_dir: {}\n  target file: {}\n  stdout: {}\n  stderr: {}",
        output.status.code(), tmp.display(), target.display(),
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_blocks_write_outside_workspace() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available (may be running under sandbox)");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    // Canonicalize to resolve /tmp → /private/tmp symlink on macOS.
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig::new(&tmp.to_string_lossy(), SandboxMode::WorkspaceOnly);
    let profile = SandboxProfile::generate(&config);

    let bad_file = format!("/tmp/clawtao-sbtest-outside-{}", std::process::id());
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg(format!("echo bad > {bad_file}"))
        .output().unwrap();

    assert!(
        !output.status.success(),
        "sandbox-exec should have BLOCKED write outside workspace, but command succeeded:\nstderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_file(&bad_file);
}
