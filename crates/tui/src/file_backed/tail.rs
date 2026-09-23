use super::{
    StdioTaskSnapshot, StdioTaskWaitContext, finish_stdio_task_if_ready,
    interrupt_stdio_task_if_active, stdio_task_snapshot,
};
use alan_agent_protocol::UiActivitySnapshot;
use anyhow::{Context, Result, anyhow, bail};

// ponytail: two 250ms retries cap startup handoff at 500ms; longer outages fail clearly.
const ROOT_AGENT_ATTACH_ATTEMPTS: usize = 3;
const ROOT_AGENT_ATTACH_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

pub(super) async fn current_root_agent_pid(shell: &alan_shell::Shell) -> Result<Option<u64>> {
    let path = "/mnt/service-manager/units/root-agent/pid";
    let bytes = shell
        .cat(path)
        .await
        .map_err(|err| anyhow!("read {path} failed: {err:?}"))?;
    let raw = String::from_utf8(bytes).with_context(|| format!("{path} is not utf8"))?;
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let pid = value
        .parse::<u64>()
        .context("Root Agent PID is not an unsigned integer")?;
    Ok((pid > 0).then_some(pid))
}

pub(super) async fn wait_for_root_agent_activity(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
) -> Result<(u64, UiActivitySnapshot)> {
    let mut last_activity_error = None;
    for attempt in 0..ROOT_AGENT_ATTACH_ATTEMPTS {
        let Some(pid) = current_root_agent_pid(shell).await? else {
            last_activity_error = None;
            if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
                tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
            }
            continue;
        };
        let Some(agent_process_path) = root_agent_path_for_pid(root_agent_path, pid) else {
            bail!("one-shot tasks require the /agent/root path");
        };
        let activity =
            match super::file_surface::read_activity_snapshot(shell, &agent_process_path).await {
                Ok(activity) => activity,
                Err(error) => {
                    last_activity_error = if current_root_agent_pid(shell).await? == Some(pid) {
                        Some(error.context("read Agent activity failed"))
                    } else {
                        None
                    };
                    if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
                        tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
                    }
                    continue;
                }
            };
        if current_root_agent_pid(shell).await? == Some(pid) {
            return Ok((pid, activity));
        }
        last_activity_error = None;
        if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
            tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
        }
    }
    if let Some(error) = last_activity_error {
        return Err(error);
    }
    bail!("Root Agent PID is unavailable")
}

pub(super) async fn tail_with_history(
    shell: &alan_shell::Shell,
    path: &str,
) -> Result<(alan_shell::Tail, Vec<u8>)> {
    // ponytail: retry twice for PID churn, then report it instead of spinning.
    let attempts = if is_root_agent_path(path) { 3 } else { 1 };
    'attempt: for _ in 0..attempts {
        let root_pid = if is_root_agent_path(path) {
            Some(
                current_root_agent_pid(shell)
                    .await?
                    .ok_or_else(|| anyhow!("Root Agent PID is unavailable while opening {path}"))?,
            )
        } else {
            None
        };
        let pinned_path = root_pid
            .and_then(|pid| root_agent_path_for_pid(path, pid))
            .unwrap_or_else(|| path.to_string());

        let existing = match shell.cat(&pinned_path).await {
            Ok(existing) => existing,
            Err(error) => {
                if root_agent_pid_changed(shell, root_pid).await? {
                    continue;
                }
                return Err(anyhow!("failed to snapshot {path}: {error:?}"));
            }
        };
        let mut tail = match shell.tail(&pinned_path).await {
            Ok(tail) => tail,
            Err(error) => {
                if root_agent_pid_changed(shell, root_pid).await? {
                    continue;
                }
                return Err(anyhow!("failed to tail {path}: {error:?}"));
            }
        };
        if root_agent_pid_changed(shell, root_pid).await? {
            let _ = tail.close().await;
            continue;
        }

        let mut skipped = 0usize;
        while skipped < existing.len() {
            let remaining = existing.len() - skipped;
            let chunk = match tail.read(remaining.min(64 * 1024) as u32).await {
                Ok(chunk) => chunk,
                Err(error) => {
                    if root_agent_pid_changed(shell, root_pid).await? {
                        let _ = tail.close().await;
                        continue 'attempt;
                    }
                    return Err(anyhow!("failed to skip existing {path} bytes: {error:?}"));
                }
            };
            if chunk.is_empty() {
                if root_agent_pid_changed(shell, root_pid).await? {
                    let _ = tail.close().await;
                    continue 'attempt;
                }
                bail!("tail for {path} closed before existing bytes were skipped");
            }
            skipped += chunk.len();
        }

        return Ok((tail, existing));
    }

    bail!("Root Agent kept changing while opening {path}; retry attach")
}

async fn root_agent_pid_changed(shell: &alan_shell::Shell, expected: Option<u64>) -> Result<bool> {
    match expected {
        Some(pid) => Ok(current_root_agent_pid(shell).await? != Some(pid)),
        None => Ok(false),
    }
}

pub(super) fn root_agent_path_for_pid(path: &str, pid: u64) -> Option<String> {
    let suffix = path.strip_prefix("/agent/root")?;
    if !suffix.is_empty() && !suffix.starts_with('/') {
        return None;
    }
    Some(format!("/agent/{pid}{suffix}"))
}

fn is_root_agent_path(path: &str) -> bool {
    root_agent_path_for_pid(path, 1).is_some()
}

pub(super) struct StdioTailAttachment {
    pub(super) root_agent_pid: u64,
    pub(super) agent_process_path: String,
    pub(super) tape_tail: alan_shell::Tail,
    pub(super) tape_history: Vec<u8>,
    pub(super) ui_tail: alan_shell::Tail,
    pub(super) ui_history: Vec<u8>,
}

pub(super) async fn open_stdio_tail_attachment(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
) -> Result<StdioTailAttachment> {
    for attempt in 0..ROOT_AGENT_ATTACH_ATTEMPTS {
        let Some(root_agent_pid) = current_root_agent_pid(shell).await? else {
            if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
                tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
                continue;
            }
            bail!("Root Agent PID is unavailable");
        };
        let agent_process_path = root_agent_path_for_pid(root_agent_path, root_agent_pid)
            .ok_or_else(|| anyhow!("one-shot tasks require the /agent/root path"))?;
        let tape_path = format!("{agent_process_path}/machine/tape");
        let ui_path = format!("{agent_process_path}/machine/ui/events");
        let (tape_tail, tape_history) = match tail_with_history(shell, &tape_path).await {
            Ok(opened) => opened,
            Err(err) => {
                if current_root_agent_pid(shell).await? != Some(root_agent_pid) {
                    continue;
                }
                if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
                    tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
                    continue;
                }
                return Err(err);
            }
        };
        let (ui_tail, ui_history) = match tail_with_history(shell, &ui_path).await {
            Ok(opened) => opened,
            Err(err) => {
                let _ = tape_tail.close().await;
                if current_root_agent_pid(shell).await? != Some(root_agent_pid) {
                    continue;
                }
                if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
                    tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
                    continue;
                }
                return Err(err);
            }
        };
        let current_pid = match current_root_agent_pid(shell).await {
            Ok(pid) => pid,
            Err(err) => {
                let _ = close_stdio_tails(tape_tail, ui_tail).await;
                return Err(err);
            }
        };
        if current_pid == Some(root_agent_pid) {
            return Ok(StdioTailAttachment {
                root_agent_pid,
                agent_process_path,
                tape_tail,
                tape_history,
                ui_tail,
                ui_history,
            });
        }
        let _ = close_stdio_tails(tape_tail, ui_tail).await;
        if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
            tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
        }
    }
    bail!("Root Agent kept changing while opening one-shot AgentFS streams; retry")
}

pub(super) async fn open_stdio_tail_attachment_when_idle(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
) -> Result<StdioTailAttachment> {
    for attempt in 0..ROOT_AGENT_ATTACH_ATTEMPTS {
        if let Some(attachment) =
            try_open_stdio_tail_attachment_when_idle(shell, root_agent_path).await?
        {
            return Ok(attachment);
        }
        if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
            tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
        }
    }
    bail!("Root Agent kept changing while preparing one-shot attachment; retry")
}

async fn try_open_stdio_tail_attachment_when_idle(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
) -> Result<Option<StdioTailAttachment>> {
    let (root_agent_pid, activity) = wait_for_root_agent_activity(shell, root_agent_path).await?;
    if current_root_agent_pid(shell).await? != Some(root_agent_pid) {
        return Ok(None);
    }
    if let Err(error) = super::require_root_agent_idle(activity.state) {
        return if current_root_agent_pid(shell).await? == Some(root_agent_pid) {
            Err(error)
        } else {
            Ok(None)
        };
    }

    let attachment = match open_stdio_tail_attachment(shell, root_agent_path).await {
        Ok(attachment) => attachment,
        Err(error) => {
            return if current_root_agent_pid(shell).await? == Some(root_agent_pid) {
                Err(error)
            } else {
                Ok(None)
            };
        }
    };
    if attachment.root_agent_pid != root_agent_pid {
        let _ = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
        return Ok(None);
    }
    if let Err(error) = require_stdio_attachment_idle(shell, &attachment).await {
        let current_pid = current_root_agent_pid(shell).await;
        let _ = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
        return match current_pid {
            Ok(Some(pid)) if pid == attachment.root_agent_pid => Err(error),
            Ok(_) => Ok(None),
            Err(pid_error) => Err(pid_error),
        };
    }
    Ok(Some(attachment))
}

async fn require_stdio_attachment_idle(
    shell: &alan_shell::Shell,
    attachment: &StdioTailAttachment,
) -> Result<()> {
    let activity =
        super::file_surface::read_activity_snapshot(shell, &attachment.agent_process_path)
            .await
            .context("read Agent activity failed")?;
    super::require_root_agent_idle(activity.state)?;
    if current_root_agent_pid(shell).await? != Some(attachment.root_agent_pid) {
        bail!("Root Agent changed before the task could be submitted; retry")
    }
    Ok(())
}

pub(super) async fn close_stdio_tails(
    tape_tail: alan_shell::Tail,
    ui_tail: alan_shell::Tail,
) -> Result<()> {
    let tape_result = tape_tail.close().await;
    let ui_result = ui_tail.close().await;
    tape_result.map_err(|err| anyhow!("close Agent tape tail failed: {err:?}"))?;
    ui_result.map_err(|err| anyhow!("close Agent UI tail failed: {err:?}"))?;
    Ok(())
}

pub(super) enum StdioTaskRecovery {
    Unchanged,
    Unavailable,
    Reattached,
    Complete(String),
}

pub(super) async fn recover_stdio_task_after_tail_close(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
    task: &StdioTaskWaitContext<'_>,
    attachment: &mut StdioTailAttachment,
    snapshot: &mut StdioTaskSnapshot,
    interrupt_requested: bool,
) -> Result<StdioTaskRecovery> {
    let recovery = recover_stdio_task_after_root_change(
        shell,
        root_agent_path,
        task,
        attachment,
        snapshot,
        interrupt_requested,
    )
    .await?;
    if !matches!(recovery, StdioTaskRecovery::Unchanged) {
        return Ok(recovery);
    }

    // The supervisor can close the old AgentFS streams before publishing PID 0.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    recover_stdio_task_after_root_change(
        shell,
        root_agent_path,
        task,
        attachment,
        snapshot,
        interrupt_requested,
    )
    .await
}

pub(super) async fn recover_stdio_task_after_root_change(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
    task: &StdioTaskWaitContext<'_>,
    attachment: &mut StdioTailAttachment,
    snapshot: &mut StdioTaskSnapshot,
    interrupt_requested: bool,
) -> Result<StdioTaskRecovery> {
    let Some(pid) = current_root_agent_pid(shell).await? else {
        return Ok(StdioTaskRecovery::Unavailable);
    };
    if pid == attachment.root_agent_pid {
        return Ok(StdioTaskRecovery::Unchanged);
    }

    let new_attachment = open_stdio_tail_attachment(shell, root_agent_path).await?;
    let recovered = match stdio_task_snapshot(
        shell,
        &new_attachment.agent_process_path,
        task,
        &new_attachment.tape_history,
        &new_attachment.ui_history,
    )
    .await
    {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = close_stdio_tails(new_attachment.tape_tail, new_attachment.ui_tail).await;
            return Err(error);
        }
    };

    let old_tape_tail = std::mem::replace(&mut attachment.tape_tail, new_attachment.tape_tail);
    let old_ui_tail = std::mem::replace(&mut attachment.ui_tail, new_attachment.ui_tail);
    attachment.root_agent_pid = new_attachment.root_agent_pid;
    attachment.agent_process_path = new_attachment.agent_process_path;
    attachment.tape_history = new_attachment.tape_history;
    attachment.ui_history = new_attachment.ui_history;
    close_stdio_tails(old_tape_tail, old_ui_tail).await?;
    *snapshot = recovered;

    if interrupt_requested
        && interrupt_stdio_task_if_active(shell, &attachment.agent_process_path, snapshot).await?
    {
        bail!("Agent task interrupted");
    }
    if snapshot.activity_state == Some(super::UiActivityState::Paused) {
        bail!("Agent task needs interactive input; attach with the TTY renderer");
    }
    if let Some(answer) = finish_stdio_task_if_ready(snapshot)? {
        return Ok(StdioTaskRecovery::Complete(answer));
    }
    if snapshot.activity_state == Some(super::UiActivityState::Idle) {
        bail!(
            "Root Agent changed before the submitted task outcome could be recovered; outcome is unknown"
        );
    }
    Ok(StdioTaskRecovery::Reattached)
}
