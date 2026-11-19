//! Native command lifetime follows the bounded Tool Process, including descendants.
use anyhow::{Context, Result};
use std::{
    process::Output,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const LIVE_OUTPUT_TAIL_BYTES: usize = 16_000;
const LIVE_OUTPUT_FLUSH_INTERVAL: Duration = Duration::from_millis(80);

/// Callback invoked incrementally while a command runs with `(stdout_tail, stderr_tail, truncated)`.
pub type OutputProgressCallback = Arc<dyn Fn(&str, &str, bool) + Send + Sync>;

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
#[cfg(test)]
pub(in crate::tools) async fn output(
    command: Command,
    timeout: Option<Duration>,
) -> Result<Output> {
    output_with_progress(command, timeout, None, None).await
}

/// Capture caller-provided pipe readers when the child must inherit named pipes.
#[cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "Named-pipe capture is used by Linux reified execution and tests"
    )
)]
pub(in crate::tools) async fn output_with_capture(
    command: Command,
    timeout: Option<Duration>,
    capture: Option<(std::fs::File, std::fs::File)>,
) -> Result<Output> {
    output_with_progress(command, timeout, capture, None).await
}

/// Execute a command, optionally streaming incremental stdout/stderr tail snapshots.
pub(in crate::tools) async fn output_with_progress(
    mut command: Command,
    timeout: Option<Duration>,
    capture: Option<(std::fs::File, std::fs::File)>,
    progress: Option<OutputProgressCallback>,
) -> Result<Output> {
    // Nested shells must not re-import functions or startup hooks after the outer
    // shell has been checked. Keep ordinary Host environment/PATH values intact.
    let shell_hooks: Vec<_> = command
        .as_std()
        .get_envs()
        .map(|(name, _)| name.to_os_string())
        .chain(std::env::vars_os().map(|(name, _)| name))
        .filter(|name| {
            name.to_str()
                .is_some_and(super::command_wrappers::is_shell_startup_variable)
        })
        .collect();
    for name in shell_hooks {
        command.env_remove(name);
    }
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
    let wait = async move {
        if let Some(callback) = progress {
            wait_streaming_output(child, callback).await
        } else {
            child
                .wait_with_output()
                .await
                .context("read command output")
        }
    };
    match timeout {
        Some(limit) => tokio::time::timeout(limit, wait)
            .await
            .with_context(|| format!("Command execution timed out after {}s", limit.as_secs()))?,
        None => wait.await,
    }
}

async fn wait_streaming_output(
    mut child: tokio::process::Child,
    callback: OutputProgressCallback,
) -> Result<Output> {
    let mut stdout_reader = child.stdout.take();
    let mut stderr_reader = child.stderr.take();
    let mut stdout_full = Vec::new();
    let mut stderr_full = Vec::new();
    let mut stdout_tail = String::new();
    let mut stderr_tail = String::new();
    let mut truncated = false;
    let mut dirty = false;
    let mut last_flush = Instant::now() - LIVE_OUTPUT_FLUSH_INTERVAL;
    let mut stdout_buf = [0u8; 4096];
    let mut stderr_buf = [0u8; 4096];

    while stdout_reader.is_some() || stderr_reader.is_some() {
        let mut flush_tick = tokio::time::interval(LIVE_OUTPUT_FLUSH_INTERVAL);
        flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Consume the immediate initial tick only if not dirty; if dirty and elapsed, flush below.
        tokio::select! {
            read_res = async {
                match stdout_reader.as_mut() {
                    Some(reader) => reader.read(&mut stdout_buf).await,
                    None => std::future::pending().await,
                }
            }, if stdout_reader.is_some() => {
                match read_res.context("read command stdout")? {
                    0 => stdout_reader = None,
                    n => {
                        let chunk = &stdout_buf[..n];
                        stdout_full.extend_from_slice(chunk);
                        append_bounded_tail(&mut stdout_tail, &mut truncated, chunk);
                        dirty = true;
                    }
                }
            }
            read_res = async {
                match stderr_reader.as_mut() {
                    Some(reader) => reader.read(&mut stderr_buf).await,
                    None => std::future::pending().await,
                }
            }, if stderr_reader.is_some() => {
                match read_res.context("read command stderr")? {
                    0 => stderr_reader = None,
                    n => {
                        let chunk = &stderr_buf[..n];
                        stderr_full.extend_from_slice(chunk);
                        append_bounded_tail(&mut stderr_tail, &mut truncated, chunk);
                        dirty = true;
                    }
                }
            }
            _ = flush_tick.tick(), if dirty => {}
        }
        if dirty && last_flush.elapsed() >= LIVE_OUTPUT_FLUSH_INTERVAL {
            callback(&stdout_tail, &stderr_tail, truncated);
            dirty = false;
            last_flush = Instant::now();
        }
    }

    if dirty {
        callback(&stdout_tail, &stderr_tail, truncated);
    }
    let status = child.wait().await.context("wait for command exit")?;
    Ok(Output {
        status,
        stdout: stdout_full,
        stderr: stderr_full,
    })
}

fn append_bounded_tail(tail: &mut String, truncated: &mut bool, chunk: &[u8]) {
    let text = String::from_utf8_lossy(chunk);
    tail.push_str(&text);
    if tail.len() > LIVE_OUTPUT_TAIL_BYTES {
        *truncated = true;
        let mut drop_prefix = tail.len() - LIVE_OUTPUT_TAIL_BYTES;
        while drop_prefix < tail.len() && !tail.is_char_boundary(drop_prefix) {
            drop_prefix += 1;
        }
        tail.drain(..drop_prefix);
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

    #[tokio::test]
    async fn output_with_progress_streams_incremental_tail_before_exit() {
        let updates = Arc::new(std::sync::Mutex::new(Vec::<(String, String, bool)>::new()));
        let sink = Arc::clone(&updates);
        let cb: OutputProgressCallback = Arc::new(move |out: &str, err: &str, trunc: bool| {
            sink.lock()
                .expect("updates lock")
                .push((out.to_string(), err.to_string(), trunc));
        });
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "printf 'first\\n'; sleep 0.15; printf 'err\\n' >&2; sleep 0.15; printf 'second\\n'",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output = output_with_progress(command, Some(Duration::from_secs(5)), None, Some(cb))
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "first\nsecond\n");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "err\n");
        let observed = updates.lock().expect("updates lock");
        assert!(
            !observed.is_empty(),
            "expected incremental progress updates"
        );
        assert!(
            observed.last().unwrap().0.contains("second"),
            "final live snapshot should include trailing output: {observed:?}"
        );
    }
}
