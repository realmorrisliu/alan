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

mod command_interpreters;
mod command_options;
mod command_wrappers;
mod path_literals;
mod path_safety;
mod sandbox_spec;
mod shell_syntax;

pub(crate) use path_safety::protected_path_component;
pub use sandbox_spec::{NetworkPosture, SandboxSpec};

use command_wrappers::{
    shell_wrapper_inline_script, validate_direct_command_shapes, validate_nested_command_evaluators,
};
use path_literals::{
    absolute_path_literal_candidates, is_allowed_absolute_command_path,
    is_file_redirection_operator, lexically_normalize_path,
    looks_like_bare_protected_subpath_token, looks_like_path_token, path_like_subtokens,
    translate_reified_shell_token,
};
use path_safety::{existing_regular_file_has_multiple_links, is_path_guard_reason};
use shell_syntax::{
    ShellWordToken, normalize_shell_line_continuations, shell_commands, shell_word_tokens,
    shell_word_tokens_with_spans, validate_shell_features,
};

use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
#[derive(Clone)]
pub struct Sandbox {
    spec: SandboxSpec,
    /// Forces a specific backend instead of host detection (tests only).
    backend_override: Option<super::sandbox_backend::SandboxBackendKind>,
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
            backend_override: None,
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
            backend_override: Some(backend),
        }
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
        Self::backend_name_static()
    }

    /// Name of the built-in host_mount path guard backend.
    pub const fn backend_name_static() -> &'static str {
        SANDBOX_BACKEND_PATH_GUARD
    }

    /// Return a stable rejection reason when a bash command shape is incompatible
    /// with the host_mount path guard backend.
    pub fn bash_preflight_reason(cmd: &str) -> Option<String> {
        validate_bash_command_shape(cmd)
            .err()
            .map(|err| err.to_string())
    }

    /// Return a stable routing reason when a bash command targets local paths
    /// outside the current host_mount.
    pub fn bash_path_guard_reason(&self, cmd: &str, cwd: &Path) -> Option<String> {
        if !self.is_writable(cwd) {
            return Some(format!(
                "Working directory outside host_mount roots: {} (allowed roots: {})",
                cwd.display(),
                self.allowed_roots_label()
            ));
        }

        match self.validate_command_paths(cmd, cwd, PathCheckMode::Full, None) {
            Ok(()) => None,
            Err(err) => {
                let reason = err.to_string();
                is_path_guard_reason(&reason).then_some(reason)
            }
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
        let read_only_command =
            matches!(capability, Some(alan_agent_protocol::ToolCapability::Read));
        let cwd_is_authorized = if read_only_command {
            self.is_readable(cwd)
        } else {
            self.is_writable(cwd)
        };
        if !cwd_is_authorized {
            return Err(anyhow!(
                "Working directory outside host_mount roots: {} (allowed roots: {})",
                cwd.display(),
                self.allowed_roots_label()
            ));
        }
        self.ensure_path_not_protected(cwd, "process cwd")?;

        // Reject shell expansion ($VAR, $(...), backticks, globs, braces) in EVERY
        // mode. Expansion defeats the static path-containment check — the parser
        // sees a literal, in-host_mount-looking token (`$HOME/.ssh/id_rsa`) but
        // `/bin/sh -c` then expands it to escape the host_mount. Seatbelt permits
        // reads, so an auto-approved read must not be able to exfiltrate this way.
        self.validate_shell_features(cmd)?;

        if self.active_backend().permits_autonomous_bash() {
            // Seatbelt kernel-confines the host_mount fs + network, so the syntactic
            // *shape* checks are dropped — they would reject commands the sandbox
            // safely contains (`bash -lc ...`, `python -c ...`). Path containment
            // and the protected-subpath check (incl. shell-wrapper-nested) still
            // run in ProtectedOnly mode.
            self.validate_command_paths(cmd, cwd, PathCheckMode::ProtectedOnly, capability)?;
        } else {
            // No kernel protected-subpath enforcement (Landlock cannot carve a
            // protected subdir out of the writable tree, or the path-guard
            // fallback): keep the full shape parser so opaque writers — which could
            // hide a protected write the kernel won't deny — are rejected.
            self.validate_command_paths(cmd, cwd, PathCheckMode::Full, capability)?;
        }

        // A command only reaches execution after policy/reviewer/human clearance.
        // If it is classified as a network capability, run it with the sandbox's
        // network restriction lifted (still filesystem-confined) so an approved
        // network call actually runs instead of failing under a deny-all profile.
        let allow_network = self.spec.network.allows_network()
            || matches!(
                capability,
                Some(alan_agent_protocol::ToolCapability::Network)
            );
        let backend = self.active_backend();
        if matches!(
            backend,
            super::sandbox_backend::SandboxBackendKind::LinuxReifiedNamespace
        ) {
            return self
                .exec_reified_namespace(cmd, cwd, timeout, allow_network)
                .await;
        }

        let mut command = self.build_confined_command(cmd, allow_network, backend)?;
        command.current_dir(cwd);
        let output = if let Some(limit) = timeout {
            match tokio::time::timeout(limit, command.output()).await {
                Ok(result) => result.map_err(|e| anyhow!("Failed to execute command: {}", e))?,
                Err(_) => {
                    return Err(anyhow!(
                        "Command execution timed out after {}s",
                        limit.as_secs()
                    ));
                }
            }
        } else {
            command
                .output()
                .await
                .map_err(|e| anyhow!("Failed to execute command: {}", e))?
        };

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
        allow_network: bool,
        backend: super::sandbox_backend::SandboxBackendKind,
    ) -> Result<tokio::process::Command> {
        // Defense in depth: start the shell with pathname expansion disabled.
        let command = match backend {
            super::sandbox_backend::SandboxBackendKind::Seatbelt => {
                let profile = super::sandbox_backend::seatbelt_profile(
                    &self.spec.writable_roots,
                    &self.spec.read_denylist,
                    allow_network,
                );
                let mut command = tokio::process::Command::new("/usr/bin/sandbox-exec");
                command
                    .arg("-p")
                    .arg(profile)
                    .arg("sh")
                    .arg("-f")
                    .arg("-c")
                    .arg(cmd);
                command
            }
            super::sandbox_backend::SandboxBackendKind::LinuxReifiedNamespace => {
                return Err(anyhow!(
                    "linux_reified_namespace backend is detectable but no runner is installed yet"
                ));
            }
            #[cfg(target_os = "linux")]
            super::sandbox_backend::SandboxBackendKind::Landlock => {
                use std::os::unix::process::CommandExt;
                let writable_roots = self.spec.writable_roots.clone();
                let read_denylist = self.spec.read_denylist.clone();
                let mut command = std::process::Command::new("sh");
                command.arg("-f").arg("-c").arg(cmd);
                // SAFETY: pre_exec runs in the forked child before exec; it only
                // applies a Landlock ruleset (no shared-state mutation).
                unsafe {
                    command.pre_exec(move || {
                        super::sandbox_backend::apply_landlock(
                            &writable_roots,
                            &read_denylist,
                            allow_network,
                        )
                    });
                }
                tokio::process::Command::from(command)
            }
            _ => {
                let mut command = tokio::process::Command::new("sh");
                command.arg("-f").arg("-c").arg(cmd);
                command
            }
        };
        Ok(command)
    }

    async fn exec_reified_namespace(
        &self,
        cmd: &str,
        cwd: &Path,
        timeout: Option<Duration>,
        allow_network: bool,
    ) -> Result<ExecResult> {
        let plan = self.reified_namespace_plan_for_command(cmd, cwd, allow_network)?;
        let runner = super::reified_namespace::LinuxReifiedNamespaceRunner::with_fallback_backend(
            super::sandbox_backend::detect_projection_backend(),
        );
        let run = move || {
            runner
                .run_with_timeout(&plan, timeout)
                .map_err(anyhow::Error::from)
        };
        tokio::task::spawn_blocking(run)
            .await
            .map_err(|err| anyhow!("reified namespace runner task failed: {err}"))?
    }

    fn reified_namespace_plan_for_command(
        &self,
        cmd: &str,
        cwd: &Path,
        allow_network: bool,
    ) -> Result<super::reified_namespace::ReifiedNamespacePlan> {
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
        let mut plan = super::reified_namespace::ReifiedNamespacePlan::derive(
            super::reified_namespace::ReifiedNamespacePlanInput::new(
                self.reified_mount_declarations(),
                cwd,
                vec![
                    "sh".to_string(),
                    "-f".to_string(),
                    "-c".to_string(),
                    cmd.to_string(),
                ],
                network,
            ),
        )
        .map_err(|err| anyhow!("failed to build reified namespace plan: {err}"))?;
        plan.argv = vec![
            "sh".to_string(),
            "-f".to_string(),
            "-c".to_string(),
            Self::translate_reified_command_host_paths(cmd, &plan),
        ];
        Ok(plan)
    }

    fn reified_mount_declarations(&self) -> Vec<super::reified_namespace::ReifiedMountDeclaration> {
        self.spec
            .host_mounts
            .iter()
            .map(|grant| {
                super::reified_namespace::ReifiedMountDeclaration::host(
                    &grant.namespace_path,
                    grant.host_path.clone(),
                    match grant.access {
                        alan_kernel::Access::ReadOnly => {
                            super::reified_namespace::ReifiedMountAccess::ReadOnly
                        }
                        alan_kernel::Access::ReadWrite => {
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

    fn validate_command_paths(
        &self,
        cmd: &str,
        cwd: &Path,
        mode: PathCheckMode,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<()> {
        let protected_only = matches!(mode, PathCheckMode::ProtectedOnly);
        let normalized = normalize_shell_line_continuations(cmd);
        let trimmed = normalized.trim();
        if trimmed.is_empty() {
            // Under an OS sandbox an empty command is the executor's problem.
            return if protected_only {
                Ok(())
            } else {
                Err(anyhow!("Command cannot be empty"))
            };
        }

        // In ProtectedOnly mode an unparseable shape is tolerated (the OS sandbox
        // confines writes + network), but parseable path operands are still
        // containment-checked below — only the syntactic shape checks are dropped.
        let tokens = match shell_word_tokens(trimmed) {
            Ok(tokens) => tokens,
            Err(err) => return if protected_only { Ok(()) } else { Err(err) },
        };
        let span_tokens = match shell_word_tokens_with_spans(trimmed) {
            Ok(tokens) => tokens,
            Err(err) => return if protected_only { Ok(()) } else { Err(err) },
        };
        let commands = match shell_commands(trimmed) {
            Ok(commands) => commands,
            Err(err) => return if protected_only { Ok(()) } else { Err(err) },
        };
        if !protected_only {
            self.validate_direct_command_shapes(&commands)?;
            self.validate_nested_command_evaluators(&commands)?;
        }

        // Wrapper forms (`bash -lc 'echo x > .git/config'`) hide their operands
        // inside a quoted script the outer tokenizer can't decompose. Under an OS
        // sandbox these are allowed to run, so recurse into the inline script and
        // apply the same protected-subpath checks to the wrapped command.
        if protected_only {
            for words in &commands {
                if let Some(inner) = shell_wrapper_inline_script(words) {
                    self.validate_command_paths(
                        &inner,
                        cwd,
                        PathCheckMode::ProtectedOnly,
                        capability,
                    )?;
                }
            }
        }

        let mut expects_redirection_target = false;
        for token in tokens {
            if expects_redirection_target {
                self.validate_redirection_target(&token, cwd)?;
                expects_redirection_target = false;
                continue;
            }

            if is_file_redirection_operator(&token) {
                expects_redirection_target = true;
                continue;
            }

            for candidate in path_like_subtokens(&token) {
                self.validate_command_path_candidate(candidate, cwd, capability)?;
            }
        }

        self.validate_absolute_path_literals(&span_tokens, capability)?;

        Ok(())
    }

    fn validate_direct_command_shapes(&self, commands: &[Vec<String>]) -> Result<()> {
        validate_direct_command_shapes(commands, self.backend_name())
    }

    fn validate_shell_features(&self, cmd: &str) -> Result<()> {
        validate_shell_features(cmd, self.backend_name())
    }

    fn validate_nested_command_evaluators(&self, commands: &[Vec<String>]) -> Result<()> {
        validate_nested_command_evaluators(commands, self.backend_name())
    }

    fn ensure_path_not_protected(&self, path: &Path, action: &str) -> Result<()> {
        if let Some(component) = self.protected_subpath_component(path) {
            return Err(anyhow!(
                "Sandbox backend {} blocks {} under protected subpath {}: {}",
                self.backend_name(),
                action,
                component,
                path.display()
            ));
        }
        Ok(())
    }

    fn ensure_path_not_multiply_linked(&self, path: &Path, action: &str) -> Result<()> {
        if existing_regular_file_has_multiple_links(path)? {
            return Err(anyhow!(
                "Sandbox backend {} blocks {} via multiply-linked file because hardlink aliases cannot be validated safely: {}",
                self.backend_name(),
                action,
                path.display()
            ));
        }
        Ok(())
    }

    fn ensure_path_not_read_denied(&self, path: &Path, action: &str) -> Result<()> {
        let resolved_path = self.normalized_path(path);
        for deny_path in &self.spec.read_denylist {
            let canonical_deny = self
                .canonicalize(deny_path)
                .unwrap_or_else(|_| lexically_normalize_path(deny_path));
            let normalized_deny = lexically_normalize_path(deny_path);
            if resolved_path.starts_with(&canonical_deny)
                || resolved_path.starts_with(&normalized_deny)
            {
                return Err(anyhow!(
                    "Sandbox backend {} blocks {} under sensitive read-deny path: {}",
                    self.backend_name(),
                    action,
                    path.display()
                ));
            }
        }
        Ok(())
    }

    fn protected_subpath_component(&self, path: &Path) -> Option<&'static str> {
        let resolved_path = self.resolved_path_with_existing_parents(path);
        for root in &self.spec.writable_roots {
            let canonical_root = self
                .canonicalize(root)
                .unwrap_or_else(|_| lexically_normalize_path(root));
            let normalized_root = lexically_normalize_path(root);
            let root_protected = protected_path_component(&canonical_root)
                .or_else(|| protected_path_component(&normalized_root));
            let Ok(relative) = resolved_path
                .strip_prefix(&canonical_root)
                .or_else(|_| resolved_path.strip_prefix(&normalized_root))
            else {
                continue;
            };
            if let Some(component) = root_protected {
                return Some(component);
            }
            if let Some(component) = protected_path_component(relative) {
                return Some(component);
            }
        }
        None
    }

    fn normalized_path(&self, path: &Path) -> PathBuf {
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.primary_host_root().join(path)
        };
        if absolute_path.exists() {
            self.canonicalize(&absolute_path)
                .unwrap_or_else(|_| lexically_normalize_path(&absolute_path))
        } else {
            lexically_normalize_path(&absolute_path)
        }
    }

    fn resolved_path_with_existing_parents(&self, path: &Path) -> PathBuf {
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.primary_host_root().join(path)
        };
        if absolute_path.exists() {
            return self.normalized_path(&absolute_path);
        }

        let mut current = absolute_path.as_path();
        let mut suffix = Vec::<OsString>::new();
        while !current.exists() {
            let Some(name) = current.file_name() else {
                return lexically_normalize_path(&absolute_path);
            };
            suffix.push(name.to_os_string());
            let Some(parent) = current.parent() else {
                return lexically_normalize_path(&absolute_path);
            };
            current = parent;
        }

        let mut resolved = self
            .canonicalize(current)
            .unwrap_or_else(|_| lexically_normalize_path(current));
        for component in suffix.iter().rev() {
            resolved.push(component);
        }
        resolved
    }
    // HostMount containment is enforced for ALL modes, including ProtectedOnly.
    // The OS sandbox confines *writes* (and network) to the host_mount, but Seatbelt
    // permits reads by default, so an auto-approved read like `cat ~/.ssh/id_rsa`
    // would otherwise exfiltrate secrets into tool output. ProtectedOnly only drops
    // the syntactic *shape* checks (so wrappers may run), never path containment.
    fn validate_command_path_candidate(
        &self,
        token: &str,
        cwd: &Path,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<()> {
        if token.is_empty() || token.starts_with('-') {
            return Ok(());
        }

        if token.starts_with('~') {
            return Err(anyhow!(
                "Command references HOME paths outside host_mount: {}",
                token
            ));
        }

        if token.contains("://") {
            return Ok(());
        }

        if looks_like_path_token(token) || looks_like_bare_protected_subpath_token(token) {
            let candidate = if Path::new(token).is_absolute() {
                PathBuf::from(token)
            } else {
                cwd.join(token)
            };
            if candidate.is_absolute() && is_allowed_absolute_command_path(&candidate) {
                return Ok(());
            }
            let validation_path = self
                .reified_namespace_path_to_host(&candidate)
                .unwrap_or(candidate);
            let read_only_command =
                matches!(capability, Some(alan_agent_protocol::ToolCapability::Read));
            let path_is_authorized = if read_only_command {
                self.is_readable(&validation_path)
            } else {
                self.is_writable(&validation_path)
            };
            if !path_is_authorized {
                return Err(anyhow!(
                    "Command references path outside host_mount: {}",
                    token
                ));
            }
            self.ensure_path_not_protected(&validation_path, "process path reference")?;
            if read_only_command {
                self.ensure_path_not_read_denied(&validation_path, "process path reference")?;
            } else {
                self.ensure_path_not_multiply_linked(&validation_path, "process path reference")?;
            }
        }

        Ok(())
    }

    fn validate_redirection_target(&self, token: &str, cwd: &Path) -> Result<()> {
        if token.is_empty() {
            return Err(anyhow!("Command ends with an incomplete redirection"));
        }

        if token.starts_with('~') {
            return Err(anyhow!(
                "Command references HOME paths outside host_mount: {}",
                token
            ));
        }

        let candidate = if Path::new(token).is_absolute() {
            PathBuf::from(token)
        } else {
            cwd.join(token)
        };
        if candidate.is_absolute() && is_allowed_absolute_command_path(&candidate) {
            return Ok(());
        }
        let validation_path = self
            .reified_namespace_path_to_host(&candidate)
            .unwrap_or(candidate);
        if !self.is_writable(&validation_path) {
            return Err(anyhow!(
                "Command references path outside host_mount: {}",
                token
            ));
        }
        self.ensure_path_not_protected(&validation_path, "process path reference")?;
        self.ensure_path_not_multiply_linked(&validation_path, "process path reference")?;
        Ok(())
    }

    fn reified_namespace_path_to_host(&self, path: &Path) -> Option<PathBuf> {
        if !matches!(
            self.active_backend(),
            super::sandbox_backend::SandboxBackendKind::LinuxReifiedNamespace
        ) || !path.is_absolute()
        {
            return None;
        }

        for grant in &self.spec.host_mounts {
            let root = &grant.host_path;
            let namespace_path = PathBuf::from(&grant.namespace_path);
            if path == namespace_path {
                return Some(root.clone());
            }
            if let Ok(suffix) = path.strip_prefix(&namespace_path) {
                return Some(root.join(suffix));
            }
        }
        None
    }

    fn validate_absolute_path_literals(
        &self,
        tokens: &[ShellWordToken],
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<()> {
        for token in tokens {
            for candidates in absolute_path_literal_candidates(&token.decoded) {
                let literal = candidates
                    .iter()
                    .find(|candidate| {
                        self.absolute_path_literal_is_allowed_or_in_host_mount(
                            candidate, capability,
                        )
                    })
                    .unwrap_or_else(|| &candidates[0]);
                self.validate_absolute_path_literal(literal, capability)?;
            }
        }
        Ok(())
    }

    fn absolute_path_literal_is_allowed_or_in_host_mount(
        &self,
        literal: &str,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> bool {
        let literal_path = Path::new(literal);
        if is_allowed_absolute_command_path(literal_path) {
            return true;
        }
        let validation_path = self
            .reified_namespace_path_to_host(literal_path)
            .unwrap_or_else(|| literal_path.to_path_buf());
        if matches!(capability, Some(alan_agent_protocol::ToolCapability::Read)) {
            self.is_readable(&validation_path)
        } else {
            self.is_writable(&validation_path)
        }
    }

    fn validate_absolute_path_literal(
        &self,
        literal: &str,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<()> {
        let literal_path = Path::new(literal);
        if !literal_path.is_absolute() || is_allowed_absolute_command_path(literal_path) {
            return Ok(());
        }
        let validation_path = self
            .reified_namespace_path_to_host(literal_path)
            .unwrap_or_else(|| literal_path.to_path_buf());
        // Containment applies in every mode: the OS sandbox does not confine
        // reads, so an out-of-host_mount absolute path (e.g. a read of a secret)
        // must still be rejected by the parser.
        let read_only_command =
            matches!(capability, Some(alan_agent_protocol::ToolCapability::Read));
        let path_is_authorized = if read_only_command {
            self.is_readable(&validation_path)
        } else {
            self.is_writable(&validation_path)
        };
        if !path_is_authorized {
            return Err(anyhow!(
                "Command contains absolute path outside host_mount: {}",
                literal
            ));
        }
        self.ensure_path_not_protected(&validation_path, "process path reference")?;
        if read_only_command {
            self.ensure_path_not_read_denied(&validation_path, "process path reference")?;
        }
        Ok(())
    }

    fn translate_reified_command_host_paths(
        cmd: &str,
        plan: &super::reified_namespace::ReifiedNamespacePlan,
    ) -> String {
        let Ok(tokens) = shell_word_tokens_with_spans(cmd) else {
            return cmd.to_string();
        };
        let mut translated = String::with_capacity(cmd.len());
        let mut last = 0;
        for token in tokens {
            let Some(rewritten) = translate_reified_shell_token(&token.decoded, plan) else {
                continue;
            };
            translated.push_str(&cmd[last..token.raw_start]);
            translated.push_str(&rewritten);
            last = token.raw_end;
        }
        translated.push_str(&cmd[last..]);
        translated
    }
}

fn validate_bash_command_shape(cmd: &str) -> Result<()> {
    let normalized = normalize_shell_line_continuations(cmd);
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Command cannot be empty"));
    }

    let commands = shell_commands(trimmed)?;
    validate_direct_command_shapes(&commands, Sandbox::backend_name_static())?;
    validate_nested_command_evaluators(&commands, Sandbox::backend_name_static())?;
    validate_shell_features(trimmed, Sandbox::backend_name_static())?;

    Ok(())
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
