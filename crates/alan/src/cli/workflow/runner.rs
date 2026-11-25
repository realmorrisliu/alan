//! Bounded one-shot execution of scheduled workflows over `alan -p --ephemeral --json`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use tokio::io::AsyncReadExt;

use super::logs::{WorkflowRunLogEntry, WorkflowRunUsage, append_run_log, scrub_and_bound_text};
use super::manifest::{
    ReleaseGatePolicy, TriggerAllowlist, WorkflowFileserverSpec, WorkflowManifest, WorkflowStore,
};

/// Environment variable overriding the `alan` executable invoked by `workflow run` / `render-schedule`.
pub const ALAN_WORKFLOW_BIN_ENV: &str = "ALAN_WORKFLOW_ALAN_BIN";

const MAX_CAPTURED_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const TIMEOUT_EXIT_CODE: i32 = 124;
const CANCELLED_EXIT_CODE: i32 = 130;
const PIPE_DRAIN_AFTER_KILL_TIMEOUT: Duration = Duration::from_secs(2);

/// Resolved dry-run execution plan emitted by `alan workflow run <id> --dry-run`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// Build the exact bounded invocation arguments for `alan -p --ephemeral`.
pub fn build_alan_args(manifest: &WorkflowManifest) -> Vec<String> {
    build_alan_args_with_prompt(manifest, None)
}

fn build_alan_args_with_prompt(
    manifest: &WorkflowManifest,
    prompt_override: Option<&str>,
) -> Vec<String> {
    let prompt = prompt_override.unwrap_or(manifest.prompt.as_str());
    let mut args = vec![
        "-p".to_string(),
        prompt.to_string(),
        "--ephemeral".to_string(),
    ];
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
        ephemeral: true,
        json_stream: manifest.json,
        trigger_allowlist: manifest.trigger_allowlist.clone(),
        release_gate: manifest.release_gate.clone(),
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

    if cancel.as_ref().is_some_and(|rx| *rx.borrow()) {
        let duration_ms = u64::try_from(timer.elapsed().as_millis()).unwrap_or(u64::MAX);
        let entry = WorkflowRunLogEntry {
            run_id,
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
            ephemeral: true,
            trust: manifest.trust.as_str().to_string(),
            model_profile: manifest.model_profile.clone(),
            tool_calls: 0,
            usage: None,
        };
        return persist_outcome(store, manifest, entry);
    }

    let args = build_alan_args_with_prompt(manifest, prompt_override);

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

    let mut child = command.spawn().with_context(|| {
        format!(
            "spawn scheduled workflow `{}` using `{}`",
            manifest.id,
            alan_bin.display()
        )
    })?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_task = tokio::spawn(read_bounded_stream(stdout_pipe, MAX_CAPTURED_OUTPUT_BYTES));
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

    let (status, exit_code, summary) = match wait_state {
        ChildWaitState::Cancelled => (
            "cancelled".to_string(),
            CANCELLED_EXIT_CODE,
            format!("workflow `{}` cancelled by termination signal", manifest.id),
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
            let status = parsed.status.unwrap_or_else(|| "completed".to_string());
            let summary = parsed
                .answer
                .or(parsed.text_fallback)
                .unwrap_or_else(|| "completed".to_string());
            (status, 0, summary)
        }
        ChildWaitState::Exited(raw_exit_code) => {
            let status = parsed.status.unwrap_or_else(|| "failed".to_string());
            let summary = parsed
                .errors
                .first()
                .cloned()
                .or(parsed.answer)
                .or(parsed.text_fallback)
                .unwrap_or_else(|| format!("exited with status {raw_exit_code}"));
            (status, raw_exit_code, summary)
        }
    };

    let entry = WorkflowRunLogEntry {
        run_id,
        workflow_id: manifest.id.clone(),
        started_at,
        finished_at,
        duration_ms,
        exit_code,
        status,
        summary: scrub_and_bound_text(&summary, 4_000),
        stderr_excerpt: scrub_and_bound_text(&stderr_text, 2_000),
        ephemeral: true,
        trust: manifest.trust.as_str().to_string(),
        model_profile: manifest.model_profile.clone(),
        tool_calls: parsed.tool_calls,
        usage: parsed.usage,
    };

    persist_outcome(store, manifest, entry)
}

enum ChildWaitState {
    Exited(i32),
    TimedOut,
    Cancelled,
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
}

fn parse_run_output(stdout: &str, expect_json: bool) -> ParsedRunOutput {
    let mut parsed = ParsedRunOutput {
        status: None,
        answer: None,
        text_fallback: None,
        errors: Vec::new(),
        tool_calls: 0,
        usage: None,
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
                "turn_finished" => {
                    if let Some(status) = value.get("status").and_then(Value::as_str) {
                        parsed.status = Some(status.to_string());
                    }
                    if let Some(answer) = value.get("answer").and_then(Value::as_str)
                        && !answer.trim().is_empty()
                    {
                        parsed.answer = Some(answer.to_string());
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
