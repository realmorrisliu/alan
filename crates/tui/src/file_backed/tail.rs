use anyhow::{Context, Result, anyhow, bail};

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
    for _ in 0..3 {
        let root_agent_pid = current_root_agent_pid(shell)
            .await?
            .ok_or_else(|| anyhow!("Root Agent PID is unavailable"))?;
        let agent_process_path = root_agent_path_for_pid(root_agent_path, root_agent_pid)
            .ok_or_else(|| anyhow!("one-shot tasks require the /agent/root path"))?;
        let tape_path = format!("{agent_process_path}/machine/tape");
        let ui_path = format!("{agent_process_path}/machine/ui/events");
        let (tape_tail, tape_history) = match tail_with_history(shell, &tape_path).await {
            Ok(opened) => opened,
            Err(_err) if current_root_agent_pid(shell).await? != Some(root_agent_pid) => continue,
            Err(err) => return Err(err),
        };
        let (ui_tail, ui_history) = match tail_with_history(shell, &ui_path).await {
            Ok(opened) => opened,
            Err(err) => {
                let _ = tape_tail.close().await;
                if current_root_agent_pid(shell).await? != Some(root_agent_pid) {
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
    }
    bail!("Root Agent kept changing while opening one-shot AgentFS streams; retry")
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
