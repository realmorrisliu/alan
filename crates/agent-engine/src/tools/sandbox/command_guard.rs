use super::command_interpreters;
use super::command_wrappers::{
    lenient_shell_wrapper_inline_script, shell_wrapper_inline_script,
    validate_direct_command_shapes, validate_nested_command_evaluators,
};
use super::path_literals::{
    TokenPathRole, absolute_path_literal_candidates, is_allowed_absolute_command_path,
    is_file_redirection_operator, lexically_normalize_path,
    looks_like_bare_protected_subpath_token, looks_like_path_token, path_like_subtokens,
    token_path_role,
};
use super::path_safety::{existing_regular_file_has_multiple_links, protected_path_component};
use super::shell_syntax::{
    ShellToken, normalize_shell_line_continuations, shell_commands, shell_tokens_with_spans,
    validate_shell_features,
};
use super::{PathCheckMode, Sandbox};
use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

impl Sandbox {
    pub(super) fn validate_command_paths(
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
            return if protected_only {
                Ok(())
            } else {
                Err(anyhow!("Command cannot be empty"))
            };
        }

        // In ProtectedOnly mode an unparseable shape is tolerated (the OS sandbox
        // or explicit trust governs execution), while parseable protected-subpath
        // and literal path containment checks still run below.
        let tokens = match shell_tokens_with_spans(trimmed) {
            Ok(tokens) => tokens,
            Err(err) => return if protected_only { Ok(()) } else { Err(err) },
        };
        let commands = match shell_commands(trimmed) {
            Ok(commands) => commands,
            Err(err) => return if protected_only { Ok(()) } else { Err(err) },
        };
        // Under ProtectedOnly (OS-enforced or explicit trust), do not reject
        // shell wrappers/evaluators statically, but extract inline `sh`/`bash`/
        // `zsh`/`eval` scripts leniently so protected `.git`/`.alan`/`.agents`
        // subpaths and out-of-mount literal reads inside wrappers remain guarded.
        if protected_only {
            for words in &commands {
                if let Some(inner) = lenient_shell_wrapper_inline_script(words) {
                    self.validate_command_paths(
                        &inner,
                        cwd,
                        PathCheckMode::ProtectedOnly,
                        capability,
                    )?;
                }
            }
        } else {
            self.validate_direct_command_shapes(&commands)?;
            self.validate_nested_command_evaluators(&commands)?;
            for words in &commands {
                if let Some(inner) = shell_wrapper_inline_script(words)? {
                    self.validate_command_paths(&inner, cwd, PathCheckMode::Full, capability)?;
                }
            }
        }

        let mut expects_redirection_target = false;
        for token in &tokens {
            let role = token_path_role(trimmed, token);
            let token = &token.decoded;
            if expects_redirection_target {
                self.validate_redirection_target(token, cwd)?;
                expects_redirection_target = false;
                continue;
            }

            if is_file_redirection_operator(token) {
                expects_redirection_target = true;
                continue;
            }

            if !matches!(role, TokenPathRole::Check | TokenPathRole::Read) {
                continue;
            }
            let capability = role.capability(capability, self.os_sandbox_or_explicit_trust());
            for candidate in path_like_subtokens(token) {
                self.validate_command_path_candidate(candidate, cwd, capability)?;
            }
        }

        self.validate_absolute_path_literals(trimmed, &tokens, cwd, mode, capability)?;

        Ok(())
    }

    fn validate_direct_command_shapes(&self, commands: &[Vec<String>]) -> Result<()> {
        validate_direct_command_shapes(commands, self.backend_name())
    }

    pub(super) fn validate_shell_features(&self, cmd: &str) -> Result<()> {
        validate_shell_features(cmd, self.backend_name())
    }

    fn validate_nested_command_evaluators(&self, commands: &[Vec<String>]) -> Result<()> {
        validate_nested_command_evaluators(commands, self.backend_name())
    }

    pub(super) fn ensure_path_not_protected(&self, path: &Path, action: &str) -> Result<()> {
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

    pub(super) fn ensure_path_not_multiply_linked(&self, path: &Path, action: &str) -> Result<()> {
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

    pub(super) fn ensure_path_not_read_denied(&self, path: &Path, action: &str) -> Result<()> {
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

    pub(super) fn normalized_path(&self, path: &Path) -> PathBuf {
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

    pub(super) fn resolved_path_with_existing_parents(&self, path: &Path) -> PathBuf {
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.primary_host_root().join(path)
        };
        if absolute_path.exists() {
            return self.normalized_path(&absolute_path);
        }

        let normalized = lexically_normalize_path(&absolute_path);
        let mut current = absolute_path.as_path();
        let mut suffix = Vec::<OsString>::new();
        while !current.exists() {
            let Some(name) = current.file_name() else {
                return self.resolved_path_with_existing_parents(&normalized);
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
            let validation_path = candidate;
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

    pub(super) fn validate_redirection_target(&self, token: &str, cwd: &Path) -> Result<()> {
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
        let validation_path = candidate;
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

    fn validate_absolute_path_literals(
        &self,
        command: &str,
        tokens: &[ShellToken],
        cwd: &Path,
        mode: PathCheckMode,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<()> {
        let protected_only = matches!(mode, PathCheckMode::ProtectedOnly);
        for token in tokens {
            let role = token_path_role(command, token);
            if role == TokenPathRole::Data {
                continue;
            }
            let capability = role.capability(capability, self.os_sandbox_or_explicit_trust());
            let path_literals = if let TokenPathRole::ExecutableData(offset) = role {
                match command_interpreters::awk_getline_file_paths(&token.decoded[offset..]) {
                    Some(getline_paths) => {
                        for path in getline_paths {
                            self.validate_command_path_candidate(&path, cwd, capability)?;
                        }
                    }
                    None if !protected_only => {
                        return Err(anyhow!(
                            "Sandbox backend {} rejects AWK getline file paths unless they are simple unescaped string literals; other paths cannot be validated against host_mount",
                            self.backend_name()
                        ));
                    }
                    None => {}
                }
                Vec::new()
            } else {
                absolute_path_literal_candidates(&token.decoded)
            };
            for candidates in path_literals {
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
        if matches!(capability, Some(alan_agent_protocol::ToolCapability::Read)) {
            self.is_readable(literal_path)
        } else {
            self.is_writable(literal_path)
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
        let read_only_command =
            matches!(capability, Some(alan_agent_protocol::ToolCapability::Read));
        let path_is_authorized = if read_only_command {
            self.is_readable(literal_path)
        } else {
            self.is_writable(literal_path)
        };
        if !path_is_authorized {
            return Err(anyhow!(
                "Command contains absolute path outside host_mount: {}",
                literal
            ));
        }
        self.ensure_path_not_protected(literal_path, "process path reference")?;
        if read_only_command {
            self.ensure_path_not_read_denied(literal_path, "process path reference")?;
        }
        Ok(())
    }
}

pub(super) fn validate_bash_command_shape(cmd: &str) -> Result<()> {
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
