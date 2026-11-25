//! Scheduled personal workflows (`alan workflow init|init-self-iterate|init-release-prep|evaluate-trigger|check-release-gate|list|run|render-schedule|logs`)
//! executed via bounded, ephemeral `alan -p --ephemeral --json` invocations.

#[cfg(test)]
mod cron_stream_tests;
mod draft;
#[cfg(test)]
mod inbox_tests;
mod logs;
mod manifest;
mod release_prep;
mod runner;
mod schedule;
mod self_iterate;
#[cfg(test)]
mod tests;
mod watch;

use std::path::Path;

use alan_os_host::SystemStorePaths;
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use serde::Serialize;

#[allow(
    unused_imports,
    reason = "re-exported for alan library callers; main.rs includes cli privately"
)]
pub use draft::{
    CrontabInstaller, ScheduleInstallOutcome, SystemCrontabInstaller, WorkflowDraftRequest,
    WorkflowDraftSummary, draft_workflow_from_natural_language,
    install_drafted_workflow_with_confirmation,
};
use logs::{WorkflowRunLogEntry, read_run_logs};
pub(crate) use manifest::validate_mount_grant_id;
pub use manifest::{
    ReleaseGatePolicy, TriggerAllowlist, WorkflowManifest, WorkflowStore, WorkflowTrustPosture,
};
#[allow(
    unused_imports,
    reason = "re-exported for alan library callers; main.rs includes cli privately"
)]
pub use manifest::{WorkflowCronStreamSpec, WorkflowFileserverSpec};
use manifest::{normalize_model_profile, parse_workflow_package_inputs, validate_workflow_id};
use release_prep::run_check_release_gate;
#[allow(
    unused_imports,
    reason = "re-exported for alan library callers; main.rs includes cli privately"
)]
pub use release_prep::{
    CheckReleaseGateArgs, InitReleasePrepArgs, ReleaseOperation, build_release_prep_manifest,
    ensure_release_operation_allowed,
};
use runner::{build_dry_run_plan, execute_workflow_run, resolve_alan_binary};
pub use schedule::ScheduleTarget;
use schedule::{install_launchd_plist, launchd_label, render_cron_entry, render_launchd_plist};
use self_iterate::run_evaluate_trigger;
#[allow(
    unused_imports,
    reason = "re-exported for alan library callers; main.rs includes cli privately"
)]
pub use self_iterate::{
    AcceptedTriggerDispatch, EvaluateTriggerArgs, InitSelfIterateArgs, TriggerDecision,
    TriggerEvent, TriggerEventKind, TriggerGuardState, TriggerRejectionReason,
    build_self_iterate_manifest, evaluate_trigger_event,
};
#[allow(
    unused_imports,
    reason = "re-exported for alan library callers; main.rs includes cli privately"
)]
pub use watch::{WatchWorkflowArgs, run_watch_with_stores, run_watch_with_stores_and_shutdown};

/// Top-level arguments for `alan workflow`.
#[derive(Debug, Args)]
pub struct WorkflowArgs {
    /// Workflow subcommand to run.
    #[command(subcommand)]
    pub action: WorkflowAction,
}

/// Supported scheduled personal workflow subcommands.
#[derive(Debug, Subcommand)]
pub enum WorkflowAction {
    /// Register a declarative workflow manifest in System Store (`services/workflows`)
    Init {
        /// Canonical workflow identifier (`[a-z0-9][a-z0-9_-]*`)
        id: String,
        /// Schedule expression: 5-field cron (`"0 9 * * 1-5"`) or interval (`"every 30m"`, `"1h"`)
        #[arg(long)]
        schedule: String,
        /// Task prompt executed on each scheduled run
        #[arg(long)]
        prompt: String,
        /// Optional Connection Service profile id (`connection_profile`)
        #[arg(long = "model-profile", alias = "profile")]
        model_profile: Option<String>,
        /// Optional bound System Store project key for writing `inbox/` run digests
        #[arg(long = "project-key")]
        project_key: Option<String>,
        /// Explicit saved Host Mount grant id (`alan host grant save <id> <path>`)
        #[arg(long = "mount", alias = "grant", value_name = "GRANT_ID")]
        mounts: Vec<String>,
        /// Explicit Quartermaster package id enabled only for this workflow's isolated run (repeatable)
        #[arg(long = "package", value_name = "PACKAGE_ID")]
        packages: Vec<String>,
        /// Explicit Quartermaster fileserver activation (`<id>[:ro|rw][:net]`, repeatable)
        #[arg(long = "fileserver", value_name = "PACKAGE_ID[:ro|rw][:net]")]
        fileservers: Vec<String>,
        /// Explicit invocation trust posture (defaults to `governed`)
        #[arg(long, value_enum, default_value_t = WorkflowTrustPosture::Governed)]
        trust: WorkflowTrustPosture,
        /// Disable `--json` event streaming on spawned runs (`json = false` in manifest)
        #[arg(long = "no-stream-json", default_value_t = false)]
        no_stream_json: bool,
        /// Hard per-run timeout in seconds (defaults to 300)
        #[arg(long = "timeout-secs")]
        timeout_secs: Option<u64>,
        /// Overwrite an existing workflow manifest with the same id
        #[arg(long)]
        force: bool,
        /// Emit structured JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
    /// Register a bounded self-iteration workflow with control-plane actor/label allowlists
    InitSelfIterate(InitSelfIterateArgs),
    /// Register a bounded, strictly human-gated release-preparation workflow
    InitReleasePrep(InitReleasePrepArgs),
    /// Evaluate an external GitHub trigger event JSON payload against a workflow's allowlist
    EvaluateTrigger(EvaluateTriggerArgs),
    /// Verify whether a release operation is allowed under a workflow's `release_gate` policy
    CheckReleaseGate(CheckReleaseGateArgs),
    /// List registered workflows from System Store
    List {
        /// Emit structured JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Execute one bounded workflow run via `alan -p --ephemeral --json`
    Run {
        /// Registered workflow identifier
        id: String,
        /// Validate and print the resolved command plan without executing
        #[arg(long)]
        dry_run: bool,
        /// Emit structured JSON instead of a plain summary
        #[arg(long)]
        json: bool,
    },
    /// Render a native scheduler entry (`launchd` `.plist` or `crontab` line)
    RenderSchedule {
        /// Registered workflow identifier
        id: String,
        /// Target scheduler format
        #[arg(long, value_enum)]
        target: ScheduleTarget,
        /// Write the generated `launchd` `.plist` into `~/Library/LaunchAgents`
        #[arg(long)]
        install: bool,
    },
    /// Inspect bounded per-run NDJSON execution logs for a workflow
    Logs {
        /// Registered workflow identifier
        id: String,
        /// Maximum number of recent log entries to show (up to 200)
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Emit structured JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
    /// Watch allowlisted GitHub events via confined `github-fileserver` + `routefs` and dispatch bounded runs
    Watch(WatchWorkflowArgs),
    /// Draft a scheduled workflow from natural language and require explicit confirmation before install
    Draft {
        /// Natural-language workflow description
        description: String,
        /// Optional workflow id override (slugified from description by default)
        #[arg(long)]
        id: Option<String>,
        /// Bound System Store project key (required when binding a named cron stream)
        #[arg(long = "project-key")]
        project_key: Option<String>,
        /// Explicit saved Host Mount grant id (repeatable)
        #[arg(long = "mount", alias = "grant", value_name = "GRANT_ID")]
        mounts: Vec<String>,
        /// Optional Stateful Cron Stream identifier
        #[arg(long = "stream")]
        stream_name: Option<String>,
        /// Target native scheduler format (`launchd` or `cron`)
        #[arg(long, value_enum, default_value_t = ScheduleTarget::Launchd)]
        target: ScheduleTarget,
        /// Explicitly confirm scheduler installation (required with `--confirm-digest`)
        #[arg(long)]
        confirm: bool,
        /// SHA-256 confirmation digest printed during draft review
        #[arg(long = "confirm-digest")]
        confirm_digest: Option<String>,
        /// Overwrite an existing workflow manifest with the same id
        #[arg(long)]
        force: bool,
        /// Emit structured JSON instead of human review text
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
struct WorkflowListItem {
    #[serde(flatten)]
    manifest: WorkflowManifest,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_run: Option<WorkflowRunLogEntry>,
}

#[derive(Debug, Serialize)]
struct WorkflowListReport {
    workflows: Vec<WorkflowListItem>,
}

#[derive(Debug, Serialize)]
struct WorkflowLogsReport {
    workflow_id: String,
    entries: Vec<WorkflowRunLogEntry>,
}

/// Entrypoint for `alan workflow ...` against the current System Store.
pub async fn run(args: WorkflowArgs) -> Result<()> {
    alan_os_host::installation::validate_current_invocation()?;
    let _installation_access = alan_os_host::installation::InstallationPaths::detect()?.access()?;
    let system = SystemStorePaths::detect()?;
    let store = WorkflowStore::from_system_store(&system)?;
    if let WorkflowAction::Watch(watch_args) = args.action {
        let packages = std::sync::Arc::new(alan_service_manager::PackageService::open(
            system.packages()?,
        )?);
        let (output, failed_msg) =
            watch::run_watch_with_stores(&store, &packages, watch_args, None).await?;
        print!("{output}");
        if let Some(error) = failed_msg {
            bail!("{error}");
        }
        return Ok(());
    }
    let home_dir = dirs::home_dir();
    let (output, failed_msg) = run_with_store(&store, args, home_dir.as_deref(), None).await?;
    print!("{output}");
    if let Some(error) = failed_msg {
        bail!("{error}");
    }
    Ok(())
}

/// Execute `WorkflowArgs` against an explicit [`WorkflowStore`] (used by CLI and tests).
///
/// Returns `(formatted_stdout, optional_failure_message)` so callers can print structured JSON
/// summaries before returning a non-zero exit status on failed runs.
pub async fn run_with_store(
    store: &WorkflowStore,
    args: WorkflowArgs,
    home_dir: Option<&Path>,
    alan_bin_override: Option<&Path>,
) -> Result<(String, Option<String>)> {
    match args.action {
        WorkflowAction::Init {
            id,
            schedule,
            prompt,
            model_profile,
            project_key,
            mounts,
            packages,
            fileservers,
            trust,
            no_stream_json,
            timeout_secs,
            force,
            json,
        } => {
            let (packages, fileservers) = parse_workflow_package_inputs(&packages, &fileservers)?;
            let manifest = WorkflowManifest {
                schema_version: manifest::SUPPORTED_MANIFEST_VERSION,
                id,
                schedule: schedule.trim().to_string(),
                prompt,
                model_profile: normalize_model_profile(model_profile),
                project_key: project_key.map(|key| key.trim().to_string()),
                mounts,
                packages,
                fileservers,
                trust,
                json: !no_stream_json,
                timeout_secs,
                trigger_allowlist: None,
                release_gate: None,
                cron_stream: None,
            };
            save_and_format_init_manifest(store, &manifest, force, json)
        }
        WorkflowAction::InitSelfIterate(init_args) => {
            let manifest = build_self_iterate_manifest(&init_args)?;
            save_and_format_init_manifest(store, &manifest, init_args.force, init_args.json)
        }
        WorkflowAction::InitReleasePrep(init_args) => {
            let manifest = build_release_prep_manifest(&init_args)?;
            save_and_format_init_manifest(store, &manifest, init_args.force, init_args.json)
        }
        WorkflowAction::EvaluateTrigger(eval_args) => run_evaluate_trigger(store, eval_args),
        WorkflowAction::CheckReleaseGate(gate_args) => run_check_release_gate(store, gate_args),
        WorkflowAction::List { json } => {
            let manifests = store.list_manifests()?;
            let mut items = Vec::with_capacity(manifests.len());
            for m in manifests {
                let last_run = logs::latest_run_log(store, &m.id)?;
                items.push(WorkflowListItem {
                    manifest: m,
                    last_run,
                });
            }
            if json {
                let out = format!(
                    "{}\n",
                    serde_json::to_string_pretty(&WorkflowListReport { workflows: items })?
                );
                return Ok((out, None));
            }
            if items.is_empty() {
                return Ok(("No workflows registered.\n".to_string(), None));
            }
            let mut lines = vec![format!(
                "{:<22} {:<18} {:<16} {:<16} LAST RUN",
                "ID", "SCHEDULE", "TRUST", "MOUNTS"
            )];
            for item in &items {
                let mounts = if item.manifest.mounts.is_empty() {
                    "-".to_string()
                } else {
                    item.manifest.mounts.join(",")
                };
                let last = match &item.last_run {
                    Some(entry) => format!(
                        "{} (exit {}, {}ms)",
                        entry.status, entry.exit_code, entry.duration_ms
                    ),
                    None => "never".to_string(),
                };
                lines.push(format!(
                    "{:<22} {:<18} {:<16} {:<16} {}",
                    item.manifest.id,
                    item.manifest.schedule,
                    item.manifest.trust.as_str(),
                    mounts,
                    last
                ));
            }
            Ok((format!("{}\n", lines.join("\n")), None))
        }
        WorkflowAction::Run { id, dry_run, json } => {
            let manifest = store.load_manifest(&id)?;
            let alan_bin = resolve_alan_binary(alan_bin_override)?;
            if dry_run {
                let plan = build_dry_run_plan(&manifest, &alan_bin)?;
                let out = if json {
                    format!("{}\n", serde_json::to_string_pretty(&plan)?)
                } else {
                    let mut text = format!(
                        "dry-run workflow `{}`\n  binary: {}\n  args: {}\n  trust: {}\n  mounts: {}\n  model_profile: {}\n  timeout_secs: {}\n",
                        plan.workflow_id,
                        plan.binary.display(),
                        plan.args.join(" "),
                        plan.trust,
                        if plan.mounts.is_empty() {
                            "none".to_string()
                        } else {
                            plan.mounts.join(", ")
                        },
                        plan.model_profile.as_deref().unwrap_or("default"),
                        plan.timeout_secs
                    );
                    if let Some(allowlist) = &plan.trigger_allowlist {
                        text.push_str(&format!(
                            "  trigger_repo: {} (actors: {}, labels: {})\n",
                            allowlist.repo,
                            allowlist.allowed_actors.join(", "),
                            allowlist.required_labels.join(", ")
                        ));
                    }
                    if let Some(gate) = &plan.release_gate {
                        text.push_str(&format!(
                            "  release_gate: human_merge={} forbid_tag_push={} forbid_publish={}\n",
                            gate.require_human_merge,
                            gate.forbid_autonomous_tag_push,
                            gate.forbid_autonomous_publish
                        ));
                    }
                    text
                };
                return Ok((out, None));
            }

            let outcome = execute_workflow_run(store, &manifest, &alan_bin).await?;
            let out = if json {
                format!("{}\n", serde_json::to_string_pretty(&outcome)?)
            } else {
                format!(
                    "workflow `{}` finished: status={} exit_code={} duration={}ms\nsummary: {}\nlog: {}\n",
                    outcome.entry.workflow_id,
                    outcome.entry.status,
                    outcome.entry.exit_code,
                    outcome.entry.duration_ms,
                    outcome.entry.summary,
                    outcome.log_path.display()
                )
            };
            let failure = if outcome.entry.exit_code == 0 {
                None
            } else {
                Some(format!(
                    "workflow `{}` exited with status {} ({})",
                    outcome.entry.workflow_id, outcome.entry.exit_code, outcome.entry.summary
                ))
            };
            Ok((out, failure))
        }
        WorkflowAction::RenderSchedule {
            id,
            target,
            install,
        } => {
            let manifest = store.load_manifest(&id)?;
            let alan_bin = resolve_alan_binary(alan_bin_override)?;
            match target {
                ScheduleTarget::Launchd => {
                    let plist = render_launchd_plist(&manifest, &alan_bin, store)?;
                    if install {
                        let home = home_dir
                            .context("cannot determine home directory for LaunchAgents install")?;
                        let installed = install_launchd_plist(&manifest.id, &plist, home)?;
                        let label = launchd_label(&manifest.id);
                        let out = format!(
                            "installed launchd plist `{label}` at {}\nload with: launchctl bootstrap gui/$(id -u) {}\n",
                            installed.display(),
                            installed.display()
                        );
                        Ok((out, None))
                    } else {
                        Ok((plist, None))
                    }
                }
                ScheduleTarget::Cron => {
                    ensure!(
                        !install,
                        "`--install` is only supported with `--target launchd`; add the rendered line via `crontab -e`"
                    );
                    let entry = render_cron_entry(&manifest, &alan_bin)?;
                    Ok((entry, None))
                }
            }
        }
        WorkflowAction::Logs { id, limit, json } => {
            // Verify manifest id validity even if manifest was later removed, so historical logs remain inspectable.
            validate_workflow_id(&id)?;
            ensure!(limit > 0, "`--limit` must be at least 1");
            let entries = read_run_logs(store, &id, limit)?;
            if json {
                let out = format!(
                    "{}\n",
                    serde_json::to_string_pretty(&WorkflowLogsReport {
                        workflow_id: id,
                        entries,
                    })?
                );
                return Ok((out, None));
            }
            if entries.is_empty() {
                return Ok((format!("No runs recorded for workflow `{id}`.\n"), None));
            }
            let mut out = String::new();
            for entry in &entries {
                out.push_str(&format!(
                    "{}  {}  status={}  exit={}  {}ms  {}\n",
                    entry.started_at,
                    entry.run_id,
                    entry.status,
                    entry.exit_code,
                    entry.duration_ms,
                    entry.summary
                ));
            }
            Ok((out, None))
        }
        WorkflowAction::Watch(watch_args) => {
            let packages = std::sync::Arc::new(alan_service_manager::PackageService::open(
                SystemStorePaths::detect()?.packages()?,
            )?);
            watch::run_watch_with_stores(store, &packages, watch_args, alan_bin_override).await
        }
        WorkflowAction::Draft {
            description,
            id,
            project_key,
            mounts,
            stream_name,
            target,
            confirm,
            confirm_digest,
            force,
            json,
        } => {
            let alan_bin = resolve_alan_binary(alan_bin_override)?;
            let summary = draft_workflow_from_natural_language(
                &WorkflowDraftRequest {
                    description,
                    id,
                    project_key,
                    mounts,
                    stream_name,
                },
                store,
                &alan_bin,
                target,
            )?;
            if confirm || confirm_digest.is_some() {
                let outcome = install_drafted_workflow_with_confirmation(
                    &summary,
                    confirm,
                    confirm_digest.as_deref(),
                    store,
                    home_dir,
                    &SystemCrontabInstaller,
                    force,
                )?;
                let out = if json {
                    format!("{}\n", serde_json::to_string_pretty(&outcome)?)
                } else {
                    format!(
                        "confirmed and installed workflow `{}` via {:?} at {}\n",
                        outcome.workflow_id, outcome.scheduler_target, outcome.installed_location
                    )
                };
                Ok((out, None))
            } else if json {
                Ok((
                    format!("{}\n", serde_json::to_string_pretty(&summary)?),
                    None,
                ))
            } else {
                Ok((format!("{}\n", summary.human_review_prompt), None))
            }
        }
    }
}

fn save_and_format_init_manifest(
    store: &WorkflowStore,
    manifest: &WorkflowManifest,
    force: bool,
    json: bool,
) -> Result<(String, Option<String>)> {
    let path = store.save_manifest(manifest, force)?;
    let out = if json {
        format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({
                "manifest": manifest,
                "path": path,
            }))?
        )
    } else {
        let mut text = format!(
            "initialized workflow `{}` at {}\n  schedule: {}\n  trust: {}\n  mounts: {}\n",
            manifest.id,
            path.display(),
            manifest.schedule,
            manifest.trust.as_str(),
            if manifest.mounts.is_empty() {
                "none".to_string()
            } else {
                manifest.mounts.join(", ")
            }
        );
        if let Some(allowlist) = &manifest.trigger_allowlist {
            text.push_str(&format_allowlist_line(allowlist));
        }
        if let Some(gate) = &manifest.release_gate {
            text.push_str(&format_release_gate_line(gate));
        }
        text
    };
    Ok((out, None))
}

fn format_allowlist_line(allowlist: &TriggerAllowlist) -> String {
    format!(
        "  trigger_repo: {} (actors: {}, labels: {}, max_retries: {}, max_concurrent: {})\n",
        allowlist.repo,
        allowlist.allowed_actors.join(", "),
        allowlist.required_labels.join(", "),
        allowlist.max_retries,
        allowlist.max_concurrent
    )
}

fn format_release_gate_line(gate: &ReleaseGatePolicy) -> String {
    format!(
        "  release_gate: human_merge={} forbid_tag_push={} forbid_publish={}\n",
        gate.require_human_merge, gate.forbid_autonomous_tag_push, gate.forbid_autonomous_publish
    )
}
