//! CLI flags and boot resolution for explicit invocation-only trust posture,
//! dedicated Guardian profile, and standing project grants.

use alan_agent_engine::TrustPosture;
use alan_agent_engine::runtime::EvaluationSurface;
use alan_os_host::HostBootConfig;
use anyhow::Result;
use clap::Args;

/// Bare `alan` flags governing invocation trust posture, Guardian profile, and grants.
#[derive(Args, Debug, Clone, Default)]
pub struct BareTrustArgs {
    /// Bypass per-action policy and Guardian prompts while keeping the OS sandbox active.
    #[arg(long)]
    pub trust: bool,
    /// Requires `--trust`; disable OS sandbox and Guardian prompts to run directly under your Host account.
    #[arg(long)]
    pub unsafe_host_account: bool,
    /// Dedicated Connection profile for Guardian escalation reviews.
    #[arg(long, value_name = "PROFILE", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub guardian_profile: Option<String>,
    /// Explicitly activate saved System Store Host Mount grants by id.
    #[arg(long = "grant", value_name = "GRANT_ID", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub grants: Vec<String>,
}

/// Validated invocation-only options resolved for one bare `alan` launch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedBareTrustOptions {
    pub posture: TrustPosture,
    pub guardian_profile: Option<String>,
    pub grants: Vec<String>,
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

/// Validate `--trust`, `--unsafe-host-account`, `--guardian-profile`, and `--grant`.
pub fn resolve_bare_trust_options(
    args: &BareTrustArgs,
    has_subcommand: bool,
) -> Result<ResolvedBareTrustOptions> {
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
        args.grants.is_empty() || !has_subcommand,
        "`--grant` only applies to bare `alan`"
    );

    let posture = TrustPosture::from_cli_flags(args.trust, args.unsafe_host_account)?;
    Ok(ResolvedBareTrustOptions {
        posture,
        guardian_profile: args.guardian_profile.clone(),
        grants: args.grants.clone(),
    })
}

/// Build the `HostBootConfig` with invocation-only posture, Guardian, resume, and grants.
pub fn apply_boot_trust_options(
    mut config: HostBootConfig,
    options: ResolvedBareTrustOptions,
    resume_rollout_id: Option<String>,
    ephemeral: bool,
    shadow_evaluator: Option<String>,
    shadow_surface: EvaluationSurface,
) -> Result<HostBootConfig> {
    config = config
        .with_trust_posture(options.posture)
        .with_ephemeral_rollout(ephemeral);
    if let Some(rollout_id) = resume_rollout_id {
        config = config.with_root_resume_id(Some(rollout_id));
    }
    if let Some(guardian_profile) = options.guardian_profile {
        config = config.with_guardian_profile(guardian_profile)?;
    }
    if !options.grants.is_empty() {
        config = config.with_saved_grants(options.grants);
    }
    if let Some(profile) = shadow_evaluator {
        config = config.with_input_shadow(profile, shadow_surface);
    }
    Ok(config)
}
