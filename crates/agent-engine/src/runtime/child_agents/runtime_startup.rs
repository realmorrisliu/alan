use crate::runtime::AgentProcessLifecycle;
use crate::runtime::child_runs::ChildRunStatus;
use crate::runtime::controller::{RuntimeController, RuntimeStartupMetadata};
use alan_agent_protocol::Submission;
use anyhow::{Context, Result, bail};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(super) const CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE: &str =
    "Child Agent Process launch cancelled";

pub(super) async fn wait_for_child_runtime_startup(
    mut runtime: RuntimeController,
    cancel: Option<&CancellationToken>,
) -> Result<(RuntimeController, RuntimeStartupMetadata)> {
    let startup_metadata = if let Some(cancel) = cancel {
        if cancel.is_cancelled() {
            runtime.abort().await;
            bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
        }
        tokio::select! {
            _ = cancel.cancelled() => {
                runtime.abort().await;
                bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
            }
            ready = runtime.wait_until_ready() => {
                ready.context("Child Agent Process runtime failed to start")?
            }
        }
    } else {
        runtime
            .wait_until_ready()
            .await
            .context("Child Agent Process runtime failed to start")?
    };

    Ok((runtime, startup_metadata))
}

pub(super) async fn send_initial_child_submission(
    runtime: RuntimeController,
    submission: Submission,
    cancel: Option<&CancellationToken>,
) -> Result<RuntimeController> {
    if let Some(cancel) = cancel {
        if cancel.is_cancelled() {
            runtime.abort().await;
            bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
        }
        tokio::select! {
            _ = cancel.cancelled() => {
                runtime.abort().await;
                bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
            }
            result = runtime.handle.submission_tx.send(submission) => {
                result.context("Failed to submit initial child Agent Process turn")?
            }
        }
    } else {
        runtime
            .handle
            .submission_tx
            .send(submission)
            .await
            .context("Failed to submit initial child Agent Process turn")?;
    }

    Ok(runtime)
}

pub(super) fn child_run_status_for_launch_error(error: &anyhow::Error) -> ChildRunStatus {
    if error.chain().any(|cause| {
        cause
            .to_string()
            .contains(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE)
    }) {
        ChildRunStatus::Cancelled
    } else {
        ChildRunStatus::Failed
    }
}

pub(super) async fn record_child_launch_failure_process(
    lifecycle: &Arc<dyn AgentProcessLifecycle>,
    error: &anyhow::Error,
) {
    let exit_code = match child_run_status_for_launch_error(error) {
        ChildRunStatus::Cancelled => 130,
        _ => 1,
    };
    lifecycle.finish(exit_code).await;
}
