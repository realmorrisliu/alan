//! Native Tool adapter sandbox derived from explicit Host Mount grants.
//!
//! This sandbox only enforces that all operations happen within
//! the host_mount directory. No OS-level sandboxing (Landlock/Seatbelt).
//! Shell enforcement is intentionally limited to direct shell syntax, explicit
//! path-like argv references, redirection targets, and a curated set of common
//! direct interpreters. It does not infer utility-specific operand roles for
//! arbitrary bare tokens, and it does not inspect arbitrary program-internal
//! writes or dispatch, such as commands that mutate program-private state
//! without an explicit path operand (`git init`, `git add`, `git config
//! --local`), utility actions like `find -delete`, build or task runner
//! recipes, or utility-specific script/DSL modes such as `sed -f`.

mod command_guard;
mod command_interpreters;
pub(in crate::tools) mod command_options;
pub(in crate::tools) mod command_process;
pub(in crate::tools) mod command_wrappers;
mod path_literals;
mod path_safety;
mod program_command;
mod sandbox_spec;
mod shell_syntax;

pub use command_process::OutputProgressCallback;
#[cfg(any(target_os = "linux", all(test, unix)))]
pub(in crate::tools) use command_wrappers::command_and_args;
pub use sandbox_spec::{NetworkPosture, SandboxHostMount, SandboxSpec};

#[cfg(test)]
use command_wrappers::{shell_wrapper_inline_script, validate_nested_command_evaluators};
use path_literals::lexically_normalize_path;
use path_safety::is_path_guard_reason;

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) use shell_syntax::parse_standalone_cd;
#[cfg(any(target_os = "linux", test))]
pub(in crate::tools) use shell_syntax::shell_commands;

const SANDBOX_BACKEND_PATH_GUARD: &str = "host_mount_path_guard";

/// How thoroughly to validate command paths. Under an OS sandbox the kernel
/// enforces host_mount containment, so only protected-subpath writes need the
/// parser; on the path-guard fallback the full syntactic checks apply.
#[derive(Debug, Clone, Copy)]
enum PathCheckMode {
    Full,
    ProtectedOnly,
}

/// Execution result from sandbox
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Simple host_mount-only sandbox
#[derive(Debug, Clone)]
pub struct Sandbox {
    spec: SandboxSpec,
    excluded_host_roots: Vec<PathBuf>,
    /// Forces a specific backend instead of host detection (tests only).
    backend_override: Option<super::sandbox_backend::SandboxBackendKind>,
    /// Explicit invocation-only trust posture governing sandbox and guard enforcement.
    trust_posture: crate::policy::TrustPosture,
}

impl Sandbox {
    /// Create a new sandbox restricted to the given host_mount
    pub fn new(host_mount_root: PathBuf) -> Self {
        Self::from_spec(SandboxSpec::seed(host_mount_root))
    }

    /// Create a new sandbox from a projected confinement spec.
    pub fn from_spec(spec: SandboxSpec) -> Self {
        let spec = spec.exclude_explicit_mounts_from_read_denylist();
        assert!(
            !spec.readable_roots.is_empty(),
            "SandboxSpec requires at least one explicit Host Mount"
        );
        Self {
            spec,
            excluded_host_roots: Vec::new(),
            backend_override: None,
            trust_posture: crate::policy::TrustPosture::Governed,
        }
    }

    /// Construct a sandbox pinned to a specific backend (tests only), so the
    /// path-guard parser can be exercised regardless of the host's OS sandbox.
    #[cfg(test)]
    pub fn with_backend(
        host_mount_root: PathBuf,
        backend: super::sandbox_backend::SandboxBackendKind,
    ) -> Self {
        Self::from_spec_with_backend(SandboxSpec::seed(host_mount_root), backend)
    }

    /// Construct a spec-based sandbox pinned to a specific backend (tests only).
    #[cfg(test)]
    pub fn from_spec_with_backend(
        spec: SandboxSpec,
        backend: super::sandbox_backend::SandboxBackendKind,
    ) -> Self {
        let spec = spec.exclude_explicit_mounts_from_read_denylist();
        assert!(
            !spec.readable_roots.is_empty(),
            "SandboxSpec requires at least one explicit Host Mount"
        );
        Self {
            spec,
            excluded_host_roots: Vec::new(),
            backend_override: Some(backend),
            trust_posture: crate::policy::TrustPosture::Governed,
        }
    }

    /// Return the underlying projected confinement spec.
    pub fn spec(&self) -> &SandboxSpec {
        &self.spec
    }

    /// Configure explicit invocation trust posture on this sandbox.
    pub fn with_trust_posture(mut self, trust_posture: crate::policy::TrustPosture) -> Self {
        self.trust_posture = trust_posture;
        self
    }

    /// Return the active invocation trust posture on this sandbox.
    pub const fn trust_posture(&self) -> crate::policy::TrustPosture {
        self.trust_posture
    }

    /// Configure explicit trust posture on this sandbox (`ConfinedTrusted` vs `Governed`).
    pub fn with_explicit_trust(mut self, explicit_trust: bool) -> Self {
        self.trust_posture = if explicit_trust {
            crate::policy::TrustPosture::ConfinedTrusted
        } else {
            crate::policy::TrustPosture::Governed
        };
        self
    }

    /// Return whether explicit trust bypasses action approvals on this sandbox.
    pub fn explicit_trust(&self) -> bool {
        self.trust_posture.bypasses_action_approvals()
    }

    fn os_sandbox_or_explicit_trust(&self) -> bool {
        self.active_backend().is_os_enforced() || self.trust_posture.bypasses_action_approvals()
    }

    fn path_check_mode(&self) -> PathCheckMode {
        if self.os_sandbox_or_explicit_trust() {
            PathCheckMode::ProtectedOnly
        } else {
            PathCheckMode::Full
        }
    }

    /// Exclude other Host grants outside this sandbox's active roots.
    /// The Host adapter supplies these roots from the current grant projection.
    pub fn with_excluded_host_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.excluded_host_roots = roots;
        self
    }

    fn primary_host_root(&self) -> &Path {
        &self.spec.readable_roots[0]
    }

    fn allowed_roots_label(&self) -> String {
        self.spec
            .readable_roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The backend in effect (override for tests, else host detection).
    fn active_backend(&self) -> super::sandbox_backend::SandboxBackendKind {
        self.backend_override
            .unwrap_or_else(super::sandbox_backend::detect_backend)
    }

    /// Name of the active sandbox backend.
    pub fn backend_name(&self) -> &'static str {
        self.active_backend().name()
    }

    /// Name of the built-in host_mount path guard backend.
    pub const fn backend_name_static() -> &'static str {
        SANDBOX_BACKEND_PATH_GUARD
    }

    /// Return a stable rejection reason when a bash command shape is incompatible
    /// with the host_mount path guard backend.
    pub fn bash_preflight_reason(cmd: &str) -> Option<String> {
        command_guard::validate_bash_command_shape(cmd)
            .err()
            .map(|err| err.to_string())
    }

    /// Return a stable routing reason when a bash command targets local paths
    /// outside the current host_mount.
    pub fn bash_path_guard_reason(&self, cmd: &str, cwd: &Path) -> Option<String> {
        if !self.command_cwd_is_authorized(cwd, None) {
            return Some(format!(
                "Working directory outside host_mount roots: {} (allowed roots: {})",
                cwd.display(),
                self.allowed_roots_label()
            ));
        }

        match self.validate_command_paths(cmd, cwd, self.path_check_mode(), None) {
            Ok(()) => None,
            Err(err) => {
                let reason = err.to_string();
                is_path_guard_reason(&reason).then_some(reason)
            }
        }
    }

    fn command_cwd_is_authorized(
        &self,
        cwd: &Path,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> bool {
        if matches!(capability, Some(alan_agent_protocol::ToolCapability::Read))
            || self.active_backend()
                == super::sandbox_backend::SandboxBackendKind::LinuxReifiedNamespace
        {
            self.is_readable(cwd)
        } else {
            self.is_writable(cwd)
        }
    }

    /// Check if a path is within the host_mount seed root or another writable root.
    pub fn is_writable(&self, path: &Path) -> bool {
        // Try to get absolute path
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.primary_host_root().join(path)
        };
        self.spec
            .writable_roots
            .iter()
            .any(|root| self.is_path_in_root(&absolute_path, root))
    }

    /// Check whether a path is reachable through an explicit Host Mount.
    pub fn is_readable(&self, path: &Path) -> bool {
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.primary_host_root().join(path)
        };
        self.spec
            .readable_roots
            .iter()
            .any(|root| self.is_path_in_root(&absolute_path, root))
    }

    fn is_path_in_root(&self, absolute_path: &Path, root: &Path) -> bool {
        let canonical_root = self
            .canonicalize(root)
            .unwrap_or_else(|_| lexically_normalize_path(root));
        let normalized_root = lexically_normalize_path(root);

        // For existing paths, use canonical path
        if absolute_path.exists() {
            let canonical_path = self
                .canonicalize(absolute_path)
                .unwrap_or_else(|_| lexically_normalize_path(absolute_path));
            let normalized_path = lexically_normalize_path(&canonical_path);
            return canonical_path.starts_with(&canonical_root)
                || normalized_path.starts_with(&normalized_root);
        }

        // For new files, check that existing parent directories are within an
        // allowed writable root.
        let mut current = absolute_path.parent();
        while let Some(parent) = current {
            if parent.exists() {
                let canonical_parent = self
                    .canonicalize(parent)
                    .unwrap_or_else(|_| lexically_normalize_path(parent));
                let normalized_parent = lexically_normalize_path(&canonical_parent);
                return canonical_parent.starts_with(&canonical_root)
                    || normalized_parent.starts_with(&normalized_root);
            }
            current = parent.parent();
        }

        // If no parent exists, check if the path itself starts with the allowed root.
        let normalized_path = lexically_normalize_path(absolute_path);
        normalized_path.starts_with(&canonical_root)
            || normalized_path.starts_with(&normalized_root)
    }

    /// Read a file within the host_mount
    pub async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        if !self.is_readable(path) {
            return Err(anyhow!(
                "Path outside host_mount roots: {} (allowed roots: {})",
                path.display(),
                self.allowed_roots_label()
            ));
        }
        self.ensure_path_not_read_denied(path, "read")?;

        tokio::fs::read(path)
            .await
            .map_err(|e| anyhow!("Failed to read file: {}", e))
    }

    /// Read file as string
    pub async fn read_string(&self, path: &Path) -> Result<String> {
        let bytes = self.read(path).await?;
        String::from_utf8(bytes).map_err(|e| anyhow!("Invalid UTF-8: {}", e))
    }

    /// Write a file within the host_mount
    pub async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
        if !self.is_writable(path) {
            return Err(anyhow!(
                "Path outside host_mount roots: {} (allowed roots: {})",
                path.display(),
                self.allowed_roots_label()
            ));
        }
        self.ensure_path_not_protected(path, "write")?;
        self.ensure_path_not_multiply_linked(path, "write")?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| anyhow!("Failed to write file: {}", e))
    }

    /// Execute a command within the host_mount
    pub async fn exec(&self, cmd: &str, cwd: &Path) -> Result<ExecResult> {
        self.exec_with_timeout_and_capability(cmd, cwd, None, None)
            .await
    }

    /// Execute a command within the host_mount with an optional timeout.
    pub async fn exec_with_timeout(
        &self,
        cmd: &str,
        cwd: &Path,
        timeout: Option<Duration>,
    ) -> Result<ExecResult> {
        self.exec_with_timeout_and_capability(cmd, cwd, timeout, None)
            .await
    }

    /// Execute a command within the host_mount with path-guard checks.
    pub async fn exec_with_timeout_and_capability(
        &self,
        cmd: &str,
        cwd: &Path,
        timeout: Option<Duration>,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<ExecResult> {
        self.exec_with_timeout_capability_and_progress(cmd, cwd, timeout, capability, None)
            .await
    }

    /// Execute a command within the host_mount, optionally streaming incremental
    /// bounded `(stdout_tail, stderr_tail, truncated)` snapshots while running.
    pub async fn exec_with_timeout_capability_and_progress(
        &self,
        cmd: &str,
        cwd: &Path,
        timeout: Option<Duration>,
        capability: Option<alan_agent_protocol::ToolCapability>,
        progress: Option<command_process::OutputProgressCallback>,
    ) -> Result<ExecResult> {
        if self.trust_posture.is_unsandboxed_host_account() {
            let mut command = tokio::process::Command::new("/bin/sh");
            command.arg("-p").arg("-c").arg(cmd);
            command.current_dir(cwd);
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let output =
                command_process::output_with_progress(command, timeout, None, progress).await?;
            return Ok(ExecResult {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            });
        }

        if !self.command_cwd_is_authorized(cwd, capability) {
            return Err(anyhow!(
                "Working directory outside host_mount roots: {} (allowed roots: {})",
                cwd.display(),
                self.allowed_roots_label()
            ));
        }
        self.ensure_path_not_protected(cwd, "process cwd")?;

        if self.os_sandbox_or_explicit_trust() {
            // An OS-enforced backend (Seatbelt, Landlock, LinuxReifiedNamespace)
            // or explicit trust confines host_mount writes/network at the kernel
            // boundary, so shell expansion (`$VAR`, `$(...)`, backticks), globs,
            // brace expansion, and shell wrappers are permitted while protected
            // `.git`/`.alan`/`.agents` subpaths and literal out-of-mount paths
            // remain guarded in ProtectedOnly mode.
            self.validate_command_paths(cmd, cwd, PathCheckMode::ProtectedOnly, capability)?;
        } else {
            // Unconfined host_mount_path_guard fallback without explicit trust:
            // keep full static shell feature and shape guards.
            self.validate_shell_features(cmd)?;
            self.validate_command_paths(cmd, cwd, PathCheckMode::Full, capability)?;
        }

        // In Governed mode, a command only reaches execution after policy/reviewer/human
        // clearance, so an approved network command lifts network confinement.
        // In ConfinedTrusted (`--trust`), per-action prompts are bypassed, so OS network
        // confinement MUST NOT be silently widened beyond the SandboxSpec posture.
        let allow_network = self.spec.network.allows_network()
            || (!self.trust_posture.bypasses_action_approvals()
                && matches!(
                    capability,
                    Some(alan_agent_protocol::ToolCapability::Network)
                ));
        let backend = self.active_backend();
        if !allow_network
            && !backend.is_os_enforced()
            && self.trust_posture.bypasses_action_approvals()
            && matches!(
                capability,
                Some(alan_agent_protocol::ToolCapability::Network)
            )
        {
            return Err(anyhow!(
                "network access is confined under `--trust` without an OS network sandbox"
            ));
        }
        if !self.excluded_host_roots.is_empty()
            && !matches!(
                backend,
                super::sandbox_backend::SandboxBackendKind::Seatbelt
                    | super::sandbox_backend::SandboxBackendKind::LinuxReifiedNamespace
            )
        {
            return Err(anyhow!(
                "This backend cannot isolate inactive Host Mount reads"
            ));
        }
        if matches!(
            backend,
            super::sandbox_backend::SandboxBackendKind::LinuxReifiedNamespace
        ) {
            return self
                .exec_reified_namespace(cmd, cwd, timeout, allow_network)
                .await;
        }

        let mut command = self.build_confined_command(cmd, cwd, allow_network, backend)?;
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output =
            command_process::output_with_progress(command, timeout, None, progress).await?;

        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Build the shell command, confined by an OS sandbox backend when one is
    /// available. On macOS with Seatbelt this wraps the shell in `sandbox-exec`
    /// with a host_mount-write/no-network profile; otherwise it runs the shell
    /// directly under the best-effort path guard.
    fn build_confined_command(
        &self,
        cmd: &str,
        cwd: &Path,
        allow_network: bool,
        backend: super::sandbox_backend::SandboxBackendKind,
    ) -> Result<tokio::process::Command> {
        // Pin the system shell; -p ignores inherited shell functions/startup hooks.
        // Disable pathname expansion (-f) only on the untrusted path-guard fallback.
        let mut argv = vec![std::ffi::OsString::from("-p")];
        if !backend.is_os_enforced() && !self.trust_posture.bypasses_action_approvals() {
            argv.push(std::ffi::OsString::from("-f"));
        }
        argv.push(std::ffi::OsString::from("-c"));
        argv.push(std::ffi::OsString::from(cmd));
        self.build_confined_argv(Path::new("/bin/sh"), &argv, cwd, allow_network, backend)
    }

    async fn exec_reified_namespace(
        &self,
        cmd: &str,
        cwd: &Path,
        timeout: Option<Duration>,
        allow_network: bool,
    ) -> Result<ExecResult> {
        let input = self.reified_namespace_input_for_command(cmd, cwd, allow_network);
        #[cfg(target_os = "linux")]
        let input = super::reified_namespace::configure_linux_command_environment(
            input,
            shell_commands(cmd)?,
        )
        .await
        .map_err(|reason| anyhow!("linux command environment unavailable: {reason}"))?;
        let plan = super::reified_namespace::ReifiedNamespacePlan::derive(input)
            .map_err(|err| anyhow!("failed to build reified namespace plan: {err}"))?;
        let runner = super::reified_namespace::LinuxReifiedNamespaceRunner::with_fallback_backend(
            super::sandbox_backend::detect_projection_backend(),
        );
        runner
            .run_cancellable(&plan, timeout)
            .await
            .map_err(anyhow::Error::from)
    }

    fn reified_namespace_input_for_command(
        &self,
        cmd: &str,
        cwd: &Path,
        allow_network: bool,
    ) -> super::reified_namespace::ReifiedNamespacePlanInput {
        let cwd = if cwd.is_absolute() {
            cwd.to_path_buf()
        } else {
            self.primary_host_root().join(cwd)
        };
        let network = if allow_network {
            NetworkPosture::Allow
        } else {
            NetworkPosture::Deny
        };
        super::reified_namespace::ReifiedNamespacePlanInput::new(
            self.reified_mount_declarations(),
            cwd,
            vec![
                "/bin/sh".to_string(),
                "-p".to_string(),
                "-c".to_string(),
                cmd.to_string(),
            ],
            network,
        )
    }

    fn reified_mount_declarations(&self) -> Vec<super::reified_namespace::ReifiedMountDeclaration> {
        self.spec
            .host_mounts
            .iter()
            .map(|grant| {
                super::reified_namespace::ReifiedMountDeclaration::host(
                    &grant.host_path,
                    grant.host_path.clone(),
                    match grant.access {
                        super::reified_namespace::ReifiedMountAccess::ReadOnly => {
                            super::reified_namespace::ReifiedMountAccess::ReadOnly
                        }
                        super::reified_namespace::ReifiedMountAccess::ReadWrite => {
                            super::reified_namespace::ReifiedMountAccess::ReadWrite
                        }
                    },
                )
            })
            .collect()
    }

    /// List directory contents
    pub async fn list_dir(&self, path: &Path) -> Result<Vec<tokio::fs::DirEntry>> {
        if !self.is_readable(path) {
            return Err(anyhow!(
                "Path outside host_mount roots: {} (allowed roots: {})",
                path.display(),
                self.allowed_roots_label()
            ));
        }
        self.ensure_path_not_read_denied(path, "list directory")?;

        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path).await?;
        while let Some(entry) = dir.next_entry().await? {
            entries.push(entry);
        }
        Ok(entries)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        Ok(dunce::canonicalize(path)?)
    }
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
