use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use alan_kernel::Pid;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::BootManifest;

const MAX_CTL_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitStatus {
    Pending,
    Starting,
    Ready,
    Backoff,
    Exited,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemStatus {
    Booting,
    Ready,
    Degraded,
    Failed,
    Stopping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UnitSnapshot {
    pub name: String,
    pub status: UnitStatus,
    pub pid: Option<u64>,
    pub attempts: u32,
    pub error: Option<String>,
    pub degraded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestartDecision {
    Stop,
    RestartAfterMs(u64),
    FailBoot,
    Degrade,
}

#[derive(Clone, Debug)]
struct UnitState {
    status: UnitStatus,
    pid: Option<Pid>,
    attempts: u32,
    error: Option<String>,
    degraded: bool,
    ready_once: bool,
}

#[derive(Debug)]
pub struct ManagerState {
    manifest: BootManifest,
    status: SystemStatus,
    units: BTreeMap<String, UnitState>,
    retry_requests: Vec<String>,
}

impl ManagerState {
    pub fn new(manifest: BootManifest) -> Self {
        let units = manifest
            .ordered()
            .map(|unit| {
                (
                    unit.name.clone(),
                    UnitState {
                        status: UnitStatus::Pending,
                        pid: None,
                        attempts: 0,
                        error: None,
                        degraded: false,
                        ready_once: false,
                    },
                )
            })
            .collect();
        Self {
            manifest,
            status: SystemStatus::Booting,
            units,
            retry_requests: Vec::new(),
        }
    }

    pub fn status(&self) -> SystemStatus {
        self.status
    }

    pub fn start_attempt(&mut self, name: &str, pid: Pid) -> Result<(), ErrorCode> {
        let state = self.units.get_mut(name).ok_or(ErrorCode::NotFound)?;
        state.attempts = state.attempts.saturating_add(1);
        state.pid = Some(pid);
        state.status = UnitStatus::Starting;
        state.error = None;
        Ok(())
    }

    pub fn start_failure(&mut self, name: &str, error: impl Into<String>) -> Result<(), ErrorCode> {
        let state = self.units.get_mut(name).ok_or(ErrorCode::NotFound)?;
        state.attempts = state.attempts.saturating_add(1);
        state.pid = None;
        state.status = UnitStatus::Starting;
        state.error = Some(error.into());
        Ok(())
    }

    pub fn mark_ready(&mut self, name: &str) -> Result<(), ErrorCode> {
        let state = self.units.get_mut(name).ok_or(ErrorCode::NotFound)?;
        state.status = UnitStatus::Ready;
        state.ready_once = true;
        state.error = None;
        if self
            .units
            .values()
            .all(|unit| unit.status == UnitStatus::Ready)
        {
            self.status = SystemStatus::Ready;
        }
        Ok(())
    }

    pub fn mark_stopping(&mut self) {
        self.status = SystemStatus::Stopping;
    }

    pub fn note_error(&mut self, name: &str, error: impl Into<String>) -> Result<(), ErrorCode> {
        self.units.get_mut(name).ok_or(ErrorCode::NotFound)?.error = Some(error.into());
        Ok(())
    }

    pub fn record_exit(
        &mut self,
        name: &str,
        exit_code: i32,
        stable_for_ms: u64,
    ) -> Result<RestartDecision, ErrorCode> {
        let unit = self.manifest.get(name).ok_or(ErrorCode::NotFound)?;
        let required = unit.required;
        let should_restart = unit.should_restart(exit_code);
        let restart_limit = unit.restart_limit;
        let stable_reset_ms = unit.stable_reset_ms;
        let initial_backoff_ms = unit.initial_backoff_ms;
        let max_backoff_ms = unit.max_backoff_ms;
        let (ready_once, attempts, terminal) = {
            let state = self.units.get_mut(name).ok_or(ErrorCode::NotFound)?;
            state.pid = None;
            state.error = Some(format!("exit {exit_code}"));
            if stable_for_ms >= stable_reset_ms {
                state.attempts = 1;
            }
            let terminal = if !should_restart {
                state.status = UnitStatus::Exited;
                true
            } else if state.attempts > restart_limit {
                state.status = UnitStatus::Failed;
                true
            } else {
                state.status = UnitStatus::Backoff;
                false
            };
            (state.ready_once, state.attempts, terminal)
        };
        if terminal {
            return Ok(self.terminal_decision(name, required, ready_once));
        }
        let shift = attempts.saturating_sub(1).min(62);
        Ok(RestartDecision::RestartAfterMs(
            initial_backoff_ms
                .saturating_mul(1_u64 << shift)
                .min(max_backoff_ms),
        ))
    }

    fn terminal_decision(
        &mut self,
        name: &str,
        required: bool,
        ready_once: bool,
    ) -> RestartDecision {
        if !required {
            return RestartDecision::Stop;
        }
        if self.status == SystemStatus::Booting && !ready_once {
            self.status = SystemStatus::Failed;
            RestartDecision::FailBoot
        } else {
            self.status = SystemStatus::Degraded;
            if let Some(state) = self.units.get_mut(name) {
                state.degraded = true;
            }
            RestartDecision::Degrade
        }
    }

    pub fn retry(&mut self, name: &str) -> Result<(), ErrorCode> {
        let state = self.units.get_mut(name).ok_or(ErrorCode::NotFound)?;
        if !matches!(state.status, UnitStatus::Failed | UnitStatus::Exited) {
            return Err(ErrorCode::BadRequest);
        }
        state.status = UnitStatus::Pending;
        state.attempts = 0;
        state.error = None;
        state.degraded = false;
        self.retry_requests.push(name.to_string());
        self.status = if self.units.values().any(|unit| unit.degraded) {
            SystemStatus::Degraded
        } else {
            SystemStatus::Ready
        };
        Ok(())
    }

    pub fn take_retry_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.retry_requests)
    }

    pub fn unit(&self, name: &str) -> Option<UnitSnapshot> {
        let state = self.units.get(name)?;
        Some(UnitSnapshot {
            name: name.to_string(),
            status: state.status,
            pid: state.pid.map(|pid| pid.0),
            attempts: state.attempts,
            error: state.error.clone(),
            degraded: state.degraded,
        })
    }

    fn unit_names(&self) -> Vec<String> {
        self.units.keys().cloned().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Node {
    Root,
    Status,
    Degraded,
    Ctl,
    Units,
    Unit(String),
    UnitStatus(String),
    UnitPid(String),
    UnitAttempts(String),
    UnitError(String),
}

struct FidState {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
}

/// Manager-owned status and retry control tree, published at `/srv/service-manager`.
pub struct ServiceManagerFs {
    state: Arc<Mutex<ManagerState>>,
    fids: Mutex<HashMap<Fid, FidState>>,
}

impl ServiceManagerFs {
    pub fn new(state: Arc<Mutex<ManagerState>>) -> Self {
        Self {
            state,
            fids: Mutex::new(HashMap::new()),
        }
    }

    pub fn state(&self) -> Arc<Mutex<ManagerState>> {
        self.state.clone()
    }

    async fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .lock()
            .await
            .get(&fid)
            .map(|fid| fid.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    async fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "status" => Ok(Node::Status),
                "degraded" => Ok(Node::Degraded),
                "ctl" => Ok(Node::Ctl),
                "units" => Ok(Node::Units),
                _ => Err(ErrorCode::NotFound),
            },
            Node::Units if self.state.lock().await.units.contains_key(name) => {
                Ok(Node::Unit(name.to_string()))
            }
            Node::Unit(unit) => match name {
                "status" => Ok(Node::UnitStatus(unit.clone())),
                "pid" => Ok(Node::UnitPid(unit.clone())),
                "attempts" => Ok(Node::UnitAttempts(unit.clone())),
                "error" => Ok(Node::UnitError(unit.clone())),
                _ => Err(ErrorCode::NotFound),
            },
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    async fn bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        let bytes = match node {
            Node::Root => b"status\ndegraded\nctl\nunits".to_vec(),
            Node::Status => format!("{}\n", status_name(state.status)).into_bytes(),
            Node::Degraded => format!("{}\n", state.status == SystemStatus::Degraded).into_bytes(),
            Node::Ctl => b"retry <unit>\n".to_vec(),
            Node::Units => state.unit_names().join("\n").into_bytes(),
            Node::Unit(_) => b"status\npid\nattempts\nerror".to_vec(),
            Node::UnitStatus(name) => format!(
                "{}\n",
                unit_status_name(state.units.get(name).ok_or(ErrorCode::NotFound)?.status)
            )
            .into_bytes(),
            Node::UnitPid(name) => state
                .units
                .get(name)
                .ok_or(ErrorCode::NotFound)?
                .pid
                .map_or_else(|| b"\n".to_vec(), |pid| format!("{}\n", pid.0).into_bytes()),
            Node::UnitAttempts(name) => format!(
                "{}\n",
                state.units.get(name).ok_or(ErrorCode::NotFound)?.attempts
            )
            .into_bytes(),
            Node::UnitError(name) => format!(
                "{}\n",
                state
                    .units
                    .get(name)
                    .ok_or(ErrorCode::NotFound)?
                    .error
                    .as_deref()
                    .unwrap_or("")
            )
            .into_bytes(),
        };
        Ok(bytes)
    }

    async fn commit_ctl(&self, bytes: &[u8]) -> Result<(), ErrorCode> {
        let command = std::str::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let mut words = command.split_whitespace();
        let (Some("retry"), Some(unit), None) = (words.next(), words.next(), words.next()) else {
            return Err(ErrorCode::BadRequest);
        };
        self.state.lock().await.retry(unit)
    }
}

#[async_trait]
impl FileServer for ServiceManagerFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let fids = self.fids.lock().await;
        if newfid == Fid::ROOT || fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = if fid == Fid::ROOT {
            Node::Root
        } else {
            fids.get(&fid)
                .map(|fid| fid.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        drop(fids);
        for name in names {
            node = self.child(&node, name).await?;
        }
        let qid = qid(&node);
        let mut fids = self.fids.lock().await;
        if fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        fids.insert(
            newfid,
            FidState {
                node,
                mode: None,
                write_buf: Vec::new(),
            },
        );
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let node = self.node_of(fid).await?;
        if node == Node::Ctl {
            if !matches!(mode, OpenMode::Read | OpenMode::Write) {
                return Err(ErrorCode::NoAccess);
            }
        } else if !matches!(mode, OpenMode::Read) {
            return Err(ErrorCode::NoAccess);
        }
        if fid != Fid::ROOT {
            let mut fids = self.fids.lock().await;
            let state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            if state.mode.is_some() {
                return Err(ErrorCode::BadRequest);
            }
            state.mode = Some(mode);
        }
        Ok(qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if fid != Fid::ROOT {
            let fids = self.fids.lock().await;
            if !matches!(
                fids.get(&fid).ok_or(ErrorCode::NotFound)?.mode,
                Some(OpenMode::Read)
            ) {
                return Err(ErrorCode::NoAccess);
            }
        }
        let bytes = self.bytes(&self.node_of(fid).await?).await?;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = start.saturating_add(count as usize).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut fids = self.fids.lock().await;
        let state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if state.node != Node::Ctl || state.mode != Some(OpenMode::Write) {
            return Err(ErrorCode::NoAccess);
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > MAX_CTL_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        state.write_buf.resize(state.write_buf.len().max(end), 0);
        state.write_buf[start..end].copy_from_slice(data);
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_of(fid).await?;
        let length = self.bytes(&node).await?.len() as u64;
        Ok(Stat {
            name: String::new(),
            qid: qid(&node),
            length,
            writable: node == Node::Ctl,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let state = self
            .fids
            .lock()
            .await
            .remove(&fid)
            .ok_or(ErrorCode::NotFound)?;
        if state.node == Node::Ctl && state.mode == Some(OpenMode::Write) {
            self.commit_ctl(&state.write_buf).await?;
        }
        Ok(())
    }
}

fn status_name(status: SystemStatus) -> &'static str {
    match status {
        SystemStatus::Booting => "booting",
        SystemStatus::Ready => "ready",
        SystemStatus::Degraded => "degraded",
        SystemStatus::Failed => "failed",
        SystemStatus::Stopping => "stopping",
    }
}

fn unit_status_name(status: UnitStatus) -> &'static str {
    match status {
        UnitStatus::Pending => "pending",
        UnitStatus::Starting => "starting",
        UnitStatus::Ready => "ready",
        UnitStatus::Backoff => "backoff",
        UnitStatus::Exited => "exited",
        UnitStatus::Failed => "failed",
    }
}

fn qid(node: &Node) -> Qid {
    let kind = match node {
        Node::Root | Node::Units | Node::Unit(_) => FileKind::Dir,
        _ => FileKind::File,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hasher);
    Qid {
        kind,
        version: 0,
        path: hasher.finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::InProcessTransport;
    use alan_shell::Shell;

    #[tokio::test]
    async fn bounded_restart_degrades_and_ctl_retries() {
        let manifest = BootManifest::system().unwrap();
        let mut state = ManagerState::new(manifest);
        for attempt in 1..=6 {
            state.start_attempt("root-agent", Pid(attempt)).unwrap();
            state.mark_ready("root-agent").unwrap();
            let decision = state.record_exit("root-agent", 1, 0).unwrap();
            if attempt < 6 {
                assert!(matches!(decision, RestartDecision::RestartAfterMs(_)));
            } else {
                assert_eq!(decision, RestartDecision::Degrade);
            }
        }
        assert_eq!(state.status(), SystemStatus::Degraded);

        let state = Arc::new(Mutex::new(state));
        let fs = Arc::new(ServiceManagerFs::new(state.clone()));
        let shell = Shell::new(InProcessTransport::new(fs));
        shell.write("/ctl", b"retry root-agent").await.unwrap();
        assert_eq!(
            state.lock().await.unit("root-agent").unwrap().status,
            UnitStatus::Pending
        );
        assert_eq!(state.lock().await.take_retry_requests(), vec!["root-agent"]);
    }

    #[test]
    fn restart_policies_are_exact() {
        let manifest = BootManifest::system().unwrap();
        assert_eq!(
            manifest.get("root-agent").unwrap().restart,
            crate::RestartPolicy::Always
        );
        assert_eq!(
            manifest.get("route").unwrap().restart,
            crate::RestartPolicy::OnFailure
        );
    }
}
