use super::*;

#[test]
fn generate_off_is_inactive() {
    let config = SandboxConfig::off();
    assert!(!config.is_active());
    assert!(SandboxProfile::wrap_command(&config, "ls").is_none());
}

#[test]
fn generate_workspace_only_includes_workspace() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(sbpl.contains("/private/tmp/test-ws") || sbpl.contains("/tmp/test-ws"));
    assert!(sbpl.contains("(allow network-outbound)"));
}

#[test]
fn generate_strict_blocks_network() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Forbidden,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(sbpl.contains("/private/tmp/test-ws") || sbpl.contains("/tmp/test-ws"));
    assert!(!sbpl.contains("(allow network-outbound)"));
}

#[test]
fn generate_read_only_no_write_section() {
    let config = SandboxConfig::read_only(Some("/tmp/test-ws".into()));
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // The SBPL_COMMON template includes file-write* for /dev/null and ptmx,
    // so we check that no WORKSPACE-specific write rule exists.
    assert!(!sbpl.contains("{{WORKSPACE_DIR}}"));
    // Network should be allowed
    assert!(sbpl.contains("(allow network-outbound)"));
}

#[test]
fn generate_read_only_without_workspace() {
    let config = SandboxConfig::read_only(None);
    assert!(config.is_active());
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // Read-only without workspace: SBPL should allow network and reads,
    // but NOT contain a workspace-specific write rule.
    assert!(sbpl.contains("(allow network-outbound)"));
    assert!(sbpl.contains("file-read*"));
}

#[test]
fn policy_clone_and_eq() {
    assert_eq!(Policy::Forbidden, Policy::Forbidden);
    assert_ne!(Policy::Forbidden, Policy::Unrestricted);
    assert_eq!(Policy::Restricted, Policy::Restricted);
}

#[test]
fn restricted_without_workspace_degrades_to_unrestricted() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: None,
    };
    assert!(!config.is_active());
    assert_eq!(config.effective_write(), Policy::Unrestricted);
    assert_eq!(config.effective_read(), Policy::Unrestricted);
    assert_eq!(config.effective_network(), Policy::Unrestricted);
}

#[test]
fn forbidden_write_always_active() {
    let config = SandboxConfig {
        write: Policy::Forbidden,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: None,
    };
    assert!(config.is_active());
    assert_eq!(config.effective_write(), Policy::Forbidden);
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

// ── SBPL content checks (fast, no sandbox-exec) ────────────────────

#[test]
fn restricted_read_includes_metadata_slash() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // file-read-metadata on "/" means ls/stat work everywhere
    assert!(sbpl.contains("(allow file-read-metadata"), "should allow metadata reads");
    assert!(sbpl.contains("(subpath \"/\")"), "metadata should be allowed on /");
}

#[test]
fn restricted_read_separates_data_from_metadata() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // System configs should use file-read-data (not file-read*),
    // so that file content outside the allowed set stays blocked.
    assert!(sbpl.contains("file-read-metadata"), "should have metadata section");
    assert!(
        sbpl.contains("(allow file-read-data"),
        "should have a file-read-data section for configs"
    );
}

#[test]
fn restricted_read_includes_frameworks() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(sbpl.contains("/System/Library/Frameworks"), "should include macOS frameworks");
    assert!(sbpl.contains("/System/Library/PrivateFrameworks"), "should include private frameworks");
    assert!(sbpl.contains("/Library/Apple/System/Library/Frameworks"), "should include Apple-signed frameworks");
}

#[test]
fn restricted_read_includes_dev() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(sbpl.contains("(subpath \"/dev\")"), "should allow reading /dev");
}

#[test]
fn restricted_read_includes_system_binaries() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(sbpl.contains("(subpath \"/bin\")"), "should allow reading /bin");
    assert!(sbpl.contains("(subpath \"/usr\")"), "should allow reading /usr");
    assert!(sbpl.contains("(subpath \"/opt\")"), "should allow reading /opt");
}

#[test]
fn restricted_read_blocks_user_home() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // /Users must NOT appear — user home directories are not readable
    assert!(!sbpl.contains("/Users"), "should NOT allow reading /Users");
}

#[test]
fn restricted_write_includes_tmp() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // Scratch space must be writable
    assert!(sbpl.contains("(subpath \"/tmp\")"), "should allow writing to /tmp");
    assert!(sbpl.contains("(subpath \"/private/tmp\")"), "should allow writing to /private/tmp");
    assert!(sbpl.contains("(subpath \"/var/tmp\")"), "should allow writing to /var/tmp");
}

#[test]
fn forbidden_read_has_no_read_section() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Forbidden,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // When read is Forbidden, no file-read rules beyond the base common
    // rules (which include /dev/null etc.) should appear.
    assert!(!sbpl.contains("(allow file-read-metadata"), "should not have metadata block");
    assert!(!sbpl.contains("(allow file-read-data"), "should not have file-read-data block");
}

#[test]
fn unrestricted_read_uses_full_read() {
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // When read is Unrestricted, we use FS_READ_FULL: (subpath "/")
    assert!(
        sbpl.contains("file-read*\n        (subpath \"/\")") ||
        sbpl.contains("file-read* \n        (subpath \"/\")"),
        "unrestricted read should allow reading everything under /"
    );
}

// ── Real sandbox-exec integration tests (macOS only) ───────────────

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_restricted_read_allows_ls_dev() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    // ls outside workspace should work (metadata is allowed everywhere)
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg("ls /bin/sh")
        .output().unwrap();

    assert!(
        output.status.success(),
        "ls outside workspace should be allowed (file-read-metadata on /)\n  stderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_restricted_read_blocks_cat_outside_workspace() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    // catting a file outside workspace should be blocked
    // (file-read-data is only allowed for system paths + workspace)
    let outside_file = format!("/tmp/clawtao-sbtest-outside-{}", std::process::id());
    std::fs::write(&outside_file, "secret").unwrap();

    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("cat")
        .arg(&outside_file)
        .output().unwrap();

    assert!(
        !output.status.success(),
        "cat outside workspace should be blocked (file-read-data on user data)\n  file: {outside_file}\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_file(&outside_file);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_restricted_read_allows_cat_workspace() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let target = tmp.join("hello.txt");
    std::fs::write(&target, "hello world").unwrap();

    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("cat")
        .arg(target.to_string_lossy().as_ref())
        .output().unwrap();

    assert!(
        output.status.success(),
        "cat inside workspace should be allowed\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_restricted_write_allows_tmp() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    // Writing to /tmp should be allowed (scratch space)
    let scratch_file = format!("/tmp/clawtao-sbtest-scratch-{}", std::process::id());
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg(format!("echo ok > {scratch_file}"))
        .output().unwrap();

    assert!(
        output.status.success(),
        "write to /tmp should be allowed (scratch space)\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_file(&scratch_file);
    let _ = std::fs::remove_dir_all(&tmp);
}

// ── Forbidden tests (read and write) ──────────────────────────────

#[test]
fn forbidden_write_has_no_write_section() {
    let config = SandboxConfig {
        write: Policy::Forbidden,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // The SBPL_COMMON template includes file-write* for /dev/null and ptmx,
    // but no workspace-or scratch-level write rules should appear.
    assert!(!sbpl.contains("write restricted"), "should not have write section");
    assert!(!sbpl.contains("Scratch directories"), "should not have scratch write section");
}

#[test]
fn forbidden_read_and_forbidden_write_no_sections() {
    let config = SandboxConfig {
        write: Policy::Forbidden,
        read: Policy::Forbidden,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    assert!(!sbpl.contains("file-read-metadata"), "should not have read section");
    assert!(!sbpl.contains("(allow file-read-data"), "should not have read section");
    assert!(!sbpl.contains("write restricted"), "should not have write section");
}

#[test]
fn forbidden_write_still_includes_read_section() {
    let config = SandboxConfig {
        write: Policy::Forbidden,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some("/tmp/test-ws".into()),
    };
    let profile = SandboxProfile::generate(&config);
    let sbpl = profile.to_sbpl();
    // Write forbidden doesn't affect read — system paths still readable.
    assert!(sbpl.contains("(allow file-read-metadata"), "read section should still be present");
    assert!(sbpl.contains("/tmp/test-ws"), "workspace should still be readable");
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_forbidden_read_blocks_ls() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    // read=Forbidden: no file-read rules beyond the base common template.
    // Even `ls` on a harmless system path should fail.
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Forbidden,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("ls")
        .arg("/bin/sh")
        .output().unwrap();

    assert!(
        !output.status.success(),
        "read=Forbidden: even ls should be blocked\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_forbidden_read_blocks_cat_workspace() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let target = tmp.join("hello.txt");
    std::fs::write(&target, "hello world").unwrap();

    // read=Forbidden: catting even workspace files should be blocked.
    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Forbidden,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("cat")
        .arg(target.to_string_lossy().as_ref())
        .output().unwrap();

    assert!(
        !output.status.success(),
        "read=Forbidden: cat workspace should be blocked\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_forbidden_write_blocks_workspace() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    // write=Forbidden: even writes inside workspace are blocked.
    let config = SandboxConfig {
        write: Policy::Forbidden,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let target = tmp.join("should-not-exist.txt");
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg(format!("echo blocked > {}", target.display()))
        .output().unwrap();

    assert!(
        !output.status.success(),
        "write=Forbidden: write inside workspace should be blocked\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_forbidden_write_blocks_tmp() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    // write=Forbidden: even scratch space is blocked.
    let config = SandboxConfig {
        write: Policy::Forbidden,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let scratch = format!("/tmp/clawtao-sbtest-forbidden-{}", std::process::id());
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg(format!("echo bad > {scratch}"))
        .output().unwrap();

    assert!(
        !output.status.success(),
        "write=Forbidden: write to /tmp should be blocked\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_file(&scratch);
    let _ = std::fs::remove_dir_all(&tmp);
}

// ── Device / config file integration tests ─────────────────────────

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_restricted_read_allows_dev_urandom() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("head").arg("-c").arg("4")
        .arg("/dev/urandom")
        .output().unwrap();

    assert!(
        output.status.success(),
        "restricted read: /dev/urandom should be readable\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_restricted_read_allows_etc_hosts() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Restricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("head").arg("-n").arg("1")
        .arg("/etc/hosts")
        .output().unwrap();

    assert!(
        output.status.success(),
        "restricted read: /etc/hosts should be readable\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── Existing tests ─────────────────────────────────────────────────

#[test]
#[cfg(target_os = "macos")]
fn sandbox_exec_allows_write_in_workspace() {
    if !sandbox_exec_works() {
        eprintln!("skipping: sandbox-exec not available (may be running under sandbox)");
        return;
    }
    let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let target = tmp.join("test.txt");
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg(format!("echo ok > {}", target.display()))
        .output().unwrap();

    assert!(
        output.status.success(),
        "sandbox-exec failed on allowed write (exit {:?}):\n  workspace_dir: {}\n  target: {}\n  stdout: {}\n  stderr: {}",
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
    let tmp = std::fs::canonicalize(&tmp).unwrap();

    let config = SandboxConfig {
        write: Policy::Restricted,
        read: Policy::Unrestricted,
        network: Policy::Unrestricted,
        workspace_dir: Some(tmp.to_string_lossy().to_string()),
    };
    let profile = SandboxProfile::generate(&config);

    let bad_file = format!("/tmp/clawtao-sbtest-outside-{}", std::process::id());
    let output = std::process::Command::new(SEATBELT_EXECUTABLE)
        .arg("-p").arg(profile.to_sbpl())
        .arg("--").arg("sh").arg("-c")
        .arg(format!("echo bad > {bad_file}"))
        .output().unwrap();

    assert!(
        !output.status.success(),
        "sandbox-exec should have blocked write outside workspace:\n  target: {bad_file}\n  stdout: {}\n  stderr: {}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );

    let _ = std::fs::remove_file(&bad_file);
    let _ = std::fs::remove_dir_all(&tmp);
}
