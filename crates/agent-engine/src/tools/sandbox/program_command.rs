//! Shared OS-confined command construction for shell invocations and packaged binaries.

use super::super::sandbox_backend::SandboxBackendKind;
use super::Sandbox;
use anyhow::{Result, anyhow};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub(super) enum ConfinedNetworkMode {
    Deny,
    AllowAll,
    LoopbackProxyPort(u16),
}

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
        let network_mode = if allow_network {
            ConfinedNetworkMode::AllowAll
        } else {
            ConfinedNetworkMode::Deny
        };
        self.build_confined_argv_with_network(
            program,
            &os_args,
            cwd,
            network_mode,
            self.active_backend(),
        )
    }

    /// Build a confined command whose outbound network is pinned to a Host-owned loopback proxy.
    pub fn build_confined_program_command_with_proxy(
        &self,
        program: &Path,
        args: &[String],
        cwd: &Path,
        proxy_port: u16,
    ) -> Result<tokio::process::Command> {
        let os_args = args.iter().map(OsString::from).collect::<Vec<_>>();
        self.build_confined_argv_with_network(
            program,
            &os_args,
            cwd,
            ConfinedNetworkMode::LoopbackProxyPort(proxy_port),
            self.active_backend(),
        )
    }

    pub(super) fn build_confined_argv(
        &self,
        program: &Path,
        args: &[OsString],
        cwd: &Path,
        allow_network: bool,
        backend: SandboxBackendKind,
    ) -> Result<tokio::process::Command> {
        let network_mode = if allow_network {
            ConfinedNetworkMode::AllowAll
        } else {
            ConfinedNetworkMode::Deny
        };
        self.build_confined_argv_with_network(program, args, cwd, network_mode, backend)
    }

    fn build_confined_argv_with_network(
        &self,
        program: &Path,
        args: &[OsString],
        cwd: &Path,
        network_mode: ConfinedNetworkMode,
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
                let mut profile = match network_mode {
                    ConfinedNetworkMode::Deny => super::super::sandbox_backend::seatbelt_profile(
                        &self.spec.writable_roots,
                        &self.spec.read_denylist,
                        false,
                    ),
                    ConfinedNetworkMode::AllowAll => {
                        super::super::sandbox_backend::seatbelt_profile(
                            &self.spec.writable_roots,
                            &self.spec.read_denylist,
                            true,
                        )
                    }
                    ConfinedNetworkMode::LoopbackProxyPort(proxy_port) => {
                        super::super::sandbox_backend::seatbelt_profile_for_loopback_proxy(
                            &self.spec.writable_roots,
                            &self.spec.read_denylist,
                            proxy_port,
                        )
                    }
                };
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
                let allow_network = !matches!(network_mode, ConfinedNetworkMode::Deny);
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
        self.maybe_apply_gh_config_dir_override(&mut command, cwd, backend);
        self.maybe_sanitize_cargo_target_dir(&mut command, cwd, backend);
        Ok(command)
    }

    fn maybe_apply_gh_config_dir_override(
        &self,
        command: &mut tokio::process::Command,
        cwd: &Path,
        backend: SandboxBackendKind,
    ) {
        self.apply_gh_config_dir_override_with_env(
            command,
            cwd,
            backend,
            GhEnvContext {
                gh_token: std::env::var_os("GH_TOKEN").as_deref(),
                github_token: std::env::var_os("GITHUB_TOKEN").as_deref(),
                gh_config_dir: std::env::var_os("GH_CONFIG_DIR").as_deref(),
                tmpdir: std::env::var_os("TMPDIR").as_deref(),
                home_dir: dirs::home_dir().as_deref(),
            },
        );
    }

    fn apply_gh_config_dir_override_with_env(
        &self,
        command: &mut tokio::process::Command,
        cwd: &Path,
        backend: SandboxBackendKind,
        env: GhEnvContext<'_>,
    ) {
        if self.trust_posture.is_unsandboxed_host_account()
            || !matches!(
                backend,
                SandboxBackendKind::Seatbelt | SandboxBackendKind::Landlock
            )
        {
            return;
        }
        let has_explicit_token = env.gh_token.is_some_and(|val| !val.is_empty())
            || env.github_token.is_some_and(|val| !val.is_empty());
        if !has_explicit_token {
            return;
        }
        let Some(home) = env.home_dir else {
            return;
        };
        let default_gh_config = home.join(".config").join("gh");
        if !super::path_matches_read_denylist(&default_gh_config, &self.spec.read_denylist) {
            return;
        }
        if let Some(configured_dir) = env.gh_config_dir.filter(|val| !val.is_empty()) {
            let configured_path = Path::new(configured_dir);
            let resolved_path = if configured_path.is_absolute() {
                configured_path.to_path_buf()
            } else {
                cwd.join(configured_path)
            };
            if !super::path_matches_read_denylist(&resolved_path, &self.spec.read_denylist) {
                return;
            }
        }
        if let Some(empty_dir) =
            select_non_denied_nonexistent_gh_config_dir(env.tmpdir, &self.spec.read_denylist)
        {
            command.env("GH_CONFIG_DIR", empty_dir);
        }
    }

    fn maybe_sanitize_cargo_target_dir(
        &self,
        command: &mut tokio::process::Command,
        cwd: &Path,
        backend: SandboxBackendKind,
    ) {
        self.sanitize_cargo_target_dir_with_env(
            command,
            cwd,
            backend,
            std::env::var_os("CARGO_TARGET_DIR").as_deref(),
        );
    }

    pub(crate) fn sanitize_cargo_target_dir_with_env(
        &self,
        command: &mut tokio::process::Command,
        cwd: &Path,
        backend: SandboxBackendKind,
        cargo_target_dir: Option<&OsStr>,
    ) {
        if self.trust_posture.is_unsandboxed_host_account()
            || !matches!(
                backend,
                SandboxBackendKind::Seatbelt | SandboxBackendKind::Landlock
            )
        {
            return;
        }
        let Some(raw) = cargo_target_dir.filter(|val| !val.is_empty()) else {
            return;
        };
        let target_path = Path::new(raw);
        let resolved = if target_path.is_absolute() {
            super::lexically_normalize_path(target_path)
        } else {
            super::lexically_normalize_path(&cwd.join(target_path))
        };
        if self.is_writable(&resolved) {
            return;
        }
        if self.is_writable(cwd) {
            command.env_remove("CARGO_TARGET_DIR");
        } else if let Some(writable_root) = self.spec.writable_roots.first() {
            command.env("CARGO_TARGET_DIR", writable_root.join("target"));
        } else {
            command.env_remove("CARGO_TARGET_DIR");
        }
    }
}

struct GhEnvContext<'a> {
    gh_token: Option<&'a OsStr>,
    github_token: Option<&'a OsStr>,
    gh_config_dir: Option<&'a OsStr>,
    tmpdir: Option<&'a OsStr>,
    home_dir: Option<&'a Path>,
}

fn select_non_denied_nonexistent_gh_config_dir(
    tmpdir: Option<&OsStr>,
    read_denylist: &[PathBuf],
) -> Option<PathBuf> {
    let bases = tmpdir
        .filter(|val| !val.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .chain(std::iter::once(PathBuf::from("/tmp")));
    let pid_leaf = format!("alan-sandbox-gh-config-empty-{}", std::process::id());
    for base in bases {
        for leaf in ["alan-sandbox-gh-config-empty", pid_leaf.as_str()] {
            let candidate = base.join(leaf);
            if candidate.symlink_metadata().is_err()
                && !super::path_matches_read_denylist(&candidate, read_denylist)
            {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::SandboxSpec;
    use super::*;
    use crate::policy::TrustPosture;
    use tempfile::TempDir;

    fn configured_gh_config_dir(command: &tokio::process::Command) -> Option<OsString> {
        command
            .as_std()
            .get_envs()
            .find_map(|(k, v)| (k == "GH_CONFIG_DIR").then(|| v.map(OsStr::to_os_string)))
            .flatten()
    }

    fn sandbox_for_home(root: &Path, home: &Path) -> Sandbox {
        Sandbox::from_spec(SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![root.to_path_buf()],
            writable_roots: vec![root.to_path_buf()],
            read_denylist: SandboxSpec::sensitive_read_denylist_for_home(home),
            network: super::super::NetworkPosture::Deny,
        })
    }

    #[test]
    fn gh_config_dir_overridden_only_when_os_sandbox_active_and_token_present() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let tmp = temp.path().join("tmp");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        let sandbox = sandbox_for_home(temp.path(), &home);
        let args = [OsString::from("-v")];

        for (backend, gh_tok, github_tok) in [
            (SandboxBackendKind::Seatbelt, Some("ghp_123"), None),
            (SandboxBackendKind::Landlock, None, Some("ghs_456")),
        ] {
            let mut cmd = sandbox
                .build_confined_argv(Path::new("gh"), &args, temp.path(), true, backend)
                .unwrap();
            sandbox.apply_gh_config_dir_override_with_env(
                &mut cmd,
                temp.path(),
                backend,
                GhEnvContext {
                    gh_token: gh_tok.map(OsStr::new),
                    github_token: github_tok.map(OsStr::new),
                    gh_config_dir: None,
                    tmpdir: Some(tmp.as_os_str()),
                    home_dir: Some(&home),
                },
            );
            assert_eq!(
                configured_gh_config_dir(&cmd),
                Some(tmp.join("alan-sandbox-gh-config-empty").into_os_string())
            );
        }

        // Absent or empty tokens must leave GH_CONFIG_DIR unset so sandbox EPERM triggers.
        for (gh_tok, github_tok) in [(None, None), (Some(""), Some(""))] {
            let mut cmd = sandbox
                .build_confined_argv(
                    Path::new("gh"),
                    &args,
                    temp.path(),
                    true,
                    SandboxBackendKind::Seatbelt,
                )
                .unwrap();
            sandbox.apply_gh_config_dir_override_with_env(
                &mut cmd,
                temp.path(),
                SandboxBackendKind::Seatbelt,
                GhEnvContext {
                    gh_token: gh_tok.map(OsStr::new),
                    github_token: github_tok.map(OsStr::new),
                    gh_config_dir: None,
                    tmpdir: Some(tmp.as_os_str()),
                    home_dir: Some(&home),
                },
            );
            assert_eq!(configured_gh_config_dir(&cmd), None);
        }

        // HostMountPathGuard and HostAccountTrusted must never inject GH_CONFIG_DIR.
        let mut guard_cmd = tokio::process::Command::new("gh");
        sandbox.apply_gh_config_dir_override_with_env(
            &mut guard_cmd,
            temp.path(),
            SandboxBackendKind::HostMountPathGuard,
            GhEnvContext {
                gh_token: Some(OsStr::new("ghp_123")),
                github_token: None,
                gh_config_dir: None,
                tmpdir: Some(tmp.as_os_str()),
                home_dir: Some(&home),
            },
        );
        assert_eq!(configured_gh_config_dir(&guard_cmd), None);

        let unsandboxed = sandbox
            .clone()
            .with_trust_posture(TrustPosture::HostAccountTrusted);
        let mut unsandboxed_cmd = tokio::process::Command::new("gh");
        unsandboxed.apply_gh_config_dir_override_with_env(
            &mut unsandboxed_cmd,
            temp.path(),
            SandboxBackendKind::Seatbelt,
            GhEnvContext {
                gh_token: Some(OsStr::new("ghp_123")),
                github_token: None,
                gh_config_dir: None,
                tmpdir: Some(tmp.as_os_str()),
                home_dir: Some(&home),
            },
        );
        assert_eq!(configured_gh_config_dir(&unsandboxed_cmd), None);
    }

    #[test]
    fn gh_config_dir_override_respects_custom_non_denied_dir_and_overrides_denied_dir() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let tmp = temp.path().join("tmp");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        let sandbox = sandbox_for_home(temp.path(), &home);

        // Explicit non-denied GH_CONFIG_DIR is preserved.
        let custom_dir = temp.path().join("custom-gh");
        let mut preserve_cmd = tokio::process::Command::new("gh");
        sandbox.apply_gh_config_dir_override_with_env(
            &mut preserve_cmd,
            temp.path(),
            SandboxBackendKind::Seatbelt,
            GhEnvContext {
                gh_token: Some(OsStr::new("ghp_123")),
                github_token: None,
                gh_config_dir: Some(custom_dir.as_os_str()),
                tmpdir: Some(tmp.as_os_str()),
                home_dir: Some(&home),
            },
        );
        assert_eq!(configured_gh_config_dir(&preserve_cmd), None);

        // Explicit GH_CONFIG_DIR inside read_denylist is overridden.
        let denied_dir = home.join(".config/gh/extra");
        let mut override_cmd = tokio::process::Command::new("gh");
        sandbox.apply_gh_config_dir_override_with_env(
            &mut override_cmd,
            temp.path(),
            SandboxBackendKind::Seatbelt,
            GhEnvContext {
                gh_token: Some(OsStr::new("ghp_123")),
                github_token: None,
                gh_config_dir: Some(denied_dir.as_os_str()),
                tmpdir: Some(tmp.as_os_str()),
                home_dir: Some(&home),
            },
        );
        assert_eq!(
            configured_gh_config_dir(&override_cmd),
            Some(tmp.join("alan-sandbox-gh-config-empty").into_os_string())
        );

        // If ~/.config/gh is explicitly mounted (removed from read_denylist), do not override.
        let mounted_gh = home.join(".config/gh");
        let mounted_sandbox = Sandbox::from_spec(SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![temp.path().to_path_buf(), mounted_gh],
            writable_roots: vec![temp.path().to_path_buf()],
            read_denylist: SandboxSpec::sensitive_read_denylist_for_home(&home),
            network: super::super::NetworkPosture::Deny,
        });
        let mut mounted_cmd = tokio::process::Command::new("gh");
        mounted_sandbox.apply_gh_config_dir_override_with_env(
            &mut mounted_cmd,
            temp.path(),
            SandboxBackendKind::Seatbelt,
            GhEnvContext {
                gh_token: Some(OsStr::new("ghp_123")),
                github_token: None,
                gh_config_dir: None,
                tmpdir: Some(tmp.as_os_str()),
                home_dir: Some(&home),
            },
        );
        assert_eq!(configured_gh_config_dir(&mounted_cmd), None);

        // Existing default leaf falls back to a nonexistent candidate.
        std::fs::create_dir_all(tmp.join("alan-sandbox-gh-config-empty")).unwrap();
        let mut fallback_cmd = tokio::process::Command::new("gh");
        sandbox.apply_gh_config_dir_override_with_env(
            &mut fallback_cmd,
            temp.path(),
            SandboxBackendKind::Seatbelt,
            GhEnvContext {
                gh_token: Some(OsStr::new("ghp_123")),
                github_token: None,
                gh_config_dir: None,
                tmpdir: Some(tmp.as_os_str()),
                home_dir: Some(&home),
            },
        );
        let chosen = PathBuf::from(configured_gh_config_dir(&fallback_cmd).unwrap());
        assert!(!chosen.exists());
    }

    fn configured_env_var(
        cmd: &tokio::process::Command,
        key: &str,
    ) -> Option<Option<std::ffi::OsString>> {
        cmd.as_std()
            .get_envs()
            .find(|(k, _)| *k == OsStr::new(key))
            .map(|(_, v)| v.map(std::ffi::OsStr::to_os_string))
    }

    #[test]
    fn sanitize_cargo_target_dir_removes_or_rewrites_out_of_mount_targets() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let in_mount_target = project.path().join("custom-target");
        let outside_target = outside.path().join("shared-target");

        let sandbox = Sandbox::from_spec(SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![project.path().to_path_buf()],
            writable_roots: vec![project.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: super::super::NetworkPosture::Deny,
        });

        // 1. Out-of-mount absolute CARGO_TARGET_DIR under Seatbelt with writable cwd -> removed.
        let mut cmd = tokio::process::Command::new("cargo");
        sandbox.sanitize_cargo_target_dir_with_env(
            &mut cmd,
            project.path(),
            SandboxBackendKind::Seatbelt,
            Some(outside_target.as_os_str()),
        );
        assert_eq!(
            configured_env_var(&cmd, "CARGO_TARGET_DIR"),
            Some(None),
            "expected CARGO_TARGET_DIR to be removed when cwd is writable"
        );

        // 2. Out-of-mount relative CARGO_TARGET_DIR under Landlock -> removed.
        let mut rel_cmd = tokio::process::Command::new("cargo");
        sandbox.sanitize_cargo_target_dir_with_env(
            &mut rel_cmd,
            project.path(),
            SandboxBackendKind::Landlock,
            Some(OsStr::new("../escaped-target")),
        );
        assert_eq!(
            configured_env_var(&rel_cmd, "CARGO_TARGET_DIR"),
            Some(None),
            "expected relative out-of-mount CARGO_TARGET_DIR to be removed"
        );

        // 3. In-mount writable CARGO_TARGET_DIR -> preserved (no override).
        let mut inside_cmd = tokio::process::Command::new("cargo");
        sandbox.sanitize_cargo_target_dir_with_env(
            &mut inside_cmd,
            project.path(),
            SandboxBackendKind::Seatbelt,
            Some(in_mount_target.as_os_str()),
        );
        assert_eq!(
            configured_env_var(&inside_cmd, "CARGO_TARGET_DIR"),
            None,
            "in-mount writable CARGO_TARGET_DIR must be left untouched"
        );

        // 4. Out-of-mount CARGO_TARGET_DIR when cwd is read-only -> redirected to writable_root/target.
        let ro_dir = tempfile::tempdir().unwrap();
        let mut ro_cmd = tokio::process::Command::new("cargo");
        sandbox.sanitize_cargo_target_dir_with_env(
            &mut ro_cmd,
            ro_dir.path(),
            SandboxBackendKind::Seatbelt,
            Some(outside_target.as_os_str()),
        );
        assert_eq!(
            configured_env_var(&ro_cmd, "CARGO_TARGET_DIR"),
            Some(Some(project.path().join("target").into_os_string())),
            "expected fallback to writable_root/target when cwd is read-only"
        );

        // 5. Unsandboxed HostAccountTrusted or HostMountPathGuard -> preserved untouched.
        let mut no_sb_cmd = tokio::process::Command::new("cargo");
        sandbox.sanitize_cargo_target_dir_with_env(
            &mut no_sb_cmd,
            project.path(),
            SandboxBackendKind::HostMountPathGuard,
            Some(outside_target.as_os_str()),
        );
        assert_eq!(configured_env_var(&no_sb_cmd, "CARGO_TARGET_DIR"), None);

        let host_sandbox = Sandbox::from_spec(SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![project.path().to_path_buf()],
            writable_roots: vec![project.path().to_path_buf()],
            read_denylist: Vec::new(),
            network: super::super::NetworkPosture::Allow,
        })
        .with_trust_posture(crate::policy::TrustPosture::HostAccountTrusted);
        let mut host_cmd = tokio::process::Command::new("cargo");
        host_sandbox.sanitize_cargo_target_dir_with_env(
            &mut host_cmd,
            project.path(),
            SandboxBackendKind::Seatbelt,
            Some(outside_target.as_os_str()),
        );
        assert_eq!(configured_env_var(&host_cmd, "CARGO_TARGET_DIR"), None);
    }
}
