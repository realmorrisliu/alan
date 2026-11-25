//! Bounded one-shot execution of scheduled workflows over `alan -p --ephemeral --json`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alan_agent_engine::{
    CronStreamExecutionSpec, CronStreamPrepareResult, StreamLeaseGuard, WorkstreamRecord,
    WorkstreamState, check_per_run_budget, finalize_stateful_cron_stream_run,
    load_workstream_record, prepare_stateful_cron_stream_run,
};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::sync::Notify;

use super::logs::{WorkflowRunLogEntry, WorkflowRunUsage, append_run_log, scrub_and_bound_text};
use super::manifest::{
    ReleaseGatePolicy, TriggerAllowlist, WorkflowCronStreamSpec, WorkflowFileserverSpec,
    WorkflowManifest, WorkflowStore,
};

/// Environment variable overriding the `alan` executable invoked by `workflow run` / `render-schedule`.
pub const ALAN_WORKFLOW_BIN_ENV: &str = "ALAN_WORKFLOW_ALAN_BIN";

const MAX_CAPTURED_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const TIMEOUT_EXIT_CODE: i32 = 124;
const CANCELLED_EXIT_CODE: i32 = 130;
const PIPE_DRAIN_AFTER_KILL_TIMEOUT: Duration = Duration::from_secs(2);

/// Resolved dry-run execution plan emitted by `alan workflow run <id> --dry-run`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkflowDryRunPlan {
    pub workflow_id: String,
    pub binary: PathBuf,
    pub args: Vec<String>,
    pub trust: String,
    pub mounts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fileservers: Vec<WorkflowFileserverSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_key: Option<String>,
    pub timeout_secs: u64,
    pub ephemeral: bool,
    pub json_stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_allowlist: Option<TriggerAllowlist>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_gate: Option<ReleaseGatePolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron_stream: Option<WorkflowCronStreamSpec>,
}

/// Structured outcome produced by `alan workflow run <id>`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkflowRunOutcome {
    pub entry: WorkflowRunLogEntry,
    pub log_path: PathBuf,
}

/// Resolve the `alan` binary path (explicit override -> `ALAN_WORKFLOW_ALAN_BIN` -> `current_exe`).
pub fn resolve_alan_binary(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(env_bin) = std::env::var_os(ALAN_WORKFLOW_BIN_ENV).filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(env_bin));
    }
    std::env::current_exe().context("resolve current `alan` executable path")
}

/// Build the exact bounded invocation arguments for `alan -p [--ephemeral]`.
pub fn build_alan_args(manifest: &WorkflowManifest) -> Vec<String> {
    build_alan_args_with_prompt(manifest, None, None)
}

fn build_alan_args_with_prompt(
    manifest: &WorkflowManifest,
    prompt_override: Option<&str>,
    resume_rollout_ref: Option<&str>,
) -> Vec<String> {
    let prompt = prompt_override.unwrap_or(manifest.prompt.as_str());
    let mut args = vec!["-p".to_string(), prompt.to_string()];
    if manifest.cron_stream.is_none() {
        args.push("--ephemeral".to_string());
    } else if let Some(rollout) = resume_rollout_ref.map(str::trim).filter(|s| !s.is_empty()) {
        args.push("--resume".to_string());
        args.push(rollout.to_string());
    }
    if manifest.json {
        args.push("--json".to_string());
    }
    for flag in manifest.trust.cli_flags() {
        args.push((*flag).to_string());
    }
    for grant_id in &manifest.mounts {
        args.push("--grant".to_string());
        args.push(grant_id.clone());
    }
    for pkg_id in &manifest.packages {
        args.push("--package".to_string());
        args.push(pkg_id.clone());
    }
    for fs in &manifest.fileservers {
        args.push("--fileserver".to_string());
        args.push(crate::cli::trust::format_fileserver_cli_spec(
            &fs.package_id,
            fs.access,
            fs.allow_network,
        ));
    }
    args
}

/// Build a side-effect-free dry-run plan for `manifest`.
pub fn build_dry_run_plan(
    manifest: &WorkflowManifest,
    alan_bin: &Path,
) -> Result<WorkflowDryRunPlan> {
    manifest.validate()?;
    if let Some(gate) = &manifest.release_gate {
        super::release_prep::ensure_release_operation_allowed(
            gate,
            super::release_prep::ReleaseOperation::OpenReleasePr,
        )?;
    }
    Ok(WorkflowDryRunPlan {
        workflow_id: manifest.id.clone(),
        binary: alan_bin.to_path_buf(),
        args: build_alan_args(manifest),
        trust: manifest.trust.as_str().to_string(),
        mounts: manifest.mounts.clone(),
        packages: manifest.packages.clone(),
        fileservers: manifest.fileservers.clone(),
        model_profile: manifest.model_profile.clone(),
        project_key: manifest.project_key.clone(),
        timeout_secs: manifest.effective_timeout_secs(),
        ephemeral: manifest.cron_stream.is_none(),
        json_stream: manifest.json,
        trigger_allowlist: manifest.trigger_allowlist.clone(),
        release_gate: manifest.release_gate.clone(),
        cron_stream: manifest.cron_stream.clone(),
    })
}

/// Execute one bounded workflow run via `alan -p --ephemeral [--json]` and record its log.
pub async fn execute_workflow_run(
    store: &WorkflowStore,
    manifest: &WorkflowManifest,
    alan_bin: &Path,
) -> Result<WorkflowRunOutcome> {
    execute_workflow_run_with_prompt(store, manifest, alan_bin, None).await
}

/// Execute one bounded workflow run with an optional per-event prompt override.
pub async fn execute_workflow_run_with_prompt(
    store: &WorkflowStore,
    manifest: &WorkflowManifest,
    alan_bin: &Path,
    prompt_override: Option<&str>,
) -> Result<WorkflowRunOutcome> {
    execute_workflow_run_cancellable(store, manifest, alan_bin, prompt_override, None).await
}

/// Execute one bounded workflow run with an optional prompt override and cooperative cancellation.
pub async fn execute_workflow_run_cancellable(
    store: &WorkflowStore,
    manifest: &WorkflowManifest,
    alan_bin: &Path,
    prompt_override: Option<&str>,
    mut cancel: Option<&mut tokio::sync::watch::Receiver<bool>>,
) -> Result<WorkflowRunOutcome> {
    manifest.validate()?;
    if let Some(gate) = &manifest.release_gate {
        super::release_prep::ensure_release_operation_allowed(
            gate,
            super::release_prep::ReleaseOperation::OpenReleasePr,
        )?;
    }
    let run_id = format!("wf-{}", uuid::Uuid::new_v4().simple());
    let started_at = Utc::now().to_rfc3339();
    let timer = Instant::now();
    let log_path_preview = store.workflow_runs_dir(&manifest.id)?.join("runs.ndjson");

    struct ActiveCronRun {
        projects_root: PathBuf,
        spec: CronStreamExecutionSpec,
        lease: StreamLeaseGuard,
        record: WorkstreamRecord,
    }

    let cron_ctx = if let Some(cron_spec) = manifest.to_cron_stream_spec() {
        let projects_root = store
            .projects_root()
            .context("stateful cron_stream workflow requires System Store projects root")?;
        let prep = prepare_stateful_cron_stream_run(
            &projects_root,
            &cron_spec,
            &run_id,
            &log_path_preview.display().to_string(),
            || validate_live_cron_grants(store, manifest),
        )
        .await;
        match prep {
            Ok(CronStreamPrepareResult::Ready { lease, record, .. }) => Some(ActiveCronRun {
                projects_root,
                spec: cron_spec,
                lease,
                record,
            }),
            Ok(CronStreamPrepareResult::DeferredLockContention { inbox_entry }) => {
                let duration_ms = u64::try_from(timer.elapsed().as_millis()).unwrap_or(u64::MAX);
                let entry = WorkflowRunLogEntry {
                    run_id,
                    workflow_id: manifest.id.clone(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                    duration_ms,
                    exit_code: 0,
                    status: "deferred_lock_contention".to_string(),
                    summary: scrub_and_bound_text(&inbox_entry.summary, 4_000),
                    stderr_excerpt: String::new(),
                    ephemeral: false,
                    trust: manifest.trust.as_str().to_string(),
                    model_profile: manifest.model_profile.clone(),
                    tool_calls: 0,
                    usage: None,
                };
                return persist_log_only(store, manifest, entry);
            }
            Ok(CronStreamPrepareResult::SkippedPausedError { inbox_entry }) => {
                let duration_ms = u64::try_from(timer.elapsed().as_millis()).unwrap_or(u64::MAX);
                let entry = WorkflowRunLogEntry {
                    run_id,
                    workflow_id: manifest.id.clone(),
                    started_at,
                    finished_at: Utc::now().to_rfc3339(),
                    duration_ms,
                    exit_code: 1,
                    status: "paused_error".to_string(),
                    summary: scrub_and_bound_text(&inbox_entry.summary, 4_000),
                    stderr_excerpt: String::new(),
                    ephemeral: false,
                    trust: manifest.trust.as_str().to_string(),
                    model_profile: manifest.model_profile.clone(),
                    tool_calls: 0,
                    usage: None,
                };
                return persist_log_only(store, manifest, entry);
            }
            Err(err) => {
                let err_msg = format!("{err:#}");
                if err_msg.contains("grant revalidation failed") {
                    let is_paused = load_workstream_record(
                        &projects_root,
                        &cron_spec.project_key,
                        &cron_spec.stream_name,
                    )
                    .ok()
                    .flatten()
                    .is_some_and(|rec| rec.state == WorkstreamState::PausedError);
                    let duration_ms =
                        u64::try_from(timer.elapsed().as_millis()).unwrap_or(u64::MAX);
                    let entry = WorkflowRunLogEntry {
                        run_id,
                        workflow_id: manifest.id.clone(),
                        started_at,
                        finished_at: Utc::now().to_rfc3339(),
                        duration_ms,
                        exit_code: 1,
                        status: if is_paused {
                            "paused_error".to_string()
                        } else {
                            "grant_revalidation_failed".to_string()
                        },
                        summary: scrub_and_bound_text(&err_msg, 4_000),
                        stderr_excerpt: String::new(),
                        ephemeral: false,
                        trust: manifest.trust.as_str().to_string(),
                        model_profile: manifest.model_profile.clone(),
                        tool_calls: 0,
                        usage: None,
                    };
                    return persist_log_only(store, manifest, entry);
                }
                return Err(err);
            }
        }
    } else {
        None
    };

    if cancel.as_ref().is_some_and(|rx| *rx.borrow()) {
        let duration_ms = u64::try_from(timer.elapsed().as_millis()).unwrap_or(u64::MAX);
        let entry = WorkflowRunLogEntry {
            run_id: run_id.clone(),
            workflow_id: manifest.id.clone(),
            started_at,
            finished_at: Utc::now().to_rfc3339(),
            duration_ms,
            exit_code: CANCELLED_EXIT_CODE,
            status: "cancelled".to_string(),
            summary: scrub_and_bound_text(
                &format!("workflow `{}` cancelled by termination signal", manifest.id),
                4_000,
            ),
            stderr_excerpt: String::new(),
            ephemeral: cron_ctx.is_none(),
            trust: manifest.trust.as_str().to_string(),
            model_profile: manifest.model_profile.clone(),
            tool_calls: 0,
            usage: None,
        };
        if let Some(ctx) = cron_ctx {
            let outcome = finalize_stateful_cron_stream_run(
                &ctx.projects_root,
                &ctx.spec,
                ctx.record,
                ctx.lease,
                &run_id,
                &log_path_preview.display().to_string(),
                false,
                None,
                &entry.summary,
                None,
                None,
            )?;
            let mut finalized = entry;
            if outcome.transitioned_to_paused_error {
                finalized.status = "paused_error".to_string();
            }
            return persist_log_only(store, manifest, finalized);
        }
        return persist_outcome(store, manifest, entry);
    }

    let resume_ref = cron_ctx
        .as_ref()
        .and_then(|ctx| ctx.record.rollout_ref.as_deref());
    let args = build_alan_args_with_prompt(manifest, prompt_override, resume_ref);

    // Keep any temporary `connection_profile` agent.toml scoped strictly to this single run.
    let _temp_config_dir = prepare_model_profile_config(manifest.model_profile.as_deref())?;

    let mut command = tokio::process::Command::new(alan_bin);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    if let Some(ref config_guard) = _temp_config_dir {
        command.env("ALAN_CONFIG_PATH", &config_guard.config_path);
    }
    if let Some(ref ctx) = cron_ctx {
        command
            .env("ALAN_CRON_STREAM", &ctx.spec.stream_name)
            .env("ALAN_CRON_PROJECT_KEY", &ctx.spec.project_key);
        if let Some(ckpt) = ctx.record.last_checkpoint_ref.as_deref() {
            command.env("ALAN_CRON_CHECKPOINT", ckpt);
        }
        if let Some(budget) = ctx.spec.per_run_budget_usd {
            command.env("ALAN_CRON_PER_RUN_BUDGET_USD", format!("{budget}"));
        }
    }

    let mut child = command.spawn().with_context(|| {
        format!(
            "spawn scheduled workflow `{}` using `{}`",
            manifest.id,
            alan_bin.display()
        )
    })?;

    let budget_limit = cron_ctx
        .as_ref()
        .and_then(|ctx| ctx.spec.per_run_budget_usd);
    let budget_notify = budget_limit.map(|_| Arc::new(Notify::new()));

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_task = tokio::spawn(read_bounded_stdout_with_budget(
        stdout_pipe,
        MAX_CAPTURED_OUTPUT_BYTES,
        budget_limit,
        budget_notify.clone(),
    ));
    let stderr_task = tokio::spawn(read_bounded_stream(stderr_pipe, MAX_CAPTURED_OUTPUT_BYTES));

    let timeout = Duration::from_secs(manifest.effective_timeout_secs());
    let timeout_sleep = tokio::time::sleep(timeout);
    tokio::pin!(timeout_sleep);

    let wait_state = tokio::select! {
        biased;
        _ = wait_for_cancel(&mut cancel) => {
            terminate_and_reap(&mut child).await;
            ChildWaitState::Cancelled
        }
        _ = wait_for_budget(&budget_notify) => {
            terminate_and_reap(&mut child).await;
            ChildWaitState::BudgetExceeded
        }
        wait_result = child.wait() => {
            let status = wait_result.with_context(|| {
                format!("wait for scheduled workflow `{}` process", manifest.id)
            })?;
            ChildWaitState::Exited(status.code().unwrap_or(1))
        }
        _ = &mut timeout_sleep => {
            terminate_and_reap(&mut child).await;
            ChildWaitState::TimedOut
        }
    };

    let wait_for_eof = matches!(wait_state, ChildWaitState::Exited(_));
    let stdout_bytes = collect_stream_bytes(stdout_task, wait_for_eof).await;
    let stderr_bytes = collect_stream_bytes(stderr_task, wait_for_eof).await;
    let duration_ms = u64::try_from(timer.elapsed().as_millis()).unwrap_or(u64::MAX);
    let finished_at = Utc::now().to_rfc3339();

    let stdout_text = String::from_utf8_lossy(&stdout_bytes);
    let stderr_text = String::from_utf8_lossy(&stderr_bytes);
    let parsed = parse_run_output(&stdout_text, manifest.json);
    let observed_cost = parsed.usage.as_ref().and_then(|u| u.cost_usd);
    let post_budget_violation = check_per_run_budget(budget_limit, observed_cost);

    let (status, exit_code, summary) = match wait_state {
        ChildWaitState::Cancelled => (
            "cancelled".to_string(),
            CANCELLED_EXIT_CODE,
            format!("workflow `{}` cancelled by termination signal", manifest.id),
        ),
        ChildWaitState::BudgetExceeded => (
            "budget_exceeded".to_string(),
            1,
            post_budget_violation.unwrap_or_else(|| {
                format!(
                    "workflow `{}` exceeded per_run_budget_usd limit",
                    manifest.id
                )
            }),
        ),
        ChildWaitState::TimedOut => (
            "timed_out".to_string(),
            TIMEOUT_EXIT_CODE,
            format!(
                "workflow `{}` timed out after {}s",
                manifest.id,
                manifest.effective_timeout_secs()
            ),
        ),
        ChildWaitState::Exited(0) => {
            if let Some(violation) = post_budget_violation {
                ("budget_exceeded".to_string(), 1, violation)
            } else {
                let status = parsed.status.unwrap_or_else(|| "completed".to_string());
                let summary = parsed
                    .answer
                    .or(parsed.text_fallback)
                    .unwrap_or_else(|| "completed".to_string());
                (status, 0, summary)
            }
        }
        ChildWaitState::Exited(raw_exit_code) => {
            let status = if post_budget_violation.is_some() {
                "budget_exceeded".to_string()
            } else {
                parsed.status.unwrap_or_else(|| "failed".to_string())
            };
            let summary = post_budget_violation
                .or_else(|| parsed.errors.first().cloned())
                .or(parsed.answer)
                .or(parsed.text_fallback)
                .unwrap_or_else(|| format!("exited with status {raw_exit_code}"));
            (status, raw_exit_code, summary)
        }
    };

    let mut entry = WorkflowRunLogEntry {
        run_id: run_id.clone(),
        workflow_id: manifest.id.clone(),
        started_at,
        finished_at,
        duration_ms,
        exit_code,
        status,
        summary: scrub_and_bound_text(&summary, 4_000),
        stderr_excerpt: scrub_and_bound_text(&stderr_text, 2_000),
        ephemeral: cron_ctx.is_none(),
        trust: manifest.trust.as_str().to_string(),
        model_profile: manifest.model_profile.clone(),
        tool_calls: parsed.tool_calls,
        usage: parsed.usage,
    };

    if let Some(ctx) = cron_ctx {
        let outcome = finalize_stateful_cron_stream_run(
            &ctx.projects_root,
            &ctx.spec,
            ctx.record,
            ctx.lease,
            &run_id,
            &log_path_preview.display().to_string(),
            entry.exit_code == 0,
            observed_cost,
            &entry.summary,
            parsed.checkpoint_ref,
            parsed.rollout_id,
        )?;
        if outcome.transitioned_to_paused_error {
            entry.status = "paused_error".to_string();
        }
        persist_log_only(store, manifest, entry)
    } else {
        persist_outcome(store, manifest, entry)
    }
}

fn validate_live_cron_grants(store: &WorkflowStore, manifest: &WorkflowManifest) -> Result<()> {
    if manifest.mounts.is_empty() {
        return Ok(());
    }
    let grants_path = store
        .host_mount_grants_path()
        .context("stateful cron_stream requires host-mount grants store for live revalidation")?;
    let grant_store = alan_os_host::SavedProjectGrantStore::from_path(grants_path);
    for grant_id in &manifest.mounts {
        grant_store.revalidate_grant(grant_id)?;
    }
    Ok(())
}

enum ChildWaitState {
    Exited(i32),
    TimedOut,
    Cancelled,
    BudgetExceeded,
}

fn persist_log_only(
    store: &WorkflowStore,
    manifest: &WorkflowManifest,
    entry: WorkflowRunLogEntry,
) -> Result<WorkflowRunOutcome> {
    let log_path = append_run_log(store, &entry)?;
    let persisted_entry = super::logs::latest_run_log(store, &manifest.id)?.unwrap_or(entry);
    Ok(WorkflowRunOutcome {
        entry: persisted_entry,
        log_path,
    })
}

fn persist_outcome(
    store: &WorkflowStore,
    manifest: &WorkflowManifest,
    entry: WorkflowRunLogEntry,
) -> Result<WorkflowRunOutcome> {
    let log_path = append_run_log(store, &entry)?;
    let persisted_entry = super::logs::latest_run_log(store, &manifest.id)?.unwrap_or(entry);
    if let Some(project_key) = manifest.project_key.as_deref()
        && let Some(projects_root) = store.projects_root()
    {
        let digest = build_inbox_digest(&persisted_entry, &log_path);
        alan_agent_engine::write_inbox_digest(&projects_root, project_key, digest)
            .with_context(|| format!("write workflow inbox digest for project `{project_key}`"))?;
    }
    Ok(WorkflowRunOutcome {
        entry: persisted_entry,
        log_path,
    })
}

fn build_inbox_digest(
    entry: &WorkflowRunLogEntry,
    log_path: &Path,
) -> alan_agent_engine::InboxDigestEntry {
    let status = if entry.exit_code == 0 {
        if entry.status.eq_ignore_ascii_case("warning") {
            alan_agent_engine::InboxDigestStatus::Warning
        } else {
            alan_agent_engine::InboxDigestStatus::Ok
        }
    } else if entry.status.eq_ignore_ascii_case("paused_error") {
        alan_agent_engine::InboxDigestStatus::PausedError
    } else {
        alan_agent_engine::InboxDigestStatus::Warning
    };

    let raw_summary = if entry.exit_code == 0 {
        entry.summary.clone()
    } else if entry.stderr_excerpt.trim().is_empty() {
        format!("[{}] {}", entry.status, entry.summary)
    } else {
        format!(
            "[{}] {} ({})",
            entry.status, entry.summary, entry.stderr_excerpt
        )
    };
    let scrubbed_summary = scrub_and_bound_text(
        &raw_summary,
        alan_agent_engine::runtime::MAX_INBOX_SUMMARY_CHARS,
    );

    let mut digest = alan_agent_engine::InboxDigestEntry::new(
        entry.run_id.clone(),
        entry.workflow_id.clone(),
        None,
        status,
        scrubbed_summary,
        log_path.display().to_string(),
    );
    digest.created_at = entry.finished_at.clone();
    digest
}

async fn wait_for_cancel(cancel: &mut Option<&mut tokio::sync::watch::Receiver<bool>>) {
    let Some(rx) = cancel.as_deref_mut() else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        if *rx.borrow_and_update() {
            return;
        }
        if rx.changed().await.is_err() {
            std::future::pending::<()>().await;
            return;
        }
    }
}

async fn terminate_and_reap(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id()
        && let Ok(pgid) = libc::pid_t::try_from(pid)
        && pgid > 0
    {
        // SAFETY: Sending SIGKILL to `-pgid` targets only the child process group
        // created via `command.process_group(0)` while `child` has not yet been reaped.
        unsafe {
            let _ = libc::kill(-pgid, libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

async fn collect_stream_bytes(
    mut task: tokio::task::JoinHandle<Vec<u8>>,
    wait_for_eof: bool,
) -> Vec<u8> {
    if wait_for_eof {
        return task.await.unwrap_or_default();
    }
    match tokio::time::timeout(PIPE_DRAIN_AFTER_KILL_TIMEOUT, &mut task).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => Vec::new(),
        Err(_) => {
            task.abort();
            Vec::new()
        }
    }
}

struct TempWorkflowConfigGuard {
    dir: PathBuf,
    config_path: PathBuf,
}

impl Drop for TempWorkflowConfigGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn prepare_model_profile_config(
    model_profile: Option<&str>,
) -> Result<Option<TempWorkflowConfigGuard>> {
    let Some(profile) = model_profile.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let dir = std::env::temp_dir().join(format!("alan-wf-cfg-{}", uuid::Uuid::new_v4().simple()));
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&dir)
        .context("create temporary workflow config directory")?;
    let path = dir.join("agent.toml");
    let guard = TempWorkflowConfigGuard {
        dir,
        config_path: path.clone(),
    };
    let mut table = if let Some(base_path) = std::env::var_os("ALAN_CONFIG_PATH")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        && base_path.is_file()
    {
        let raw = std::fs::read_to_string(&base_path)
            .with_context(|| format!("read base ALAN_CONFIG_PATH {}", base_path.display()))?;
        toml::from_str::<toml::Table>(&raw)
            .with_context(|| format!("parse base ALAN_CONFIG_PATH {}", base_path.display()))?
    } else {
        toml::Table::new()
    };
    table.insert(
        "connection_profile".to_string(),
        toml::Value::String(profile.to_string()),
    );
    let encoded = toml::to_string(&table).context("encode workflow ALAN_CONFIG_PATH override")?;
    std::fs::write(&path, encoded)
        .with_context(|| format!("write temporary workflow config {}", path.display()))?;
    Ok(Some(guard))
}

struct ParsedRunOutput {
    status: Option<String>,
    answer: Option<String>,
    text_fallback: Option<String>,
    errors: Vec<String>,
    tool_calls: u64,
    usage: Option<WorkflowRunUsage>,
    rollout_id: Option<String>,
    checkpoint_ref: Option<String>,
}

fn parse_run_output(stdout: &str, expect_json: bool) -> ParsedRunOutput {
    let mut parsed = ParsedRunOutput {
        status: None,
        answer: None,
        text_fallback: None,
        errors: Vec::new(),
        tool_calls: 0,
        usage: None,
        rollout_id: None,
        checkpoint_ref: None,
    };
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return parsed;
    }

    if expect_json {
        let mut text_deltas = String::new();
        let mut saw_json = false;
        for line in trimmed.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let Some(kind) = value.get("type").and_then(Value::as_str) else {
                continue;
            };
            saw_json = true;
            match kind {
                "text_delta" => {
                    if let Some(text) = value.get("text").and_then(Value::as_str) {
                        text_deltas.push_str(text);
                    }
                }
                "tool_start" => {
                    parsed.tool_calls += 1;
                }
                "usage" => {
                    if let (Some(input_tokens), Some(output_tokens), Some(context_window)) = (
                        value.get("input_tokens").and_then(Value::as_u64),
                        value.get("output_tokens").and_then(Value::as_u64),
                        value.get("context_window").and_then(Value::as_u64),
                    ) {
                        parsed.usage = Some(WorkflowRunUsage {
                            input_tokens,
                            output_tokens,
                            cached_input_tokens: value
                                .get("cached_input_tokens")
                                .and_then(Value::as_u64),
                            reasoning_tokens: value.get("reasoning_tokens").and_then(Value::as_u64),
                            context_window,
                            cost_usd: value.get("cost_usd").and_then(Value::as_f64),
                        });
                    }
                }
                "session_started" | "checkpoint" => {
                    if let Some(rollout) = value.get("rollout_id").and_then(Value::as_str)
                        && !rollout.trim().is_empty()
                    {
                        parsed.rollout_id = Some(rollout.to_string());
                    }
                    if let Some(ckpt) = value.get("checkpoint_ref").and_then(Value::as_str)
                        && !ckpt.trim().is_empty()
                    {
                        parsed.checkpoint_ref = Some(ckpt.to_string());
                    }
                }
                "turn_finished" => {
                    if let Some(status) = value.get("status").and_then(Value::as_str) {
                        parsed.status = Some(status.to_string());
                    }
                    if let Some(answer) = value.get("answer").and_then(Value::as_str)
                        && !answer.trim().is_empty()
                    {
                        parsed.answer = Some(answer.to_string());
                    }
                    if let Some(rollout) = value.get("rollout_id").and_then(Value::as_str)
                        && !rollout.trim().is_empty()
                    {
                        parsed.rollout_id = Some(rollout.to_string());
                    }
                    if let Some(ckpt) = value.get("checkpoint_ref").and_then(Value::as_str)
                        && !ckpt.trim().is_empty()
                    {
                        parsed.checkpoint_ref = Some(ckpt.to_string());
                    }
                }
                "error" => {
                    if let Some(msg) = value.get("message").and_then(Value::as_str)
                        && !msg.trim().is_empty()
                    {
                        parsed.errors.push(msg.to_string());
                    }
                }
                _ => {}
            }
        }
        if !text_deltas.trim().is_empty() {
            parsed.text_fallback = Some(text_deltas);
        } else if !saw_json {
            parsed.text_fallback = Some(trimmed.to_string());
        }
    } else {
        parsed.text_fallback = Some(trimmed.to_string());
    }

    parsed
}

async fn wait_for_budget(notify: &Option<Arc<Notify>>) {
    let Some(notify) = notify.as_ref() else {
        std::future::pending::<()>().await;
        return;
    };
    notify.notified().await;
}

async fn read_bounded_stdout_with_budget<R: tokio::io::AsyncRead + Unpin>(
    pipe: Option<R>,
    limit: usize,
    budget_limit_usd: Option<f64>,
    budget_notify: Option<Arc<Notify>>,
) -> Vec<u8> {
    let Some(mut reader) = pipe else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut line_buf = Vec::new();
    let mut buf = [0u8; 4_096];
    let mut triggered = false;
    loop {
        match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.len() < limit {
                    let take = (limit - out.len()).min(n);
                    out.extend_from_slice(&buf[..take]);
                }
                if !triggered
                    && let (Some(limit_usd), Some(notify)) =
                        (budget_limit_usd, budget_notify.as_ref())
                {
                    line_buf.extend_from_slice(&buf[..n]);
                    while let Some(pos) = line_buf.iter().position(|b| *b == b'\n') {
                        let line_bytes: Vec<u8> = line_buf.drain(..=pos).collect();
                        if let Ok(text) = std::str::from_utf8(&line_bytes)
                            && let Ok(val) = serde_json::from_str::<Value>(text.trim())
                            && val.get("type").and_then(Value::as_str) == Some("usage")
                            && let Some(cost) = val.get("cost_usd").and_then(Value::as_f64)
                            && check_per_run_budget(Some(limit_usd), Some(cost)).is_some()
                        {
                            triggered = true;
                            notify.notify_waiters();
                            break;
                        }
                    }
                }
            }
        }
    }
    out
}

async fn read_bounded_stream<R: tokio::io::AsyncRead + Unpin>(
    pipe: Option<R>,
    limit: usize,
) -> Vec<u8> {
    let Some(mut reader) = pipe else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut buf = [0u8; 4_096];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.len() < limit {
                    let take = (limit - out.len()).min(n);
                    out.extend_from_slice(&buf[..take]);
                }
            }
        }
    }
    out
}
