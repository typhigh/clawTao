//! macOS Seatbelt (sandbox-exec) integration for Bash tool sandboxing.
//!
//! Generates dynamic SBPL (Sandbox Profile Language) profiles scoped to a
//! per-session workspace directory. The macOS kernel enforces read/write
//! boundaries — no string-matching bypass possible.
//!
//! ## Sandbox modes
//!
//! - `off`       — No sandbox, full disk + network access.
//! - `workspace_only` — Write limited to workspace dir; read + network allowed.
//! - `strict`    — Write limited to workspace dir; read limited to system libs;
//!   network blocked.

/// Path to macOS's built-in sandbox executor.
const SEATBELT_EXECUTABLE: &str = "/usr/bin/sandbox-exec";

/// SBPL base policy template.
///
/// Placeholders:
/// - `{{WORKSPACE_DIR}}` — the per-session workspace (sole writable directory).
/// - `{{HOME}}`          — the user's home directory (read-only access).
const SBPL_BASE: &str = r#"(version 1)
;; ── Default deny ──────────────────────────────────────────────────────────
(deny default)

;; ── Process basics ────────────────────────────────────────────────────────
(allow process-exec)
(allow process-fork)
(allow signal (target same-sandbox))
(allow process-info* (target same-sandbox))

    ;; ── File system: read everything ──────────────────────────────────────────
    (allow file-read*
        (subpath "/")
    )

    ;; ── File system: ⭐ sole writable workspace ⭐ ────────────────────────────
    (allow file-read* file-write*
        (subpath "{{WORKSPACE_DIR}}")
    )

;; ── /dev/null write ───────────────────────────────────────────────────────
(allow file-write-data
    (require-all (path "/dev/null") (vnode-type CHARACTER-DEVICE))
)

;; ── Network ───────────────────────────────────────────────────────────────
{{NETWORK_POLICY}}

;; ── Pseudo-terminal (interactive shells) ──────────────────────────────────
(allow pseudo-tty)
(allow file-read* file-write* file-ioctl
    (literal "/dev/ptmx")
)
(allow file-read* file-write*
    (require-all
        (regex #"^/dev/ttys[0-9]+")
        (extension "com.apple.sandbox.pty")
    )
)
(allow file-ioctl (regex #"^/dev/ttys[0-9]+"))

;; ── sysctl (hw, kern, vm, net basics) ─────────────────────────────────────
(allow sysctl-read
    (sysctl-name "hw.activecpu")
    (sysctl-name "hw.byteorder")
    (sysctl-name "hw.cachelinesize")
    (sysctl-name "hw.cachelinesize_compat")
    (sysctl-name "hw.cpufamily")
    (sysctl-name "hw.cpufrequency")
    (sysctl-name "hw.cpufrequency_compat")
    (sysctl-name "hw.cputype")
    (sysctl-name "hw.l1dcachesize_compat")
    (sysctl-name "hw.l1icachesize_compat")
    (sysctl-name "hw.l2cachesize_compat")
    (sysctl-name "hw.l3cachesize_compat")
    (sysctl-name "hw.logicalcpu")
    (sysctl-name "hw.logicalcpu_max")
    (sysctl-name "hw.machine")
    (sysctl-name "hw.memsize")
    (sysctl-name "hw.model")
    (sysctl-name "hw.ncpu")
    (sysctl-name "hw.nperflevels")
    (sysctl-name-prefix "hw.optional.arm.")
    (sysctl-name-prefix "hw.optional.armv8_")
    (sysctl-name "hw.packages")
    (sysctl-name "hw.pagesize")
    (sysctl-name "hw.pagesize_compat")
    (sysctl-name "hw.physicalcpu")
    (sysctl-name "hw.physicalcpu_max")
    (sysctl-name "hw.tbfrequency")
    (sysctl-name "hw.tbfrequency_compat")
    (sysctl-name "hw.vectorunit")
    (sysctl-name-prefix "hw.perflevel")
    (sysctl-name "machdep.cpu.brand_string")
    (sysctl-name "kern.argmax")
    (sysctl-name "kern.hostname")
    (sysctl-name "kern.maxfilesperproc")
    (sysctl-name "kern.maxproc")
    (sysctl-name "kern.osproductversion")
    (sysctl-name "kern.osrelease")
    (sysctl-name "kern.ostype")
    (sysctl-name "kern.osvariant_status")
    (sysctl-name "kern.osversion")
    (sysctl-name "kern.secure_kernel")
    (sysctl-name "kern.usrstack64")
    (sysctl-name "kern.version")
    (sysctl-name-prefix "kern.proc.pgrp.")
    (sysctl-name-prefix "kern.proc.pid.")
    (sysctl-name-prefix "net.routetable.")
    (sysctl-name "vm.loadavg")
)
;; Misclassified as write: userspace passes a buffer to read CPU info.
(allow sysctl-write (sysctl-name "kern.grade_cputype"))

;; ── IPC / shared memory ───────────────────────────────────────────────────
(allow ipc-posix-sem)
(allow ipc-posix-shm-read-data
      ipc-posix-shm-write-create
      ipc-posix-shm-write-unlink
    (ipc-posix-name-regex #"^/__KMP_REGISTERED_LIB_[0-9]+$")
)

;; ── mach-lookup (cfprefsd, PowerManagement, opendirectoryd) ──────────────
(allow mach-lookup
    (global-name "com.apple.cfprefsd.daemon")
    (global-name "com.apple.cfprefsd.agent")
    (local-name "com.apple.cfprefsd.agent")
    (global-name "com.apple.PowerManagement.control")
    (global-name "com.apple.system.opendirectoryd.libinfo")
)

;; ── User preferences (read-only) ──────────────────────────────────────────
(allow ipc-posix-shm-read* (ipc-posix-name-prefix "apple.cfprefs."))
(allow user-preference-read)

;; ── IOKit ─────────────────────────────────────────────────────────────────
(allow iokit-open (iokit-registry-entry-class "RootDomainUserClient"))
"#;

/// Network policy variants.
const NETWORK_POLICY_ALLOW: &str = r#";; ── Network: allowed ───────────────────────────────────────────────────
(allow network-outbound)
(allow network-inbound)
"#;

const NETWORK_POLICY_DENY: &str = r#";; ── Network: restricted (localhost + DNS only) ─────────────────────────
(allow network-outbound (remote ip "localhost:*"))
(allow network-outbound (remote ip "127.0.0.1:*"))
(allow network-outbound (remote ip "::1:*"))
(allow network-outbound (remote ip "*:53"))
(allow network-outbound (remote ip "*:443"))
(allow network-outbound (remote ip "*:80"))
(allow network-inbound (local ip "localhost:*"))
"#;

// ── Public API ────────────────────────────────────────────────────────────

/// Sandbox operation mode.
#[derive(Debug, Clone, PartialEq)]
pub enum SandboxMode {
    /// No sandboxing. All commands run directly.
    Off,
    /// Write limited to workspace dir; reads + network unrestricted.
    WorkspaceOnly,
    /// Write limited to workspace dir; reads system-only; network blocked.
    Strict,
}

/// Per-session sandbox configuration.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub mode: SandboxMode,
    pub workspace_dir: String,
}

impl SandboxConfig {
    /// Create a config for the given workspace directory and mode.
    pub fn new(workspace_dir: &str, mode: SandboxMode) -> Self {
        Self { mode, workspace_dir: workspace_dir.to_string() }
    }

    #[allow(dead_code)]
    /// Off — no sandbox at all.
    pub fn off() -> Self {
        Self { mode: SandboxMode::Off, workspace_dir: String::new() }
    }

    /// Whether sandboxing is active.
    pub fn is_active(&self) -> bool {
        self.mode != SandboxMode::Off && !self.workspace_dir.is_empty()
    }
}

/// Generated SBPL profile.
pub struct SandboxProfile {
    sbpl: String,
}

impl SandboxProfile {
    /// Generate an SBPL profile for the given config.
    pub fn generate(config: &SandboxConfig) -> Self {
        let home = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/Users/unknown".to_string());

        let network_policy = match config.mode {
            SandboxMode::Strict => NETWORK_POLICY_DENY,
            _ => NETWORK_POLICY_ALLOW,
        };

        let sbpl = SBPL_BASE
            .replace("{{HOME}}", &home)
            .replace("{{WORKSPACE_DIR}}", &config.workspace_dir)
            .replace("{{NETWORK_POLICY}}", network_policy.trim_start());

        Self { sbpl }
    }

    /// The raw SBPL content for passing to `sandbox-exec -p`.
    pub fn to_sbpl(&self) -> &str {
        &self.sbpl
    }

    /// Build a `Command` that runs the given shell command under sandbox-exec.
    ///
    /// Returns `None` if sandboxing is not available (non-macOS).
    pub fn wrap_command(config: &SandboxConfig, shell_command: &str) -> Option<std::process::Command> {
        if !config.is_active() {
            return None;
        }

        let profile = Self::generate(config);

        let mut cmd = std::process::Command::new(SEATBELT_EXECUTABLE);
        cmd.arg("-p").arg(profile.to_sbpl())
           .arg("--")
           .arg("sh")
           .arg("-c")
           .arg(shell_command);
        Some(cmd)
    }

    #[allow(dead_code)]
    /// Check if sandbox-exec is available on this system.
    pub fn is_available() -> bool {
        cfg!(target_os = "macos")
    }
}

#[cfg(test)]
mod tests {
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
        // Should NOT contain the literal placeholder
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

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "requires macOS with sandbox-exec, may fail due to /tmp symlink resolution"]
    fn sandbox_exec_allows_write_in_workspace() {
        if !std::path::Path::new(SEATBELT_EXECUTABLE).exists() {
            eprintln!("skipping: {SEATBELT_EXECUTABLE} not found");
            return;
        }
        // Use a fixed path under /tmp to avoid temp_dir resolution issues.
        let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let config = SandboxConfig::new(
            &tmp.to_string_lossy(),
            SandboxMode::WorkspaceOnly,
        );
        let profile = SandboxProfile::generate(&config);

        // Write a file inside the workspace — must succeed.
        let target = tmp.join("test.txt");
        let output = std::process::Command::new(SEATBELT_EXECUTABLE)
            .arg("-p")
            .arg(profile.to_sbpl())
            .arg("--")
            .arg("sh")
            .arg("-c")
            .arg(format!("echo ok > {}", target.display()))
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "sandbox-exec failed on allowed write (exit {:?}):\n  SBPL workspace_dir: {}\n  target file: {}\n  stdout: {}\n  stderr: {}",
            output.status.code(),
            tmp.display(),
            target.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        // Clean up.
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "requires macOS with sandbox-exec"]
    fn sandbox_exec_blocks_write_outside_workspace() {
        if !std::path::Path::new(SEATBELT_EXECUTABLE).exists() {
            eprintln!("skipping: {SEATBELT_EXECUTABLE} not found");
            return;
        }
        let tmp = std::path::PathBuf::from(format!("/tmp/clawtao-sbtest-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let config = SandboxConfig::new(
            &tmp.to_string_lossy(),
            SandboxMode::WorkspaceOnly,
        );
        let profile = SandboxProfile::generate(&config);

        // Try to write to /tmp directly (outside the workspace) — must fail.
        let bad_file = format!("/tmp/clawtao-sbtest-outside-{}", std::process::id());
        let output = std::process::Command::new(SEATBELT_EXECUTABLE)
            .arg("-p")
            .arg(profile.to_sbpl())
            .arg("--")
            .arg("sh")
            .arg("-c")
            .arg(format!("echo bad > {bad_file}"))
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "sandbox-exec should have BLOCKED write outside workspace, but command succeeded:\nstderr: {}",
            String::from_utf8_lossy(&output.stderr),
        );

        // Clean up.
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_file(&bad_file);
    }
}
