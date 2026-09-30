use super::app::FileBackedEvent;
use alan_agent_protocol::UiActivitySnapshot;
use anyhow::{Context, Result, anyhow, bail};

// ponytail: two 250ms retries cap initial readiness wait at 500ms.
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

pub(super) fn spawn_root_agent_pid_refresh(
    shell: alan_shell::Shell,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut retry: tokio::sync::watch::Receiver<()>,
    initial_pid: Option<u64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last = Some(Ok(initial_pid));

        loop {
            tokio::select! {
                _ = tx.closed() => break,
                changed = retry.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    last = None;
                }
                _ = tick.tick() => {
                    let current = tokio::select! {
                        _ = tx.closed() => break,
                        current = current_root_agent_pid(&shell) => {
                            current.map_err(|error| format!("{error:#}"))
                        }
                    };
                    if last.as_ref() != Some(&current) {
                        if tx.send(FileBackedEvent::RootAgentPidRefresh(current.clone())).await.is_err() {
                            break;
                        }
                        last = Some(current);
                    }
                }
            }
        }
    })
}

pub(super) async fn wait_for_root_agent_activity(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
) -> Result<(u64, UiActivitySnapshot)> {
    let mut last_activity_error = None;
    let mut pinned_pid = None;
    for attempt in 0..ROOT_AGENT_ATTACH_ATTEMPTS {
        let current = current_root_agent_pid(shell).await?;
        if let Some(expected) = pinned_pid {
            anyhow::ensure!(
                current == Some(expected),
                "Root Agent changed during startup"
            );
        }
        let Some(pid) = current else {
            last_activity_error = None;
            if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
                tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
            }
            continue;
        };
        pinned_pid = Some(pid);
        let Some(agent_process_path) = root_agent_path_for_pid(root_agent_path, pid) else {
            bail!("one-shot tasks require the /agent/root path");
        };
        let activity =
            match super::file_surface::read_activity_snapshot(shell, &agent_process_path).await {
                Ok(activity) => activity,
                Err(error) => {
                    require_root_agent_pid(shell, pid).await?;
                    last_activity_error = Some(error.context("read Agent activity failed"));
                    if attempt + 1 < ROOT_AGENT_ATTACH_ATTEMPTS {
                        tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
                    }
                    continue;
                }
            };
        require_root_agent_pid(shell, pid).await?;
        return Ok((pid, activity));
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
    pub(super) ui_tail: alan_shell::Tail,
}

pub(super) async fn open_stdio_tail_attachment(
    shell: &alan_shell::Shell,
    root_agent_path: &str,
) -> Result<StdioTailAttachment> {
    let (root_agent_pid, _) = wait_for_root_agent_activity(shell, root_agent_path).await?;
    let agent_process_path = root_agent_path_for_pid(root_agent_path, root_agent_pid)
        .ok_or_else(|| anyhow!("one-shot tasks require the /agent/root path"))?;
    for attempt in 0..ROOT_AGENT_ATTACH_ATTEMPTS {
        require_root_agent_pid(shell, root_agent_pid).await?;
        let (tape_tail, _) =
            match tail_with_history(shell, &format!("{agent_process_path}/machine/tape")).await {
                Ok(opened) => opened,
                Err(error) => {
                    require_root_agent_pid(shell, root_agent_pid).await?;
                    if attempt + 1 == ROOT_AGENT_ATTACH_ATTEMPTS {
                        return Err(error);
                    }
                    tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
                    continue;
                }
            };
        let (ui_tail, _) = match tail_with_history(
            shell,
            &format!("{agent_process_path}/machine/ui/events"),
        )
        .await
        {
            Ok(opened) => opened,
            Err(error) => {
                let _ = tape_tail.close().await;
                require_root_agent_pid(shell, root_agent_pid).await?;
                if attempt + 1 == ROOT_AGENT_ATTACH_ATTEMPTS {
                    return Err(error);
                }
                tokio::time::sleep(ROOT_AGENT_ATTACH_RETRY_DELAY).await;
                continue;
            }
        };
        let attachment = StdioTailAttachment {
            root_agent_pid,
            agent_process_path,
            tape_tail,
            ui_tail,
        };
        if let Err(error) = require_stdio_attachment_current(shell, &attachment).await {
            let _ = close_stdio_tails(attachment.tape_tail, attachment.ui_tail).await;
            return Err(error);
        }
        return Ok(attachment);
    }
    bail!("Root Agent streams are unavailable")
}

pub(super) async fn require_stdio_attachment_current(
    shell: &alan_shell::Shell,
    attachment: &StdioTailAttachment,
) -> Result<()> {
    require_root_agent_pid(shell, attachment.root_agent_pid).await
}

async fn require_root_agent_pid(shell: &alan_shell::Shell, pid: u64) -> Result<()> {
    if current_root_agent_pid(shell).await? != Some(pid) {
        bail!("Root Agent changed or disappeared; task outcome is unknown")
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
