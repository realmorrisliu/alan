//! Native command lifetime follows the bounded Tool Process, including descendants.
use anyhow::{Context, Result};
use std::{process::Output, time::Duration};
use tokio::process::Command;

#[cfg(unix)]
struct ProcessGroup(u32);

#[cfg(unix)]
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // The command owns this fresh process group; cancellation must also stop
        // descendants that inherited its IO or could still write project files.
        // SAFETY: kill takes integer IDs only; the negative PID selects our child group.
        let result = unsafe { libc::kill(-(self.0 as i32), libc::SIGKILL) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            tracing::warn!(pid = self.0, error = %std::io::Error::last_os_error(), "failed to stop native command group");
        }
    }
}

/// Preserve the caller's IO bindings while owning the native process lifetime.
pub(in crate::tools) async fn output(
    command: Command,
    timeout: Option<Duration>,
) -> Result<Output> {
    output_with_capture(command, timeout, None).await
}

/// Capture caller-provided pipe readers when the child must inherit named pipes.
pub(in crate::tools) async fn output_with_capture(
    mut command: Command,
    timeout: Option<Duration>,
    capture: Option<(std::fs::File, std::fs::File)>,
) -> Result<Output> {
    command.kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().context("Failed to execute command")?;
    #[cfg(unix)]
    let _group = ProcessGroup(child.id().expect("spawned child has a PID"));
    if let Some((stdout, stderr)) = capture {
        child.stdout = Some(tokio::process::ChildStdout::from_std(
            std::os::fd::OwnedFd::from(stdout).into(),
        )?);
        child.stderr = Some(tokio::process::ChildStderr::from_std(
            std::os::fd::OwnedFd::from(stderr).into(),
        )?);
    }
    drop(command);
    let wait = child.wait_with_output();
    match timeout {
        Some(limit) => tokio::time::timeout(limit, wait)
            .await
            .with_context(|| format!("Command execution timed out after {}s", limit.as_secs()))?
            .context("read command output"),
        None => wait.await.context("read command output"),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dropped_execution_kills_shell_and_descendant() {
        let dir = tempfile::tempdir().unwrap();
        let mut command = Command::new("sh");
        command
            .current_dir(dir.path())
            .args(["-c", "echo $$ > shell; sleep 30 & echo $! > child; wait"]);
        let task = tokio::spawn(output(command, None));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !dir.path().join("child").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let pid = std::fs::read_to_string(dir.path().join("shell"))
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(5), async {
            // SAFETY: signal zero probes the test-owned group without changing it.
            while unsafe { libc::kill(-pid, 0) } == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("native command process group survived cancellation");
    }
}
