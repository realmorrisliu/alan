//! Pure-Rust `gdrivefs` aP file server projecting Google Drive at `/srv/gdrive` -> `/mnt/gdrive`.
//!
//! Namespace tree:
//! - `status`: read-only JSON (OAuth scope summary, sync cursor, quota, rate limits; zero raw tokens)
//! - `ctl`: commit-on-clunk control (`sync`, `refresh <file_id>`, `pin <folder_id>`)
//! - `by-id/<file_id>/{meta.json,content}`: binary range reads (`alt=media` + ETag/revision check)
//!   and automatic Google Workspace `files.export` (Docs -> Markdown, Sheets -> CSV, Slides -> text,
//!   Apps Script -> JSON; fails explicitly above 10 MB)
//! - `files/<path...>`: lazy cached directory projection with `<name>__<short_id>` disambiguation
//! - `search/<query>`: read-only Drive query returning bounded JSON hits
//! - `uploads/new`: commit-on-clunk upload admitted ONLY when mounted `ReadWrite` with `drive.file`

pub mod backend;
pub mod http_client;
#[cfg(test)]
mod tests;
mod tree;

use std::collections::BTreeSet;
use std::sync::Arc;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, WireError,
    export_file_server,
};
use async_trait::async_trait;
use tokio::io::{AsyncBufRead, AsyncWrite, BufReader};
use tokio::sync::Mutex;

pub use backend::{
    DEFAULT_SEARCH_LIMIT, DriveAuthSummary, DriveAuthToken, DriveCursorStatus, DriveFileMeta,
    DriveQuotaSummary, DriveRangeData, DriveRateLimitStatus, DriveSearchHit, DriveSearchResponse,
    DriveSyncOutcome, DriveTokenFuture, DriveTokenProvider, DriveUploadedReceipt,
    GOOGLE_DRIVE_FILE_SCOPE, GOOGLE_DRIVE_READONLY_SCOPE, GdriveBackend, GdriveBackendError,
    GdriveStatusSnapshot, MAX_CTL_BYTES, MAX_EXPORT_BYTES, MAX_RANGE_READ_BYTES, MAX_UPLOAD_BYTES,
    MemoryGdriveBackend, NewDriveUploadRequest, WORKSPACE_DOC_MIME, WORKSPACE_FOLDER_MIME,
    WORKSPACE_SCRIPT_MIME, WORKSPACE_SHEET_MIME, WORKSPACE_SLIDES_MIME, export_mime_for_workspace,
    redact_google_tokens, valid_drive_id,
};
pub use http_client::{
    DEFAULT_DRIVE_API_BASE_URL, DEFAULT_DRIVE_UPLOAD_BASE_URL, GoogleDriveApiBackend,
};
use tree::{
    CachedExport, FsState, GdriveFid, Node, ProjectedEntry, decode_search_query,
    encode_pretty_json, join_lines, parse_single_line_ctl, project_folder_entries, slice_bytes,
};

/// Canonical `/srv` handle name for `gdrivefs`.
pub const GDRIVE_SRV_HANDLE: &str = "gdrive";
/// Canonical namespace mount path for `gdrivefs`.
pub const GDRIVE_MOUNT_PATH: &str = "/mnt/gdrive";

/// Google Drive aP [`FileServer`] backed by a [`GdriveBackend`].
pub struct GdriveFs<B: GdriveBackend> {
    backend: Arc<B>,
    state: Mutex<FsState>,
    sync_lock: Mutex<()>,
}

impl<B: GdriveBackend> GdriveFs<B> {
    /// Construct a `GdriveFs` instance deferring mount-level R/O vs R/W gating to Kernel `Access`
    /// while still enforcing `drive.file` scope on `uploads/new`.
    pub fn new(backend: Arc<B>) -> Self {
        Self::with_read_only(backend, false)
    }

    /// Construct a `GdriveFs` instance with an explicit local read-only clamp.
    pub fn with_read_only(backend: Arc<B>, read_only: bool) -> Self {
        Self {
            backend,
            state: Mutex::new(FsState::new(read_only)),
            sync_lock: Mutex::new(()),
        }
    }

    /// Access the underlying backend handle.
    pub fn backend(&self) -> &Arc<B> {
        &self.backend
    }

    /// Perform one Drive sync pass updating cursor, quota, cached files, and pinned folders.
    pub async fn sync_drive(&self) -> Result<(), ErrorCode> {
        let _guard = self.sync_lock.lock().await;
        let (start_token, pinned) = {
            let state = self.state.lock().await;
            (
                state.cursor.start_page_token.clone(),
                state.pinned_folders.iter().cloned().collect::<Vec<_>>(),
            )
        };
        match self.backend.sync(start_token.as_deref(), &pinned).await {
            Ok(outcome) => {
                self.state.lock().await.apply_sync(outcome);
                Ok(())
            }
            Err(err) => {
                self.state.lock().await.record_api_error(
                    err.code,
                    &err.message,
                    err.retry_after_secs,
                );
                Err(err.code)
            }
        }
    }

    async fn read_file_meta(&self, file_id: &str) -> Result<DriveFileMeta, ErrorCode> {
        if !valid_drive_id(file_id) {
            return Err(ErrorCode::NotFound);
        }
        if let Some(cached) = self.state.lock().await.files.get(file_id).cloned() {
            return Ok(cached);
        }
        match self.backend.get_file_meta(file_id).await {
            Ok(meta) => {
                let mut state = self.state.lock().await;
                state.record_api_success();
                state.cache_file_meta(meta.clone());
                Ok(meta)
            }
            Err(err) => {
                self.state.lock().await.record_api_error(
                    err.code,
                    &err.message,
                    err.retry_after_secs,
                );
                Err(err.code)
            }
        }
    }

    async fn projected_folder_entries(
        &self,
        folder_id: &str,
    ) -> Result<Vec<ProjectedEntry>, ErrorCode> {
        let (cached_children, pinned_non_root) = {
            let state = self.state.lock().await;
            let children = state.folders.get(folder_id).cloned();
            let pinned: Vec<DriveFileMeta> = if folder_id == "root" {
                state
                    .pinned_folders
                    .iter()
                    .filter_map(|pid| state.files.get(pid).cloned())
                    .collect()
            } else {
                Vec::new()
            };
            (children, pinned)
        };

        let mut children = match cached_children {
            Some(list) => list,
            None => match self.backend.list_folder(folder_id).await {
                Ok(list) => {
                    let mut state = self.state.lock().await;
                    state.record_api_success();
                    for item in &list {
                        state.cache_file_meta(item.clone());
                    }
                    state.folders.insert(folder_id.to_string(), list.clone());
                    list
                }
                Err(err) => {
                    self.state.lock().await.record_api_error(
                        err.code,
                        &err.message,
                        err.retry_after_secs,
                    );
                    return Err(err.code);
                }
            },
        };

        for pinned_meta in pinned_non_root {
            if !children.iter().any(|c| c.id == pinned_meta.id) {
                children.push(pinned_meta);
            }
        }
        Ok(project_folder_entries(&children))
    }

    async fn read_workspace_export(&self, meta: &DriveFileMeta) -> Result<Vec<u8>, ErrorCode> {
        if meta.is_folder {
            return Err(ErrorCode::IsDirectory);
        }
        let export_mime =
            export_mime_for_workspace(&meta.mime_type).ok_or(ErrorCode::Unsupported)?;
        if let Some(cached) = self.state.lock().await.exports.get(&meta.id).cloned()
            && cached.modified_time == meta.modified_time
            && cached.head_revision_id == meta.head_revision_id
        {
            return Ok(cached.bytes);
        }
        match self.backend.export_workspace_file(meta, export_mime).await {
            Ok(bytes) => {
                if bytes.len() > MAX_EXPORT_BYTES {
                    self.state.lock().await.record_api_error(
                        ErrorCode::BadRequest,
                        &format!(
                            "Google Workspace export for `{}` exceeds 10 MB limit ({} bytes)",
                            meta.id,
                            bytes.len()
                        ),
                        None,
                    );
                    return Err(ErrorCode::BadRequest);
                }
                let mut state = self.state.lock().await;
                state.record_api_success();
                state.exports.insert(
                    meta.id.clone(),
                    CachedExport {
                        modified_time: meta.modified_time.clone(),
                        head_revision_id: meta.head_revision_id.clone(),
                        bytes: bytes.clone(),
                    },
                );
                Ok(bytes)
            }
            Err(err) => {
                self.state.lock().await.record_api_error(
                    err.code,
                    &err.message,
                    err.retry_after_secs,
                );
                Err(err.code)
            }
        }
    }

    async fn read_binary_content_range(
        &self,
        fid: Fid,
        meta: &DriveFileMeta,
        offset: Offset,
        count: u32,
    ) -> Result<Vec<u8>, ErrorCode> {
        let (pinned_etag, pinned_revision) = {
            let state = self.state.lock().await;
            let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            (f.pinned_etag.clone(), f.pinned_revision.clone())
        };
        let res = self
            .backend
            .read_binary_range(
                meta,
                offset,
                count,
                pinned_etag.as_deref(),
                pinned_revision.as_deref(),
            )
            .await;
        match res {
            Ok(chunk) => {
                let mut state = self.state.lock().await;
                state.record_api_success();
                if let Some(f) = state.fids.get_mut(&fid) {
                    if f.pinned_etag.is_none() {
                        f.pinned_etag = chunk.etag;
                    }
                    if f.pinned_revision.is_none() {
                        f.pinned_revision = chunk.revision_id;
                    }
                }
                Ok(chunk.bytes)
            }
            Err(err) => {
                self.state.lock().await.record_api_error(
                    err.code,
                    &err.message,
                    err.retry_after_secs,
                );
                Err(err.code)
            }
        }
    }

    async fn run_search(&self, query: &str) -> Result<Vec<u8>, ErrorCode> {
        if let Some(cached) = self.state.lock().await.searches.get(query).cloned() {
            return encode_pretty_json(&cached);
        }
        match self.backend.search(query, DEFAULT_SEARCH_LIMIT).await {
            Ok(resp) => {
                let mut state = self.state.lock().await;
                state.record_api_success();
                state.searches.insert(query.to_string(), resp.clone());
                state.bump(&Node::SearchDir);
                encode_pretty_json(&resp)
            }
            Err(err) => {
                self.state.lock().await.record_api_error(
                    err.code,
                    &err.message,
                    err.retry_after_secs,
                );
                Err(err.code)
            }
        }
    }

    async fn resolve_child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "status" => Ok(Node::Status),
                "ctl" => Ok(Node::Ctl),
                "by-id" => Ok(Node::ByIdDir),
                "files" => Ok(Node::FilesDir("root".to_string())),
                "search" => Ok(Node::SearchDir),
                "uploads" => Ok(Node::UploadsDir),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ByIdDir => {
                self.read_file_meta(name).await?;
                Ok(Node::ByIdEntryDir(name.to_string()))
            }
            Node::ByIdEntryDir(id) => match name {
                "meta.json" => Ok(Node::ByIdMeta(id.clone())),
                "content" => Ok(Node::ByIdContent(id.clone())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::FilesDir(folder_id) => {
                let entries = self.projected_folder_entries(folder_id).await?;
                let matched = entries
                    .into_iter()
                    .find(|entry| entry.projected_name == name)
                    .ok_or(ErrorCode::NotFound)?;
                if matched.meta.is_folder {
                    Ok(Node::FilesDir(matched.meta.id))
                } else {
                    Ok(Node::FilesEntry(matched.meta.id))
                }
            }
            Node::SearchDir => {
                let query = decode_search_query(name)?;
                Ok(Node::SearchResult(query))
            }
            Node::UploadsDir => match name {
                "new" => Ok(Node::UploadsNew),
                _ => Err(ErrorCode::NotFound),
            },
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    async fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        if let Some(static_bytes) = node.static_bytes() {
            return Ok(static_bytes.to_vec());
        }
        match node {
            Node::Status => {
                let auth = self.backend.auth_summary();
                let snapshot = self.state.lock().await.status_snapshot(auth);
                encode_pretty_json(&snapshot)
            }
            Node::ByIdDir => {
                let ids: BTreeSet<String> = self.state.lock().await.files.keys().cloned().collect();
                Ok(join_lines(ids))
            }
            Node::ByIdMeta(id) => encode_pretty_json(&self.read_file_meta(id).await?),
            Node::ByIdContent(id) | Node::FilesEntry(id) => {
                let meta = self.read_file_meta(id).await?;
                if meta.is_folder {
                    return Err(ErrorCode::IsDirectory);
                }
                if meta.is_workspace_doc {
                    self.read_workspace_export(&meta).await
                } else {
                    let data = self
                        .backend
                        .read_binary_range(
                            &meta,
                            0,
                            MAX_EXPORT_BYTES as u32,
                            meta.etag.as_deref(),
                            meta.head_revision_id.as_deref(),
                        )
                        .await
                        .map_err(|err| err.code)?;
                    Ok(data.bytes)
                }
            }
            Node::FilesDir(folder_id) => {
                let entries = self.projected_folder_entries(folder_id).await?;
                Ok(join_lines(entries.into_iter().map(|e| e.projected_name)))
            }
            Node::SearchDir => {
                let queries: Vec<String> =
                    self.state.lock().await.searches.keys().cloned().collect();
                Ok(join_lines(queries))
            }
            Node::SearchResult(query) => self.run_search(query).await,
            Node::UploadsNew => {
                let receipt = self.state.lock().await.last_uploaded.clone();
                receipt.map_or_else(|| Ok(Vec::new()), |r| encode_pretty_json(&r))
            }
            _ => unreachable!("handled by static_bytes"),
        }
    }

    async fn commit_write(&self, node: Node, bytes: Vec<u8>) -> Result<(), ErrorCode> {
        let read_only = self.state.lock().await.read_only;
        if read_only {
            return Err(ErrorCode::NoAccess);
        }
        match node {
            Node::Ctl => self.commit_ctl(&bytes).await,
            Node::UploadsNew => self.commit_upload(&bytes).await,
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn commit_ctl(&self, bytes: &[u8]) -> Result<(), ErrorCode> {
        let cmd = parse_single_line_ctl(bytes)?;
        let mut parts = cmd.split_whitespace();
        match (parts.next(), parts.next(), parts.next()) {
            (Some("sync"), None, None) => self.sync_drive().await,
            (Some("refresh"), Some(file_id), None) => {
                if !valid_drive_id(file_id) {
                    return Err(ErrorCode::BadRequest);
                }
                {
                    let mut state = self.state.lock().await;
                    state.files.remove(file_id);
                    state.exports.remove(file_id);
                    state.folders.remove(file_id);
                }
                let meta = self.read_file_meta(file_id).await?;
                if meta.is_folder {
                    let _ = self.projected_folder_entries(file_id).await?;
                    self.state
                        .lock()
                        .await
                        .bump(&Node::FilesDir(file_id.to_string()));
                }
                Ok(())
            }
            (Some("pin"), Some(folder_id), None) => {
                if !valid_drive_id(folder_id) {
                    return Err(ErrorCode::BadRequest);
                }
                let meta = self.read_file_meta(folder_id).await?;
                if !meta.is_folder {
                    return Err(ErrorCode::BadRequest);
                }
                self.state
                    .lock()
                    .await
                    .pinned_folders
                    .insert(folder_id.to_string());
                self.state.lock().await.folders.remove(folder_id);
                let _ = self.projected_folder_entries(folder_id).await?;
                let mut state = self.state.lock().await;
                state.bump(&Node::FilesDir("root".to_string()));
                state.bump(&Node::FilesDir(folder_id.to_string()));
                state.bump(&Node::Status);
                Ok(())
            }
            // Fail closed on any unknown or destructive verb (`delete`, `rm`, `trash`, `empty-trash`, ...).
            _ => Err(ErrorCode::BadRequest),
        }
    }

    async fn commit_upload(&self, bytes: &[u8]) -> Result<(), ErrorCode> {
        if !self.backend.has_file_write_scope().await {
            self.state.lock().await.record_api_error(
                ErrorCode::NoAccess,
                "uploads/new denied: missing Google Drive `drive.file` OAuth scope",
                None,
            );
            return Err(ErrorCode::NoAccess);
        }
        let req: NewDriveUploadRequest =
            serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let decoded = req.validate_and_decode()?;
        let meta = match self.backend.upload_file(&req, &decoded).await {
            Ok(meta) => meta,
            Err(err) => {
                self.state.lock().await.record_api_error(
                    err.code,
                    &err.message,
                    err.retry_after_secs,
                );
                return Err(err.code);
            }
        };
        let receipt = DriveUploadedReceipt {
            id: meta.id.clone(),
            name: meta.name.clone(),
            mime_type: meta.mime_type.clone(),
            web_view_link: meta.web_view_link.clone(),
        };
        let parents = meta.parents.clone();
        let mut state = self.state.lock().await;
        state.record_api_success();
        state.cache_file_meta(meta);
        state.last_uploaded = Some(receipt);
        state.folders.remove("root");
        state.bump(&Node::FilesDir("root".to_string()));
        for parent in parents {
            state.folders.remove(&parent);
            state.bump(&Node::FilesDir(parent));
        }
        state.bump(&Node::UploadsNew);
        Ok(())
    }
}

#[async_trait]
impl<B: GdriveBackend + 'static> FileServer for GdriveFs<B> {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut node = {
            let state = self.state.lock().await;
            if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            state.node_of(fid)?
        };
        for name in names {
            node = self.resolve_child(&node, name).await?;
        }
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid, GdriveFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let (node, read_only) = {
            let state = self.state.lock().await;
            if fid == Fid::ROOT {
                if !matches!(mode, OpenMode::Read) {
                    return Err(ErrorCode::NoAccess);
                }
                return Ok(state.qid(&Node::Root));
            }
            if state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
                return Err(ErrorCode::BadRequest);
            }
            (state.node_of(fid)?, state.read_only)
        };
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            if !node.is_writable(read_only) {
                return Err(ErrorCode::NoAccess);
            }
            if matches!(node, Node::UploadsNew) && !self.backend.has_file_write_scope().await {
                return Err(ErrorCode::NoAccess);
            }
        }
        if matches!(mode, OpenMode::Read | OpenMode::ReadWrite)
            && let Node::ByIdContent(ref id) | Node::FilesEntry(ref id) = node
        {
            let meta = self.read_file_meta(id).await?;
            let mut state = self.state.lock().await;
            let qid = state.qid(&node);
            let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.pinned_etag = meta.etag;
            f.pinned_revision = meta.head_revision_id;
            f.mode = Some(mode);
            return Ok(qid);
        }
        let mut state = self.state.lock().await;
        let qid = state.qid(&node);
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        f.mode = Some(mode);
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let node = {
            let state = self.state.lock().await;
            if fid != Fid::ROOT {
                let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
            }
            state.node_of(fid)?
        };
        if fid == Fid::ROOT {
            let bytes = self.computed_bytes(&node).await?;
            return Ok(slice_bytes(&bytes, offset, count));
        }

        if let Node::ByIdContent(ref id) | Node::FilesEntry(ref id) = node {
            let meta = self.read_file_meta(id).await?;
            if meta.is_folder {
                return Err(ErrorCode::IsDirectory);
            }
            if !meta.is_workspace_doc {
                return self
                    .read_binary_content_range(fid, &meta, offset, count)
                    .await;
            }
        }

        let need_refresh = {
            let state = self.state.lock().await;
            let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            offset == 0 || f.read_snapshot.is_none()
        };
        if need_refresh {
            let bytes = self.computed_bytes(&node).await?;
            let mut state = self.state.lock().await;
            if let Some(f) = state.fids.get_mut(&fid) {
                f.read_snapshot = Some(bytes);
            } else {
                return Err(ErrorCode::NotFound);
            }
        }
        let state = self.state.lock().await;
        let bytes = state
            .fids
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?
            .read_snapshot
            .as_ref()
            .ok_or(ErrorCode::Io)?;
        Ok(slice_bytes(bytes, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let read_only = state.read_only;
        let node = state.node_of(fid)?;
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if !node.is_writable(read_only) {
            return Err(ErrorCode::Unsupported);
        }
        f.buffer_write(offset, data, node.max_write_bytes())
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let (node, qid, read_only) = {
            let state = self.state.lock().await;
            let node = state.node_of(fid)?;
            let qid = state.qid(&node);
            (node, qid, state.read_only)
        };
        let length = if qid.kind == FileKind::Dir {
            0
        } else if let Node::ByIdContent(ref id) | Node::FilesEntry(ref id) = node {
            let meta = self.read_file_meta(id).await?;
            if meta.is_workspace_doc {
                self.read_workspace_export(&meta).await?.len() as u64
            } else {
                meta.size.unwrap_or(0)
            }
        } else {
            self.computed_bytes(&node).await?.len() as u64
        };
        Ok(Stat {
            name: String::new(),
            qid,
            length,
            executable: false,
            writable: node.is_writable(read_only),
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
        let f = self
            .state
            .lock()
            .await
            .fids
            .remove(&fid)
            .ok_or(ErrorCode::NotFound)?;
        if f.wrote
            && !f.write_failed
            && matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite))
        {
            self.commit_write(f.node, f.write_buf).await?;
        }
        Ok(())
    }
}

/// Export `server` over arbitrary async reader/writer streams using aP wire framing.
pub async fn serve_io<B, R, W>(
    server: Arc<GdriveFs<B>>,
    reader: R,
    writer: W,
) -> Result<(), WireError>
where
    B: GdriveBackend + 'static,
    R: AsyncBufRead + Send + Unpin + 'static,
    W: AsyncWrite + Unpin,
{
    let file_server: Arc<dyn FileServer> = server;
    export_file_server(file_server, reader, writer).await
}

/// Export `server` over process stdio using framed aP wire transport.
pub async fn serve_stdio<B: GdriveBackend + 'static>(
    server: Arc<GdriveFs<B>>,
) -> Result<(), WireError> {
    let stdin = BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    serve_io(server, stdin, stdout).await
}
