use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;
use tokio::sync::watch;

use super::HostMountService;

const MAX_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub(super) struct HostMountEventStream {
    inner: Arc<EventStreamInner>,
}

pub(super) struct HostMountEventStreams {
    all: HostMountEventStream,
    by_process: Mutex<BTreeMap<u64, HostMountEventStream>>,
}

impl HostMountEventStreams {
    pub(super) fn new() -> Self {
        Self {
            all: HostMountEventStream::new(),
            by_process: Mutex::new(BTreeMap::new()),
        }
    }

    pub(super) fn stream(&self, pid: Option<u64>) -> HostMountEventStream {
        let Some(pid) = pid else {
            return self.all.clone();
        };
        self.by_process
            .lock()
            .unwrap()
            .entry(pid)
            .or_insert_with(HostMountEventStream::new)
            .clone()
    }

    pub(super) fn append_for(&self, pid: u64, bytes: &[u8]) {
        self.all.append(bytes);
        self.stream(Some(pid)).append(bytes);
    }

    pub(super) fn append_for_many(&self, pids: &[u64], bytes: &[u8]) {
        self.all.append(bytes);
        for pid in pids.iter().copied().collect::<BTreeSet<_>>() {
            self.stream(Some(pid)).append(bytes);
        }
    }
}

struct EventStreamInner {
    bytes: Mutex<Vec<u8>>,
    length: watch::Sender<u64>,
}

impl HostMountEventStream {
    pub(super) fn new() -> Self {
        let (length, _) = watch::channel(0);
        Self {
            inner: Arc::new(EventStreamInner {
                bytes: Mutex::new(Vec::new()),
                length,
            }),
        }
    }

    pub(super) fn append(&self, bytes: &[u8]) {
        let length = {
            let mut retained = self.inner.bytes.lock().unwrap();
            retained.extend_from_slice(bytes);
            retained.len() as u64
        };
        let _ = self.inner.length.send(length);
    }

    pub(super) fn len(&self) -> u64 {
        self.inner.bytes.lock().unwrap().len() as u64
    }

    pub(super) async fn read(&self, offset: Offset, count: u32) -> Vec<u8> {
        if count == 0 {
            return Vec::new();
        }
        let mut changes = self.inner.length.subscribe();
        loop {
            {
                let retained = self.inner.bytes.lock().unwrap();
                let start = offset as usize;
                if start < retained.len() {
                    let end = retained.len().min(start.saturating_add(count as usize));
                    return retained[start..end].to_vec();
                }
            }
            if changes.changed().await.is_err() {
                return Vec::new();
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Root,
    Requests,
    RequestsClone,
    RequestsEvents,
    Request(String),
    RequestField(String, RequestField),
    Grants,
    Grant(String),
    GrantRecord(String),
    Events,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestField {
    Request,
    Status,
    Grant,
    Error,
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

/// Per-mount aP view over the shared Host Mount Service state.
///
/// A Process-scoped view binds request creation to the caller PID. The global
/// `/srv/host-mount` view remains readable for Host-side inspection but cannot
/// allocate requests because aP fids intentionally carry no ambient caller ID.
pub(super) struct HostMountFs {
    service: Arc<HostMountService>,
    requesting_pid: Option<u64>,
    fids: tokio::sync::Mutex<HashMap<Fid, FidState>>,
}

impl HostMountFs {
    pub(super) fn new(service: Arc<HostMountService>, requesting_pid: Option<u64>) -> Self {
        Self {
            service,
            requesting_pid,
            fids: tokio::sync::Mutex::new(HashMap::new()),
        }
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

    fn child(&self, parent: &Node, name: &str) -> Result<Node, ErrorCode> {
        match parent {
            Node::Root => match name {
                "requests" => Ok(Node::Requests),
                "grants" => Ok(Node::Grants),
                "events" => Ok(Node::Events),
                _ => Err(ErrorCode::NotFound),
            },
            Node::Requests => match name {
                "clone" => Ok(Node::RequestsClone),
                "events" => Ok(Node::RequestsEvents),
                id if self.service.request_is_visible_to(id, self.requesting_pid) => {
                    Ok(Node::Request(id.to_string()))
                }
                _ => Err(ErrorCode::NotFound),
            },
            Node::Request(id) => match name {
                "request" => Ok(Node::RequestField(id.clone(), RequestField::Request)),
                "status" => Ok(Node::RequestField(id.clone(), RequestField::Status)),
                "grant" => Ok(Node::RequestField(id.clone(), RequestField::Grant)),
                "error" => Ok(Node::RequestField(id.clone(), RequestField::Error)),
                _ => Err(ErrorCode::NotFound),
            },
            Node::Grants if self.service.grant_is_visible_to(name, self.requesting_pid) => {
                Ok(Node::Grant(name.to_string()))
            }
            Node::Grant(id) if name == "record" => Ok(Node::GrantRecord(id.clone())),
            Node::RequestsClone
            | Node::RequestsEvents
            | Node::RequestField(..)
            | Node::GrantRecord(_)
            | Node::Events
            | Node::Grants
            | Node::Grant(_) => Err(ErrorCode::NotDirectory),
        }
    }

    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => b"requests\ngrants\nevents".to_vec(),
            Node::Requests => {
                let mut names = vec!["clone".to_string(), "events".to_string()];
                names.extend(self.service.request_ids_visible_to(self.requesting_pid));
                names.join("\n").into_bytes()
            }
            Node::Request(_id) => b"request\nstatus\ngrant\nerror".to_vec(),
            Node::RequestField(id, field) => {
                let request = self
                    .service
                    .request_snapshot(id)
                    .ok_or(ErrorCode::NotFound)?;
                match field {
                    RequestField::Request => {
                        serde_json::to_vec(&request.request).map_err(|_| ErrorCode::Io)?
                    }
                    RequestField::Status => format!("{}\n", request.status.as_str()).into_bytes(),
                    RequestField::Grant => request
                        .grant
                        .map(|grant| format!("{grant}\n").into_bytes())
                        .unwrap_or_default(),
                    RequestField::Error => request
                        .error
                        .map(|error| format!("{error}\n").into_bytes())
                        .unwrap_or_default(),
                }
            }
            Node::Grants => self
                .service
                .grant_ids_visible_to(self.requesting_pid)
                .join("\n")
                .into_bytes(),
            Node::Grant(_) => b"record".to_vec(),
            Node::GrantRecord(id) => {
                serde_json::to_vec(&self.service.grant_record(id).ok_or(ErrorCode::NotFound)?)
                    .map_err(|_| ErrorCode::Io)?
            }
            Node::RequestsClone | Node::RequestsEvents | Node::Events => Vec::new(),
        };
        Ok(bytes)
    }
}

#[async_trait]
impl FileServer for HostMountFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if newfid == Fid::ROOT {
            return Err(ErrorCode::BadRequest);
        }
        let mut fids = self.fids.lock().await;
        if fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = if fid == Fid::ROOT {
            Node::Root
        } else {
            fids.get(&fid)
                .map(|state| state.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        for name in names {
            node = self.child(&node, name)?;
        }
        let qid = qid(&node, self.service.generation());
        fids.insert(newfid, FidState::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT {
            return if mode == OpenMode::Read {
                Ok(qid(&Node::Root, self.service.generation()))
            } else {
                Err(ErrorCode::NoAccess)
            };
        }
        let mut fids = self.fids.lock().await;
        let state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if state.mode.is_some() {
            return Err(ErrorCode::BadRequest);
        }
        match &state.node {
            Node::RequestsClone => {
                if mode != OpenMode::ReadWrite || self.requesting_pid.is_none() {
                    return Err(ErrorCode::NoAccess);
                }
                state.clone_id = Some(self.service.allocate_request_id());
            }
            _ if mode != OpenMode::Read => return Err(ErrorCode::NoAccess),
            _ => {}
        }
        state.mode = Some(mode);
        Ok(qid(&state.node, self.service.generation()))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, clone_id, mode) = if fid == Fid::ROOT {
            (Node::Root, None, Some(OpenMode::Read))
        } else {
            let fids = self.fids.lock().await;
            let state = fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            (state.node.clone(), state.clone_id.clone(), state.mode)
        };
        if !matches!(mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if let Some(id) = clone_id {
            return Ok(slice(id.into_bytes(), offset, count));
        }
        match node {
            Node::RequestsEvents => Ok(self
                .service
                .request_events(self.requesting_pid)
                .read(offset, count)
                .await),
            Node::Events => Ok(self
                .service
                .events(self.requesting_pid)
                .read(offset, count)
                .await),
            other => Ok(slice(self.computed_bytes(&other)?, offset, count)),
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut fids = self.fids.lock().await;
        let state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(state.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if !matches!(state.node, Node::RequestsClone) {
            return Err(ErrorCode::Unsupported);
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > MAX_REQUEST_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        if state.write_buf.len() < end {
            state.write_buf.resize(end, 0);
        }
        state.write_buf[start..end].copy_from_slice(data);
        state.wrote = true;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_of(fid).await?;
        let length = match &node {
            Node::RequestsEvents => self.service.request_events(self.requesting_pid).len(),
            Node::Events => self.service.events(self.requesting_pid).len(),
            _ => self.computed_bytes(&node)?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: qid(&node, self.service.generation()),
            length,
            executable: false,
            writable: matches!(node, Node::RequestsClone) && self.requesting_pid.is_some(),
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
        if matches!(state.node, Node::RequestsClone) && state.wrote {
            let pid = self.requesting_pid.ok_or(ErrorCode::NoAccess)?;
            let request_id = state.clone_id.ok_or(ErrorCode::BadRequest)?;
            self.service
                .commit_request(pid, request_id, &state.write_buf)?;
        }
        Ok(())
    }
}

fn qid(node: &Node, generation: u32) -> Qid {
    let (kind, stable_key, mutable) = match node {
        Node::Root => (FileKind::Dir, "/".to_string(), true),
        Node::Requests => (FileKind::Dir, "requests".to_string(), true),
        Node::RequestsClone => (FileKind::Clone, "requests/clone".to_string(), false),
        Node::RequestsEvents => (FileKind::Stream, "requests/events".to_string(), false),
        Node::Request(id) => (FileKind::Dir, format!("requests/{id}"), false),
        Node::RequestField(id, field) => (
            FileKind::File,
            format!("requests/{id}/{}", request_field_name(*field)),
            true,
        ),
        Node::Grants => (FileKind::Dir, "grants".to_string(), true),
        Node::Grant(id) => (FileKind::Dir, format!("grants/{id}"), false),
        Node::GrantRecord(id) => (FileKind::File, format!("grants/{id}/record"), true),
        Node::Events => (FileKind::Stream, "events".to_string(), false),
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    stable_key.hash(&mut hasher);
    Qid {
        kind,
        version: if mutable { generation } else { 0 },
        path: hasher.finish(),
    }
}

fn request_field_name(field: RequestField) -> &'static str {
    match field {
        RequestField::Request => "request",
        RequestField::Status => "status",
        RequestField::Grant => "grant",
        RequestField::Error => "error",
    }
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start.saturating_add(count as usize));
    bytes[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::{InProcessTransport, Request, Response};

    async fn call(transport: &InProcessTransport, request: Request) -> Result<Response, ErrorCode> {
        transport.call(request).await
    }

    #[tokio::test]
    async fn global_view_cannot_allocate_process_requests() {
        let service = HostMountService::unavailable();
        let transport = InProcessTransport::new(service.file_server());
        call(
            &transport,
            Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["requests".into(), "clone".into()],
            },
        )
        .await
        .unwrap();
        assert_eq!(
            call(
                &transport,
                Request::Open {
                    fid: Fid(1),
                    mode: OpenMode::ReadWrite,
                },
            )
            .await,
            Err(ErrorCode::NoAccess)
        );
    }

    #[tokio::test]
    async fn clone_commit_rejects_host_path_without_publishing_request() {
        let service = HostMountService::unavailable();
        let transport = InProcessTransport::new(service.file_server_for_process(7));
        call(
            &transport,
            Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["requests".into(), "clone".into()],
            },
        )
        .await
        .unwrap();
        call(
            &transport,
            Request::Open {
                fid: Fid(1),
                mode: OpenMode::ReadWrite,
            },
        )
        .await
        .unwrap();
        let request_id = match call(
            &transport,
            Request::Read {
                fid: Fid(1),
                offset: 0,
                count: 128,
            },
        )
        .await
        .unwrap()
        {
            Response::Read { data } => String::from_utf8(data).unwrap(),
            other => panic!("unexpected response: {other:?}"),
        };
        let document = br#"{"namespace_path":"/mnt/project","host_path":"/tmp/project","access":"read_only","reason":"read"}"#;
        call(
            &transport,
            Request::Write {
                fid: Fid(1),
                offset: 0,
                data: document.to_vec(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            call(&transport, Request::Clunk { fid: Fid(1) }).await,
            Err(ErrorCode::BadRequest)
        );
        assert!(!service.has_request(&request_id));
    }

    #[tokio::test]
    async fn clone_commit_rejects_non_normal_and_reserved_namespace_paths() {
        for (index, namespace_path) in [
            "/mnt/project/../private",
            "/mnt/project//private",
            "/mnt/project/",
            "/mnt/host-mount",
            "/mnt/package/private",
        ]
        .into_iter()
        .enumerate()
        {
            let service = HostMountService::unavailable();
            let transport = InProcessTransport::new(service.file_server_for_process(7));
            let fid = Fid(index as u64 + 1);
            call(
                &transport,
                Request::Walk {
                    fid: Fid::ROOT,
                    newfid: fid,
                    names: vec!["requests".into(), "clone".into()],
                },
            )
            .await
            .unwrap();
            call(
                &transport,
                Request::Open {
                    fid,
                    mode: OpenMode::ReadWrite,
                },
            )
            .await
            .unwrap();
            let document = serde_json::to_vec(&serde_json::json!({
                "namespace_path": namespace_path,
                "access": "read_only",
                "reason": "read"
            }))
            .unwrap();
            call(
                &transport,
                Request::Write {
                    fid,
                    offset: 0,
                    data: document,
                },
            )
            .await
            .unwrap();
            assert_eq!(
                call(&transport, Request::Clunk { fid }).await,
                Err(ErrorCode::BadRequest),
                "{namespace_path} must be rejected"
            );
        }
    }

    #[tokio::test]
    async fn process_view_hides_other_process_requests_and_events() {
        let service = HostMountService::unavailable();
        let owner = InProcessTransport::new(service.file_server_for_process(7));
        call(
            &owner,
            Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["requests".into(), "clone".into()],
            },
        )
        .await
        .unwrap();
        call(
            &owner,
            Request::Open {
                fid: Fid(1),
                mode: OpenMode::ReadWrite,
            },
        )
        .await
        .unwrap();
        let request_id = match call(
            &owner,
            Request::Read {
                fid: Fid(1),
                offset: 0,
                count: 128,
            },
        )
        .await
        .unwrap()
        {
            Response::Read { data } => String::from_utf8(data).unwrap(),
            other => panic!("unexpected response: {other:?}"),
        };
        call(
            &owner,
            Request::Write {
                fid: Fid(1),
                offset: 0,
                data: br#"{"namespace_path":"/mnt/project","access":"read_only","reason":"read"}"#
                    .to_vec(),
            },
        )
        .await
        .unwrap();
        call(&owner, Request::Clunk { fid: Fid(1) }).await.unwrap();

        let other = InProcessTransport::new(service.file_server_for_process(8));
        let shell = alan_shell::Shell::new(other);
        assert_eq!(
            shell.ls("/requests").await.unwrap(),
            vec!["clone", "events"]
        );
        assert!(
            shell
                .stat(&format!("/requests/{request_id}"))
                .await
                .is_err()
        );
        assert!(shell.cat("/requests/events").await.unwrap().is_empty());
        assert!(shell.cat("/events").await.unwrap().is_empty());

        let global = alan_shell::Shell::new(InProcessTransport::new(service.file_server()));
        assert!(global.ls("/requests").await.unwrap().contains(&request_id));
        assert!(
            String::from_utf8(global.cat("/requests/events").await.unwrap())
                .unwrap()
                .contains(&request_id)
        );
    }

    #[tokio::test]
    async fn clone_commit_publishes_one_logical_request_with_immutable_terminal_status() {
        let service = HostMountService::unavailable();
        let transport = InProcessTransport::new(service.file_server_for_process(7));
        call(
            &transport,
            Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["requests".into(), "clone".into()],
            },
        )
        .await
        .unwrap();
        call(
            &transport,
            Request::Open {
                fid: Fid(1),
                mode: OpenMode::ReadWrite,
            },
        )
        .await
        .unwrap();
        let request_id = match call(
            &transport,
            Request::Read {
                fid: Fid(1),
                offset: 0,
                count: 128,
            },
        )
        .await
        .unwrap()
        {
            Response::Read { data } => String::from_utf8(data).unwrap(),
            other => panic!("unexpected response: {other:?}"),
        };
        let document = br#"{"namespace_path":"/mnt/project","access":"read_only","reason":"Read project files","label":"Project"}"#;
        call(
            &transport,
            Request::Write {
                fid: Fid(1),
                offset: 0,
                data: document.to_vec(),
            },
        )
        .await
        .unwrap();
        call(&transport, Request::Clunk { fid: Fid(1) })
            .await
            .unwrap();

        let shell = alan_shell::Shell::new(transport);
        assert_eq!(
            shell.ls("/").await.unwrap(),
            vec!["requests", "grants", "events"]
        );
        for retired in ["request", "projection", "approval", "status"] {
            assert!(shell.stat(&format!("/{retired}")).await.is_err());
        }
        assert_eq!(
            shell.ls("/requests").await.unwrap(),
            vec!["clone", "events", request_id.as_str()]
        );
        let request: serde_json::Value = serde_json::from_slice(
            &shell
                .cat(&format!("/requests/{request_id}/request"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(request["namespace_path"], "/mnt/project");
        assert_eq!(request["requesting_pid"], 7);
        assert!(!request.to_string().contains("host_path"));
        assert_eq!(
            shell
                .cat(&format!("/requests/{request_id}/status"))
                .await
                .unwrap(),
            b"pending\n"
        );
        assert!(
            String::from_utf8(shell.cat("/requests/events").await.unwrap())
                .unwrap()
                .contains(&format!(r#""request_id":"{request_id}""#))
        );

        service
            .reject_request(&request_id, "User declined", "test-host")
            .unwrap();
        assert_eq!(
            shell
                .cat(&format!("/requests/{request_id}/status"))
                .await
                .unwrap(),
            b"rejected\n"
        );
        assert_eq!(
            shell
                .cat(&format!("/requests/{request_id}/error"))
                .await
                .unwrap(),
            b"User declined\n"
        );
        assert!(
            service
                .cancel_request(&request_id, "changed mind", "test-host")
                .is_err(),
            "terminal request status must be immutable"
        );
    }

    #[test]
    fn all_terminal_statuses_are_terminal() {
        for status in [
            crate::host_mount::HostMountStatus::Approved,
            crate::host_mount::HostMountStatus::Rejected,
            crate::host_mount::HostMountStatus::Cancelled,
            crate::host_mount::HostMountStatus::Failed,
        ] {
            assert!(status.is_terminal());
        }
        assert!(!crate::host_mount::HostMountStatus::Pending.is_terminal());
    }
}
