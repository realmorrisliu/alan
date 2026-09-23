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
