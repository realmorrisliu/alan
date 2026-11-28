//! Lifecycle wait helpers for Root and child Agent Processes.

use std::time::Duration;

use alan_agent_engine::{
    AgentExecutableResult, RuntimeController, UiActivitySnapshot, UiActivityState, UiNoticeKind,
    UiNoticeSnapshot,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Pid, ProcessOutcome};
use anyhow::{Context, Result};

use super::{
    pause_answer::{ChildTerminal, decide_child_terminal},
    pause_record::read_child_pause,
};

pub(super) async fn wait_for_root_stop(
    mut stop: tokio::sync::oneshot::Receiver<()>,
    controller: &RuntimeController,
) -> i32 {
    loop {
        tokio::select! {
            _ = &mut stop => return 0,
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                if controller.is_finished() {
                    return 1;
                }
            }
        }
    }
}

pub(super) async fn wait_for_child_terminal(
    root: &InProcessTransport,
    pid: Pid,
    controller: &RuntimeController,
    run_timeout: Option<Duration>,
) -> Result<AgentExecutableResult> {
    // The run's own timeout bounds the wait; a clock is never the reason a paused
    // Process stops being answerable.
    let deadline = run_timeout.map(|timeout| std::time::Instant::now() + timeout);
    let shell = alan_shell::Shell::new(root.clone());
    let activity_path = format!("/agent/{}/machine/ui/activity", pid.0);
    let events_path = format!("/agent/{}/machine/ui/events", pid.0);
    let notice_path = format!("/agent/{}/machine/ui/notice", pid.0);
    loop {
        if controller.is_finished() {
            anyhow::bail!("child Agent Machine stopped before publishing a terminal result");
        }
        let stopped = deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline);
        let events_started = shell.stat(&events_path).await.map(|stat| stat.length > 0)?;
        if events_started {
            let activity: UiActivitySnapshot =
                serde_json::from_slice(&shell.cat(&activity_path).await?)?;
            if matches!(
                activity.state,
                UiActivityState::Idle | UiActivityState::Paused
            ) {
                let notice: UiNoticeSnapshot =
                    serde_json::from_slice(&shell.cat(&notice_path).await?)?;
                let output_text =
                    String::from_utf8(shell.cat(&format!("/agent/{}/io/output", pid.0)).await?)
                        .context("child Agent output is utf8")?;
                let warnings = (notice.kind == UiNoticeKind::Warning)
                    .then(|| notice.message.clone())
                    .into_iter()
                    .collect();
                let pause = read_child_pause(&shell, pid).await?;
                let terminal = decide_child_terminal(
                    activity.state,
                    pause.is_some(),
                    notice.kind == UiNoticeKind::Error,
                    stopped,
                );
                if terminal == ChildTerminal::Wait {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
                if terminal == ChildTerminal::Failed {
                    return Ok(AgentExecutableResult::failed_with_output(
                        output_text,
                        warnings,
                        notice.message,
                    ));
                }
                let Some(pause) = pause.filter(|_| terminal == ChildTerminal::Paused) else {
                    return Ok(AgentExecutableResult::completed(output_text, warnings));
                };
                return Ok(AgentExecutableResult::paused(output_text, warnings, pause));
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub(super) async fn wait_for_process_exit(
    procfs: &alan_kernel::ProcFs,
    pid: Pid,
    timeout: Duration,
) -> Result<()> {
    let (view, fid) = match crate::runtime::boot_setup::open_proc_wait(procfs, pid).await {
        Ok(opened) => opened,
        Err(_) => {
            // If the process is no longer present or already gone, treat as exited.
            return Ok(());
        }
    };
    let result = tokio::time::timeout(
        timeout,
        crate::runtime::boot_setup::read_opened_wait_record(&view, fid, pid),
    )
    .await;
    let _ = alan_ap::FileServer::clunk(&view, fid).await;
    result.context("timed out waiting for Agent Process exit")??;
    Ok(())
}

pub(super) fn process_error(error: anyhow::Error) -> ProcessOutcome {
    let result = AgentExecutableResult::failed(format!("alan-agent: {error:#}"));
    ProcessOutcome::exited(
        1,
        result
            .to_process_output_record()
            .unwrap_or_else(|_| format!("alan-agent: {error:#}\n").into_bytes()),
    )
}
