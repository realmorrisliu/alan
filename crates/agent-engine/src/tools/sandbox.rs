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

use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const SANDBOX_BACKEND_PATH_GUARD: &str = "host_mount_path_guard";
pub(crate) const PROTECTED_SUBPATHS: [&str; 3] = [".git", ".alan", ".agents"];

pub(crate) fn protected_path_component(path: &Path) -> Option<&'static str> {
    path.components().find_map(protected_component)
}

fn protected_component(component: Component<'_>) -> Option<&'static str> {
    let Component::Normal(name) = component else {
        return None;
    };
    let candidate = name.to_str()?;
    PROTECTED_SUBPATHS
        .iter()
        .copied()
        .find(|protected| *protected == candidate)
}

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

/// Default network posture for commands run inside a sandbox.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPosture {
    #[default]
    Deny,
    Allow,
}

impl NetworkPosture {
    pub(crate) const fn allows_network(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Projected OS-sandbox confinement input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxSpec {
    pub host_mounts: Vec<crate::HostMountGrant>,
    pub readable_roots: Vec<PathBuf>,
    pub writable_roots: Vec<PathBuf>,
    pub read_denylist: Vec<PathBuf>,
    pub network: NetworkPosture,
}

impl SandboxSpec {
    /// Build a single writable mount spec for isolated tests and adapters.
    pub fn seed(root: PathBuf) -> Self {
        let readable_roots = vec![root.clone()];
        let writable_roots = vec![root.clone()];
        let read_denylist = super::sandbox_backend::read_denylist_excluding_writable_roots(
            &Self::default_sensitive_read_denylist(),
            &writable_roots,
        );
        Self {
            host_mounts: vec![crate::HostMountGrant {
                namespace_path: "/mnt/source".to_string(),
                host_path: root,
                access: alan_kernel::Access::ReadWrite,
            }],
            readable_roots,
            writable_roots,
            read_denylist,
            network: NetworkPosture::Deny,
        }
    }

    /// Derive native write authority from the same Host Mount grants used by the namespace.
    pub fn from_host_mounts(grants: &[crate::HostMountGrant]) -> Self {
        let host_mounts = grants
            .iter()
            .map(|grant| crate::HostMountGrant {
                namespace_path: grant.namespace_path.clone(),
                host_path: dunce::canonicalize(&grant.host_path)
                    .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf()),
                access: grant.access,
            })
            .collect::<Vec<_>>();
        let readable_roots = host_mounts
            .iter()
            .map(|grant| grant.host_path.clone())
            .collect::<Vec<_>>();
        let writable_roots = grants
            .iter()
            .filter(|grant| grant.access == alan_kernel::Access::ReadWrite)
            .map(|grant| {
                dunce::canonicalize(&grant.host_path)
                    .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf())
            })
            .collect::<Vec<_>>();
        let read_denylist = super::sandbox_backend::read_denylist_excluding_writable_roots(
            &Self::default_sensitive_read_denylist(),
            &readable_roots,
        );
        Self {
            host_mounts,
            readable_roots,
            writable_roots,
            read_denylist,
            network: NetworkPosture::Deny,
        }
    }

    /// Build the default sensitive-read denylist from the current user's home
    /// directory. If the host home cannot be detected, keep the list empty
    /// rather than guessing.
    pub fn default_sensitive_read_denylist() -> Vec<PathBuf> {
        dirs::home_dir()
            .map(|home| Self::sensitive_read_denylist_for_home(&home))
            .unwrap_or_default()
    }

    /// Derive sensitive read-deny paths from an explicit home directory.
    pub fn sensitive_read_denylist_for_home(home_dir: &Path) -> Vec<PathBuf> {
        let mut paths = [".alan", ".alan-dev"]
            .into_iter()
            .map(|name| home_dir.join(name))
            .collect::<Vec<_>>();

        paths.extend(
            [
                ".ssh",
                ".aws",
                ".azure",
                ".config/gcloud",
                ".config/gh",
                ".docker",
                ".gnupg",
                ".kube",
                ".netrc",
                ".npmrc",
                ".pypirc",
                "Library/Keychains",
                "Library/Safari",
                "Library/Application Support/Arc",
                "Library/Application Support/BraveSoftware",
                "Library/Application Support/Chromium",
                "Library/Application Support/Firefox",
                "Library/Application Support/Google/Chrome",
                "Library/Application Support/com.apple.Safari",
            ]
            .into_iter()
            .map(|relative| home_dir.join(relative)),
        );

        paths
    }

    fn exclude_explicit_mounts_from_read_denylist(mut self) -> Self {
        self.read_denylist = super::sandbox_backend::read_denylist_excluding_writable_roots(
            &self.read_denylist,
            &self.readable_roots,
        );
        self
    }
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
            if let Some(component) = relative.components().find_map(protected_component) {
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

fn validate_nested_command_evaluators(commands: &[Vec<String>], backend_name: &str) -> Result<()> {
    for words in commands {
        let Some(view) = nested_evaluator_view(words) else {
            continue;
        };
        if let Some(display) = view.opaque_wrapper_display.as_deref() {
            return Err(anyhow!(
                "Sandbox backend {} rejects nested command evaluators like {} because inner paths cannot be validated safely",
                backend_name,
                display
            ));
        }
        if is_shell_eval_builtin(view.command) {
            return Err(anyhow!(
                "Sandbox backend {} rejects nested command evaluators like {} because inner paths cannot be validated safely",
                backend_name,
                view.display
            ));
        }
        if let Some(dispatcher) =
            opaque_command_dispatcher_display(&view.display, view.command, view.args)
        {
            return Err(anyhow!(
                "Sandbox backend {} rejects opaque command dispatchers like {} because child command paths cannot be validated safely",
                backend_name,
                dispatcher
            ));
        }
        if let Some(flag) = leading_eval_flag(view.command, view.args) {
            return Err(anyhow!(
                "Sandbox backend {} rejects nested command evaluators like {} {} because inner paths cannot be validated safely",
                backend_name,
                view.display,
                flag
            ));
        }
        if let Some(interpreter) =
            opaque_script_interpreter_display(&view.display, view.command, view.args)
        {
            return Err(anyhow!(
                "Sandbox backend {} rejects opaque script interpreters like {} because script bodies cannot be validated safely",
                backend_name,
                interpreter
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellWordToken {
    decoded: String,
    raw_start: usize,
    raw_end: usize,
}

fn translate_reified_shell_token(
    token: &str,
    plan: &super::reified_namespace::ReifiedNamespacePlan,
) -> Option<String> {
    if let Some(rewritten) = translate_reified_nested_shell_token(token, plan) {
        return Some(shell_quote_token(&rewritten));
    }

    let mut replacements = Vec::new();
    for range in reified_shell_token_path_candidate_ranges(token) {
        if replacements
            .iter()
            .any(|(existing, _)| ranges_overlap(existing, &range))
        {
            continue;
        }

        let candidate = &token[range.clone()];
        let candidate_path = Path::new(candidate);
        if !candidate_path.is_absolute() || is_allowed_absolute_command_path(candidate_path) {
            continue;
        }

        let Some(namespace_path) =
            plan.translate_projected_host_path(candidate_path)
                .or_else(|| {
                    plan.translate_projected_host_path(&lexically_normalize_path(candidate_path))
                })
        else {
            continue;
        };
        replacements.push((range, namespace_path.display().to_string()));
    }

    if replacements.is_empty() {
        return None;
    }

    replacements.sort_by_key(|(range, _)| range.start);
    let replacement_len = replacements
        .iter()
        .map(|(_, replacement)| replacement.len())
        .sum::<usize>();
    let mut rewritten = String::with_capacity(token.len() + replacement_len);
    let mut last = 0;
    for (range, replacement) in replacements {
        rewritten.push_str(&token[last..range.start]);
        rewritten.push_str(&replacement);
        last = range.end;
    }
    rewritten.push_str(&token[last..]);

    Some(shell_quote_reified_token(&rewritten))
}

fn translate_reified_nested_shell_token(
    token: &str,
    plan: &super::reified_namespace::ReifiedNamespacePlan,
) -> Option<String> {
    let tokens = shell_word_tokens_with_spans(token).ok()?;
    if !looks_like_nested_shell_script(&tokens) {
        return None;
    }

    let mut translated = String::with_capacity(token.len());
    let mut last = 0;
    let mut changed = false;
    for nested_token in tokens {
        let Some(rewritten) = translate_reified_shell_token(&nested_token.decoded, plan) else {
            continue;
        };
        translated.push_str(&token[last..nested_token.raw_start]);
        translated.push_str(&rewritten);
        last = nested_token.raw_end;
        changed = true;
    }
    if !changed {
        return None;
    }

    translated.push_str(&token[last..]);
    Some(translated)
}

fn looks_like_nested_shell_script(tokens: &[ShellWordToken]) -> bool {
    if tokens.len() < 2 {
        return false;
    }
    let Some(command) = tokens
        .iter()
        .find(|token| !is_env_assignment(&token.decoded))
    else {
        return false;
    };

    !looks_like_path_token(&command.decoded)
        && !looks_like_bare_protected_subpath_token(&command.decoded)
}

fn shell_quote_reified_token(token: &str) -> String {
    if is_env_assignment(token) {
        let (name, value) = token
            .split_once('=')
            .expect("is_env_assignment requires an equals sign");
        return format!("{name}={}", shell_quote_token(value));
    }
    shell_quote_token(token)
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn reified_shell_token_path_candidate_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for range in colon_separated_absolute_path_component_ranges(token) {
        push_unique_range(&mut ranges, range);
    }
    for range in path_like_subtoken_ranges(token) {
        push_unique_range(&mut ranges, range);
    }
    for range in embedded_absolute_path_literal_ranges(token) {
        push_path_literal_candidate_ranges(token, range, &mut ranges);
    }
    ranges
}

fn push_path_literal_candidate_ranges(
    token: &str,
    range: Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    if let Some(split_end) = first_later_absolute_path_split(token, &range) {
        let first_operand = trim_trailing_whitespace_range(token, range.start..split_end);
        if first_operand.start < first_operand.end {
            if path_literal_range_contains_flag_segment(token, &first_operand) {
                push_whitespace_prefix_ranges(token, first_operand.clone(), ranges);
                push_unique_range(ranges, first_operand);
            } else {
                push_unique_range(ranges, first_operand.clone());
                push_whitespace_prefix_ranges(token, first_operand, ranges);
            }
        }
        return;
    }

    let trimmed = trim_trailing_whitespace_range(token, range.clone());
    if trimmed.start < trimmed.end {
        push_unique_range(ranges, trimmed);
    }

    let literal = &token[range.clone()];
    for (offset, ch) in literal.char_indices() {
        if ch.is_whitespace() && offset > 0 {
            let prefix = range.start..range.start + offset;
            push_unique_range(ranges, prefix);
        }
    }
}

fn push_whitespace_prefix_ranges(token: &str, range: Range<usize>, ranges: &mut Vec<Range<usize>>) {
    let literal = &token[range.clone()];
    for (offset, ch) in literal.char_indices() {
        if ch.is_whitespace() && offset > 0 {
            push_unique_range(ranges, range.start..range.start + offset);
        }
    }
}

fn path_literal_range_contains_flag_segment(token: &str, range: &Range<usize>) -> bool {
    token[range.clone()]
        .split_whitespace()
        .skip(1)
        .any(|segment| segment.starts_with('-'))
}

fn first_later_absolute_path_split(token: &str, range: &Range<usize>) -> Option<usize> {
    let literal = &token[range.clone()];
    let mut whitespace_start = None;
    let mut in_whitespace = false;
    for (offset, ch) in literal.char_indices().skip(1) {
        if ch.is_whitespace() {
            if !in_whitespace {
                whitespace_start = Some(offset);
                in_whitespace = true;
            }
            continue;
        }

        if ch == '/' && !absolute_path_match_has_path_prefix(literal, offset) {
            return whitespace_start.map(|split| range.start + split);
        }

        whitespace_start = None;
        in_whitespace = false;
    }
    None
}

fn trim_trailing_whitespace_range(token: &str, range: Range<usize>) -> Range<usize> {
    let trimmed = token[range.clone()].trim_end_matches(char::is_whitespace);
    range.start..range.start + trimmed.len()
}

fn push_unique_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if !ranges.contains(&range) {
        ranges.push(range);
    }
}

fn path_like_subtoken_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if looks_like_path_token(token) || looks_like_bare_protected_subpath_token(token) {
        ranges.push(0..token.len());
    }
    if let Some(index) = token.rfind('=') {
        let start = index + 1;
        if start < token.len() {
            ranges.push(start..token.len());
        }
    }
    if let Some(range) = short_option_attached_path_subtoken_range(token)
        && !ranges.contains(&range)
    {
        ranges.push(range);
    }
    ranges
}

fn colon_separated_absolute_path_component_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if token.starts_with('/') {
        push_colon_separated_absolute_path_components(token, 0..token.len(), &mut ranges);
    }
    if let Some(index) = token.rfind('=') {
        let start = index + 1;
        if start < token.len() {
            push_colon_separated_absolute_path_components(token, start..token.len(), &mut ranges);
        }
    }
    ranges
}

fn push_colon_separated_absolute_path_components(
    token: &str,
    range: Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    let value = &token[range.clone()];
    let mut component_start = range.start;
    for (offset, ch) in value.char_indices() {
        if ch != ':' {
            continue;
        }
        push_absolute_path_component_range(token, component_start..range.start + offset, ranges);
        component_start = range.start + offset + ch.len_utf8();
    }
    push_absolute_path_component_range(token, component_start..range.end, ranges);
}

fn push_absolute_path_component_range(
    token: &str,
    range: Range<usize>,
    ranges: &mut Vec<Range<usize>>,
) {
    if range.start >= range.end {
        return;
    }
    if token[range.clone()].starts_with('/') {
        push_unique_range(ranges, range);
    }
}

fn absolute_path_literal_candidates(token: &str) -> Vec<Vec<String>> {
    let mut literals = Vec::new();
    for range in colon_separated_absolute_path_component_ranges(token) {
        push_absolute_path_literal_candidates(token, range, &mut literals);
    }
    for range in path_like_subtoken_ranges(token) {
        push_absolute_path_literal_candidates(token, range, &mut literals);
    }

    for range in embedded_absolute_path_literal_ranges(token) {
        push_absolute_path_literal_candidates(token, range, &mut literals);
    }

    literals
}

fn push_absolute_path_literal_candidates(
    token: &str,
    range: Range<usize>,
    literals: &mut Vec<Vec<String>>,
) {
    let literal = &token[range];
    if !Path::new(literal).is_absolute() {
        return;
    }

    let mut candidates = vec![literal.to_string()];
    for (offset, ch) in literal.char_indices() {
        if ch.is_whitespace() && offset > 0 {
            let prefix = literal[..offset].to_string();
            if !candidates.contains(&prefix) {
                candidates.push(prefix);
            }
        }
    }
    if !literals.iter().any(|existing| existing == &candidates) {
        literals.push(candidates);
    }
}

fn embedded_absolute_path_literal_ranges(token: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let indices = token.char_indices().collect::<Vec<_>>();
    for (position, &(start, ch)) in indices.iter().enumerate() {
        if ch != '/' || absolute_path_match_has_path_prefix(token, start) {
            continue;
        }

        let mut end = token.len();
        for &(index, next) in &indices[position + 1..] {
            if is_absolute_path_literal_terminator(next) {
                end = index;
                break;
            }
        }
        ranges.push(start..end);
    }
    ranges
}

fn absolute_path_match_has_path_prefix(text: &str, start: usize) -> bool {
    if start == 0 {
        return false;
    }
    let prev = text.as_bytes()[start - 1];
    prev == b':'
        || prev == b'.'
        || prev == b'/'
        || prev == b'_'
        || prev == b'-'
        || prev == b'*'
        || prev == b'?'
        || prev == b']'
        || prev.is_ascii_alphanumeric()
}

fn is_absolute_path_literal_terminator(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '|' | '&' | ';' | ','
    )
}

fn shell_quote_token(token: &str) -> String {
    if !token.is_empty()
        && token.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '@' | '%' | '_' | '+' | '=' | ':' | ',' | '.' | '/' | '-'
                )
        })
    {
        return token.to_string();
    }

    let mut quoted = String::from("'");
    for ch in token.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
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

fn validate_direct_command_shapes(commands: &[Vec<String>], backend_name: &str) -> Result<()> {
    for words in commands {
        let Some(command_index) = words.iter().position(|word| !is_env_assignment(word)) else {
            continue;
        };

        let command_word = words[command_index].as_str();
        if is_shell_control_prefix(command_word) {
            return Err(anyhow!(
                "Sandbox backend {} rejects shell control flow like {} because host_mount_path_guard only supports direct commands with statically checkable paths",
                backend_name,
                command_word
            ));
        }

        let command = command_basename(command_word);
        if is_unsupported_shell_wrapper(command) {
            return Err(anyhow!(
                "Sandbox backend {} rejects shell wrappers like {} because host_mount_path_guard only supports direct commands with statically checkable paths",
                backend_name,
                command
            ));
        }
    }

    Ok(())
}

/// Extract the inline script of a shell wrapper command (`sh -c <script>`,
/// `bash -lc <script>`, …) so it can be recursively inspected. Returns `None`
/// for non-wrapper commands or wrappers without an inline script argument.
fn shell_wrapper_inline_script(words: &[String]) -> Option<String> {
    // Peel transparent wrappers (`env VAR=x`, `command`, `timeout 5`, `nice`,
    // `nohup`, `stdbuf`, `setsid`, ...) so the inline script is found even when the
    // shell is not the direct head — e.g. `env bash -lc '...'`. Otherwise the
    // quoted script stays an opaque token and its `.git`/out-of-host_mount paths
    // escape the ProtectedOnly checks.
    let view = nested_evaluator_view(words)?;
    if !matches!(view.command, "sh" | "bash" | "zsh" | "dash" | "ksh") {
        return None;
    }
    // The script follows the first short-flag cluster containing `c` (e.g. `-c`,
    // `-lc`, `-ic`).
    let mut index = 0;
    while index < view.args.len() {
        let word = &view.args[index];
        if word.starts_with('-') && !word.starts_with("--") && word.contains('c') {
            return view.args.get(index + 1).cloned();
        }
        index += 1;
    }
    None
}

fn validate_shell_features(cmd: &str, backend_name: &str) -> Result<()> {
    let normalized = normalize_shell_line_continuations(cmd);
    let comment_free = strip_shell_comments(&normalized);
    if contains_shell_expansion(&comment_free)
        || contains_shell_brace_expansion(&comment_free)
        || contains_shell_globbing(&comment_free)
    {
        return Err(anyhow!(
            "Sandbox backend {} rejects shell variable, command, brace, or glob expansion because path references cannot be validated safely",
            backend_name
        ));
    }
    Ok(())
}

fn looks_like_path_token(token: &str) -> bool {
    token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token == "."
        || token == ".."
        || token.contains('/')
}

fn looks_like_bare_protected_subpath_token(token: &str) -> bool {
    PROTECTED_SUBPATHS
        .iter()
        .copied()
        .any(|protected| token.trim_end_matches('/') == protected)
}

fn path_like_subtokens(token: &str) -> Vec<&str> {
    let mut candidates = vec![token];
    if let Some((_, rhs)) = token.rsplit_once('=')
        && !rhs.is_empty()
    {
        candidates.push(rhs);
    }
    if let Some(attached) = short_option_attached_path_subtoken(token)
        && !candidates.contains(&attached)
    {
        candidates.push(attached);
    }
    candidates
}

fn short_option_attached_path_subtoken(token: &str) -> Option<&str> {
    let range = short_option_attached_path_subtoken_range(token)?;
    Some(&token[range])
}

fn short_option_attached_path_subtoken_range(token: &str) -> Option<Range<usize>> {
    if token.starts_with("--") {
        return None;
    }
    let rest = token.strip_prefix('-')?;
    if rest.len() < 2 {
        return None;
    }

    rest.char_indices().skip(1).find_map(|(index, _)| {
        let candidate = &rest[index..];
        if candidate.starts_with('~')
            || looks_like_path_token(candidate)
            || looks_like_bare_protected_subpath_token(candidate)
        {
            Some((index + 1)..token.len())
        } else {
            None
        }
    })
}

fn is_file_redirection_operator(token: &str) -> bool {
    matches!(token, "<" | ">" | ">>" | "<>" | ">|")
}

fn is_allowed_absolute_command_path(path: &Path) -> bool {
    matches!(
        path.to_str(),
        Some("/dev/null" | "/dev/stdin" | "/dev/stdout" | "/dev/stderr")
    )
}

fn is_path_guard_reason(reason: &str) -> bool {
    reason.contains("outside host_mount")
}

#[cfg(unix)]
fn existing_regular_file_has_multiple_links(path: &Path) -> Result<bool> {
    use std::io::ErrorKind;
    use std::os::unix::fs::MetadataExt;

    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(anyhow!(
                "Failed to inspect path link count for {}: {}",
                path.display(),
                error
            ));
        }
    };

    Ok(metadata.is_file() && metadata.nlink() > 1)
}

#[cfg(not(unix))]
fn existing_regular_file_has_multiple_links(_path: &Path) -> Result<bool> {
    Ok(false)
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn contains_shell_expansion(command: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            match ch {
                '\\' => escaped = true,
                '"' => in_double = false,
                '$' | '`' => return true,
                _ => {}
            }
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\'' => in_single = true,
            '"' => in_double = true,
            '$' | '`' => return true,
            _ => {}
        }
    }

    false
}

fn contains_shell_brace_expansion(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for (index, ch) in chars.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            match ch {
                '\\' => escaped = true,
                '"' => in_double = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\'' => in_single = true,
            '"' => in_double = true,
            '{' | '}' if is_brace_expansion_position(&chars, index) => return true,
            _ => {}
        }
    }

    false
}

fn contains_shell_globbing(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for (index, ch) in chars.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }

        if in_double {
            match ch {
                '\\' => escaped = true,
                '"' => in_double = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '\'' => in_single = true,
            '"' => in_double = true,
            '*' | '?' => return true,
            '[' if !is_test_bracket_token(&chars, index) => return true,
            _ => {}
        }
    }

    false
}

fn is_test_bracket_token(chars: &[char], index: usize) -> bool {
    let mut end = index;
    while let Some(ch) = chars.get(end) {
        if ch.is_whitespace() || is_shell_separator(*ch) {
            break;
        }
        end += 1;
    }

    match end.saturating_sub(index) {
        1 => chars[index] == '[',
        2 => chars[index] == '[' && chars.get(index + 1).copied() == Some('['),
        _ => false,
    }
}

fn is_brace_expansion_position(chars: &[char], index: usize) -> bool {
    let prev = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
    let next = chars.get(index + 1).copied();
    brace_neighbor_requires_expansion(prev) || brace_neighbor_requires_expansion(next)
}

fn brace_neighbor_requires_expansion(ch: Option<char>) -> bool {
    matches!(ch, Some(value) if !value.is_whitespace() && !is_shell_separator(value))
}

fn is_shell_separator(ch: char) -> bool {
    matches!(ch, ';' | '|' | '&' | '(' | ')' | '<' | '>')
}

fn is_shell_word_boundary(ch: char) -> bool {
    ch.is_whitespace() || is_shell_separator(ch) || matches!(ch, '{' | '}')
}

fn normalize_shell_line_continuations(command: &str) -> String {
    let mut normalized = String::with_capacity(command.len());
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            normalized.push(ch);
            if matches!(ch, '\n' | '\r') {
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            normalized.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            normalized.push(ch);
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if consume_shell_line_continuation(&mut chars) {
                        continue;
                    }
                    normalized.push(ch);
                    escaped = true;
                }
                '"' => {
                    in_double = false;
                    normalized.push(ch);
                    word_started = true;
                }
                _ => {
                    normalized.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                if consume_shell_line_continuation(&mut chars) {
                    continue;
                }
                normalized.push(ch);
                escaped = true;
                word_started = true;
            }
            '\'' => {
                in_single = true;
                normalized.push(ch);
                word_started = true;
            }
            '"' => {
                in_double = true;
                normalized.push(ch);
                word_started = true;
            }
            '#' if !word_started => {
                in_comment = true;
                normalized.push(ch);
            }
            c if is_shell_word_boundary(c) => {
                normalized.push(c);
                word_started = false;
            }
            _ => {
                normalized.push(ch);
                word_started = true;
            }
        }
    }

    normalized
}

fn strip_shell_comments(command: &str) -> String {
    let mut stripped = String::with_capacity(command.len());
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    for ch in command.chars() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
                stripped.push(ch);
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            stripped.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            stripped.push(ch);
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    stripped.push(ch);
                    escaped = true;
                }
                '"' => {
                    in_double = false;
                    stripped.push(ch);
                    word_started = true;
                }
                _ => {
                    stripped.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                stripped.push(ch);
                escaped = true;
                word_started = true;
            }
            '\'' => {
                in_single = true;
                stripped.push(ch);
                word_started = true;
            }
            '"' => {
                in_double = true;
                stripped.push(ch);
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            c if is_shell_word_boundary(c) => {
                stripped.push(c);
                word_started = false;
            }
            _ => {
                stripped.push(ch);
                word_started = true;
            }
        }
    }

    stripped
}

fn consume_shell_line_continuation<I>(chars: &mut std::iter::Peekable<I>) -> bool
where
    I: Iterator<Item = char>,
{
    match chars.peek().copied() {
        Some('\n') => {
            chars.next();
            true
        }
        Some('\r') => {
            chars.next();
            if matches!(chars.peek(), Some('\n')) {
                chars.next();
            }
            true
        }
        _ => false,
    }
}

fn shell_word_tokens_with_spans(command: &str) -> Result<Vec<ShellWordToken>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = command.char_indices().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;
    let mut raw_start = None;

    while let Some((index, ch)) = chars.next() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            current.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
            }
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if let Some((_, next)) = chars.next() {
                        current.push(next);
                        word_started = true;
                    } else {
                        return Err(anyhow!("Command ends with an incomplete escape sequence"));
                    }
                }
                '"' => {
                    in_double = false;
                    word_started = true;
                }
                _ => {
                    current.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                raw_start.get_or_insert(index);
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                    word_started = true;
                } else {
                    return Err(anyhow!("Command ends with an incomplete escape sequence"));
                }
            }
            '\'' => {
                raw_start.get_or_insert(index);
                in_single = true;
                word_started = true;
            }
            '"' => {
                raw_start.get_or_insert(index);
                in_double = true;
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            c if c.is_whitespace() => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);
                word_started = false;
            }
            ';' | '(' | ')' | '{' | '}' => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);
                word_started = false;
            }
            '&' | '|' => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);

                if matches!(chars.peek(), Some((_, next)) if *next == ch) {
                    chars.next();
                }
                word_started = false;
            }
            '<' | '>' => {
                push_shell_word_token(&mut tokens, &mut current, &mut raw_start, index);

                match (ch, chars.peek().copied()) {
                    ('<', Some((_, '<' | '>' | '&'))) | ('>', Some((_, '>' | '&' | '|'))) => {
                        chars.next();
                        if ch == '<' && matches!(chars.peek(), Some((_, '-'))) {
                            chars.next();
                        }
                    }
                    _ => {}
                }
                word_started = false;
            }
            _ => {
                raw_start.get_or_insert(index);
                current.push(ch);
                word_started = true;
            }
        }
    }

    if escaped {
        return Err(anyhow!("Command ends with an incomplete escape sequence"));
    }
    if in_single || in_double {
        return Err(anyhow!("Command contains an unterminated quoted string"));
    }
    push_shell_word_token(&mut tokens, &mut current, &mut raw_start, command.len());

    Ok(tokens)
}

fn push_shell_word_token(
    tokens: &mut Vec<ShellWordToken>,
    current: &mut String,
    raw_start: &mut Option<usize>,
    raw_end: usize,
) {
    let Some(start) = raw_start.take() else {
        return;
    };
    tokens.push(ShellWordToken {
        decoded: std::mem::take(current),
        raw_start: start,
        raw_end,
    });
}

fn shell_word_tokens(command: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            current.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
            }
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                        word_started = true;
                    } else {
                        return Err(anyhow!("Command ends with an incomplete escape sequence"));
                    }
                }
                '"' => {
                    in_double = false;
                    word_started = true;
                }
                _ => {
                    current.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    word_started = true;
                } else {
                    return Err(anyhow!("Command ends with an incomplete escape sequence"));
                }
            }
            '\'' => {
                in_single = true;
                word_started = true;
            }
            '"' => {
                in_double = true;
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                word_started = false;
            }
            ';' | '(' | ')' | '{' | '}' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                word_started = false;
            }
            '&' | '|' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }

                if matches!(chars.peek(), Some(next) if *next == ch) {
                    chars.next();
                }
                word_started = false;
            }
            '<' | '>' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }

                let mut operator = String::new();
                operator.push(ch);
                match (ch, chars.peek().copied()) {
                    ('<', Some('<')) => {
                        operator.push('<');
                        chars.next();
                        if matches!(chars.peek(), Some('-')) {
                            operator.push('-');
                            chars.next();
                        }
                    }
                    ('<', Some('>')) => {
                        operator.push('>');
                        chars.next();
                    }
                    ('<', Some('&')) => {
                        operator.push('&');
                        chars.next();
                    }
                    ('>', Some('>')) => {
                        operator.push('>');
                        chars.next();
                    }
                    ('>', Some('&')) => {
                        operator.push('&');
                        chars.next();
                    }
                    ('>', Some('|')) => {
                        operator.push('|');
                        chars.next();
                    }
                    _ => {}
                }
                tokens.push(operator);
                word_started = false;
            }
            _ => {
                current.push(ch);
                word_started = true;
            }
        }
    }

    if escaped {
        return Err(anyhow!("Command ends with an incomplete escape sequence"));
    }
    if in_single || in_double {
        return Err(anyhow!("Command contains an unterminated quoted string"));
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

fn shell_commands(command: &str) -> Result<Vec<Vec<String>>> {
    let mut commands = Vec::new();
    let mut current_command = Vec::new();
    let mut current_word = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_comment = false;
    let mut escaped = false;
    let mut word_started = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            if matches!(ch, '\n' | '\r') {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                if !current_command.is_empty() {
                    commands.push(std::mem::take(&mut current_command));
                }
                in_comment = false;
                word_started = false;
            }
            continue;
        }

        if escaped {
            current_word.push(ch);
            escaped = false;
            word_started = true;
            continue;
        }

        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current_word.push(ch);
            }
            word_started = true;
            continue;
        }

        if in_double {
            match ch {
                '\\' => {
                    if let Some(next) = chars.next() {
                        current_word.push(next);
                        word_started = true;
                    } else {
                        return Err(anyhow!("Command ends with an incomplete escape sequence"));
                    }
                }
                '"' => {
                    in_double = false;
                    word_started = true;
                }
                _ => {
                    current_word.push(ch);
                    word_started = true;
                }
            }
            continue;
        }

        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current_word.push(next);
                    word_started = true;
                } else {
                    return Err(anyhow!("Command ends with an incomplete escape sequence"));
                }
            }
            '\'' => {
                in_single = true;
                word_started = true;
            }
            '"' => {
                in_double = true;
                word_started = true;
            }
            '#' if !word_started => in_comment = true,
            '\n' | '\r' => {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                if !current_command.is_empty() {
                    commands.push(std::mem::take(&mut current_command));
                }
                word_started = false;
            }
            c if c.is_whitespace() => {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                word_started = false;
            }
            ';' | '|' | '&' | '(' | ')' | '{' | '}' => {
                if !current_word.is_empty() {
                    current_command.push(std::mem::take(&mut current_word));
                }
                if !current_command.is_empty() {
                    commands.push(std::mem::take(&mut current_command));
                }
                if matches!(chars.peek(), Some(next) if *next == ch && matches!(ch, '|' | '&')) {
                    chars.next();
                }
                word_started = false;
            }
            _ => {
                current_word.push(ch);
                word_started = true;
            }
        }
    }

    if escaped {
        return Err(anyhow!("Command ends with an incomplete escape sequence"));
    }
    if in_single || in_double {
        return Err(anyhow!("Command contains an unterminated quoted string"));
    }
    if !current_word.is_empty() {
        current_command.push(current_word);
    }
    if !current_command.is_empty() {
        commands.push(current_command);
    }

    Ok(commands)
}

fn command_basename(command: &str) -> &str {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
}

struct NestedEvaluatorView<'a> {
    display: String,
    command: &'a str,
    args: &'a [String],
    opaque_wrapper_display: Option<String>,
}

fn nested_evaluator_view(words: &[String]) -> Option<NestedEvaluatorView<'_>> {
    let mut command_index = next_command_offset(words)?;
    let mut display = command_basename(&words[command_index]).to_string();

    loop {
        let command = command_basename(&words[command_index]);
        let args = &words[command_index + 1..];
        let next_offset = if command == "env" {
            if let Some(flag) = env_split_string_flag(args) {
                return Some(NestedEvaluatorView {
                    display: display.clone(),
                    command,
                    args,
                    opaque_wrapper_display: Some(format!("{display} {flag}")),
                });
            }
            env_command_offset(args)
        } else if is_transparent_command_wrapper(command) {
            transparent_wrapper_offset(command, args)
        } else {
            None
        };

        let Some(next_relative_offset) = next_offset else {
            return Some(NestedEvaluatorView {
                display,
                command,
                args,
                opaque_wrapper_display: None,
            });
        };

        command_index += 1 + next_relative_offset;
        display.push(' ');
        display.push_str(command_basename(&words[command_index]));
    }
}

fn next_command_offset(words: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(word) = words.get(index).map(|word| word.as_str()) {
        if is_env_assignment(word) || is_shell_control_prefix(word) {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

fn env_command_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            index += 1;
            break;
        }
        if is_env_assignment(arg) {
            index += 1;
            continue;
        }
        match env_option_behavior(arg) {
            Some(
                EnvOptionBehavior::Passthrough
                | EnvOptionBehavior::InlineValue
                | EnvOptionBehavior::SplitStringInlineValue,
            ) => {
                index += 1;
                continue;
            }
            Some(EnvOptionBehavior::TakesNextArg | EnvOptionBehavior::SplitStringNextArg) => {
                index += 2;
                continue;
            }
            None => {}
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn transparent_wrapper_offset(command: &str, args: &[String]) -> Option<usize> {
    match command {
        "command" => command_wrapper_offset(args),
        "exec" => exec_wrapper_offset(args),
        "builtin" => builtin_wrapper_offset(args),
        "nice" => nice_wrapper_offset(args),
        "nohup" => nohup_wrapper_offset(args),
        "timeout" => timeout_wrapper_offset(args),
        "stdbuf" => stdbuf_wrapper_offset(args),
        "setsid" => setsid_wrapper_offset(args),
        _ => None,
    }
}

fn command_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            index += 1;
            break;
        }
        if command_wrapper_is_query_flag(arg) {
            return None;
        }
        if command_wrapper_is_exec_flag(arg) {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn builtin_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    if let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            index += 1;
        } else if builtin_query_flag(arg) {
            return None;
        }
    }

    args.get(index)?;
    Some(index)
}

fn exec_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            index += 1;
            break;
        }
        if arg == "-a" {
            index += 2;
            continue;
        }
        if has_inline_exec_argv0(arg) || is_exec_wrapper_flag(arg) {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn nice_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if exact_or_inline_option_with_value(arg, &["-n"], &["--adjustment"]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn nohup_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    if let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
        }
    }

    args.get(index)?;
    Some(index)
}

fn timeout_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if exact_or_inline_option_with_value(arg, &["-k", "-s"], &["--kill-after", "--signal"]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if matches!(
            arg,
            "-v" | "--verbose" | "--foreground" | "--preserve-status"
        ) {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    index += 1;
    args.get(index)?;
    Some(index)
}

fn stdbuf_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if exact_or_inline_option_with_value(arg, &["-i", "-o", "-e"], &[]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn setsid_wrapper_offset(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if matches!(arg, "-h" | "-V") || common_wrapper_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            index += 1;
            break;
        }
        if matches!(arg, "-c" | "-f" | "-w") {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        break;
    }

    args.get(index)?;
    Some(index)
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_shell_eval_builtin(command: &str) -> bool {
    matches!(command, "eval" | "." | "source")
}

fn is_shell_control_prefix(word: &str) -> bool {
    matches!(
        word,
        "!" | "if"
            | "then"
            | "elif"
            | "else"
            | "fi"
            | "for"
            | "while"
            | "until"
            | "do"
            | "done"
            | "case"
            | "esac"
            | "select"
            | "function"
    )
}

fn is_transparent_command_wrapper(command: &str) -> bool {
    matches!(
        command,
        "command" | "builtin" | "exec" | "nice" | "nohup" | "timeout" | "stdbuf" | "setsid"
    )
}

fn is_unsupported_shell_wrapper(command: &str) -> bool {
    matches!(
        command,
        "env"
            | "command"
            | "builtin"
            | "exec"
            | "time"
            | "nice"
            | "nohup"
            | "timeout"
            | "stdbuf"
            | "setsid"
    )
}

fn common_wrapper_query_flag(arg: &str) -> bool {
    matches!(arg, "--help" | "--version")
}

fn env_split_string_flag(args: &[String]) -> Option<&str> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            return None;
        }
        if is_env_assignment(arg) {
            index += 1;
            continue;
        }
        match env_option_behavior(arg) {
            Some(
                EnvOptionBehavior::SplitStringInlineValue | EnvOptionBehavior::SplitStringNextArg,
            ) => return Some(arg),
            Some(EnvOptionBehavior::Passthrough | EnvOptionBehavior::InlineValue) => {
                index += 1;
                continue;
            }
            Some(EnvOptionBehavior::TakesNextArg) => {
                index += 2;
                continue;
            }
            None => {}
        }
        break;
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnvOptionBehavior {
    Passthrough,
    TakesNextArg,
    InlineValue,
    SplitStringNextArg,
    SplitStringInlineValue,
}

fn env_option_behavior(arg: &str) -> Option<EnvOptionBehavior> {
    if matches!(arg, "--ignore-environment" | "--null") {
        return Some(EnvOptionBehavior::Passthrough);
    }
    if arg == "--split-string" {
        return Some(EnvOptionBehavior::SplitStringNextArg);
    }
    if arg.starts_with("--split-string=") {
        return Some(EnvOptionBehavior::SplitStringInlineValue);
    }
    if matches!(arg, "--unset" | "--chdir") {
        return Some(EnvOptionBehavior::TakesNextArg);
    }
    if arg.starts_with("--unset=") || arg.starts_with("--chdir=") {
        return Some(EnvOptionBehavior::InlineValue);
    }
    env_short_option_behavior(arg)
}

fn env_short_option_behavior(arg: &str) -> Option<EnvOptionBehavior> {
    if arg.starts_with("--") {
        return None;
    }
    let rest = arg.strip_prefix('-')?;
    if rest.is_empty() {
        return None;
    }

    let mut saw_passthrough = false;
    for (index, ch) in rest.char_indices() {
        match ch {
            'i' | '0' => saw_passthrough = true,
            'u' | 'C' => {
                return Some(if rest[index + ch.len_utf8()..].is_empty() {
                    EnvOptionBehavior::TakesNextArg
                } else {
                    EnvOptionBehavior::InlineValue
                });
            }
            'S' => {
                return Some(if rest[index + ch.len_utf8()..].is_empty() {
                    EnvOptionBehavior::SplitStringNextArg
                } else {
                    EnvOptionBehavior::SplitStringInlineValue
                });
            }
            _ => return None,
        }
    }

    saw_passthrough.then_some(EnvOptionBehavior::Passthrough)
}

fn command_wrapper_is_exec_flag(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| ch == 'p')
}

fn command_wrapper_is_query_flag(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    !rest.is_empty()
        && rest.chars().all(|ch| matches!(ch, 'p' | 'v' | 'V'))
        && rest.chars().any(|ch| matches!(ch, 'v' | 'V'))
}

fn builtin_query_flag(arg: &str) -> bool {
    arg == "-p"
}

fn is_exec_wrapper_flag(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|ch| matches!(ch, 'c' | 'l'))
}

fn has_inline_exec_argv0(arg: &str) -> bool {
    arg.starts_with("-a") && arg.len() > 2
}

fn opaque_command_dispatcher_display(
    display: &str,
    command: &str,
    args: &[String],
) -> Option<String> {
    if command == "xargs" {
        return Some(display.to_string());
    }
    (command == "find")
        .then_some(())
        .and_then(|()| find_dispatch_clause(args))
        .map(|clause| format!("{display} {clause}"))
}

fn find_dispatch_clause(args: &[String]) -> Option<&'static str> {
    const FIND_DISPATCH_FLAGS: [&str; 4] = ["-exec", "-execdir", "-ok", "-okdir"];

    args.iter().enumerate().find_map(|(index, arg)| {
        let flag = FIND_DISPATCH_FLAGS
            .iter()
            .copied()
            .find(|flag| *flag == arg)?;
        let tail = &args[index + 1..];
        let first_child_arg = tail.first()?;
        if first_child_arg.starts_with('-') {
            return None;
        }
        tail.iter()
            .any(|candidate| candidate == ";" || candidate == "+")
            .then_some(flag)
    })
}

fn opaque_script_interpreter_display(
    display: &str,
    command: &str,
    args: &[String],
) -> Option<String> {
    match command {
        "sh" | "bash" | "dash" | "zsh" | "ksh" => shell_script_interpreter_display(display, args),
        "python" | "python3" => python_script_interpreter_display(display, args),
        "node" => node_script_interpreter_display(display, args),
        "perl" => perl_script_interpreter_display(display, args),
        "ruby" => ruby_script_interpreter_display(display, args),
        "lua" => lua_script_interpreter_display(display, args),
        "php" => php_script_interpreter_display(display, args),
        "awk" | "gawk" | "mawk" | "nawk" => awk_script_interpreter_display(display, args),
        _ => None,
    }
}

fn shell_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if shell_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-s" {
            return Some(format!("{display} -s"));
        }
        if let Some(step) = shell_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn python_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if python_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if matches!(arg, "-m" | "--module") {
            let module = args.get(index + 1).map(|value| value.as_str());
            if module.is_some_and(is_safe_python_module_runner) {
                return None;
            }
            return Some(format!("{display} {arg}"));
        }
        if arg == "-" {
            return Some(format!("{display} {arg}"));
        }
        if let Some(step) = python_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn is_safe_python_module_runner(module: &str) -> bool {
    matches!(module, "pytest" | "unittest")
}

fn node_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if node_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = node_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn perl_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if perl_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = perl_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn ruby_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if ruby_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = ruby_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn lua_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if lua_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = lua_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn php_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if php_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if matches!(arg, "-B" | "-E" | "-R" | "-F" | "-") {
            return Some(format!("{display} {arg}"));
        }
        if exact_or_inline_option_with_value(arg, &["-f"], &["--file"]) {
            return Some(format!("{display} -f"));
        }
        if let Some(step) = php_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn awk_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if awk_query_flag(arg) {
            return None;
        }
        if arg == "-W" {
            if matches!(
                args.get(index + 1).map(|value| value.as_str()),
                Some("version" | "help")
            ) {
                return None;
            }
            index += 2;
            continue;
        }
        if arg == "--" {
            return args.get(index + 1).map(|_| format!("{display} program"));
        }
        if exact_or_inline_option_with_value(arg, &["-f"], &["--file"]) {
            return Some(format!("{display} -f"));
        }
        if exact_or_inline_option_with_value(arg, &["-F", "-v", "-W"], &[]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(format!("{display} program"));
    }
    None
}

fn shell_query_flag(arg: &str) -> bool {
    matches!(arg, "--help" | "--version")
}

fn python_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "--version") || arg.starts_with("-V")
}

fn node_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version")
}

fn perl_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help") || arg.starts_with("-v") || arg.starts_with("-V")
}

fn ruby_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version")
}

fn lua_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version")
}

fn php_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version" | "-i" | "-m")
}

fn awk_query_flag(arg: &str) -> bool {
    matches!(arg, "--help" | "--version" | "-Wversion" | "-Whelp")
}

fn is_shell_eval_wrapper(command: &str, flag: &str) -> bool {
    matches!(command, "sh" | "bash" | "dash" | "zsh" | "ksh")
        && shell_flag_contains_short_option(flag, 'c')
}

fn is_code_eval_wrapper(command: &str, flag: &str) -> bool {
    match command {
        "python" | "python3" => shell_flag_contains_short_option(flag, 'c'),
        "node" => {
            shell_flag_contains_short_option(flag, 'e')
                || shell_flag_contains_short_option(flag, 'p')
                || flag == "--print"
        }
        "perl" => {
            shell_flag_contains_short_option(flag, 'e')
                || shell_flag_contains_short_option(flag, 'E')
        }
        "ruby" | "lua" => shell_flag_contains_short_option(flag, 'e'),
        "php" => shell_flag_contains_short_option(flag, 'r'),
        _ => false,
    }
}

fn leading_eval_flag<'a>(command: &str, args: &'a [String]) -> Option<&'a str> {
    match command {
        "sh" | "bash" | "dash" | "zsh" | "ksh" => scan_leading_args(
            args,
            |arg| is_shell_eval_wrapper("sh", arg),
            shell_wrapper_advance,
        ),
        "python" | "python3" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("python3", arg),
            python_wrapper_advance,
        ),
        "node" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("node", arg),
            node_wrapper_advance,
        ),
        "perl" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("perl", arg),
            perl_wrapper_advance,
        ),
        "ruby" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("ruby", arg),
            ruby_wrapper_advance,
        ),
        "lua" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("lua", arg),
            lua_wrapper_advance,
        ),
        "php" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("php", arg),
            php_wrapper_advance,
        ),
        _ => None,
    }
}

fn scan_leading_args<F, G>(args: &[String], matches_eval: F, advance: G) -> Option<&str>
where
    F: Fn(&str) -> bool,
    G: Fn(&str) -> Option<usize>,
{
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            break;
        }
        if matches_eval(arg) {
            return Some(arg);
        }
        index += advance(arg)?;
    }
    None
}

fn shell_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-o", "+o", "-O", "+O"],
        &["--rcfile", "--init-file"],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') || arg.starts_with('+') {
        Some(1)
    } else {
        None
    }
}

fn python_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-W", "-X"], &["--check-hash-based-pycs"]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if matches!(arg, "-m" | "--module" | "-") {
        None
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn node_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-r", "-C"],
        &[
            "--require",
            "--loader",
            "--experimental-loader",
            "--import",
            "--watch-path",
            "--conditions",
            "--input-type",
            "--inspect",
            "--inspect-brk",
            "--inspect-port",
            "--openssl-config",
            "--redirect-warnings",
            "--trace-event-categories",
            "--trace-event-file-pattern",
            "--diagnostic-dir",
            "--icu-data-dir",
            "--title",
        ],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn perl_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-I", "-M", "-m"], &[]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn ruby_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-C", "-E", "-F", "-I", "-r"],
        &["--enable", "--disable", "--encoding"],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn lua_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-l"], &[]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn php_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-c", "-d", "-z"], &["--define"]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if matches!(arg, "-f" | "--file") {
        None
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn exact_or_inline_option_with_value(arg: &str, short: &[&str], long: &[&str]) -> bool {
    short
        .iter()
        .any(|flag| arg == *flag || arg.starts_with(flag))
        || long
            .iter()
            .any(|flag| arg == *flag || arg.starts_with(&format!("{flag}=")))
}

fn has_attached_option_value(arg: &str) -> bool {
    arg.contains('=') || (arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2)
}

fn shell_flag_contains_short_option(flag: &str, option: char) -> bool {
    if let Some(rest) = flag
        .strip_prefix("--")
        .map(|rest| rest.split_once('=').map_or(rest, |(name, _)| name))
    {
        return matches!(
            (rest, option),
            ("command", 'c') | ("eval", 'e') | ("print", 'p') | ("run", 'r')
        );
    }

    flag.starts_with('-') && flag.chars().skip(1).any(|ch| ch == option)
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
