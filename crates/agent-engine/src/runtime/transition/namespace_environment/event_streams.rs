//! Event-driven stream watchers over `/proc/<pid>/wait` and `/agent/<pid>/events`.
//!
//! Watchers run in dedicated Tokio tasks and forward notifications over `watch`
//! and `oneshot` channels so callers multiplexing with `tokio::select!` never
//! hold a pinned `MountFs` future across unrelated arms (see `MountFs` lock
//! discipline). Dropping [`ChildEventWatchers`] aborts its reader tasks cleanly
//! and clunks any opened stream fids via `NamespaceFidGuard`.

use alan_ap::OpenMode;
use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;

use super::client::NamespaceClient;
use super::{NamespaceAgentFiles, NamespaceProcessFiles};

/// Single-line terminal JSON record published by `/proc/<pid>/wait`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ProcessWaitEvent {
    #[allow(dead_code, reason = "Part of canonical /proc/<pid>/wait JSON schema")]
    pub(crate) pid: u64,
    #[allow(dead_code, reason = "Part of canonical /proc/<pid>/wait JSON schema")]
    pub(crate) status: String,
    pub(crate) exit_code: i32,
    #[allow(dead_code, reason = "Part of canonical /proc/<pid>/wait JSON schema")]
    pub(crate) reason: String,
}

impl NamespaceProcessFiles {
    /// Block on `/proc/<pid>/wait` until the process exits (or return immediately
    /// if the process has already exited).
    pub(crate) async fn wait_for_process_exit(&self, pid: &str) -> Result<ProcessWaitEvent> {
        let wait_path = format!("/proc/{pid}/wait");
        let client = NamespaceClient::new(self.root.clone());
        let fid = client
            .open_path_guarded(&wait_path, OpenMode::Read)
            .await
            .with_context(|| format!("open {wait_path}"))?;
        let read_res = async {
            let mut bytes = Vec::new();
            let mut offset = 0_u64;
            loop {
                let chunk = client
                    .read_at(fid.fid(), offset, 4096)
                    .await
                    .with_context(|| format!("read {wait_path}"))?;
                if chunk.is_empty() {
                    break;
                }
                offset += chunk.len() as u64;
                bytes.extend_from_slice(&chunk);
                if bytes.contains(&b'\n') {
                    break;
                }
            }
            let line = std::str::from_utf8(&bytes)
                .with_context(|| format!("decode {wait_path} utf8"))?
                .lines()
                .next()
                .with_context(|| format!("empty {wait_path} stream"))?;
            serde_json::from_str::<ProcessWaitEvent>(line)
                .with_context(|| format!("parse {wait_path} record"))
        }
        .await;
        let _ = fid.close().await;
        read_res
    }
}

/// Wakeup delivered by [`ChildEventWatchers::next_wake`].
pub(crate) enum ChildEventWake {
    /// `/proc/<pid>/wait` delivered a terminal exit record (or failed to open/read).
    ProcessExited(Result<ProcessWaitEvent>),
    /// `/agent/<pid>/events` appended new bytes.
    AgentEvents,
    /// Both `/proc/<pid>/wait` and `/agent/<pid>/events` ended without further updates.
    StreamsClosed,
}

/// Multiplexed background stream watchers for one child Process (`/proc/<pid>/wait`
/// and `/agent/<pid>/events`). Dropping this handle aborts both reader tasks.
pub(crate) struct ChildEventWatchers {
    wait_rx: Option<oneshot::Receiver<Result<ProcessWaitEvent>>>,
    events_rx: watch::Receiver<u64>,
    events_open: bool,
    wait_task: JoinHandle<()>,
    events_task: JoinHandle<()>,
}

impl ChildEventWatchers {
    pub(crate) fn spawn(
        process_files: &NamespaceProcessFiles,
        agent_files: &NamespaceAgentFiles,
        pid: &str,
    ) -> Self {
        let (wait_tx, wait_rx) = oneshot::channel();
        let wait_task = {
            let process_files = process_files.clone();
            let pid = pid.to_string();
            tokio::spawn(async move {
                let result = process_files.wait_for_process_exit(&pid).await;
                let _ = wait_tx.send(result);
            })
        };

        let (events_tx, events_rx) = watch::channel(0_u64);
        let events_task = {
            let root = agent_files.root.clone();
            let events_path = format!("{}/events", agent_files.agent_path);
            tokio::spawn(async move {
                let client = NamespaceClient::new(root);
                let Ok(fid) = client.open_path_guarded(&events_path, OpenMode::Read).await else {
                    return;
                };
                let mut offset = 0_u64;
                loop {
                    let Ok(chunk) = client.read_at(fid.fid(), offset, 4096).await else {
                        break;
                    };
                    if chunk.is_empty() {
                        break;
                    }
                    offset += chunk.len() as u64;
                    events_tx.send_modify(|version| *version = version.saturating_add(1));
                }
                let _ = fid.close().await;
            })
        };

        Self {
            wait_rx: Some(wait_rx),
            events_rx,
            events_open: true,
            wait_task,
            events_task,
        }
    }

    /// Snapshot the current `/agent/<pid>/events` watch generation before inspecting
    /// files so any event arriving concurrently with inspection triggers `next_wake`.
    pub(crate) fn mark_events_seen(&mut self) {
        let _ = self.events_rx.borrow_and_update();
    }

    /// Whether at least one underlying stream watcher is still active.
    pub(crate) fn has_open_stream(&self) -> bool {
        self.wait_rx.is_some() || self.events_open
    }

    /// Wait for the next notification from `/proc/<pid>/wait` or `/agent/<pid>/events`.
    /// Cancel-safe for use inside `tokio::select!`.
    pub(crate) async fn next_wake(&mut self) -> ChildEventWake {
        loop {
            if !self.has_open_stream() {
                return ChildEventWake::StreamsClosed;
            }
            tokio::select! {
                biased;
                res = async {
                    self.wait_rx
                        .as_mut()
                        .expect("guarded by is_some")
                        .await
                }, if self.wait_rx.is_some() => {
                    self.wait_rx = None;
                    if let Ok(exit_res) = res {
                        return ChildEventWake::ProcessExited(exit_res);
                    }
                }
                res = self.events_rx.changed(), if self.events_open => {
                    match res {
                        Ok(()) => return ChildEventWake::AgentEvents,
                        Err(_) => {
                            self.events_open = false;
                        }
                    }
                }
            }
        }
    }
}

impl Drop for ChildEventWatchers {
    fn drop(&mut self) {
        self.wait_task.abort();
        self.events_task.abort();
    }
}
