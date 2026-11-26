//! CLI flags and boot resolution for explicit invocation-only trust posture,
//! dedicated Guardian profile, and standing project grants.

use alan_agent_engine::TrustPosture;
use alan_agent_engine::runtime::EvaluationSurface;
use alan_os_host::HostBootConfig;
use alan_service_manager::{
    FileserverMountAccess, RootBindingMode, RootFileserverActivation, RootPackageBinding,
    validate_package_id,
};
use anyhow::{Context, Result, bail, ensure};
use clap::Args;

/// Bare `alan` flags governing invocation trust posture, Guardian profile, grants, and packages.
#[derive(Args, Debug, Clone, Default)]
pub struct BareTrustArgs {
    /// Keep the OS sandbox active with adaptive smart action review.
    #[arg(long)]
    pub smart: bool,
    /// Bypass per-action policy and Guardian prompts while keeping the OS sandbox active.
    #[arg(long)]
    pub trust: bool,
    /// Requires `--trust`; disable OS sandbox and Guardian prompts to run directly under your Host account.
    #[arg(long)]
    pub unsafe_host_account: bool,
    /// Dedicated Connection profile for Guardian escalation reviews.
    #[arg(long, value_name = "PROFILE", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub guardian_profile: Option<String>,
    /// Dedicated Connection profile for Smart Approval (`--smart`) System 1 evaluation.
    #[arg(long = "smart-profile", value_name = "PROFILE", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub smart_profile: Option<String>,
    /// Explicitly activate saved System Store Host Mount grants by id.
    #[arg(long = "grant", value_name = "GRANT_ID", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub grants: Vec<String>,
    /// Explicitly activate an installed Quartermaster package (`skill`, `tool`, `mcp`, `provider`) for this invocation.
    #[arg(long = "package", value_name = "PACKAGE_ID", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub packages: Vec<String>,
    /// Explicitly activate an installed `fileserver` package for this invocation (`<id>[:ro|rw][:net]`).
    #[arg(long = "fileserver", value_name = "PACKAGE_ID[:ro|rw][:net]", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub fileservers: Vec<String>,
}

/// Validated invocation-only options resolved for one bare `alan` launch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedBareTrustOptions {
    pub posture: TrustPosture,
    pub guardian_profile: Option<String>,
    pub smart_profile: Option<String>,
    pub grants: Vec<String>,
    pub explicit_packages: Vec<RootPackageBinding>,
}

/// Parse `<package-id>[:ro|rw][:net]` into a fail-closed active [`RootPackageBinding`].
pub fn parse_fileserver_cli_spec(raw: &str) -> Result<RootPackageBinding> {
    let parts: Vec<&str> = raw.split(':').collect();
    let package_id = parts.first().copied().unwrap_or("").trim();
    validate_package_id(package_id)
        .with_context(|| format!("invalid `--fileserver` package spec `{raw}`"))?;

    let mut access: Option<FileserverMountAccess> = None;
    let mut allow_network = false;
    for token in &parts[1..] {
        match token.trim() {
            "ro" | "read-only" | "read_only" => {
                ensure!(
                    access.is_none() || access == Some(FileserverMountAccess::ReadOnly),
                    "conflicting access flags in `--fileserver {raw}`"
                );
                access = Some(FileserverMountAccess::ReadOnly);
            }
            "rw" | "read-write" | "read_write" => {
                ensure!(
                    access.is_none() || access == Some(FileserverMountAccess::ReadWrite),
                    "conflicting access flags in `--fileserver {raw}`"
                );
                access = Some(FileserverMountAccess::ReadWrite);
            }
            "net" | "network" => {
                ensure!(
                    !allow_network,
                    "duplicate `:net` modifier in `--fileserver {raw}`"
                );
                allow_network = true;
            }
            other => {
                bail!(
                    "invalid modifier `{other}` in `--fileserver {raw}` (expected `:ro`, `:rw`, or `:net`)"
                );
            }
        }
    }

    Ok(RootPackageBinding {
        package_id: package_id.to_string(),
        mode: RootBindingMode::Active,
        fileserver: Some(RootFileserverActivation {
            access: access.unwrap_or(FileserverMountAccess::ReadOnly),
            allow_network,
            host_grant_ids: Vec::new(),
        }),
        updated_at: chrono::Utc::now(),
    })
}

/// Render a [`FileserverMountAccess`] and `allow_network` pair as canonical `--fileserver` spec.
pub fn format_fileserver_cli_spec(
    package_id: &str,
    access: FileserverMountAccess,
    allow_network: bool,
) -> String {
    let access_flag = match access {
        FileserverMountAccess::ReadOnly => "ro",
        FileserverMountAccess::ReadWrite => "rw",
    };
    if allow_network {
        format!("{package_id}:{access_flag}:net")
    } else {
        format!("{package_id}:{access_flag}")
    }
}

/// Parse bare `--resume` vs `--resume <ROLLOUT_ID>`.
pub fn parse_resume_arg(raw: Option<&str>, has_subcommand: bool) -> Result<(bool, Option<String>)> {
    let Some(raw) = raw else {
        return Ok((false, None));
    };
    anyhow::ensure!(!has_subcommand, "`--resume` only applies to bare `alan`");
    if raw.is_empty() {
        Ok((true, None))
    } else {
        let trimmed = raw.trim();
        anyhow::ensure!(
            !trimmed.is_empty(),
            "`--resume` rollout id must not be empty"
        );
        Ok((true, Some(trimmed.to_string())))
    }
}

/// Validate `--smart`, `--trust`, `--unsafe-host-account`, `--guardian-profile`, `--grant`,
/// `--package`, and `--fileserver`.
pub fn resolve_bare_trust_options(
    args: &BareTrustArgs,
    has_subcommand: bool,
) -> Result<ResolvedBareTrustOptions> {
    anyhow::ensure!(
        !args.smart || !has_subcommand,
        "`--smart` only applies to bare `alan`"
    );
    anyhow::ensure!(
        !args.trust || !has_subcommand,
        "`--trust` only applies to bare `alan`"
    );
    anyhow::ensure!(
        !args.unsafe_host_account || !has_subcommand,
        "`--unsafe-host-account` only applies to bare `alan`"
    );
    anyhow::ensure!(
        args.guardian_profile.is_none() || !has_subcommand,
        "`--guardian-profile` only applies to bare `alan`"
    );
    anyhow::ensure!(
        args.smart_profile.is_none() || !has_subcommand,
        "`--smart-profile` only applies to bare `alan`"
    );
    anyhow::ensure!(
        args.grants.is_empty() || !has_subcommand,
        "`--grant` only applies to bare `alan`"
    );
    anyhow::ensure!(
        args.packages.is_empty() || !has_subcommand,
        "`--package` only applies to bare `alan`"
    );
    anyhow::ensure!(
        args.fileservers.is_empty() || !has_subcommand,
        "`--fileserver` only applies to bare `alan`"
    );

    let posture = TrustPosture::from_cli_posture(args.smart, args.trust, args.unsafe_host_account)?;
    let mut explicit_packages = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let now = chrono::Utc::now();

    for package_id in &args.packages {
        let id = package_id.trim();
        validate_package_id(id)?;
        ensure!(
            seen.insert(id.to_string()),
            "duplicate `--package` / `--fileserver` selection `{id}`"
        );
        explicit_packages.push(RootPackageBinding {
            package_id: id.to_string(),
            mode: RootBindingMode::Active,
            fileserver: None,
            updated_at: now,
        });
    }
    for spec in &args.fileservers {
        let binding = parse_fileserver_cli_spec(spec)?;
        ensure!(
            seen.insert(binding.package_id.clone()),
            "duplicate `--package` / `--fileserver` selection `{}`",
            binding.package_id
        );
        explicit_packages.push(binding);
    }

    Ok(ResolvedBareTrustOptions {
        posture,
        guardian_profile: args.guardian_profile.clone(),
        smart_profile: args.smart_profile.clone(),
        grants: args.grants.clone(),
        explicit_packages,
    })
}

/// Build the `HostBootConfig` with invocation-only posture, Guardian, resume, grants, and packages.
#[allow(
    clippy::too_many_arguments,
    reason = "Trust and rollout flags are passed directly from foreground CLI dispatch"
)]
pub fn apply_boot_trust_options(
    mut config: HostBootConfig,
    options: ResolvedBareTrustOptions,
    resume_rollout_id: Option<String>,
    ephemeral: bool,
    no_resume_update: bool,
    shadow_evaluator: Option<String>,
    shadow_surface: EvaluationSurface,
) -> Result<HostBootConfig> {
    config = config
        .with_trust_posture(options.posture)
        .with_ephemeral_rollout(ephemeral)
        .with_no_resume_update(no_resume_update);
    if ephemeral {
        config = config.with_root_package_bindings(Vec::new());
    }
    if !options.explicit_packages.is_empty() {
        config = config.with_explicit_root_package_bindings(options.explicit_packages);
    }
    if let Some(rollout_id) = resume_rollout_id {
        config = config.with_root_resume_id(Some(rollout_id));
    }
    if let Some(guardian_profile) = options.guardian_profile {
        config = config.with_guardian_profile(guardian_profile)?;
    }
    if let Some(smart_profile) = options.smart_profile {
        config = config.with_smart_evaluator_profile(smart_profile)?;
    }
    if !options.grants.is_empty() {
        config = config.with_saved_grants(options.grants);
    }
    if let Some(profile) = shadow_evaluator {
        config = config.with_input_shadow(profile, shadow_surface);
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fileserver_spec_parser_handles_defaults_modifiers_and_errors() {
        let default_fs = parse_fileserver_cli_spec("github-fileserver").unwrap();
        assert_eq!(default_fs.package_id, "github-fileserver");
        assert_eq!(default_fs.mode, RootBindingMode::Active);
        assert_eq!(
            default_fs.fileserver.as_ref().unwrap().access,
            FileserverMountAccess::ReadOnly
        );
        assert!(!default_fs.fileserver.as_ref().unwrap().allow_network);

        let rw_net = parse_fileserver_cli_spec("github-fileserver:rw:net").unwrap();
        assert_eq!(
            rw_net.fileserver.as_ref().unwrap().access,
            FileserverMountAccess::ReadWrite
        );
        assert!(rw_net.fileserver.as_ref().unwrap().allow_network);
        assert_eq!(
            format_fileserver_cli_spec("github-fileserver", FileserverMountAccess::ReadWrite, true),
            "github-fileserver:rw:net"
        );

        assert!(parse_fileserver_cli_spec("github-fileserver:ro:rw").is_err());
        assert!(parse_fileserver_cli_spec("github-fileserver:net:net").is_err());
        assert!(parse_fileserver_cli_spec("github-fileserver:bogus").is_err());
        assert!(parse_fileserver_cli_spec("../bad").is_err());
    }

    #[test]
    fn resolve_bare_trust_options_validates_package_and_fileserver_scope() {
        let valid = BareTrustArgs {
            packages: vec!["alan-self-iterate".to_string()],
            fileservers: vec!["github-fileserver:rw:net".to_string()],
            ..Default::default()
        };
        let resolved = resolve_bare_trust_options(&valid, false).unwrap();
        assert_eq!(resolved.explicit_packages.len(), 2);
        assert!(resolve_bare_trust_options(&valid, true).is_err());

        let dup = BareTrustArgs {
            packages: vec!["github-fileserver".to_string()],
            fileservers: vec!["github-fileserver:ro".to_string()],
            ..Default::default()
        };
        assert!(resolve_bare_trust_options(&dup, false).is_err());
    }

    #[test]
    fn resolve_bare_trust_options_handles_smart_and_rejects_conflicts_and_subcommands() {
        let smart = BareTrustArgs {
            smart: true,
            ..Default::default()
        };
        let resolved = resolve_bare_trust_options(&smart, false).unwrap();
        assert_eq!(resolved.posture, TrustPosture::ConfinedSmart);
        assert!(resolve_bare_trust_options(&smart, true).is_err());

        let smart_and_trust = BareTrustArgs {
            smart: true,
            trust: true,
            ..Default::default()
        };
        assert!(resolve_bare_trust_options(&smart_and_trust, false).is_err());

        let smart_and_unsafe = BareTrustArgs {
            smart: true,
            unsafe_host_account: true,
            ..Default::default()
        };
        assert!(resolve_bare_trust_options(&smart_and_unsafe, false).is_err());
    }
}
