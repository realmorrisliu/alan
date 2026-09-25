use super::Sandbox;
use super::command_interpreters::awk_getline_file_paths;
use super::path_literals::{
    TokenPathRole, absolute_path_literal_candidates, is_allowed_absolute_command_path,
    quoted_absolute_path_literal_candidates, token_path_role,
};
use super::shell_syntax::ShellToken;
use anyhow::{Result, anyhow};
use std::path::Path;

impl Sandbox {
    pub(super) fn validate_absolute_path_literals(
        &self,
        command: &str,
        tokens: &[ShellToken],
        absolute_executable_token_starts: &[usize],
        cwd: &Path,
        capability: Option<alan_agent_protocol::ToolCapability>,
    ) -> Result<()> {
        for token in tokens {
            let role = token_path_role(command, token);
            if absolute_executable_token_starts.contains(&token.raw_start)
                || role == TokenPathRole::Data
            {
                continue;
            }
            let path_literals = if role == TokenPathRole::ExecutableData {
                let Some(getline_paths) = awk_getline_file_paths(&token.decoded) else {
                    return Err(anyhow!(
                        "Sandbox backend {} rejects AWK getline file paths unless they are simple unescaped string literals; other paths cannot be validated against host_mount",
                        self.backend_name()
                    ));
                };
                for path in getline_paths {
                    self.validate_command_path_candidate(&path, cwd, capability)?;
                }
                quoted_absolute_path_literal_candidates(&token.decoded)
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
        // Containment applies in every mode: the OS sandbox does not confine
        // reads, so an out-of-host_mount absolute path (e.g. a read of a secret)
        // must still be rejected by the parser.
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
