//! Human-gated release-preparation workflow template (`alan workflow init-release-prep`)
//! and non-bypassable release operation guard.

use anyhow::{Result, bail, ensure};
use clap::Args;

use super::manifest::{
    DEFAULT_TRIGGER_MAX_CONCURRENT, ReleaseGatePolicy, SUPPORTED_MANIFEST_VERSION,
    TriggerAllowlist, WorkflowManifest, WorkflowStore, WorkflowTrustPosture,
    ensure_confined_template_inputs, normalize_model_profile, normalize_unique_entries,
    parse_workflow_package_inputs,
};
use crate::cli::update::semver::normalize_version_str;

/// Default workflow identifier for `alan workflow init-release-prep`.
pub const DEFAULT_RELEASE_PREP_WORKFLOW_ID: &str = "release-prep";
/// Default GitHub label when optional trigger allowlist gating is configured on release prep.
pub const DEFAULT_RELEASE_PREP_LABEL: &str = "alan:release";
const DEFAULT_RELEASE_PREP_MAX_RETRIES: u32 = 2;
const DEFAULT_RELEASE_PREP_TIMEOUT_SECS: u64 = 1_800;

/// Arguments for `alan workflow init-release-prep`.
#[derive(Debug, Clone, Args)]
pub struct InitReleasePrepArgs {
    /// Canonical workflow identifier (`[a-z0-9][a-z0-9_-]*`)
    #[arg(default_value = DEFAULT_RELEASE_PREP_WORKFLOW_ID)]
    pub id: String,
    /// Optional target GitHub repository slug (`owner/repo`) for trigger gating
    #[arg(long, value_name = "OWNER/REPO")]
    pub repo: Option<String>,
    /// Optional explicitly allowlisted GitHub actor login (repeatable; requires `--repo`)
    #[arg(long = "allowed-actor", value_name = "LOGIN")]
    pub allowed_actors: Vec<String>,
    /// Required GitHub label when trigger gating is configured (defaults to `alan:release`)
    #[arg(long = "required-label", value_name = "LABEL")]
    pub required_labels: Vec<String>,
    /// Self or bot GitHub actor login ignored to prevent feedback loops
    #[arg(long = "ignored-actor", value_name = "LOGIN")]
    pub ignored_actors: Vec<String>,
    /// Maximum bounded retries per release PR before escalating to human review
    #[arg(long = "max-retries", default_value_t = DEFAULT_RELEASE_PREP_MAX_RETRIES)]
    pub max_retries: u32,
    /// Maximum concurrent release-prep workers allowed
    #[arg(long = "max-concurrent", default_value_t = DEFAULT_TRIGGER_MAX_CONCURRENT)]
    pub max_concurrent: u32,
    /// Optional target semantic version (e.g. `0.1.5` or `v0.1.5`)
    #[arg(long = "target-version", value_name = "SEMVER")]
    pub target_version: Option<String>,
    /// Schedule expression: 5-field cron or interval (defaults to `@weekly`)
    #[arg(long, default_value = "@weekly")]
    pub schedule: String,
    /// Optional Connection Service profile id (`connection_profile`)
    #[arg(long = "model-profile", alias = "profile")]
    pub model_profile: Option<String>,
    /// Explicit saved Host Mount grant id (`alan host grant save <id> <path>`)
    #[arg(
        long = "grant",
        alias = "mount",
        value_name = "GRANT_ID",
        required = true
    )]
    pub mounts: Vec<String>,
    /// Explicit Quartermaster package id enabled only for this workflow's isolated run (repeatable)
    #[arg(long = "package", value_name = "PACKAGE_ID")]
    pub packages: Vec<String>,
    /// Explicit Quartermaster fileserver activation (`<id>[:ro|rw][:net]`, repeatable)
    #[arg(long = "fileserver", value_name = "PACKAGE_ID[:ro|rw][:net]")]
    pub fileservers: Vec<String>,
    /// OS-confined invocation trust posture (`governed` or `confined_trusted`)
    #[arg(long, value_enum, default_value_t = WorkflowTrustPosture::Governed)]
    pub trust: WorkflowTrustPosture,
    /// Disable `--json` event streaming on spawned runs (`json = false` in manifest)
    #[arg(long = "no-stream-json", default_value_t = false)]
    pub no_stream_json: bool,
    /// Hard per-run timeout in seconds (defaults to 1800)
    #[arg(long = "timeout-secs", default_value_t = DEFAULT_RELEASE_PREP_TIMEOUT_SECS)]
    pub timeout_secs: u64,
    /// Overwrite an existing workflow manifest with the same id
    #[arg(long)]
    pub force: bool,
    /// Emit structured JSON instead of plain text
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `alan workflow check-release-gate`.
#[derive(Debug, Clone, Args)]
pub struct CheckReleaseGateArgs {
    /// Registered workflow identifier
    pub id: String,
    /// Release lifecycle operation to verify against `release_gate`
    #[arg(long, value_enum)]
    pub operation: ReleaseOperation,
    /// Emit structured JSON output
    #[arg(long)]
    pub json: bool,
}

/// High-risk release lifecycle actions guarded by [`ReleaseGatePolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ReleaseOperation {
    /// Draft and open a verified release pull request against `main`.
    OpenReleasePr,
    /// Merge the release pull request into `main`.
    MergeReleasePr,
    /// Create or push a `v*` release git tag.
    PushReleaseTag,
    /// Publish release archives, manifests, or crates externally.
    PublishReleaseArtifacts,
}

impl ReleaseOperation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::OpenReleasePr => "open-release-pr",
            Self::MergeReleasePr => "merge-release-pr",
            Self::PushReleaseTag => "push-release-tag",
            Self::PublishReleaseArtifacts => "publish-release-artifacts",
        }
    }
}

/// Evaluate whether `operation` is permitted for a stored workflow's `release_gate`.
pub(crate) fn run_check_release_gate(
    store: &WorkflowStore,
    args: CheckReleaseGateArgs,
) -> Result<(String, Option<String>)> {
    let manifest = store.load_manifest(&args.id)?;
    let Some(gate) = manifest.release_gate.as_ref() else {
        let msg = format!(
            "workflow `{}` does not declare a `release_gate` policy (failing closed)",
            manifest.id
        );
        let out = if args.json {
            format!(
                "{}\n",
                serde_json::to_string_pretty(&serde_json::json!({
                    "workflow_id": manifest.id,
                    "operation": args.operation.as_str(),
                    "allowed": false,
                    "reason": msg,
                }))?
            )
        } else {
            format!("{msg}\n")
        };
        return Ok((out, Some(msg)));
    };
    match ensure_release_operation_allowed(gate, args.operation) {
        Ok(()) => {
            let out = if args.json {
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "workflow_id": manifest.id,
                        "operation": args.operation.as_str(),
                        "allowed": true,
                        "release_gate": gate,
                    }))?
                )
            } else {
                format!(
                    "release gate check passed for `{}`: operation `{}` allowed\n",
                    manifest.id,
                    args.operation.as_str()
                )
            };
            Ok((out, None))
        }
        Err(err) => {
            let msg = format!("{err:#}");
            let out = if args.json {
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "workflow_id": manifest.id,
                        "operation": args.operation.as_str(),
                        "allowed": false,
                        "reason": msg,
                        "release_gate": gate,
                    }))?
                )
            } else {
                format!("release gate blocked `{}`: {msg}\n", manifest.id)
            };
            Ok((out, Some(msg)))
        }
    }
}

/// Verify whether `operation` is permitted under a validated [`ReleaseGatePolicy`].
///
/// Fails closed on any tampered policy (`validate()` must pass) and strictly blocks
/// autonomous PR merge, tag push, and artifact publication.
pub fn ensure_release_operation_allowed(
    gate: &ReleaseGatePolicy,
    operation: ReleaseOperation,
) -> Result<()> {
    gate.validate()?;
    match operation {
        ReleaseOperation::OpenReleasePr => Ok(()),
        ReleaseOperation::MergeReleasePr => {
            bail!("blocked by release_gate.require_human_merge: autonomous PR merge is forbidden")
        }
        ReleaseOperation::PushReleaseTag => {
            bail!(
                "blocked by release_gate.forbid_autonomous_tag_push: autonomous git tag push is forbidden"
            )
        }
        ReleaseOperation::PublishReleaseArtifacts => {
            bail!(
                "blocked by release_gate.forbid_autonomous_publish: autonomous release publication is forbidden"
            )
        }
    }
}

/// Build and validate a human-gated release-preparation [`WorkflowManifest`] from CLI args.
pub fn build_release_prep_manifest(args: &InitReleasePrepArgs) -> Result<WorkflowManifest> {
    ensure_confined_template_inputs(&args.mounts, args.trust, "release-prep")?;

    let target_version = match args.target_version.as_deref() {
        Some(raw) => Some(normalize_version_str(raw)?),
        None => None,
    };

    let has_trigger_fields = args.repo.is_some()
        || !args.allowed_actors.is_empty()
        || !args.required_labels.is_empty()
        || !args.ignored_actors.is_empty();
    let trigger_allowlist = if has_trigger_fields {
        let repo = args
            .repo
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let Some(repo) = repo else {
            bail!(
                "release-prep trigger gating requires `--repo <OWNER/REPO>` alongside actor/label options"
            );
        };
        ensure!(
            !args.allowed_actors.is_empty(),
            "release-prep trigger gating requires at least one `--allowed-actor <LOGIN>`"
        );
        let required_labels = if args.required_labels.is_empty() {
            vec![DEFAULT_RELEASE_PREP_LABEL.to_string()]
        } else {
            normalize_unique_entries(args.required_labels.clone())
        };
        let ignored_actors = if args.ignored_actors.is_empty() {
            vec![
                "alan-bot[bot]".to_string(),
                "github-actions[bot]".to_string(),
            ]
        } else {
            normalize_unique_entries(args.ignored_actors.clone())
        };
        Some(TriggerAllowlist {
            repo,
            allowed_actors: normalize_unique_entries(args.allowed_actors.clone()),
            required_labels,
            ignored_actors,
            max_retries: args.max_retries,
            max_concurrent: args.max_concurrent,
        })
    } else {
        None
    };

    let release_gate = ReleaseGatePolicy::enforced();
    let (packages, fileservers) = parse_workflow_package_inputs(&args.packages, &args.fileservers)?;
    let prompt = build_release_prep_prompt(
        target_version.as_deref(),
        trigger_allowlist.as_ref(),
        &args.mounts,
        &release_gate,
    );

    let manifest = WorkflowManifest {
        schema_version: SUPPORTED_MANIFEST_VERSION,
        id: args.id.clone(),
        schedule: args.schedule.trim().to_string(),
        prompt,
        model_profile: normalize_model_profile(args.model_profile.clone()),
        project_key: None,
        mounts: args.mounts.clone(),
        packages,
        fileservers,
        trust: args.trust,
        json: !args.no_stream_json,
        timeout_secs: Some(args.timeout_secs),
        trigger_allowlist,
        release_gate: Some(release_gate),
        cron_stream: None,
    };
    manifest.validate()?;
    Ok(manifest)
}

/// Render the bounded release-preparation prompt encoded in the registered manifest.
pub fn build_release_prep_prompt(
    target_version: Option<&str>,
    allowlist: Option<&TriggerAllowlist>,
    mounts: &[String],
    release_gate: &ReleaseGatePolicy,
) -> String {
    let version_directive = match target_version {
        Some(version) => format!("target version `{version}`"),
        None => {
            "the next semantic patch/minor version derived from the previous `v*` tag".to_string()
        }
    };
    let repo_context = match allowlist {
        Some(list) => format!(
            "Target repository `{}` with allowlisted actors [{}] and required labels [{}].",
            list.repo,
            list.allowed_actors.join(", "),
            list.required_labels.join(", ")
        ),
        None => "Target repository accessed solely via explicit mounted grants.".to_string(),
    };
    let grants = mounts.join(", ");
    format!(
        "Run the `release-prep` (or `alan-release-prep`) workflow skill for {version_directive} under explicit \
         grants [{grants}].\n\
         {repo_context}\n\
         Strict human gate enforced: require_human_merge={merge}, \
         forbid_autonomous_tag_push={tag}, forbid_autonomous_publish={publish}.\n\
         Protocol:\n\
         1. Inspect commits and merged PRs since the previous `v*` tag (`git describe --tags --abbrev=0 --match 'v*' origin/main`).\n\
         2. Create an isolated git worktree off `origin/main` (`chore/release-v<version>`).\n\
         3. Bump `[workspace.package].version` in `Cargo.toml`, update `Cargo.lock`, and draft the `CHANGELOG.md` release section.\n\
         4. Run `./scripts/check-rust-source-size.sh`, `just quality`, `cargo test --workspace`, `just release-check`, and `just standalone-distribution-test`.\n\
         5. Commit cleanly without `--no-verify`, push only the release-prep branch, and open a release PR with verification output.\n\
         6. STOP at the human gate: NEVER merge the PR, push any `v*` git tag, or publish release artifacts. Include post-release `alan update --check && alan update && alan --version` close-out instructions in the PR body.",
        merge = release_gate.require_human_merge,
        tag = release_gate.forbid_autonomous_tag_push,
        publish = release_gate.forbid_autonomous_publish,
    )
}

#[cfg(test)]
mod tests {
    use super::super::manifest::WorkflowStore;
    use super::super::runner::{self, build_dry_run_plan};
    use super::super::self_iterate::build_self_iterate_manifest;
    use super::super::tests::{sample_self_iterate_args, write_mock_script};
    use super::*;

    #[tokio::test]
    async fn release_gate_policy_fails_closed_when_any_flag_is_false_or_tampered() {
        let mut base = build_self_iterate_manifest(&sample_self_iterate_args()).unwrap();

        for (flag_name, mutator) in [
            (
                "require_human_merge",
                (|g: &mut ReleaseGatePolicy| g.require_human_merge = false)
                    as fn(&mut ReleaseGatePolicy),
            ),
            (
                "forbid_autonomous_tag_push",
                (|g: &mut ReleaseGatePolicy| g.forbid_autonomous_tag_push = false)
                    as fn(&mut ReleaseGatePolicy),
            ),
            (
                "forbid_autonomous_publish",
                (|g: &mut ReleaseGatePolicy| g.forbid_autonomous_publish = false)
                    as fn(&mut ReleaseGatePolicy),
            ),
        ] {
            let mut gate = ReleaseGatePolicy::enforced();
            mutator(&mut gate);
            assert!(gate.validate().is_err(), "{flag_name} must fail closed");
            base.release_gate = Some(gate.clone());
            assert!(base.validate().is_err(), "{flag_name} manifest must fail");
            assert!(
                ensure_release_operation_allowed(&gate, ReleaseOperation::OpenReleasePr).is_err(),
                "{flag_name} must block even OpenReleasePr when tampered"
            );
        }

        // Missing any required field inside `[release_gate]` in TOML must fail deserialization.
        let partial_toml = r#"
id = "release-prep"
schedule = "@weekly"
prompt = "Prepare release"
mounts = ["alan-repo"]

[release_gate]
require_human_merge = true
forbid_autonomous_tag_push = true
"#;
        assert!(toml::from_str::<WorkflowManifest>(partial_toml).is_err());

        // Tampered manifest on disk fails closed at load, dry-run, and execution boundaries.
        let temp = tempfile::tempdir().unwrap();
        let store = WorkflowStore::from_root(temp.path().join("workflows")).unwrap();
        let tampered_toml = r#"
schema_version = 1
id = "tampered-release"
schedule = "@weekly"
prompt = "Prepare release"
mounts = ["alan-repo"]
trust = "governed"
json = true

[release_gate]
require_human_merge = false
forbid_autonomous_tag_push = true
forbid_autonomous_publish = true
"#;
        std::fs::create_dir_all(store.manifests_dir()).unwrap();
        std::fs::write(
            store.manifest_path("tampered-release").unwrap(),
            tampered_toml,
        )
        .unwrap();
        assert!(store.load_manifest("tampered-release").is_err());

        let tampered_manifest: WorkflowManifest = toml::from_str(tampered_toml).unwrap();
        let mock_bin = write_mock_script(temp.path(), "mock-alan.sh", "#!/bin/sh\nexit 0\n");
        assert!(build_dry_run_plan(&tampered_manifest, &mock_bin).is_err());
        assert!(
            runner::execute_workflow_run(&store, &tampered_manifest, &mock_bin)
                .await
                .is_err()
        );

        // Enforced policy allows only OpenReleasePr and blocks merge, tag push, and publish.
        let enforced = ReleaseGatePolicy::enforced();
        assert!(
            ensure_release_operation_allowed(&enforced, ReleaseOperation::OpenReleasePr).is_ok()
        );
        for forbidden in [
            ReleaseOperation::MergeReleasePr,
            ReleaseOperation::PushReleaseTag,
            ReleaseOperation::PublishReleaseArtifacts,
        ] {
            assert!(ensure_release_operation_allowed(&enforced, forbidden).is_err());
        }
    }
}
