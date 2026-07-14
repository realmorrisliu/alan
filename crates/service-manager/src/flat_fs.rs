use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;
use tokio::sync::Mutex;

const MAX_WRITE_BYTES: usize = 1 << 20;

#[async_trait]
pub(crate) trait FlatFileService: Send + Sync {
    fn files(&self) -> &'static [(&'static str, bool)];
    fn read(&self, name: &str) -> Result<Vec<u8>, ErrorCode>;
    async fn commit(&self, name: &str, bytes: &[u8]) -> Result<(), ErrorCode>;

    fn max_write_bytes(&self) -> usize {
        MAX_WRITE_BYTES
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Node {
    Root,
    File(String),
}

struct FidState {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
}

pub(crate) struct FlatServiceFs {
    service: Arc<dyn FlatFileService>,
    fids: Mutex<HashMap<Fid, FidState>>,
}

impl FlatServiceFs {
    pub(crate) fn new(service: Arc<dyn FlatFileService>) -> Self {
        Self {
            service,
            fids: Mutex::new(HashMap::new()),
        }
    }

    fn exists(&self, name: &str) -> bool {
        self.service.files().iter().any(|(file, _)| *file == name)
    }

    fn writable(&self, name: &str) -> bool {
        self.service
            .files()
            .iter()
            .find(|(file, _)| *file == name)
            .is_some_and(|(_, writable)| *writable)
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

    fn bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        match node {
            Node::Root => Ok(self
                .service
                .files()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes()),
            Node::File(name) => self.service.read(name),
        }
    }
}

#[async_trait]
impl FileServer for FlatServiceFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut fids = self.fids.lock().await;
        if newfid == Fid::ROOT || fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let start = if fid == Fid::ROOT {
            Node::Root
        } else {
            fids.get(&fid)
                .map(|state| state.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        let node = match (start, names) {
            (node, []) => node,
            (Node::Root, [name]) if self.exists(name) => Node::File(name.clone()),
            (Node::Root, [_]) => return Err(ErrorCode::NotFound),
            _ => return Err(ErrorCode::NotDirectory),
        };
        let qid = qid(&node);
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
        let allowed = match &node {
            Node::Root => mode == OpenMode::Read,
            Node::File(name) if self.writable(name) => {
                matches!(mode, OpenMode::Read | OpenMode::Write)
            }
            Node::File(_) => mode == OpenMode::Read,
        };
        if !allowed {
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
            if fids.get(&fid).ok_or(ErrorCode::NotFound)?.mode != Some(OpenMode::Read) {
                return Err(ErrorCode::NoAccess);
            }
        }
        let bytes = self.bytes(&self.node_of(fid).await?)?;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = start.saturating_add(count as usize).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut fids = self.fids.lock().await;
        let state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        let Node::File(name) = &state.node else {
            return Err(ErrorCode::NoAccess);
        };
        if state.mode != Some(OpenMode::Write) || !self.writable(name) {
            return Err(ErrorCode::NoAccess);
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > self.service.max_write_bytes() {
            return Err(ErrorCode::BadRequest);
        }
        state.write_buf.resize(state.write_buf.len().max(end), 0);
        state.write_buf[start..end].copy_from_slice(data);
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_of(fid).await?;
        let length = self.bytes(&node)?.len() as u64;
        Ok(Stat {
            name: String::new(),
            qid: qid(&node),
            length,
            writable: matches!(&node, Node::File(name) if self.writable(name)),
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
        if let Node::File(name) = state.node
            && state.mode == Some(OpenMode::Write)
        {
            self.service.commit(&name, &state.write_buf).await?;
        }
        Ok(())
    }
}

fn qid(node: &Node) -> Qid {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hasher);
    Qid {
        kind: if *node == Node::Root {
            FileKind::Dir
        } else {
            FileKind::File
        },
        version: 0,
        path: hasher.finish(),
    }
}
