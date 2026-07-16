//! macOS Seatbelt (sandbox-exec) integration for Bash tool sandboxing.
//!
//! Generates dynamic SBPL (Sandbox Profile Language) profiles scoped to a
//! per-session workspace directory. The macOS kernel enforces read/write
//! boundaries — no string-matching bypass possible.
//!
//! ## Sandbox configuration
//!
//! Three independent policy axes, each with three levels:
//!
//! - **Forbidden**   — always enforced, regardless of workspace.
//! - **Restricted**  — enforced only when a workspace directory is configured;
//!                     without one it degrades to Unrestricted.
//! - **Unrestricted** — never enforced.
//!
//! SBPL profiles are assembled from independent fragments driven by the
//! **effective** policy (after workspace degradation).  When write is
//! `Forbidden`, no `file-write*` rules are emitted — `deny default`
//! ensures the kernel rejects every write attempt.

/// Path to macOS's built-in sandbox executor.
const SEATBELT_EXECUTABLE: &str = "/usr/bin/sandbox-exec";

// ── SBPL fragments ──────────────────────────────────────────────────────

/// Everything except filesystem + network rules — shared across all profiles.
const SBPL_COMMON: &str = r#"(version 1)
;; ── Default deny ──────────────────────────────────────────────────────────
(deny default)

;; ── Process basics ────────────────────────────────────────────────────────
(allow process-exec)
(allow process-fork)
(allow signal (target same-sandbox))
(allow process-info* (target same-sandbox))

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

/// Filesystem sections — inlined based on effective policy.
const FS_READ_FULL: &str = r#";; ── File system: read everything ─────────────────────────────────────
    (allow file-read*
        (subpath "/")
    )
"#;

fn fs_read_restricted(workspace_dir: &str) -> String {
    format!(
        r#";; ── File system: read restricted ─────────────────────────────────────
    ;; Metadata (stat, ls, directory traversal) — allowed everywhere so
    ;; basic shell operations work without exposing file contents.
    (allow file-read-metadata
        (subpath "/")
    )
    ;; System binaries and libraries (needed to exec any command).
    (allow file-read*
        (subpath "/bin")
        (subpath "/sbin")
        (subpath "/usr")
        (subpath "/opt")
    )
    ;; macOS frameworks (dynamic linker requires these).
    (allow file-read*
        (subpath "/System/Library/Frameworks")
        (subpath "/System/Library/PrivateFrameworks")
        (subpath "/Library/Apple/System/Library/Frameworks")
        (subpath "/Library/Apple/System/Library/PrivateFrameworks")
    )
    ;; System configuration (hostname, timezone, DNS, passwd/group DB).
    (allow file-read-data
        (subpath "/etc")
        (subpath "/private/etc")
        (subpath "/var/db")
        (subpath "/private/var/db")
    )
    ;; Device files (/dev/urandom, /dev/null, /dev/fd/* etc.).
    (allow file-read*
        (subpath "/dev")
    )
    ;; Scratch directories (temp files for compilers, interpreters).
    (allow file-read*
        (subpath "/tmp")
        (subpath "/private/tmp")
        (subpath "/var/tmp")
        (subpath "/private/var/tmp")
    )
    ;; Workspace — user task files, full read access.
    (allow file-read*
        (subpath "{}")
    )
"#,
        workspace_dir
    )
}

fn fs_write_restricted(workspace_dir: &str) -> String {
    format!(
        r#";; ── File system: write restricted ────────────────────────────────────
    ;; Scratch directories — many tools (gcc, go, python, etc.) need
    ;; temp files to function.  /tmp is world-writable by convention and
    ;; does not expose user data.
    (allow file-write*
        (subpath "/tmp")
        (subpath "/private/tmp")
        (subpath "/var/tmp")
        (subpath "/private/var/tmp")
    )
    ;; Workspace — user task files, full write access.
    (allow file-write*
        (subpath "{}")
    )
"#,
        workspace_dir
    )
}

/// Network policy fragments.
const NETWORK_POLICY_ALLOW: &str = r#";; ── Network: allowed ───────────────────────────────────────────────────
(allow network-outbound)
(allow network-inbound)
"#;

const NETWORK_POLICY_FORBIDDEN: &str = "";

// ── Public types ───────────────────────────────────────────────────────────

/// Single-axis sandbox policy level.
///
/// Each of write / read / network is independently configured.
/// `Restricted` depends on a workspace directory — without one it
/// degrades to `Unrestricted`.  `Forbidden` and `Unrestricted` are
/// always effective regardless of workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Always enforced — no exceptions.
    Forbidden,
    /// Enforced when workspace_dir is set; degrades to Unrestricted otherwise.
    Restricted,
    /// Never enforced.
    Unrestricted,
}

/// Complete sandbox configuration for a single turn.
///
/// Three independent policy axes + optional workspace directory.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub write: Policy,
    pub read: Policy,
    pub network: Policy,
    pub workspace_dir: Option<String>,
}

impl SandboxConfig {
    // ── Constructors ──────────────────────────────────────────────────

    /// No sandbox — all policies unrestricted, no workspace.
    pub fn off() -> Self {
        Self {
            write: Policy::Unrestricted,
            read: Policy::Unrestricted,
            network: Policy::Unrestricted,
            workspace_dir: None,
        }
    }

    /// Read‑only sandbox: write completely forbidden, read + network free.
    /// Used for plan‑mode exploration.  Workspace is optional — if set it
    /// is used as default cwd in Bash but does not affect sandbox rules.
    pub fn read_only(workspace_dir: Option<String>) -> Self {
        Self {
            write: Policy::Forbidden,
            read: Policy::Unrestricted,
            network: Policy::Unrestricted,
            workspace_dir,
        }
    }

    // ── Resolution ────────────────────────────────────────────────────

    /// The write policy after workspace availability is factored in.
    pub fn effective_write(&self) -> Policy {
        self.write.resolve(self.workspace_dir.is_some())
    }

    /// The read policy after workspace availability is factored in.
    pub fn effective_read(&self) -> Policy {
        self.read.resolve(self.workspace_dir.is_some())
    }

    /// The network policy after workspace availability is factored in.
    pub fn effective_network(&self) -> Policy {
        self.network.resolve(self.workspace_dir.is_some())
    }

    /// Whether sandbox-exec wrapping is needed.
    /// Wrapping is needed whenever write is not fully Unrestricted.
    pub fn is_active(&self) -> bool {
        !matches!(self.effective_write(), Policy::Unrestricted)
    }
}

impl Policy {
    /// Resolve the declared policy against workspace availability.
    ///
    /// `Restricted` degrades to `Unrestricted` when there is no workspace
    /// to restrict to — the concept of "limit to this directory" collapses
    /// without a target.
    fn resolve(self, has_workspace: bool) -> Self {
        match self {
            Policy::Forbidden | Policy::Unrestricted => self,
            Policy::Restricted if has_workspace => Policy::Restricted,
            _ => Policy::Unrestricted,
        }
    }
}

/// Sandbox rules shared across all tools in a single turn.
///
/// Carries the resolved policies so each tool's `check_sandbox` can
/// enforce them.  Path-level workspace checks still use `workspace_dir`.
#[derive(Debug, Clone)]
pub struct SandboxRules {
    /// Whether any non-Unrestricted policy is active — recorded for
    /// introspection / tests; runtime checks use the individual *_policy
    /// fields directly.
    #[allow(dead_code)]
    pub active: bool,
    pub workspace_dir: String,
    pub read_policy: Policy,
    pub write_policy: Policy,
    /// Runtime defense-in-depth: stored so future tools can check at
    /// call time even though the primary enforcement is at the registry
    /// level (web tools are unregistered when network is Forbidden).
    #[allow(dead_code)]
    pub network_policy: Policy,
}

impl SandboxRules {
    /// Fully unrestricted — used in tests and as a sentinel for
    /// "no sandbox at all".
    #[allow(dead_code)]
    pub fn off() -> Self {
        Self {
            active: false,
            workspace_dir: String::new(),
            read_policy: Policy::Unrestricted,
            write_policy: Policy::Unrestricted,
            network_policy: Policy::Unrestricted,
        }
    }

    /// Workspace-only write restriction (legacy convenience).
    /// Prefer `with_policies` for new code.
    #[allow(dead_code)]
    pub fn new(workspace_dir: &str) -> Self {
        Self {
            active: !workspace_dir.is_empty(),
            workspace_dir: workspace_dir.to_string(),
            // Defaults preserve the old behaviour for callers that only set a
            // workspace dir.  Use `with_policies` to attach actual policy
            // levels from a SandboxConfig.
            read_policy: Policy::Unrestricted,
            write_policy: Policy::Restricted,
            network_policy: Policy::Unrestricted,
        }
    }

    /// Build rules with explicit policies (preferred — keeps every policy
    /// in one place).  Active = any policy is non-Unrestricted.
    pub fn with_policies(
        workspace_dir: Option<&str>,
        read: Policy,
        write: Policy,
        network: Policy,
    ) -> Self {
        let ws = workspace_dir.unwrap_or("");
        let active = !ws.is_empty()
            || read != Policy::Unrestricted
            || write != Policy::Unrestricted
            || network != Policy::Unrestricted;
        Self {
            active,
            workspace_dir: ws.to_string(),
            read_policy: read,
            write_policy: write,
            network_policy: network,
        }
    }

    /// Check whether the given path is allowed for **read** access.
    pub fn read_path_is_allowed(&self, raw_path: &str) -> Result<(), String> {
        match self.read_policy {
            Policy::Forbidden => Err(format!("read blocked by sandbox policy: {raw_path}")),
            Policy::Unrestricted => Ok(()),
            Policy::Restricted => self.path_in_workspace(raw_path),
        }
    }

    /// Check whether the given path is allowed for **write** access.
    pub fn write_path_is_allowed(&self, raw_path: &str) -> Result<(), String> {
        match self.write_policy {
            Policy::Forbidden => Err(format!("write blocked by sandbox policy: {raw_path}")),
            Policy::Unrestricted => Ok(()),
            Policy::Restricted => self.path_in_workspace(raw_path),
        }
    }

    /// Back-compat: `path_is_allowed` aliases write semantics (the original
    /// behaviour was implicit-write).  Existing callers keep working.
    pub fn path_is_allowed(&self, raw_path: &str) -> Result<(), String> {
        self.write_path_is_allowed(raw_path)
    }

    fn path_in_workspace(&self, raw_path: &str) -> Result<(), String> {
        if self.workspace_dir.is_empty() {
            // Restricted but no workspace — degrade to Unrestricted.
            return Ok(());
        }
        if !std::path::Path::new(raw_path).is_absolute()
            && raw_path.split('/').any(|s| s == "..")
        {
            return Err(format!("path {raw_path} escapes workspace {}", self.workspace_dir));
        }
        let resolved = if std::path::Path::new(raw_path).is_absolute() {
            raw_path.to_string()
        } else {
            std::path::Path::new(&self.workspace_dir)
                .join(raw_path)
                .to_string_lossy()
                .to_string()
        };
        if resolved.starts_with(&self.workspace_dir) {
            Ok(())
        } else {
            Err(format!(
                "path {raw_path} (→ {resolved}) is outside workspace {}",
                self.workspace_dir
            ))
        }
    }
}

// ── SBPL profile generation ────────────────────────────────────────────

/// Generated SBPL profile.
pub struct SandboxProfile {
    sbpl: String,
}

impl SandboxProfile {
    /// Generate an SBPL profile from the sandbox config.
    ///
    /// The profile is assembled from independent fragments driven by the
    /// **effective** policies (after workspace degradation).
    pub fn generate(config: &SandboxConfig) -> Self {
        // ── File‑read ─────────────────────────────────────────────
        let read_section = match config.effective_read() {
            Policy::Unrestricted => FS_READ_FULL.to_string(),
            Policy::Restricted => {
                let ws = config.workspace_dir.as_deref().unwrap_or("/dev/null");
                let canonical = std::fs::canonicalize(ws)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ws.to_string());
                fs_read_restricted(&canonical)
            }
            Policy::Forbidden => String::new(),
        };

        // ── File‑write ────────────────────────────────────────────
        let write_section = match config.effective_write() {
            Policy::Unrestricted => String::new(),
            Policy::Restricted => {
                let ws = config.workspace_dir.as_deref().unwrap_or("/dev/null");
                let canonical = std::fs::canonicalize(ws)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ws.to_string());
                fs_write_restricted(&canonical)
            }
            Policy::Forbidden => String::new(),
        };

        // ── Network ───────────────────────────────────────────────
        // Network is binary: either allowed or completely forbidden.
        // There is no "Restricted" workspace to scope network to.
        let network_section = match config.effective_network() {
            Policy::Forbidden => NETWORK_POLICY_FORBIDDEN,
            _ => NETWORK_POLICY_ALLOW,
        };

        let sbpl = {
            let mut parts: Vec<String> = Vec::new();
            parts.push(SBPL_COMMON.replace("{{NETWORK_POLICY}}", network_section.trim_start()));
            if !read_section.is_empty() {
                parts.push(read_section);
            }
            if !write_section.is_empty() {
                parts.push(write_section);
            }
            parts.join("\n")
        };

        Self { sbpl }
    }

    /// The raw SBPL content for passing to `sandbox-exec -p`.
    pub fn to_sbpl(&self) -> &str {
        &self.sbpl
    }

    /// Build a `Command` that runs the given shell command under sandbox-exec.
    ///
    /// Returns `None` if sandboxing is not needed (write is Unrestricted).
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "tests/sandbox_tests.rs"]
mod sandbox_tests;

#[cfg(test)]
#[path = "tests/sandbox_integration_tests.rs"]
mod tests;
