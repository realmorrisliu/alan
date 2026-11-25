//! Internal state machine, aP node helpers, and event log types for `alan-branchfs`.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use alan_ap::{ErrorCode, FileKind, Offset, OpenMode, Qid, Stream, VersionTable};
use alan_knowledge::{ContentHash, KnowledgeStore};
use serde::Serialize;

use super::{
    BranchRecord, BranchStatus, WorkspaceBackingKind, validate_no_raw_host_path,
    validate_opaque_workspace_ref,
};

#[derive(Serialize)]
struct SelectedBranch<'a> {
    id: &'a str,
    root: &'a ContentHash,
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_ref: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backing_kind: Option<WorkspaceBackingKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff_summary: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checks_summary: Option<&'a str>,
}

pub(crate) struct State {
    pub(crate) store: KnowledgeStore,
    pub(crate) branches: BTreeMap<String, BranchRecord>,
    pub(crate) selected: Option<String>,
    pub(crate) events: Stream,
    pub(crate) versions: VersionTable,
    pub(crate) fids: HashMap<alan_ap::Fid, BranchFid>,
}

pub(crate) struct BranchFid {
    pub(crate) node: Node,
    pub(crate) mode: Option<OpenMode>,
    pub(crate) write_buf: Vec<u8>,
    pub(crate) wrote: bool,
}

impl BranchFid {
    pub(crate) fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            wrote: false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Node {
    Root,
    Ctl,
    BranchesDir,
    Branch(String),
    Selected,
    Events,
}

pub(crate) struct InsertForkedBranch {
    pub(crate) id: String,
    pub(crate) from: String,
    pub(crate) delta: String,
    pub(crate) workspace_ref: Option<String>,
    pub(crate) backing_kind: Option<WorkspaceBackingKind>,
    pub(crate) diff_summary: Option<String>,
    pub(crate) checks_summary: Option<String>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            store: KnowledgeStore::new(),
            branches: BTreeMap::new(),
            selected: None,
            events: Stream::new(),
            versions: VersionTable::new(),
            fids: HashMap::new(),
        }
    }

    pub(crate) fn node_of(&self, fid: alan_ap::Fid) -> Result<Node, ErrorCode> {
        if fid == alan_ap::Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .get(&fid)
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    pub(crate) fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        Qid {
            kind,
            version: self.versions.get(path),
            path,
        }
    }

    pub(crate) fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "ctl" => Ok(Node::Ctl),
                "branches" => Ok(Node::BranchesDir),
                "selected" => Ok(Node::Selected),
                "events" => Ok(Node::Events),
                _ => Err(ErrorCode::NotFound),
            },
            Node::BranchesDir => {
                if self.branches.contains_key(name) {
                    Ok(Node::Branch(name.to_string()))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    pub(crate) fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => b"ctl\nbranches\nselected\nevents".to_vec(),
            Node::Ctl => {
                b"# branchfs ctl: write one branch command JSON document, then clunk\n".to_vec()
            }
            Node::BranchesDir => listing(self.branches.keys()),
            Node::Branch(id) => json_line(self.branches.get(id).ok_or(ErrorCode::NotFound)?)?,
            Node::Selected => self.selected_bytes()?,
            Node::Events => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }

    pub(crate) fn stream_for(&self, node: &Node) -> Option<Stream> {
        match node {
            Node::Events => Some(self.events.clone()),
            _ => None,
        }
    }

    fn selected_bytes(&self) -> Result<Vec<u8>, ErrorCode> {
        let Some(id) = self.selected.as_deref() else {
            return Ok(b"null\n".to_vec());
        };
        let branch = self.branches.get(id).ok_or(ErrorCode::NotFound)?;
        json_line(&SelectedBranch {
            id,
            root: &branch.root,
            score: branch.score,
            workspace_ref: branch.workspace_ref.as_deref(),
            backing_kind: branch.backing_kind,
            diff_summary: branch.diff_summary.as_deref(),
            checks_summary: branch.checks_summary.as_deref(),
        })
    }

    pub(crate) async fn insert_forked_branch(
        &mut self,
        args: InsertForkedBranch,
    ) -> Result<(), ErrorCode> {
        if self.branches.contains_key(&args.id) {
            return Err(ErrorCode::BadRequest);
        }
        let base = self.branches.get(&args.from).ok_or(ErrorCode::NotFound)?;
        let root = self
            .store
            .fork_append_bytes(&base.root, [args.delta.into_bytes()])
            .map_err(knowledge_error)?;
        let record = BranchRecord::fork(
            args.id.clone(),
            args.from.clone(),
            root.clone(),
            args.workspace_ref,
            args.backing_kind,
            args.diff_summary,
            args.checks_summary,
        );
        let ws_ref = record.workspace_ref.clone();
        let backing = record.backing_kind;
        let diff = record.diff_summary.clone();
        let checks = record.checks_summary.clone();
        self.branches.insert(args.id.clone(), record);
        self.versions.bump(node_identity(&Node::BranchesDir).1);
        self.versions
            .bump(node_identity(&Node::Branch(args.id.clone())).1);
        self.append_event(EventRecord::Fork {
            id: &args.id,
            from: &args.from,
            root: &root,
            workspace_ref: ws_ref.as_deref(),
            backing_kind: backing,
            diff_summary: diff.as_deref(),
            checks_summary: checks.as_deref(),
        })
        .await
    }

    pub(crate) async fn score_branch(
        &mut self,
        id: String,
        score: f64,
        summary: Option<String>,
        diff_summary: Option<String>,
        checks_summary: Option<String>,
    ) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        if !score.is_finite() {
            return Err(ErrorCode::BadRequest);
        }
        if let Some(summary) = summary.as_deref() {
            validate_no_raw_host_path(summary)?;
        }
        validate_workspace_fields(None, diff_summary.as_deref(), checks_summary.as_deref())?;

        let branch = self.branches.get_mut(&id).ok_or(ErrorCode::NotFound)?;
        branch.score = Some(score);
        branch.summary = summary;
        if let Some(diff) = diff_summary {
            branch.diff_summary = Some(diff);
        }
        if let Some(checks) = checks_summary {
            branch.checks_summary = Some(checks);
        }
        let summary = branch.summary.clone();
        let diff_summary = branch.diff_summary.clone();
        let checks_summary = branch.checks_summary.clone();
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        if self.selected.as_deref() == Some(&id) {
            self.versions.bump(node_identity(&Node::Selected).1);
        }
        self.append_event(EventRecord::Score {
            id: &id,
            score,
            summary: summary.as_deref(),
            diff_summary: diff_summary.as_deref(),
            checks_summary: checks_summary.as_deref(),
        })
        .await
    }

    pub(crate) async fn update_workspace(
        &mut self,
        id: String,
        workspace_ref: Option<String>,
        backing_kind: Option<WorkspaceBackingKind>,
        diff_summary: Option<String>,
        checks_summary: Option<String>,
    ) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        validate_workspace_fields(
            workspace_ref.as_deref(),
            diff_summary.as_deref(),
            checks_summary.as_deref(),
        )?;
        let branch = self.branches.get_mut(&id).ok_or(ErrorCode::NotFound)?;
        if let Some(ws_ref) = workspace_ref {
            branch.workspace_ref = Some(ws_ref);
        }
        if let Some(kind) = backing_kind {
            branch.backing_kind = Some(kind);
        }
        if let Some(diff) = diff_summary {
            branch.diff_summary = Some(diff);
        }
        if let Some(checks) = checks_summary {
            branch.checks_summary = Some(checks);
        }
        let ws_ref = branch.workspace_ref.clone();
        let kind = branch.backing_kind;
        let diff = branch.diff_summary.clone();
        let checks = branch.checks_summary.clone();
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        if self.selected.as_deref() == Some(&id) {
            self.versions.bump(node_identity(&Node::Selected).1);
        }
        self.append_event(EventRecord::WorkspaceUpdated {
            id: &id,
            workspace_ref: ws_ref.as_deref(),
            backing_kind: kind,
            diff_summary: diff.as_deref(),
            checks_summary: checks.as_deref(),
        })
        .await
    }

    pub(crate) async fn select_branch(&mut self, id: String) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        if !self.branches.contains_key(&id) {
            return Err(ErrorCode::NotFound);
        }
        if let Some(previous) = self.selected.take()
            && let Some(branch) = self.branches.get_mut(&previous)
        {
            branch.status = BranchStatus::Active;
            self.versions.bump(node_identity(&Node::Branch(previous)).1);
        }
        let branch = self.branches.get_mut(&id).ok_or(ErrorCode::NotFound)?;
        branch.status = BranchStatus::Selected;
        let root = branch.root.clone();
        let workspace_ref = branch.workspace_ref.clone();
        let backing_kind = branch.backing_kind;
        self.selected = Some(id.clone());
        self.versions.bump(node_identity(&Node::Selected).1);
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        self.append_event(EventRecord::Select {
            id: &id,
            root: &root,
            workspace_ref: workspace_ref.as_deref(),
            backing_kind,
        })
        .await
    }

    pub(crate) async fn discard_branch(&mut self, id: String) -> Result<(), ErrorCode> {
        validate_branch_id(&id)?;
        let branch = self.branches.remove(&id).ok_or(ErrorCode::NotFound)?;
        if self.selected.as_deref() == Some(&id) {
            self.selected = None;
            self.versions.bump(node_identity(&Node::Selected).1);
        }
        self.versions.bump(node_identity(&Node::BranchesDir).1);
        self.versions
            .bump(node_identity(&Node::Branch(id.clone())).1);
        self.append_event(EventRecord::Discard {
            id: &id,
            root: &branch.root,
            workspace_ref: branch.workspace_ref.as_deref(),
            backing_kind: branch.backing_kind,
        })
        .await
    }

    pub(crate) async fn append_event(&self, event: EventRecord<'_>) -> Result<(), ErrorCode> {
        let mut bytes = serde_json::to_vec(&event).map_err(|_| ErrorCode::BadRequest)?;
        bytes.push(b'\n');
        self.events.append(&bytes).await;
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub(crate) enum EventRecord<'a> {
    Base {
        id: &'a str,
        root: &'a ContentHash,
    },
    Fork {
        id: &'a str,
        from: &'a str,
        root: &'a ContentHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace_ref: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        backing_kind: Option<WorkspaceBackingKind>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff_summary: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        checks_summary: Option<&'a str>,
    },
    Score {
        id: &'a str,
        score: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff_summary: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        checks_summary: Option<&'a str>,
    },
    WorkspaceUpdated {
        id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace_ref: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        backing_kind: Option<WorkspaceBackingKind>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff_summary: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        checks_summary: Option<&'a str>,
    },
    Select {
        id: &'a str,
        root: &'a ContentHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace_ref: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        backing_kind: Option<WorkspaceBackingKind>,
    },
    Discard {
        id: &'a str,
        root: &'a ContentHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        workspace_ref: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        backing_kind: Option<WorkspaceBackingKind>,
    },
    ExitFlush {
        id: &'a str,
        workspace_ref: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        backing_kind: Option<WorkspaceBackingKind>,
        #[serde(skip_serializing_if = "Option::is_none")]
        patch_artifact_ref: Option<&'a str>,
        clone_purged: bool,
    },
}

pub(crate) fn validate_workspace_fields(
    workspace_ref: Option<&str>,
    diff_summary: Option<&str>,
    checks_summary: Option<&str>,
) -> Result<(), ErrorCode> {
    if let Some(ws_ref) = workspace_ref {
        validate_opaque_workspace_ref(ws_ref)?;
    }
    if let Some(diff) = diff_summary {
        validate_no_raw_host_path(diff)?;
    }
    if let Some(checks) = checks_summary {
        validate_no_raw_host_path(checks)?;
    }
    Ok(())
}

pub(crate) fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Ctl)
}

pub(crate) fn validate_branch_id(id: &str) -> Result<(), ErrorCode> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ErrorCode::BadRequest);
    }
    Ok(())
}

pub(crate) fn knowledge_error(error: alan_knowledge::KnowledgeError) -> ErrorCode {
    match error {
        alan_knowledge::KnowledgeError::MissingBlock(_)
        | alan_knowledge::KnowledgeError::MissingNode(_)
        | alan_knowledge::KnowledgeError::UnknownRoot(_) => ErrorCode::NotFound,
        alan_knowledge::KnowledgeError::NoAccess => ErrorCode::NoAccess,
        alan_knowledge::KnowledgeError::HashMismatch(_)
        | alan_knowledge::KnowledgeError::Cycle(_) => ErrorCode::BadRequest,
    }
}

fn json_line(value: &impl Serialize) -> Result<Vec<u8>, ErrorCode> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| ErrorCode::BadRequest)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn listing<'a>(names: impl Iterator<Item = &'a String>) -> Vec<u8> {
    let mut out = Vec::new();
    for name in names {
        out.extend_from_slice(name.as_bytes());
        out.push(b'\n');
    }
    out
}

pub(crate) fn node_identity(node: &Node) -> (FileKind, u64) {
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::Ctl => (FileKind::File, "ctl".to_string()),
        Node::BranchesDir => (FileKind::Dir, "branches".to_string()),
        Node::Branch(id) => (FileKind::File, format!("branches/{id}")),
        Node::Selected => (FileKind::File, "selected".to_string()),
        Node::Events => (FileKind::Stream, "events".to_string()),
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    (kind, h.finish())
}

pub(crate) fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
