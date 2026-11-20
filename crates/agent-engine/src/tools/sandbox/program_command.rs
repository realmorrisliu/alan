//! Shared OS-confined command construction for shell invocations and packaged binaries.

use super::super::sandbox_backend::SandboxBackendKind;
use super::Sandbox;
use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::path::Path;

impl Sandbox {
    /// Build a confined command for launching a direct non-shell executable
    /// (such as a Quartermaster `tool` package binary or `mcp` stdio server).
    pub fn build_confined_program_command(
        &self,
        program: &Path,
        args: &[String],
        cwd: &Path,
        allow_network: bool,
    ) -> Result<tokio::process::Command> {
        let os_args = args.iter().map(OsString::from).collect::<Vec<_>>();
        self.build_confined_argv(program, &os_args, cwd, allow_network, self.active_backend())
    }

    pub(super) fn build_confined_argv(
        &self,
        program: &Path,
        args: &[OsString],
        cwd: &Path,
        allow_network: bool,
        backend: SandboxBackendKind,
    ) -> Result<tokio::process::Command> {
        if self.trust_posture.is_unsandboxed_host_account() {
            let mut command = tokio::process::Command::new(program);
            command.args(args);
            command.current_dir(cwd);
            return Ok(command);
        }

        if !self.excluded_host_roots.is_empty()
            && !matches!(
                backend,
                SandboxBackendKind::Seatbelt | SandboxBackendKind::LinuxReifiedNamespace
            )
        {
            return Err(anyhow!(
                "This backend cannot isolate inactive Host Mount reads"
            ));
        }

        let mut command = match backend {
            SandboxBackendKind::Seatbelt => {
                let mut profile = super::super::sandbox_backend::seatbelt_profile(
                    &self.spec.writable_roots,
                    &self.spec.read_denylist,
                    allow_network,
                );
                profile.push_str(&super::super::sandbox_backend::seatbelt_read_only(
                    &self.spec,
                ));
                profile.push_str(
                    &super::super::sandbox_backend::seatbelt_host_mount_exclusions(
                        &self.excluded_host_roots,
                        &self.spec.readable_roots,
                    ),
                );
                let mut command = tokio::process::Command::new("/usr/bin/sandbox-exec");
                command.arg("-p").arg(profile).arg(program).args(args);
                command
            }
            SandboxBackendKind::LinuxReifiedNamespace => {
                return Err(anyhow!(
                    "linux_reified_namespace backend is detectable but no runner is installed yet"
                ));
            }
            #[cfg(target_os = "linux")]
            SandboxBackendKind::Landlock => {
                use std::os::unix::process::CommandExt;
                let writable_roots = self.spec.writable_roots.clone();
                let read_denylist = self.spec.read_denylist.clone();
                let mut command = std::process::Command::new(program);
                command.args(args);
                // SAFETY: pre_exec runs in the forked child before exec; it only
                // applies a Landlock ruleset (no shared-state mutation).
                unsafe {
                    command.pre_exec(move || {
                        super::super::sandbox_backend::apply_landlock(
                            &writable_roots,
                            &read_denylist,
                            allow_network,
                        )
                    });
                }
                tokio::process::Command::from(command)
            }
            _ => {
                let mut command = tokio::process::Command::new(program);
                command.args(args);
                command
            }
        };
        command.current_dir(cwd);
        Ok(command)
    }
}
