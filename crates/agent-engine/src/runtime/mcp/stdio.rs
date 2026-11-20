//! Confined stdio MCP JSON-RPC 2.0 subprocess transport.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::{Mutex, oneshot};

use crate::runtime::tool_packages::configure_sanitized_subprocess;
use crate::tools::Sandbox;

use super::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use super::transport::{McpStdioTransportConfig, McpTransport};

const MAX_STDERR_LINES: usize = 64;
const MAX_LINE_BYTES: usize = 2 * 1024 * 1024;

struct ProcessGuard {
    child: Mutex<Option<Child>>,
    #[cfg(unix)]
    pgid: Mutex<Option<i32>>,
}

impl ProcessGuard {
    async fn kill_group(&self) {
        #[cfg(unix)]
        {
            if let Some(pid) = *self.pgid.lock().await {
                // SAFETY: `pid` is the positive OS process group ID initialized via `process_group(0)`
                // for the still-tracked child process; sending SIGKILL terminates any orphaned children.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
            }
        }
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Ok(mut guard) = self.pgid.try_lock()
            && let Some(pid) = guard.take()
        {
            // SAFETY: `pid` is the positive OS process group ID owned by this guard; on Drop we
            // terminate the process group before clearing it so subprocesses do not leak.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
        if let Ok(mut guard) = self.child.try_lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.start_kill();
        }
    }
}

/// JSON-RPC 2.0 client transport communicating with one confined MCP stdio subprocess.
pub(crate) struct StdioMcpTransport {
    server_id: String,
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
    next_id: AtomicU64,
    guard: Arc<ProcessGuard>,
    reader_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    stderr_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl StdioMcpTransport {
    pub(crate) async fn spawn(
        server_id: &str,
        config: &McpStdioTransportConfig,
        sandbox: &Sandbox,
    ) -> Result<Self> {
        std::fs::create_dir_all(&config.scratch_dir).with_context(|| {
            format!(
                "failed to prepare MCP scratch dir `{}`",
                config.scratch_dir.display()
            )
        })?;
        let mut command = sandbox
            .build_confined_program_command(
                &config.executable,
                &config.args,
                &config.cwd,
                config.allow_network,
            )
            .with_context(|| format!("failed to confine MCP stdio server `{server_id}`"))?;
        configure_sanitized_subprocess(&mut command, &config.scratch_dir, server_id);
        command.stdin(std::process::Stdio::piped());
        command.env("ALAN_MCP_SERVER", server_id);
        for (key, value) in &config.env {
            command.env(key, value);
        }

        let mut child = command.spawn().with_context(|| {
            format!(
                "failed to spawn MCP stdio server `{server_id}` at `{}`",
                config.executable.display()
            )
        })?;
        #[cfg(unix)]
        let pgid = child.id().and_then(|id| i32::try_from(id).ok());

        let stdin = child
            .stdin
            .take()
            .context("MCP stdio child missing piped stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("MCP stdio child missing piped stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("MCP stdio child missing piped stderr")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let stderr_lines = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_STDERR_LINES)));

        let stderr_task = tokio::spawn(drain_stderr(stderr, Arc::clone(&stderr_lines)));
        let reader_task = tokio::spawn(read_stdout_responses(
            server_id.to_string(),
            stdout,
            Arc::clone(&pending),
            Arc::clone(&stderr_lines),
        ));

        Ok(Self {
            server_id: server_id.to_string(),
            stdin: Mutex::new(stdin),
            pending,
            stderr_lines,
            next_id: AtomicU64::new(1),
            guard: Arc::new(ProcessGuard {
                child: Mutex::new(Some(child)),
                #[cfg(unix)]
                pgid: Mutex::new(pgid),
            }),
            reader_task: Mutex::new(Some(reader_task)),
            stderr_task: Mutex::new(Some(stderr_task)),
        })
    }

    async fn write_line(&self, payload: &[u8]) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(payload).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn recent_stderr(&self) -> String {
        let lines = self.stderr_lines.lock().await;
        lines.iter().cloned().collect::<Vec<_>>().join(" | ")
    }
}

#[async_trait]
impl McpTransport for StdioMcpTransport {
    async fn request(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };
        let encoded = serde_json::to_vec(&request)?;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        if let Err(error) = self.write_line(&encoded).await {
            self.pending.lock().await.remove(&id);
            let stderr = self.recent_stderr().await;
            return Err(anyhow!(
                "failed writing MCP stdio request `{method}` for `{}`: {error}{}",
                self.server_id,
                format_stderr_suffix(&stderr)
            ));
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                let stderr = self.recent_stderr().await;
                bail!(
                    "MCP stdio server `{}` closed before answering `{method}`{}",
                    self.server_id,
                    format_stderr_suffix(&stderr)
                )
            }
            Err(_) => {
                self.pending.lock().await.remove(&id);
                bail!(
                    "MCP stdio server `{}` timed out answering `{method}` after {}s",
                    self.server_id,
                    timeout.as_secs()
                )
            }
        }
    }

    async fn notify(&self, method: &str, params: Option<Value>, timeout: Duration) -> Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };
        let encoded = serde_json::to_vec(&notification)?;
        tokio::time::timeout(timeout, self.write_line(&encoded))
            .await
            .map_err(|_| {
                anyhow!(
                    "MCP stdio notification `{method}` timed out for `{}`",
                    self.server_id
                )
            })??;
        Ok(())
    }

    async fn shutdown(&self) {
        self.guard.kill_group().await;
        if let Some(handle) = self.reader_task.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.stderr_task.lock().await.take() {
            handle.abort();
        }
        let mut pending = self.pending.lock().await;
        for (_, waiter) in pending.drain() {
            let _ = waiter.send(Err(anyhow!(
                "MCP stdio server `{}` shut down",
                self.server_id
            )));
        }
    }
}

async fn read_stdout_responses(
    server_id: String,
    stdout: ChildStdout,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
) {
    let mut lines = BufReader::new(stdout).lines();
    let reason = loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if line.len() > MAX_LINE_BYTES {
                    break "MCP stdio response exceeded maximum frame size".to_string();
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(response) = serde_json::from_str::<JsonRpcResponse>(trimmed) else {
                    tracing::debug!(
                        server = %server_id,
                        line_len = trimmed.len(),
                        "ignoring non-JSON-RPC stdio line from MCP server"
                    );
                    continue;
                };
                let Some(id) = response.id.as_ref().and_then(Value::as_u64) else {
                    continue;
                };
                let Some(waiter) = pending.lock().await.remove(&id) else {
                    continue;
                };
                let outcome = if let Some(error) = response.error {
                    Err(anyhow!(
                        "MCP error {} on `{server_id}`: {}",
                        error.code,
                        error.message
                    ))
                } else {
                    Ok(response.result.unwrap_or(Value::Null))
                };
                let _ = waiter.send(outcome);
            }
            Ok(None) => break format!("MCP stdio server `{server_id}` closed stdout"),
            Err(error) => break format!("MCP stdio read failed for `{server_id}`: {error}"),
        }
    };

    let stderr = {
        let lines = stderr_lines.lock().await;
        lines.iter().cloned().collect::<Vec<_>>().join(" | ")
    };
    let full_error = format!("{reason}{}", format_stderr_suffix(&stderr));
    let mut waiters = pending.lock().await;
    for (_, waiter) in waiters.drain() {
        let _ = waiter.send(Err(anyhow!(full_error.clone())));
    }
}

async fn drain_stderr(stderr: ChildStderr, sink: Arc<Mutex<VecDeque<String>>>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut bounded = trimmed.chars().take(512).collect::<String>();
        if trimmed.chars().count() > 512 {
            bounded.push('…');
        }
        let mut sink = sink.lock().await;
        if sink.len() >= MAX_STDERR_LINES {
            sink.pop_front();
        }
        sink.push_back(bounded);
    }
}

fn format_stderr_suffix(stderr: &str) -> String {
    if stderr.trim().is_empty() {
        String::new()
    } else {
        format!(" (stderr: {})", stderr.trim())
    }
}
