use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use alan_kernel::{Access, Credentials, LiveNamespace, MountFs, Pid, ProcFs};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{process_runner::SystemProcessRunner, process_spawn::spawn_process};

const SHELL_EXECUTABLE: &str = "/bin/alan-shell";
const MAX_CTL_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryStatus {
    Ready,
    Drained,
}

struct Entry {
    pid: Pid,
    status: EntryStatus,
    namespace: LiveNamespace,
}

struct State {
    service_pid: Option<Pid>,
    next_id: u64,
    entries: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Node {
    Root,
    Clone,
    Entry(String),
    Status(String),
    Process(String),
    Handoff(String),
    Ctl(String),
}

struct FidState {
    node: Node,
    mode: Option<OpenMode>,
    clone_name: Option<String>,
    write_buf: Vec<u8>,
}

/// Creates ordinary Shell Processes from the fixed Login Namespace Template.
pub struct LocalEntryService {
    procfs: ProcFs,
    login_namespace: LiveNamespace,
    state: Mutex<State>,
    fids: Mutex<HashMap<Fid, FidState>>,
}

impl LocalEntryService {
    pub fn new(procfs: ProcFs, system_namespace: LiveNamespace) -> Arc<Self> {
        Arc::new(Self {
            procfs,
            login_namespace: LiveNamespace::new(system_namespace.snapshot().child()),
            state: Mutex::new(State {
                service_pid: None,
                next_id: 1,
                entries: BTreeMap::new(),
            }),
            fids: Mutex::new(HashMap::new()),
        })
    }

    pub async fn set_service_pid(&self, pid: Pid) -> Result<(), ErrorCode> {
        self.state.lock().await.service_pid = Some(pid);
        Ok(())
    }

    pub async fn handoff(&self, entry_id: &str) -> Option<(Pid, Arc<MountFs>)> {
        let state = self.state.lock().await;
        let entry = state.entries.get(entry_id)?;
        (entry.status == EntryStatus::Ready).then(|| {
            (
                entry.pid,
                Arc::new(MountFs::from_live_namespace(entry.namespace.clone())),
            )
        })
    }

    pub async fn create_and_handoff(&self) -> Result<(String, Pid, Arc<MountFs>), ErrorCode> {
        let entry_id = self.allocate_entry().await?;
        let (pid, namespace) = self.handoff(&entry_id).await.ok_or(ErrorCode::Io)?;
        Ok((entry_id, pid, namespace))
    }

    pub async fn drain_entry(&self, entry_id: &str) -> Result<(), ErrorCode> {
        self.drain(entry_id).await
    }

    async fn allocate_entry(&self) -> Result<String, ErrorCode> {
        let (service_pid, id) = {
            let mut state = self.state.lock().await;
            let service_pid = state.service_pid.ok_or(ErrorCode::Io)?;
            let id = format!("entry-{}", state.next_id);
            state.next_id += 1;
            (service_pid, id)
        };
        let namespace = LiveNamespace::new(self.login_namespace.snapshot().child());
        let pid = spawn_process(
            &self.procfs,
            Some(service_pid),
            namespace.clone(),
            Credentials::user("alan"),
            SHELL_EXECUTABLE,
        )
        .await
        .map_err(|_| ErrorCode::Io)?;
        self.procfs
            .bind_live_namespace(pid, namespace.clone())
            .await;
        let command_procfs = self
            .procfs
            .clone()
            .with_runner(Arc::new(SystemProcessRunner::new(None, None)));
        namespace.replace_mount(
            "/proc",
            alan_ap::InProcessTransport::new(Arc::new(command_procfs.for_live_spawner(
                Some(pid),
                namespace.clone(),
                Credentials::user("alan"),
            ))),
            Access::ReadWrite,
        );
        self.state.lock().await.entries.insert(
            id.clone(),
            Entry {
                pid,
                status: EntryStatus::Ready,
                namespace,
            },
        );
        Ok(id)
    }

    async fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .lock()
            .await
            .get(&fid)
            .map(|state| state.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    async fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root if name == "clone" => Ok(Node::Clone),
            Node::Root if self.state.lock().await.entries.contains_key(name) => {
                Ok(Node::Entry(name.to_string()))
            }
            Node::Root => Err(ErrorCode::NotFound),
            Node::Entry(id) => match name {
                "status" => Ok(Node::Status(id.clone())),
                "process" => Ok(Node::Process(id.clone())),
                "handoff" => Ok(Node::Handoff(id.clone())),
                "ctl" => Ok(Node::Ctl(id.clone())),
                _ => Err(ErrorCode::NotFound),
            },
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    async fn bytes(&self, node: &Node, clone_name: Option<&str>) -> Result<Vec<u8>, ErrorCode> {
        if let Node::Clone = node
            && let Some(name) = clone_name
        {
            return Ok(name.as_bytes().to_vec());
        }
        let state = self.state.lock().await;
        let bytes = match node {
            Node::Root => std::iter::once("clone".to_string())
                .chain(state.entries.keys().cloned())
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes(),
            Node::Clone => b"open to create an entry\n".to_vec(),
            Node::Entry(_) => b"status\nprocess\nhandoff\nctl".to_vec(),
            Node::Status(id) => match state.entries.get(id).ok_or(ErrorCode::NotFound)?.status {
                EntryStatus::Ready => b"ready\n".to_vec(),
                EntryStatus::Drained => b"drained\n".to_vec(),
            },
            Node::Process(id) => format!(
                "{}\n",
                state.entries.get(id).ok_or(ErrorCode::NotFound)?.pid.0
            )
            .into_bytes(),
            Node::Handoff(id) => format!(
                "process:{}\n",
                state.entries.get(id).ok_or(ErrorCode::NotFound)?.pid.0
            )
            .into_bytes(),
            Node::Ctl(_) => b"drain\n".to_vec(),
        };
        Ok(bytes)
    }

    async fn drain(&self, entry_id: &str) -> Result<(), ErrorCode> {
        let pid = {
            let mut state = self.state.lock().await;
            let entry = state.entries.get_mut(entry_id).ok_or(ErrorCode::NotFound)?;
            if entry.status == EntryStatus::Drained {
                return Err(ErrorCode::BadRequest);
            }
            entry.status = EntryStatus::Drained;
            entry.pid
        };
        self.procfs.record_exit(pid, 0).await;
        Ok(())
    }
}

#[async_trait]
impl FileServer for LocalEntryService {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let fids = self.fids.lock().await;
        if newfid == Fid::ROOT || fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = if fid == Fid::ROOT {
            Node::Root
        } else {
            fids.get(&fid)
                .map(|state| state.node.clone())
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
                clone_name: None,
                write_buf: Vec::new(),
            },
        );
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let node = self.node_of(fid).await?;
        let allowed = match &node {
            Node::Ctl(_) => matches!(mode, OpenMode::Read | OpenMode::Write),
            _ => mode == OpenMode::Read,
        };
        if !allowed {
            return Err(ErrorCode::NoAccess);
        }
        let clone_name = if node == Node::Clone {
            Some(self.allocate_entry().await?)
        } else {
            None
        };
        if fid != Fid::ROOT {
            let mut fids = self.fids.lock().await;
            let state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            if state.mode.is_some() {
                return Err(ErrorCode::BadRequest);
            }
            state.mode = Some(mode);
            state.clone_name = clone_name;
        }
        Ok(qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, clone_name) = if fid == Fid::ROOT {
            (Node::Root, None)
        } else {
            let fids = self.fids.lock().await;
            let state = fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if state.mode != Some(OpenMode::Read) {
                return Err(ErrorCode::NoAccess);
            }
            (state.node.clone(), state.clone_name.clone())
        };
        let bytes = self.bytes(&node, clone_name.as_deref()).await?;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = start.saturating_add(count as usize).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut fids = self.fids.lock().await;
        let state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(state.node, Node::Ctl(_)) || state.mode != Some(OpenMode::Write) {
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
        let length = self.bytes(&node, None).await?.len() as u64;
        Ok(Stat {
            name: String::new(),
            qid: qid(&node),
            length,
            executable: false,
            writable: matches!(node, Node::Ctl(_)),
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
        if let Node::Ctl(entry_id) = state.node
            && state.mode == Some(OpenMode::Write)
        {
            if std::str::from_utf8(&state.write_buf)
                .map_err(|_| ErrorCode::BadRequest)?
                .trim()
                != "drain"
            {
                return Err(ErrorCode::BadRequest);
            }
            self.drain(&entry_id).await?;
        }
        Ok(())
    }
}

fn qid(node: &Node) -> Qid {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hasher);
    Qid {
        kind: match node {
            Node::Root | Node::Entry(_) => FileKind::Dir,
            Node::Clone => FileKind::Clone,
            _ => FileKind::File,
        },
        version: 0,
        path: hasher.finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::{InProcessTransport, Request, Response};
    use alan_kernel::{ExecSpec, Namespace, Status};
    use alan_shell::Shell;

    #[tokio::test]
    async fn shell_is_child_of_entry_service_and_drain_keeps_agent_child() {
        let procfs = ProcFs::new();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/proc",
            InProcessTransport::new(Arc::new(procfs.for_spawner(
                None,
                Namespace::new(),
                Credentials::system(),
            ))),
            Access::ReadWrite,
        );
        for executable in [
            "/bin/local-entry-service",
            SHELL_EXECUTABLE,
            "/bin/alan-agent",
        ] {
            namespace.mount(
                executable,
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                Access::ReadOnly,
            );
        }
        let system = LiveNamespace::new(namespace);
        let service_pid = spawn_process(
            &procfs,
            None,
            system.clone(),
            Credentials::system(),
            "/bin/local-entry-service",
        )
        .await
        .unwrap();
        let service = LocalEntryService::new(procfs.clone(), system);
        service.set_service_pid(service_pid).await.unwrap();
        let service_transport = InProcessTransport::new(service.clone());
        service_transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(90),
                names: vec!["clone".to_string()],
            })
            .await
            .unwrap();
        service_transport
            .call(Request::Open {
                fid: Fid(90),
                mode: OpenMode::Read,
            })
            .await
            .unwrap();
        let Response::Read { data } = service_transport
            .call(Request::Read {
                fid: Fid(90),
                offset: 0,
                count: 64,
            })
            .await
            .unwrap()
        else {
            panic!("clone read response");
        };
        let entry_id = String::from_utf8(data).unwrap();
        service_transport
            .call(Request::Clunk { fid: Fid(90) })
            .await
            .unwrap();
        let shell = Shell::new(service_transport);
        let (shell_pid, shell_fs) = service.handoff(&entry_id).await.unwrap();
        let process_shell = Shell::new(InProcessTransport::new(shell_fs));
        assert_eq!(
            String::from_utf8(
                process_shell
                    .cat(&format!("/proc/{}/parent", shell_pid.0))
                    .await
                    .unwrap()
            )
            .unwrap()
            .trim(),
            service_pid.0.to_string()
        );
        let agent_pid = process_shell
            .spawn(
                &serde_json::to_string(&ExecSpec {
                    executable: "/bin/alan-agent".to_string(),
                    args: Vec::new(),
                    namespace: None,
                    descriptors: Default::default(),
                })
                .unwrap(),
            )
            .await
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert_eq!(
            procfs.try_observe_process_lifecycle(shell_pid),
            Some((Status::Running, None))
        );
        shell
            .write(&format!("/{entry_id}/ctl"), b"drain")
            .await
            .unwrap();
        assert_eq!(
            procfs.try_observe_process_lifecycle(shell_pid),
            Some((Status::Exited, Some(0)))
        );
        assert_eq!(
            procfs.try_observe_process_lifecycle(Pid(agent_pid)),
            Some((Status::Running, None))
        );
    }
}
