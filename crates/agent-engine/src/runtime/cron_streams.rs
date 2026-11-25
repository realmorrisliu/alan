//! Guarded Stateful Cron Stream lifecycle helpers: per-stream lease lock arbitration,
//! live grant & trust revalidation, automatic rollout compaction, per-run USD budget
//! checks, and `max_consecutive_failures` -> `paused_error` auto-pause alerts to `inbox/`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::agent_machine::AgentMachine;
use crate::rollout::{CompactedItem, RolloutItem, RolloutRecorder};

use super::compaction::build_degraded_compaction_summary;
use super::project_workstreams::{
    CronStreamPolicy, InboxDigestEntry, InboxDigestStatus, StreamLeaseAcquireResult,
    StreamLeaseGuard, StreamLeaseHolderKind, WorkstreamPersistence, WorkstreamRecord,
    WorkstreamState, load_project_binding, load_workstream_record, pause_workstream_error,
    scrub_inbox_summary, upsert_workstream, write_inbox_digest,
};

const CRON_COMPACT_KEEP_LAST_MESSAGES: usize = 12;

/// Declarative Stateful Cron Stream policy bound to a workflow manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CronStreamExecutionSpec {
    pub workflow_id: String,
    pub project_key: String,
    pub stream_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub max_consecutive_failures: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_threshold_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_run_budget_usd: Option<f64>,
}

/// Pre-flight preparation result before spawning a Stateful Cron Stream run.
#[derive(Debug)]
pub enum CronStreamPrepareResult {
    Ready {
        lease: StreamLeaseGuard,
        record: WorkstreamRecord,
        compacted_tokens_before: Option<usize>,
        compacted_tokens_after: Option<usize>,
    },
    /// Interactive session (or another run) holds `streams/<name>.lock`; deferred cleanly.
    DeferredLockContention { inbox_entry: InboxDigestEntry },
    /// Stream is already in `paused_error` awaiting manual `/streams resume <name>`.
    SkippedPausedError { inbox_entry: InboxDigestEntry },
}

/// Outcome of one completed or failed Stateful Cron Stream wake.
#[derive(Debug, Clone, PartialEq)]
pub struct CronStreamFinalizeOutcome {
    pub record: WorkstreamRecord,
    pub inbox_entry: InboxDigestEntry,
    pub transitioned_to_paused_error: bool,
}

/// Prepare a Stateful Cron Stream wake under strict guardrails:
/// 1. Verify project binding & stream state (`Persistent`, not `PausedError` / `Archived`).
/// 2. Acquire non-blocking `streams/<name>.lock` lease; on contention, defer cleanly without
///    mutating the checkpoint and record an `inbox/` contention notice.
/// 3. Validate live grants via caller-provided `revalidate_grants` callback.
/// 4. If `compaction_threshold_tokens` is configured and the stream's checkpoint rollout exceeds
///    it, perform automatic tape compaction before resuming.
pub async fn prepare_stateful_cron_stream_run<F>(
    projects_root: &Path,
    spec: &CronStreamExecutionSpec,
    run_id: &str,
    evidence_ref: &str,
    revalidate_grants: F,
) -> Result<CronStreamPrepareResult>
where
    F: FnOnce() -> Result<()>,
{
    let binding = load_project_binding(projects_root, &spec.project_key)?.with_context(|| {
        format!(
            "project `{}` is not bound in {}",
            spec.project_key,
            projects_root.display()
        )
    })?;
    ensure!(
        binding.project_key == spec.project_key,
        "project binding mismatch for `{}`",
        spec.project_key
    );

    let lease = match StreamLeaseGuard::try_acquire(
        projects_root,
        &spec.project_key,
        &spec.stream_name,
        StreamLeaseHolderKind::Cron,
        Some(&spec.workflow_id),
    )? {
        StreamLeaseAcquireResult::Acquired(guard) => guard,
        StreamLeaseAcquireResult::Contention { holder } => {
            let holder_desc = holder
                .as_ref()
                .map(|h| format!("{:?} pid={}", h.holder_kind, h.pid))
                .unwrap_or_else(|| "active session".to_string());
            let summary = scrub_inbox_summary(&format!(
                "Deferred stateful cron workflow `{}` for stream `{}` because {} currently holds the stream lease lock; checkpoint left untouched.",
                spec.workflow_id, spec.stream_name, holder_desc
            ));
            let entry = write_inbox_digest(
                projects_root,
                &spec.project_key,
                InboxDigestEntry::new(
                    run_id,
                    &spec.workflow_id,
                    Some(spec.stream_name.clone()),
                    InboxDigestStatus::Warning,
                    summary,
                    evidence_ref,
                ),
            )?;
            return Ok(CronStreamPrepareResult::DeferredLockContention { inbox_entry: entry });
        }
    };

    let existing = load_workstream_record(projects_root, &spec.project_key, &spec.stream_name)?;
    let mut record = match existing {
        Some(rec) => {
            ensure!(
                rec.persistence == WorkstreamPersistence::Persistent,
                "stateful cron stream `{}` must have `persistent` lifecycle",
                spec.stream_name
            );
            match rec.state {
                WorkstreamState::PausedError => {
                    let entry = write_inbox_digest(
                        projects_root,
                        &spec.project_key,
                        InboxDigestEntry::new(
                            run_id,
                            &spec.workflow_id,
                            Some(spec.stream_name.clone()),
                            InboxDigestStatus::PausedError,
                            format!(
                                "Skipped stateful cron workflow `{}` because stream `{}` is in paused_error; run `/streams resume {}` to re-enable.",
                                spec.workflow_id, spec.stream_name, spec.stream_name
                            ),
                            evidence_ref,
                        ),
                    )?;
                    return Ok(CronStreamPrepareResult::SkippedPausedError { inbox_entry: entry });
                }
                WorkstreamState::Archived => {
                    bail!(
                        "stateful cron stream `{}` is archived and cannot run",
                        spec.stream_name
                    );
                }
                WorkstreamState::Running | WorkstreamState::Idle => rec,
            }
        }
        None => {
            let scope = spec
                .scope
                .clone()
                .unwrap_or_else(|| format!("Scheduled workflow {}", spec.workflow_id));
            let mut rec =
                WorkstreamRecord::new(&spec.stream_name, scope, WorkstreamPersistence::Persistent);
            rec.state = WorkstreamState::Idle;
            rec
        }
    };

    let prev_failures = record
        .cron_binding
        .as_ref()
        .map_or(0, |b| b.consecutive_failures);
    record.cron_binding = Some(CronStreamPolicy {
        workflow_id: spec.workflow_id.clone(),
        max_consecutive_failures: spec.max_consecutive_failures.max(1),
        consecutive_failures: prev_failures,
        compaction_threshold_tokens: spec.compaction_threshold_tokens,
        per_run_budget_usd: spec.per_run_budget_usd,
    });

    if let Err(err) = revalidate_grants() {
        let failed = record_cron_failure_state(
            projects_root,
            spec,
            record,
            run_id,
            evidence_ref,
            &format!("Live Host Mount grant revalidation failed: {err:#}"),
            None,
            None,
        )?;
        bail!(
            "stateful cron grant revalidation failed (consecutive_failures={}): {err:#}",
            failed
                .record
                .cron_binding
                .as_ref()
                .map_or(0, |b| b.consecutive_failures)
        );
    }

    let (compacted_tokens_before, compacted_tokens_after) =
        if let (Some(threshold), Some(ckpt_ref)) = (
            spec.compaction_threshold_tokens,
            record.last_checkpoint_ref.as_deref(),
        ) {
            maybe_compact_rollout_checkpoint(Path::new(ckpt_ref), threshold).await?
        } else {
            (None, None)
        };

    record.state = WorkstreamState::Running;
    record.updated_at = Utc::now().to_rfc3339();
    let persisted = upsert_workstream(projects_root, &spec.project_key, record)?;

    Ok(CronStreamPrepareResult::Ready {
        lease,
        record: persisted,
        compacted_tokens_before,
        compacted_tokens_after,
    })
}

/// Finalize a Stateful Cron Stream wake, enforcing `per_run_budget_usd`, tracking
/// `consecutive_failures`, transitioning to `paused_error` + emitting an `inbox/` alert
/// when `max_consecutive_failures` is reached, and releasing `lease` on return.
#[allow(
    clippy::too_many_arguments,
    reason = "Cron stream finalizer takes explicit store root, spec, record, lease guard, run/evidence refs, outcome, and budget"
)]
pub fn finalize_stateful_cron_stream_run(
    projects_root: &Path,
    spec: &CronStreamExecutionSpec,
    mut record: WorkstreamRecord,
    lease: StreamLeaseGuard,
    run_id: &str,
    evidence_ref: &str,
    run_succeeded: bool,
    observed_cost_usd: Option<f64>,
    summary_text: &str,
    next_checkpoint_ref: Option<String>,
    next_rollout_ref: Option<String>,
) -> Result<CronStreamFinalizeOutcome> {
    let _lease = lease;
    let budget_violation = check_per_run_budget(spec.per_run_budget_usd, observed_cost_usd);

    let result = if run_succeeded && budget_violation.is_none() {
        if let Some(ckpt) = next_checkpoint_ref
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            record.last_checkpoint_ref = Some(ckpt);
        }
        if let Some(rollout) = next_rollout_ref
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            record.rollout_ref = Some(rollout);
        }
        record.state = WorkstreamState::Idle;
        record.cron_binding = Some(CronStreamPolicy {
            workflow_id: spec.workflow_id.clone(),
            max_consecutive_failures: spec.max_consecutive_failures.max(1),
            consecutive_failures: 0,
            compaction_threshold_tokens: spec.compaction_threshold_tokens,
            per_run_budget_usd: spec.per_run_budget_usd,
        });
        record.updated_at = Utc::now().to_rfc3339();
        let saved = upsert_workstream(projects_root, &spec.project_key, record)?;
        let entry = write_inbox_digest(
            projects_root,
            &spec.project_key,
            InboxDigestEntry::new(
                run_id,
                &spec.workflow_id,
                Some(spec.stream_name.clone()),
                InboxDigestStatus::Ok,
                summary_text,
                evidence_ref,
            ),
        )?;
        CronStreamFinalizeOutcome {
            record: saved,
            inbox_entry: entry,
            transitioned_to_paused_error: false,
        }
    } else {
        let reason = budget_violation.unwrap_or_else(|| summary_text.to_string());
        record_cron_failure_state(
            projects_root,
            spec,
            record,
            run_id,
            evidence_ref,
            &reason,
            next_checkpoint_ref,
            next_rollout_ref,
        )?
    };

    Ok(result)
}

/// Check whether `observed_cost_usd` exceeds `per_run_budget_usd`.
pub fn check_per_run_budget(
    per_run_budget_usd: Option<f64>,
    observed_cost_usd: Option<f64>,
) -> Option<String> {
    let cap = per_run_budget_usd?;
    let spent = observed_cost_usd?;
    if spent > cap {
        Some(format!(
            "Stateful cron run exceeded per_run_budget_usd (${spent:.4} > ${cap:.4})"
        ))
    } else {
        None
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Cron stream finalizer takes explicit store root, spec, record, lease guard, run/evidence refs, outcome, and budget"
)]
fn record_cron_failure_state(
    projects_root: &Path,
    spec: &CronStreamExecutionSpec,
    mut record: WorkstreamRecord,
    run_id: &str,
    evidence_ref: &str,
    reason: &str,
    next_checkpoint_ref: Option<String>,
    next_rollout_ref: Option<String>,
) -> Result<CronStreamFinalizeOutcome> {
    let threshold = spec.max_consecutive_failures.max(1);
    let next_failures = record
        .cron_binding
        .as_ref()
        .map_or(0, |b| b.consecutive_failures)
        .saturating_add(1);
    record.cron_binding = Some(CronStreamPolicy {
        workflow_id: spec.workflow_id.clone(),
        max_consecutive_failures: threshold,
        consecutive_failures: next_failures,
        compaction_threshold_tokens: spec.compaction_threshold_tokens,
        per_run_budget_usd: spec.per_run_budget_usd,
    });

    if next_failures >= threshold {
        // Persist updated failure count first, then transition atomically to PausedError.
        let _ = upsert_workstream(projects_root, &spec.project_key, record)?;
        let paused = pause_workstream_error(
            projects_root,
            &spec.project_key,
            &spec.stream_name,
            next_checkpoint_ref,
            next_rollout_ref,
        )?;
        let alert = write_inbox_digest(
            projects_root,
            &spec.project_key,
            InboxDigestEntry::new(
                run_id,
                &spec.workflow_id,
                Some(spec.stream_name.clone()),
                InboxDigestStatus::PausedError,
                format!(
                    "ALERT: Stateful cron stream `{}` auto-paused (`paused_error`) after {next_failures}/{threshold} consecutive failures: {reason}",
                    spec.stream_name
                ),
                evidence_ref,
            ),
        )?;
        Ok(CronStreamFinalizeOutcome {
            record: paused,
            inbox_entry: alert,
            transitioned_to_paused_error: true,
        })
    } else {
        if let Some(ckpt) = next_checkpoint_ref
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            record.last_checkpoint_ref = Some(ckpt);
        }
        if let Some(rollout) = next_rollout_ref
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            record.rollout_ref = Some(rollout);
        }
        record.state = WorkstreamState::Idle;
        record.updated_at = Utc::now().to_rfc3339();
        let saved = upsert_workstream(projects_root, &spec.project_key, record)?;
        let entry = write_inbox_digest(
            projects_root,
            &spec.project_key,
            InboxDigestEntry::new(
                run_id,
                &spec.workflow_id,
                Some(spec.stream_name.clone()),
                InboxDigestStatus::Warning,
                format!(
                    "Stateful cron stream `{}` run failed ({next_failures}/{threshold} consecutive failures): {reason}",
                    spec.stream_name
                ),
                evidence_ref,
            ),
        )?;
        Ok(CronStreamFinalizeOutcome {
            record: saved,
            inbox_entry: entry,
            transitioned_to_paused_error: false,
        })
    }
}

/// Estimate prompt tokens on a rollout checkpoint file and compact it automatically if
/// `estimated_tokens >= threshold_tokens`.
pub async fn maybe_compact_rollout_checkpoint(
    rollout_path: &Path,
    threshold_tokens: u64,
) -> Result<(Option<usize>, Option<usize>)> {
    if !rollout_path.is_file() {
        return Ok((None, None));
    }
    let path_buf = rollout_path.to_path_buf();
    let items = RolloutRecorder::load_history(&path_buf).await?;
    if !matches!(items.first(), Some(RolloutItem::AgentMachineMeta(_))) {
        return Ok((None, None));
    }
    let (mut machine, _) = AgentMachine::load_from_rollout_with_recorder_cwd(
        &path_buf,
        "/proc/cron",
        "cron",
        None,
        None,
        None,
        false,
    )
    .await?;

    let before = machine.estimated_prompt_tokens();
    if (before as u64) < threshold_tokens {
        return Ok((Some(before), None));
    }

    let summary = build_degraded_compaction_summary(machine.messages(), machine.tape_summary())
        .unwrap_or_else(|| {
            "Compacted earlier cron stream turns to stay under threshold.".to_string()
        });
    machine.compact_tape(summary.clone(), CRON_COMPACT_KEEP_LAST_MESSAGES);
    let after = machine.estimated_prompt_tokens();

    let mut compacted = CompactedItem::new(summary);
    compacted.retained_messages = Some(machine.messages().to_vec());
    compacted.input_tokens = Some(before);
    compacted.output_tokens = Some(after);
    let line = serde_json::to_string(&RolloutItem::Compacted(compacted))?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(&path_buf)
        .with_context(|| format!("append compacted checkpoint to {}", path_buf.display()))?;
    writeln!(file, "{line}")?;
    file.sync_all()?;

    Ok((Some(before), Some(after)))
}

#[cfg(test)]
#[path = "cron_streams_tests.rs"]
mod tests;
