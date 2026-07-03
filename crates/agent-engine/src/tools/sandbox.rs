//! Simple workspace-only sandbox.
//!
//! This sandbox only enforces that all operations happen within
//! the workspace directory. No OS-level sandboxing (Landlock/Seatbelt).
//! Shell enforcement is intentionally limited to direct shell syntax, explicit
//! path-like argv references, redirection targets, and a curated set of common
//! direct interpreters. It does not infer utility-specific operand roles for
//! arbitrary bare tokens, and it does not inspect arbitrary program-internal
//! writes or dispatch, such as commands that mutate program-private state
//! without an explicit path operand (`git init`, `git add`, `git config
//! --local`), utility actions like `find -delete`, build or task runner
//! recipes, or utility-specific script/DSL modes such as `sed -f`.

use anyhow::{Result, anyhow};
use regex::Regex;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const SANDBOX_BACKEND_WORKSPACE_PATH_GUARD: &str = "workspace_path_guard";
pub(crate) const PROTECTED_SUBPATHS: [&str; 3] = [".git", ".alan", ".agents"];

/// How thoroughly to validate command paths. Under an OS sandbox the kernel
/// enforces workspace containment, so only protected-subpath writes need the
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
    const fn allows_network(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Projected OS-sandbox confinement input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxSpec {
    pub writable_roots: Vec<PathBuf>,
    pub read_denylist: Vec<PathBuf>,
    pub network: NetworkPosture,
}

impl SandboxSpec {
    /// Build the current single-workspace seed spec.
    pub fn seed(workspace_root: PathBuf) -> Self {
        Self {
            writable_roots: vec![workspace_root],
            read_denylist: Vec::new(),
            network: NetworkPosture::Deny,
        }
    }
}

/// Simple workspace-only sandbox
#[derive(Clone)]
pub struct Sandbox {
    spec: SandboxSpec,
    /// Forces a specific backend instead of host detection (tests only).
    backend_override: Option<super::sandbox_backend::SandboxBackendKind>,
}

impl Sandbox {
    /// Create a new sandbox restricted to the given workspace
    pub fn new(workspace_root: PathBuf) -> Self {
        Self::from_spec(SandboxSpec::seed(workspace_root))
    }

    /// Create a new sandbox from a projected confinement spec.
    pub fn from_spec(spec: SandboxSpec) -> Self {
        assert!(
            !spec.writable_roots.is_empty(),
            "SandboxSpec requires at least one writable root"
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
        workspace_root: PathBuf,
        backend: super::sandbox_backend::SandboxBackendKind,
    ) -> Self {
        Self::from_spec_with_backend(SandboxSpec::seed(workspace_root), backend)
    }

    /// Construct a spec-based sandbox pinned to a specific backend (tests only).
    #[cfg(test)]
    pub fn from_spec_with_backend(
        spec: SandboxSpec,
        backend: super::sandbox_backend::SandboxBackendKind,
    ) -> Self {
        assert!(
            !spec.writable_roots.is_empty(),
            "SandboxSpec requires at least one writable root"
        );
        Self {
            spec,
            backend_override: Some(backend),
        }
    }

    fn workspace_root(&self) -> &Path {
        &self.spec.writable_roots[0]
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

    /// Name of the built-in workspace path guard backend.
    pub const fn backend_name_static() -> &'static str {
        SANDBOX_BACKEND_WORKSPACE_PATH_GUARD
    }

    /// Return a stable rejection reason when a bash command shape is incompatible
    /// with the workspace path guard backend.
    pub fn bash_preflight_reason(cmd: &str) -> Option<String> {
        validate_bash_command_shape(cmd)
            .err()
            .map(|err| err.to_string())
    }

    /// Return a stable routing reason when a bash command targets local paths
    /// outside the current workspace.
    pub fn bash_workspace_routing_reason(&self, cmd: &str, cwd: &Path) -> Option<String> {
        if !self.is_in_workspace(cwd) {
            return Some(format!(
                "Working directory outside workspace: {} (workspace: {})",
                cwd.display(),
                self.workspace_root().display()
            ));
        }

        match self.validate_command_paths(cmd, cwd, PathCheckMode::Full) {
            Ok(()) => None,
            Err(err) => {
                let reason = err.to_string();
                is_workspace_routing_reason(&reason).then_some(reason)
            }
        }
    }

    /// Check if a path is within the workspace seed root or another writable root.
    pub fn is_in_workspace(&self, path: &Path) -> bool {
        // Try to get absolute path
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root().join(path)
        };

        // For existing paths, use canonical path
        if absolute_path.exists() {
            let canonical_path = self
                .canonicalize(&absolute_path)
                .unwrap_or_else(|_| dunce::simplified(&absolute_path).to_path_buf());
            return self.is_under_writable_root(&canonical_path);
        }

        // For new files, check that existing parent directories are within an
        // allowed writable root.
        let mut current = absolute_path.parent();
        while let Some(parent) = current {
            if parent.exists() {
                let canonical_parent = self
                    .canonicalize(parent)
                    .unwrap_or_else(|_| dunce::simplified(parent).to_path_buf());
                return self.is_under_writable_root(&canonical_parent);
            }
            current = parent.parent();
        }

        // If no parent exists, check if the path itself starts with a writable root.
        self.is_under_writable_root(&lexically_normalize_path(&absolute_path))
    }

    /// Read a file within the workspace
    pub async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        if !self.is_in_workspace(path) {
            return Err(anyhow!(
                "Path outside workspace: {} (workspace: {})",
                path.display(),
                self.workspace_root().display()
            ));
        }

        tokio::fs::read(path)
            .await
            .map_err(|e| anyhow!("Failed to read file: {}", e))
    }

    /// Read file as string
    pub async fn read_string(&self, path: &Path) -> Result<String> {
        let bytes = self.read(path).await?;
        String::from_utf8(bytes).map_err(|e| anyhow!("Invalid UTF-8: {}", e))
    }

    /// Write a file within the workspace
    pub async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
        if !self.is_in_workspace(path) {
            return Err(anyhow!(
                "Path outside workspace: {} (workspace: {})",
                path.display(),
                self.workspace_root().display()
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

    /// Execute a command within the workspace
    pub async fn exec(&self, cmd: &str, cwd: &Path) -> Result<ExecResult> {
        self.exec_with_timeout_and_capability(cmd, cwd, None, None)
            .await
    }

    /// Execute a command within the workspace with an optional timeout.
    pub async fn exec_with_timeout(
        &self,
        cmd: &str,
        cwd: &Path,
        timeout: Option<Duration>,
    ) -> Result<ExecResult> {
        self.exec_with_timeout_and_capability(cmd, cwd, timeout, None)
            .await
    }

    /// Execute a command within the workspace with path-guard checks.
    pub async fn exec_with_timeout_and_capability(
        &self,
        cmd: &str,
        cwd: &Path,
        timeout: Option<Duration>,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<ExecResult> {
        if !self.is_in_workspace(cwd) {
            return Err(anyhow!(
                "Working directory outside workspace: {} (workspace: {})",
                cwd.display(),
                self.workspace_root().display()
            ));
        }
        self.ensure_path_not_protected(cwd, "process cwd")?;

        // Reject shell expansion ($VAR, $(...), backticks, globs, braces) in EVERY
        // mode. Expansion defeats the static path-containment check — the parser
        // sees a literal, in-workspace-looking token (`$HOME/.ssh/id_rsa`) but
        // `/bin/sh -c` then expands it to escape the workspace. Seatbelt permits
        // reads, so an auto-approved read must not be able to exfiltrate this way.
        self.validate_shell_features(cmd)?;

        if self.active_backend().permits_autonomous_bash() {
            // Seatbelt kernel-confines the workspace fs + network, so the syntactic
            // *shape* checks are dropped — they would reject commands the sandbox
            // safely contains (`bash -lc ...`, `python -c ...`). Path containment
            // and the protected-subpath check (incl. shell-wrapper-nested) still
            // run in ProtectedOnly mode.
            self.validate_command_paths(cmd, cwd, PathCheckMode::ProtectedOnly)?;
        } else {
            // No kernel protected-subpath enforcement (Landlock cannot carve a
            // protected subdir out of the writable tree, or the path-guard
            // fallback): keep the full shape parser so opaque writers — which could
            // hide a protected write the kernel won't deny — are rejected.
            self.validate_command_paths(cmd, cwd, PathCheckMode::Full)?;
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
        let mut command = self.build_confined_command(cmd, allow_network);
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
    /// with a workspace-write/no-network profile; otherwise it runs the shell
    /// directly under the best-effort path guard.
    fn build_confined_command(&self, cmd: &str, allow_network: bool) -> tokio::process::Command {
        // Defense in depth: start the shell with pathname expansion disabled.
        match self.active_backend() {
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
        }
    }

    /// List directory contents
    pub async fn list_dir(&self, path: &Path) -> Result<Vec<tokio::fs::DirEntry>> {
        if !self.is_in_workspace(path) {
            return Err(anyhow!(
                "Path outside workspace: {} (workspace: {})",
                path.display(),
                self.workspace_root().display()
            ));
        }

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

    fn validate_command_paths(&self, cmd: &str, cwd: &Path, mode: PathCheckMode) -> Result<()> {
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
        // apply the same protected-subpath checks (with their carve-outs, e.g.
        // `.alan/memory`) to the wrapped command.
        if protected_only {
            for words in &commands {
                if let Some(inner) = shell_wrapper_inline_script(words) {
                    self.validate_command_paths(&inner, cwd, PathCheckMode::ProtectedOnly)?;
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
                self.validate_command_path_candidate(candidate, cwd)?;
            }
        }

        let comment_free = strip_shell_comments(trimmed);
        let regex = Regex::new(r"/[A-Za-z0-9._/-]+").expect("absolute-path regex is valid");
        for matched in regex.find_iter(&comment_free) {
            let start = matched.start();
            if start > 0 {
                let prev = comment_free.as_bytes()[start - 1];
                if prev == b':'
                    || prev == b'.'
                    || prev == b'/'
                    || prev == b'_'
                    || prev == b'-'
                    || prev == b'*'
                    || prev == b'?'
                    || prev == b']'
                    || prev.is_ascii_alphanumeric()
                {
                    // Skip URL fragments and path segments within relative paths or identifiers.
                    continue;
                }
            }
            let literal = matched.as_str();
            if is_allowed_absolute_command_path(Path::new(literal)) {
                continue;
            }
            // Containment applies in every mode: the OS sandbox does not confine
            // reads, so an out-of-workspace absolute path (e.g. a read of a secret)
            // must still be rejected by the parser.
            if !self.is_in_workspace(Path::new(literal)) {
                return Err(anyhow!(
                    "Command contains absolute path outside workspace: {}",
                    literal
                ));
            }
            self.ensure_path_not_protected(Path::new(literal), "process path reference")?;
        }

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

    fn protected_subpath_component(&self, path: &Path) -> Option<&'static str> {
        let resolved_path = self.resolved_path_with_existing_parents(path);
        for root in &self.spec.writable_roots {
            let canonical_root = self
                .canonicalize(root)
                .unwrap_or_else(|_| lexically_normalize_path(root));
            let normalized_root = lexically_normalize_path(root);
            let Ok(relative) = resolved_path
                .strip_prefix(&canonical_root)
                .or_else(|_| resolved_path.strip_prefix(&normalized_root))
            else {
                continue;
            };
            if is_allowed_protected_relative_path(relative) {
                return None;
            }
            if let Some(component) = relative.components().find_map(|component| match component {
                Component::Normal(name) => {
                    let candidate = name.to_str()?;
                    PROTECTED_SUBPATHS
                        .iter()
                        .copied()
                        .find(|protected| *protected == candidate)
                }
                _ => None,
            }) {
                return Some(component);
            }
        }
        None
    }

    fn is_under_writable_root(&self, path: &Path) -> bool {
        let normalized_path = lexically_normalize_path(path);
        self.spec.writable_roots.iter().any(|root| {
            let canonical_root = self
                .canonicalize(root)
                .unwrap_or_else(|_| lexically_normalize_path(root));
            let normalized_root = lexically_normalize_path(root);
            path.starts_with(&canonical_root) || normalized_path.starts_with(&normalized_root)
        })
    }

    fn normalized_path(&self, path: &Path) -> PathBuf {
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root().join(path)
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
            self.workspace_root().join(path)
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
    // Workspace containment is enforced for ALL modes, including ProtectedOnly.
    // The OS sandbox confines *writes* (and network) to the workspace, but Seatbelt
    // permits reads by default, so an auto-approved read like `cat ~/.ssh/id_rsa`
    // would otherwise exfiltrate secrets into tool output. ProtectedOnly only drops
    // the syntactic *shape* checks (so wrappers may run), never path containment.
    fn validate_command_path_candidate(&self, token: &str, cwd: &Path) -> Result<()> {
        if token.is_empty() || token.starts_with('-') {
            return Ok(());
        }

        if token.starts_with('~') {
            return Err(anyhow!(
                "Command references HOME paths outside workspace: {}",
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
            if !self.is_in_workspace(&candidate) {
                return Err(anyhow!(
                    "Command references path outside workspace: {}",
                    token
                ));
            }
            self.ensure_path_not_protected(&candidate, "process path reference")?;
            self.ensure_path_not_multiply_linked(&candidate, "process path reference")?;
        }

        Ok(())
    }

    fn validate_redirection_target(&self, token: &str, cwd: &Path) -> Result<()> {
        if token.is_empty() {
            return Err(anyhow!("Command ends with an incomplete redirection"));
        }

        if token.starts_with('~') {
            return Err(anyhow!(
                "Command references HOME paths outside workspace: {}",
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
        if !self.is_in_workspace(&candidate) {
            return Err(anyhow!(
                "Command references path outside workspace: {}",
                token
            ));
        }
        self.ensure_path_not_protected(&candidate, "process path reference")?;
        self.ensure_path_not_multiply_linked(&candidate, "process path reference")?;
        Ok(())
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
                "Sandbox backend {} rejects shell control flow like {} because workspace_path_guard only supports direct commands with statically checkable paths",
                backend_name,
                command_word
            ));
        }

        let command = command_basename(command_word);
        if is_unsupported_shell_wrapper(command) {
            return Err(anyhow!(
                "Sandbox backend {} rejects shell wrappers like {} because workspace_path_guard only supports direct commands with statically checkable paths",
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
    // quoted script stays an opaque token and its `.git`/out-of-workspace paths
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

fn is_allowed_protected_relative_path(relative: &Path) -> bool {
    let mut components = Vec::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return false;
        };
        let Some(name) = name.to_str() else {
            return false;
        };
        components.push(name);
    }

    matches!(
        components.as_slice(),
        [".alan", "memory", ..]
            | [".alan", "agent", "persona", ..]
            | [".alan", "agents", _, "persona", ..]
    )
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
    if token.starts_with("--") {
        return None;
    }
    let rest = token.strip_prefix('-')?;
    if rest.len() < 2 {
        return None;
    }

    rest.char_indices()
        .skip(1)
        .map(|(index, _)| &rest[index..])
        .find(|candidate| {
            candidate.starts_with('~')
                || looks_like_path_token(candidate)
                || looks_like_bare_protected_subpath_token(candidate)
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

fn is_workspace_routing_reason(reason: &str) -> bool {
    reason.contains("outside workspace")
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
