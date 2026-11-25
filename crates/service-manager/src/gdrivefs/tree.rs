//! Internal fid, state cache, node identity, and path disambiguation helpers for `GdriveFs`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use alan_ap::{ErrorCode, Fid, FileKind, Offset, OpenMode, Qid, VersionTable};
use chrono::Utc;

use super::backend::{
    DriveAuthSummary, DriveCursorStatus, DriveFileMeta, DriveQuotaSummary, DriveRateLimitStatus,
    DriveSearchResponse, DriveSyncOutcome, DriveUploadedReceipt, GdriveStatusSnapshot,
    MAX_CTL_BYTES, MAX_UPLOAD_BYTES, redact_google_tokens,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Node {
    Root,
    Status,
    Ctl,
    ByIdDir,
    ByIdEntryDir(String),
    ByIdMeta(String),
    ByIdContent(String),
    FilesDir(String),
    FilesEntry(String),
    SearchDir,
    SearchResult(String),
    UploadsDir,
    UploadsNew,
}

impl Node {
    pub(super) fn static_bytes(&self) -> Option<&'static [u8]> {
        match self {
            Node::Root => Some(b"status\nctl\nby-id\nfiles\nsearch\nuploads"),
            Node::Ctl => Some(b"sync\nrefresh <file_id>\npin <folder_id>\n"),
            Node::ByIdEntryDir(_) => Some(b"meta.json\ncontent"),
            Node::UploadsDir => Some(b"new"),
            _ => None,
        }
    }

    pub(super) fn is_writable(&self, read_only: bool) -> bool {
        if read_only {
            return false;
        }
        matches!(self, Node::Ctl | Node::UploadsNew)
    }

    pub(super) fn max_write_bytes(&self) -> usize {
        match self {
            Node::Ctl => MAX_CTL_BYTES,
            _ => MAX_UPLOAD_BYTES,
        }
    }
}

pub(super) struct GdriveFid {
    pub(super) node: Node,
    pub(super) mode: Option<OpenMode>,
    pub(super) write_buf: Vec<u8>,
    pub(super) wrote: bool,
    pub(super) write_failed: bool,
    pub(super) read_snapshot: Option<Vec<u8>>,
    pub(super) pinned_etag: Option<String>,
    pub(super) pinned_revision: Option<String>,
}

impl GdriveFid {
    pub(super) fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            wrote: false,
            write_failed: false,
            read_snapshot: None,
            pinned_etag: None,
            pinned_revision: None,
        }
    }

    pub(super) fn buffer_write(
        &mut self,
        offset: Offset,
        data: &[u8],
        max_bytes: usize,
    ) -> Result<u32, ErrorCode> {
        if self.write_failed {
            return Err(ErrorCode::BadRequest);
        }
        let Ok(start) = usize::try_from(offset) else {
            self.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        let Some(end) = start.checked_add(data.len()).filter(|e| *e <= max_bytes) else {
            self.write_failed = true;
            return Err(ErrorCode::BadRequest);
        };
        if self.write_buf.len() < end {
            self.write_buf.resize(end, 0);
        }
        self.write_buf[start..end].copy_from_slice(data);
        self.wrote = true;
        Ok(data.len() as u32)
    }
}

#[derive(Clone)]
pub(super) struct CachedExport {
    pub(super) modified_time: Option<String>,
    pub(super) head_revision_id: Option<String>,
    pub(super) bytes: Vec<u8>,
}

/// Deterministic projected directory entry under `files/<path...>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectedEntry {
    pub(super) projected_name: String,
    pub(super) meta: DriveFileMeta,
}

/// Disambiguate sibling Drive items deterministically using `<name>__<short_id>` on collisions.
pub(super) fn project_folder_entries(children: &[DriveFileMeta]) -> Vec<ProjectedEntry> {
    let sanitized: Vec<(String, &DriveFileMeta)> = children
        .iter()
        .filter(|m| !m.trashed)
        .map(|m| (sanitize_drive_segment(&m.name), m))
        .collect();

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (name, _) in &sanitized {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }

    let mut by_name: HashMap<String, Vec<&DriveFileMeta>> = HashMap::new();
    for (name, meta) in sanitized {
        by_name.entry(name).or_default().push(meta);
    }

    let mut out = Vec::with_capacity(children.len());
    for (name, mut group) in by_name {
        if group.len() == 1 {
            out.push(ProjectedEntry {
                projected_name: name,
                meta: group.pop().expect("single item").clone(),
            });
            continue;
        }
        group.sort_by(|a, b| a.id.cmp(&b.id));
        let ids: Vec<&str> = group.iter().map(|m| m.id.as_str()).collect();
        for meta in group {
            let suffix = unique_short_id(&meta.id, &ids);
            out.push(ProjectedEntry {
                projected_name: format!("{name}__{suffix}"),
                meta: meta.clone(),
            });
        }
    }
    out.sort_by(|a, b| {
        a.projected_name
            .cmp(&b.projected_name)
            .then_with(|| a.meta.id.cmp(&b.meta.id))
    });
    out
}

fn sanitize_drive_segment(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | '\0' | '\r' | '\n' => '_',
            other => other,
        })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "untitled".to_string()
    } else {
        cleaned
    }
}

fn unique_short_id(id: &str, sibling_ids: &[&str]) -> String {
    let total_chars = id.chars().count();
    let mut len = 6usize.min(total_chars).max(1);
    while len <= total_chars {
        let prefix: String = id.chars().take(len).collect();
        let collisions = sibling_ids
            .iter()
            .filter(|other| other.chars().take(len).collect::<String>() == prefix)
            .count();
        if collisions <= 1 {
            return prefix;
        }
        len += 1;
    }
    id.to_string()
}

pub(super) struct FsState {
    pub(super) read_only: bool,
    pub(super) files: BTreeMap<String, DriveFileMeta>,
    pub(super) folders: BTreeMap<String, Vec<DriveFileMeta>>,
    pub(super) exports: BTreeMap<String, CachedExport>,
    pub(super) searches: BTreeMap<String, DriveSearchResponse>,
    pub(super) pinned_folders: BTreeSet<String>,
    pub(super) cursor: DriveCursorStatus,
    pub(super) quota: Option<DriveQuotaSummary>,
    pub(super) rate_limit: DriveRateLimitStatus,
    pub(super) last_error: Option<String>,
    pub(super) last_uploaded: Option<DriveUploadedReceipt>,
    versions: VersionTable,
    pub(super) fids: HashMap<Fid, GdriveFid>,
}

impl FsState {
    pub(super) fn new(read_only: bool) -> Self {
        Self {
            read_only,
            files: BTreeMap::new(),
            folders: BTreeMap::new(),
            exports: BTreeMap::new(),
            searches: BTreeMap::new(),
            pinned_folders: BTreeSet::new(),
            cursor: DriveCursorStatus::default(),
            quota: None,
            rate_limit: DriveRateLimitStatus::default(),
            last_error: None,
            last_uploaded: None,
            versions: VersionTable::new(),
            fids: HashMap::new(),
        }
    }

    pub(super) fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .get(&fid)
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    pub(super) fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        Qid {
            kind,
            version: self.versions.get(path),
            path,
        }
    }

    pub(super) fn bump(&mut self, node: &Node) {
        self.versions.bump(node_identity(node).1);
    }

    pub(super) fn status_snapshot(&self, auth: DriveAuthSummary) -> GdriveStatusSnapshot {
        GdriveStatusSnapshot {
            mount_access: if self.read_only {
                "read_only".to_string()
            } else {
                "read_write".to_string()
            },
            auth,
            cursor: self.cursor.clone(),
            quota: self.quota.clone(),
            rate_limit: self.rate_limit.clone(),
            pinned_folders: self.pinned_folders.iter().cloned().collect(),
            cached_files: self.files.len(),
            last_error: self.last_error.clone(),
        }
    }

    pub(super) fn record_api_success(&mut self) {
        self.rate_limit.api_requests = self.rate_limit.api_requests.saturating_add(1);
        self.last_error = None;
        self.bump(&Node::Status);
    }

    pub(super) fn record_api_error(
        &mut self,
        code: ErrorCode,
        message: &str,
        retry_after: Option<u64>,
    ) {
        self.rate_limit.api_requests = self.rate_limit.api_requests.saturating_add(1);
        if code == ErrorCode::RateLimited {
            self.rate_limit.rate_limited_responses =
                self.rate_limit.rate_limited_responses.saturating_add(1);
            if retry_after.is_some() {
                self.rate_limit.last_retry_after_secs = retry_after;
            }
        }
        self.last_error = Some(redact_google_tokens(message, None));
        self.bump(&Node::Status);
    }

    pub(super) fn cache_file_meta(&mut self, meta: DriveFileMeta) {
        let id = meta.id.clone();
        if let Some(cached) = self.exports.get(&id)
            && (cached.modified_time != meta.modified_time
                || cached.head_revision_id != meta.head_revision_id)
        {
            self.exports.remove(&id);
        }
        self.files.insert(id.clone(), meta);
        self.bump(&Node::ByIdDir);
        self.bump(&Node::ByIdMeta(id.clone()));
        self.bump(&Node::ByIdContent(id));
    }

    pub(super) fn apply_sync(&mut self, outcome: DriveSyncOutcome) {
        self.rate_limit.api_requests = self
            .rate_limit
            .api_requests
            .saturating_add(outcome.api_requests.max(1));
        self.cursor.sync_generation = self.cursor.sync_generation.saturating_add(1);
        self.cursor.last_sync_at = Some(Utc::now().to_rfc3339());
        if let Some(tok) = outcome.start_page_token {
            self.cursor.start_page_token = Some(tok);
        }
        if let Some(quota) = outcome.quota {
            self.quota = Some(quota);
        }
        for removed in outcome.removed_file_ids {
            self.files.remove(&removed);
            self.exports.remove(&removed);
            self.folders.remove(&removed);
        }
        for file in outcome.updated_files {
            self.cache_file_meta(file);
        }
        self.folders.clear();
        for (folder_id, children) in outcome.pinned_children {
            for child in &children {
                self.cache_file_meta(child.clone());
            }
            self.folders.insert(folder_id.clone(), children);
            self.bump(&Node::FilesDir(folder_id));
        }
        self.last_error = None;
        self.bump(&Node::FilesDir("root".to_string()));
        self.bump(&Node::Status);
    }
}

pub(super) fn parse_single_line_ctl(bytes: &[u8]) -> Result<String, ErrorCode> {
    let text = std::str::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
    if text.contains('\0') {
        return Err(ErrorCode::BadRequest);
    }
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(ErrorCode::BadRequest);
    }
    Ok(trimmed.to_string())
}

pub(super) fn decode_search_query(raw: &str) -> Result<String, ErrorCode> {
    if raw.is_empty() || raw.len() > 512 || raw.bytes().any(|b| matches!(b, b'\0' | b'\r' | b'\n'))
    {
        return Err(ErrorCode::BadRequest);
    }
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    let decoded = String::from_utf8(out).map_err(|_| ErrorCode::BadRequest)?;
    let trimmed = decoded.trim();
    if trimmed.is_empty() || trimmed.bytes().any(|b| matches!(b, b'\0' | b'\r' | b'\n')) {
        return Err(ErrorCode::BadRequest);
    }
    Ok(trimmed.to_string())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(super) fn join_lines(lines: impl IntoIterator<Item = String>) -> Vec<u8> {
    lines
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

pub(super) fn encode_pretty_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, ErrorCode> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| ErrorCode::Io)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn slice_bytes(bytes: &[u8], offset: Offset, count: u32) -> Vec<u8> {
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let end = start.saturating_add(count as usize).min(bytes.len());
    bytes[start..end].to_vec()
}

fn node_identity(node: &Node) -> (FileKind, u64) {
    let kind = match node {
        Node::Root
        | Node::ByIdDir
        | Node::ByIdEntryDir(_)
        | Node::FilesDir(_)
        | Node::SearchDir
        | Node::UploadsDir => FileKind::Dir,
        Node::Status
        | Node::Ctl
        | Node::ByIdMeta(_)
        | Node::ByIdContent(_)
        | Node::FilesEntry(_)
        | Node::SearchResult(_)
        | Node::UploadsNew => FileKind::File,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    node.hash(&mut hasher);
    (kind, hasher.finish())
}
