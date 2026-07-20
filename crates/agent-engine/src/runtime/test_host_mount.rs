use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
struct RequestRecord {
    document: serde_json::Value,
    status: String,
    grant: String,
    error: String,
    decision_in_progress: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Root,
    Requests,
    Clone,
    Request(String),
    Field(String, &'static str),
}

struct FidState {
    node: Node,
    mode: Option<OpenMode>,
    clone_id: Option<String>,
    write_buf: Vec<u8>,
    wrote: bool,
}

impl FidState {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            clone_id: None,
            write_buf: Vec::new(),
            wrote: false,
        }
    }
}

#[derive(Default)]
struct State {
    next_id: u64,
    requests: BTreeMap<String, RequestRecord>,
    fids: HashMap<Fid, FidState>,
}

/// Test-only aP implementation of the logical request/status seam.
pub(crate) struct TestHostMountFs {
    state: Mutex<State>,
}

impl TestHostMountFs {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
        })
    }

    pub(crate) async fn settle(
        &self,
        request_id: &str,
        status: &str,
        grant: Option<&str>,
        error: Option<&str>,
    ) {
        let mut state = self.state.lock().await;
        let request = state
            .requests
            .get_mut(request_id)
            .expect("test Host Mount request exists");
        assert_eq!(request.status, "pending", "terminal status is immutable");
        request.status = status.to_string();
        request.grant = grant.unwrap_or_default().to_string();
        request.error = error.unwrap_or_default().to_string();
        request.decision_in_progress = false;
    }

    pub(crate) async fn begin_decision(&self, request_id: &str) {
        let mut state = self.state.lock().await;
        let request = state
            .requests
            .get_mut(request_id)
            .expect("test Host Mount request exists");
        assert_eq!(
            request.status, "pending",
            "only pending requests can be claimed"
        );
        assert!(!request.decision_in_progress, "request is already claimed");
        request.decision_in_progress = true;
    }

    pub(crate) async fn status(&self, request_id: &str) -> Option<String> {
        self.state
            .lock()
            .await
            .requests
            .get(request_id)
            .map(|request| request.status.clone())
    }

    fn qid(node: &Node) -> Qid {
        let (kind, key) = match node {
            Node::Root => (FileKind::Dir, "/".to_string()),
            Node::Requests => (FileKind::Dir, "requests".to_string()),
            Node::Clone => (FileKind::Clone, "requests/clone".to_string()),
            Node::Request(id) => (FileKind::Dir, format!("requests/{id}")),
            Node::Field(id, field) => (FileKind::File, format!("requests/{id}/{field}")),
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        Qid {
            kind,
            version: 0,
            path: hasher.finish(),
        }
    }

    fn bytes(state: &State, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        match node {
            Node::Root => Ok(b"requests".to_vec()),
            Node::Requests => {
                let mut names = vec!["clone".to_string()];
                names.extend(state.requests.keys().cloned());
                Ok(names.join("\n").into_bytes())
            }
            Node::Request(_) => Ok(b"request\nstatus\ngrant\nerror".to_vec()),
            Node::Field(id, field) => {
                let request = state.requests.get(id).ok_or(ErrorCode::NotFound)?;
                let bytes = match *field {
                    "request" => serde_json::to_vec(&request.document).unwrap(),
                    "status" => format!("{}\n", request.status).into_bytes(),
                    "grant" => request.grant.as_bytes().to_vec(),
                    "error" => request.error.as_bytes().to_vec(),
                    _ => return Err(ErrorCode::NotFound),
                };
                Ok(bytes)
            }
            Node::Clone => Ok(Vec::new()),
        }
    }
}

#[async_trait]
impl FileServer for TestHostMountFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if newfid == Fid::ROOT {
            return Err(ErrorCode::BadRequest);
        }
        let mut state = self.state.lock().await;
        if state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = if fid == Fid::ROOT {
            Node::Root
        } else {
            state
                .fids
                .get(&fid)
                .map(|fid| fid.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        for name in names {
            node = match (&node, name.as_str()) {
                (Node::Root, "requests") => Node::Requests,
                (Node::Requests, "clone") => Node::Clone,
                (Node::Requests, id) if state.requests.contains_key(id) => {
                    Node::Request(id.to_string())
                }
                (Node::Request(id), field)
                    if matches!(field, "request" | "status" | "grant" | "error") =>
                {
                    Node::Field(
                        id.clone(),
                        match field {
                            "request" => "request",
                            "status" => "status",
                            "grant" => "grant",
                            _ => "error",
                        },
                    )
                }
                (Node::Root | Node::Requests | Node::Request(_), _) => {
                    return Err(ErrorCode::NotFound);
                }
                _ => return Err(ErrorCode::NotDirectory),
            };
        }
        let qid = Self::qid(&node);
        state.fids.insert(newfid, FidState::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT {
            return (mode == OpenMode::Read)
                .then(|| Self::qid(&Node::Root))
                .ok_or(ErrorCode::NoAccess);
        }
        let mut state = self.state.lock().await;
        let is_clone = matches!(state.fids.get(&fid).map(|fid| &fid.node), Some(Node::Clone));
        let is_status = matches!(
            state.fids.get(&fid).map(|fid| &fid.node),
            Some(Node::Field(_, "status"))
        );
        let valid_mode = (is_clone && mode == OpenMode::ReadWrite)
            || (is_status && matches!(mode, OpenMode::Read | OpenMode::Write))
            || (!is_clone && !is_status && mode == OpenMode::Read);
        if !valid_mode {
            return Err(ErrorCode::NoAccess);
        }
        let clone_id = if is_clone {
            state.next_id += 1;
            Some(format!("request-{}", state.next_id))
        } else {
            None
        };
        let fid_state = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if fid_state.mode.is_some() {
            return Err(ErrorCode::BadRequest);
        }
        fid_state.mode = Some(mode);
        fid_state.clone_id = clone_id;
        Ok(Self::qid(&fid_state.node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        let (node, clone_id, mode) = if fid == Fid::ROOT {
            (Node::Root, None, Some(OpenMode::Read))
        } else {
            let fid = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            (fid.node.clone(), fid.clone_id.clone(), fid.mode)
        };
        if !matches!(mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        let bytes = clone_id
            .map(String::into_bytes)
            .unwrap_or(Self::bytes(&state, &node)?);
        Ok(slice(bytes, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let fid = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        let valid_write = (matches!(fid.node, Node::Clone)
            && fid.mode == Some(OpenMode::ReadWrite))
            || (matches!(fid.node, Node::Field(_, "status")) && fid.mode == Some(OpenMode::Write));
        if !valid_write {
            return Err(ErrorCode::NoAccess);
        }
        let start = offset as usize;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        let max_bytes = if matches!(fid.node, Node::Clone) {
            64 * 1024
        } else {
            64
        };
        if end > max_bytes {
            return Err(ErrorCode::BadRequest);
        }
        fid.write_buf.resize(fid.write_buf.len().max(end), 0);
        fid.write_buf[start..end].copy_from_slice(data);
        fid.wrote = true;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = if fid == Fid::ROOT {
            Node::Root
        } else {
            state
                .fids
                .get(&fid)
                .map(|fid| fid.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        Ok(Stat {
            name: String::new(),
            qid: Self::qid(&node),
            length: Self::bytes(&state, &node)?.len() as u64,
            executable: false,
            writable: matches!(node, Node::Clone | Node::Field(_, "status")),
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
        let mut state = self.state.lock().await;
        let fid = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        if fid.wrote {
            match fid.node {
                Node::Clone => {
                    let document = serde_json::from_slice(&fid.write_buf)
                        .map_err(|_| ErrorCode::BadRequest)?;
                    state.requests.insert(
                        fid.clone_id.ok_or(ErrorCode::BadRequest)?,
                        RequestRecord {
                            document,
                            status: "pending".to_string(),
                            grant: String::new(),
                            error: String::new(),
                            decision_in_progress: false,
                        },
                    );
                }
                Node::Field(request_id, "status") => {
                    let command = std::str::from_utf8(&fid.write_buf)
                        .map_err(|_| ErrorCode::BadRequest)?
                        .trim();
                    if command != "cancelled" {
                        return Err(ErrorCode::BadRequest);
                    }
                    let request = state
                        .requests
                        .get_mut(&request_id)
                        .ok_or(ErrorCode::NotFound)?;
                    if request.status != "pending" || request.decision_in_progress {
                        return Err(ErrorCode::BadRequest);
                    }
                    request.status = "cancelled".to_string();
                    request.error = "cancelled by requesting Process".to_string();
                }
                _ => return Err(ErrorCode::Unsupported),
            }
        }
        Ok(())
    }
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start.saturating_add(count as usize));
    bytes[start..end].to_vec()
}
