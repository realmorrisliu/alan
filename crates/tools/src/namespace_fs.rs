//! Shared aP namespace filesystem helper for builtin file and exploration tools.
//!
//! Authorization is owned strictly by the Process's Kernel namespace view and its
//! mounted `FileServer`s — this module contains no hardcoded path allowlists.
//! Every operation inspects `Qid`/`Stat` before calling `Open` so `FileKind::Stream`
//! and `FileKind::Clone` nodes are never opened, blocked on, or allocated.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use alan_agent_engine::tools::alan_ap::{
    ErrorCode, Fid, FileKind, InProcessTransport, OpenMode, Qid, Request, Response, Stat,
};
use anyhow::{Context, Result, anyhow, bail};

/// Disjoint fid allocation range for builtin tools (separate from `alan-shell` 1..,
/// engine `NamespaceClient` 10_000.., and service-manager 90_000..).
static NEXT_TOOL_FID: AtomicU64 = AtomicU64::new(50_000_000);

const IO_CHUNK_BYTES: usize = 64 * 1024;
pub(crate) const MAX_NAMESPACE_READ_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_NAMESPACE_GREP_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_NAMESPACE_DIR_LISTING_BYTES: usize = 1024 * 1024;
const MAX_NAMESPACE_DIR_ENTRIES: usize = 10_000;
pub(crate) const MAX_NAMESPACE_WALK_DEPTH: usize = 32;
pub(crate) const MAX_NAMESPACE_WALK_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamespaceDirEntry {
    pub name: String,
    pub kind: &'static str,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamespaceWalkFile {
    pub path: String,
    pub size: u64,
    pub version: u32,
}

enum ReadBoundedError {
    Protocol(ErrorCode),
    TooLarge,
}

/// Normalize a raw path (or default cwd when `raw` is `None`) against `namespace_cwd`
/// into a canonical absolute alan9 namespace path, rejecting `..` escapes past root.
pub(crate) fn resolve_namespace_path(namespace_cwd: &Path, raw: Option<&str>) -> Result<String> {
    let joined = match raw {
        Some("") => bail!("Path must not be empty"),
        Some(raw) => {
            if raw.contains(['\0', '\n', '\r']) {
                bail!("Namespace path contains invalid control characters");
            }
            let path = Path::new(raw);
            if path.is_absolute() {
                PathBuf::from(path)
            } else {
                namespace_cwd.join(path)
            }
        }
        None => namespace_cwd.to_path_buf(),
    };

    if !joined.is_absolute() {
        bail!(
            "Process working directory '{}' is not an absolute namespace path",
            namespace_cwd.display()
        );
    }

    let mut components = Vec::new();
    for component in joined.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Prefix(_) => bail!("Namespace path must not contain a host prefix"),
            Component::ParentDir => {
                if components.pop().is_none() {
                    bail!("Namespace path escapes root");
                }
            }
            Component::Normal(part) => {
                let segment = part
                    .to_str()
                    .context("Namespace path must be valid UTF-8")?;
                components.push(segment.to_string());
            }
        }
    }

    Ok(join_components(&components))
}

#[derive(Clone)]
pub(crate) struct NamespaceFs {
    transport: InProcessTransport,
}

impl NamespaceFs {
    pub(crate) fn new(transport: InProcessTransport) -> Self {
        Self { transport }
    }

    /// Inspect a namespace path without opening it.
    pub(crate) async fn stat(&self, path: &str) -> Result<Stat> {
        self.stat_raw(path)
            .await
            .map_err(|err| anyhow!("Failed to stat '{path}': {err}"))
    }

    /// Read a regular file (`FileKind::File`) up to `max_bytes`.
    /// Never opens directories, streams, or clone files.
    pub(crate) async fn read_file_bytes(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>> {
        let (fid, qid) = self
            .walk_to(path)
            .await
            .map_err(|err| anyhow!("Failed to open '{path}': {err}"))?;
        if let Err(err) = ensure_regular_file(path, qid.kind) {
            self.clunk_quietly(fid).await;
            return Err(err);
        }
        let stat = match self.stat_fid(fid).await {
            Ok(stat) => stat,
            Err(err) => {
                self.clunk_quietly(fid).await;
                return Err(anyhow!("Failed to stat '{path}': {err}"));
            }
        };
        if let Err(err) = ensure_regular_file(path, stat.qid.kind) {
            self.clunk_quietly(fid).await;
            return Err(err);
        }
        if stat.length > max_bytes as u64 {
            self.clunk_quietly(fid).await;
            bail!(
                "File '{path}' ({} bytes) exceeds {} byte read limit",
                stat.length,
                max_bytes
            );
        }
        if let Err(err) = self.open_fid(fid, OpenMode::Read).await {
            self.clunk_quietly(fid).await;
            return Err(anyhow!("Failed to open '{path}': {err}"));
        }
        let read_res = self.read_bounded_opened(fid, max_bytes).await;
        self.clunk_quietly(fid).await;
        match read_res {
            Ok(bytes) => Ok(bytes),
            Err(ReadBoundedError::TooLarge) => {
                bail!("File '{path}' exceeds {} byte read limit", max_bytes)
            }
            Err(ReadBoundedError::Protocol(err)) => Err(anyhow!("Failed to read '{path}': {err}")),
        }
    }

    /// List immediate entries of a directory (`FileKind::Dir`) with types and sizes.
    pub(crate) async fn list_dir_entries(&self, path: &str) -> Result<Vec<NamespaceDirEntry>> {
        let names = self
            .read_dir_names_raw(path)
            .await
            .map_err(|err| anyhow!("Failed to list directory '{path}': {err}"))?;
        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            let child_path = join_child(path, &name);
            match self.stat_raw(&child_path).await {
                Ok(stat) => {
                    entries.push(NamespaceDirEntry {
                        name,
                        kind: file_kind_label(stat.qid.kind),
                        size: stat.length,
                    });
                }
                Err(ErrorCode::NotFound | ErrorCode::NoAccess) => {
                    // Entry vanished (e.g. exited /proc pid) or is an inaccessible symlink.
                }
                Err(err) => {
                    return Err(anyhow!("Failed to stat '{child_path}': {err}"));
                }
            }
        }
        entries.sort_by(|a, b| {
            let a_is_dir = a.kind == "directory";
            let b_is_dir = b.kind == "directory";
            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        Ok(entries)
    }

    /// Create or overwrite a regular file over aP, creating missing parent directories.
    /// Never opens directories, streams, or clone files; enforces Kernel/FileServer access.
    pub(crate) async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        let components = split_path(path);
        let Some((file_name, parent_components)) = components.split_last() else {
            bail!("Cannot write to directory '/'");
        };

        // Fast path: overwrite an existing regular file (truncating via OpenMode::Write).
        match self.walk_to(path).await {
            Ok((fid, qid)) => {
                if let Err(err) = ensure_regular_file(path, qid.kind) {
                    self.clunk_quietly(fid).await;
                    return Err(err);
                }
                let stat = match self.stat_fid(fid).await {
                    Ok(stat) => stat,
                    Err(err) => {
                        self.clunk_quietly(fid).await;
                        return Err(anyhow!("Failed to stat '{path}': {err}"));
                    }
                };
                if let Err(err) = ensure_regular_file(path, stat.qid.kind) {
                    self.clunk_quietly(fid).await;
                    return Err(err);
                }
                return self.open_write_and_commit(fid, path, data).await;
            }
            Err(ErrorCode::NotFound) => {}
            Err(err) => {
                return Err(anyhow!("Failed to open '{path}' for write: {err}"));
            }
        }

        // File does not yet exist: ensure parent directories exist, then create and write.
        self.ensure_dir_chain(parent_components).await?;

        let parent_path = join_components(parent_components);
        let (parent_fid, parent_qid) = self
            .walk_to(&parent_path)
            .await
            .map_err(|err| anyhow!("Failed to walk parent directory '{parent_path}': {err}"))?;
        if parent_qid.kind != FileKind::Dir {
            self.clunk_quietly(parent_fid).await;
            bail!("Parent path '{parent_path}' is not a directory");
        }

        let file_fid = alloc_fid();
        let create_res = self
            .transport
            .call(Request::Create {
                fid: parent_fid,
                newfid: file_fid,
                name: file_name.clone(),
                kind: FileKind::File,
            })
            .await;
        self.clunk_quietly(parent_fid).await;

        match create_res {
            Ok(Response::Create { .. }) => self.open_write_and_commit(file_fid, path, data).await,
            Ok(_) => {
                self.clunk_quietly(file_fid).await;
                bail!("Unexpected create response for '{path}'")
            }
            Err(err) => Err(anyhow!("Failed to create '{path}': {err}")),
        }
    }

    /// Recursively collect regular files (`FileKind::File`) under directory `root`,
    /// skipping `FileKind::Stream` and `FileKind::Clone` without opening them.
    pub(crate) async fn walk_files(
        &self,
        root: &str,
        include_hidden: bool,
        respect_gitignore: bool,
        max_depth: usize,
        max_entries: usize,
    ) -> Result<(Vec<NamespaceWalkFile>, bool)> {
        let mut files = Vec::new();
        let mut truncated = false;
        let mut visited = 0usize;
        let mut stack = vec![(root.to_string(), 0usize)];

        while let Some((dir_path, depth)) = stack.pop() {
            let names = match self.read_dir_names_raw(&dir_path).await {
                Ok(names) => names,
                Err(_) if dir_path != root => continue,
                Err(err) => return Err(anyhow!("Failed to list directory '{dir_path}': {err}")),
            };
            let mut subdirs = Vec::new();
            for name in names {
                if visited >= max_entries {
                    truncated = true;
                    break;
                }
                visited += 1;

                let child_path = join_child(&dir_path, &name);
                let stat = match self.stat_raw(&child_path).await {
                    Ok(stat) => stat,
                    Err(ErrorCode::NotFound | ErrorCode::NoAccess | ErrorCode::Unsupported) => {
                        continue;
                    }
                    Err(_) if dir_path != root => continue,
                    Err(err) => return Err(anyhow!("Failed to stat '{child_path}': {err}")),
                };

                match stat.qid.kind {
                    FileKind::Dir => {
                        if !include_hidden && name.starts_with('.') {
                            continue;
                        }
                        if respect_gitignore
                            && matches!(name.as_str(), ".git" | "target" | "node_modules")
                        {
                            continue;
                        }
                        if depth < max_depth {
                            subdirs.push(child_path);
                        } else {
                            truncated = true;
                        }
                    }
                    FileKind::File => {
                        files.push(NamespaceWalkFile {
                            path: child_path,
                            size: stat.length,
                            version: stat.qid.version,
                        });
                    }
                    FileKind::Stream | FileKind::Clone => {
                        // Never open or traverse streams or clone control files.
                    }
                }
            }
            if truncated && visited >= max_entries {
                break;
            }
            for subdir in subdirs.into_iter().rev() {
                stack.push((subdir, depth + 1));
            }
        }

        Ok((files, truncated))
    }

    async fn ensure_dir_chain(&self, parent_components: &[String]) -> Result<()> {
        for len in 1..=parent_components.len() {
            let prefix = &parent_components[..len];
            let dir_path = join_components(prefix);
            match self.walk_to(&dir_path).await {
                Ok((fid, qid)) => {
                    self.clunk_quietly(fid).await;
                    if qid.kind != FileKind::Dir {
                        bail!("Parent path '{dir_path}' is not a directory");
                    }
                }
                Err(ErrorCode::NotFound) => {
                    let parent_path = join_components(&prefix[..len - 1]);
                    let (parent_fid, parent_qid) =
                        self.walk_to(&parent_path).await.map_err(|err| {
                            anyhow!("Failed to walk parent directory '{parent_path}': {err}")
                        })?;
                    if parent_qid.kind != FileKind::Dir {
                        self.clunk_quietly(parent_fid).await;
                        bail!("Parent path '{parent_path}' is not a directory");
                    }
                    let new_fid = alloc_fid();
                    let create_res = self
                        .transport
                        .call(Request::Create {
                            fid: parent_fid,
                            newfid: new_fid,
                            name: prefix[len - 1].clone(),
                            kind: FileKind::Dir,
                        })
                        .await;
                    self.clunk_quietly(parent_fid).await;
                    match create_res {
                        Ok(Response::Create { .. }) => {
                            self.clunk_quietly(new_fid).await;
                        }
                        Ok(_) => {
                            self.clunk_quietly(new_fid).await;
                            bail!("Unexpected create response for directory '{dir_path}'");
                        }
                        Err(ErrorCode::BadRequest) => {
                            // Re-check in case a concurrent writer created the directory.
                            match self.walk_to(&dir_path).await {
                                Ok((fid, qid)) if qid.kind == FileKind::Dir => {
                                    self.clunk_quietly(fid).await;
                                }
                                Ok((fid, _)) => {
                                    self.clunk_quietly(fid).await;
                                    bail!("Parent path '{dir_path}' is not a directory");
                                }
                                Err(_) => {
                                    bail!(
                                        "Failed to create directory '{dir_path}': {}",
                                        ErrorCode::BadRequest
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            return Err(anyhow!("Failed to create directory '{dir_path}': {err}"));
                        }
                    }
                }
                Err(err) => {
                    return Err(anyhow!("Failed to walk directory '{dir_path}': {err}"));
                }
            }
        }
        Ok(())
    }

    async fn open_write_and_commit(&self, fid: Fid, path: &str, data: &[u8]) -> Result<()> {
        let write_res = async {
            self.open_fid(fid, OpenMode::Write)
                .await
                .map_err(|err| anyhow!("Failed to open '{path}' for write: {err}"))?;
            self.write_all_opened(fid, data)
                .await
                .with_context(|| format!("Failed to write '{path}'"))
        }
        .await;

        match write_res {
            Ok(()) => self
                .clunk(fid)
                .await
                .map_err(|err| anyhow!("Failed to commit write to '{path}': {err}")),
            Err(err) => {
                self.clunk_quietly(fid).await;
                Err(err)
            }
        }
    }

    async fn write_all_opened(&self, fid: Fid, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            match self
                .transport
                .call(Request::Write {
                    fid,
                    offset: 0,
                    data: Vec::new(),
                })
                .await
                .map_err(|err| anyhow!("{err}"))?
            {
                Response::Write { .. } => return Ok(()),
                _ => bail!("Unexpected write response"),
            }
        }

        let mut offset = 0u64;
        let mut remaining = data;
        while !remaining.is_empty() {
            let chunk_len = remaining.len().min(IO_CHUNK_BYTES);
            let accepted = match self
                .transport
                .call(Request::Write {
                    fid,
                    offset,
                    data: remaining[..chunk_len].to_vec(),
                })
                .await
                .map_err(|err| anyhow!("{err}"))?
            {
                Response::Write { count } => count as usize,
                _ => bail!("Unexpected write response"),
            };
            if accepted == 0 || accepted > chunk_len {
                bail!("Invalid write byte count from file server");
            }
            offset += accepted as u64;
            remaining = &remaining[accepted..];
        }
        Ok(())
    }

    async fn read_dir_names_raw(&self, path: &str) -> std::result::Result<Vec<String>, ErrorCode> {
        let (fid, qid) = self.walk_to(path).await?;
        if qid.kind != FileKind::Dir {
            self.clunk_quietly(fid).await;
            return Err(ErrorCode::NotDirectory);
        }
        let stat = match self.stat_fid(fid).await {
            Ok(stat) => stat,
            Err(err) => {
                self.clunk_quietly(fid).await;
                return Err(err);
            }
        };
        if stat.qid.kind != FileKind::Dir {
            self.clunk_quietly(fid).await;
            return Err(ErrorCode::NotDirectory);
        }
        if let Err(err) = self.open_fid(fid, OpenMode::Read).await {
            self.clunk_quietly(fid).await;
            return Err(err);
        }
        let read_res = self
            .read_bounded_opened(fid, MAX_NAMESPACE_DIR_LISTING_BYTES)
            .await;
        self.clunk_quietly(fid).await;
        let bytes = match read_res {
            Ok(bytes) => bytes,
            Err(ReadBoundedError::Protocol(err)) => return Err(err),
            Err(ReadBoundedError::TooLarge) => return Err(ErrorCode::Io),
        };
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::Io)?;
        let mut unique = BTreeSet::new();
        for line in text.lines() {
            if line.is_empty() || line == "." || line == ".." || line.contains('/') {
                continue;
            }
            unique.insert(line.to_string());
            if unique.len() >= MAX_NAMESPACE_DIR_ENTRIES {
                break;
            }
        }
        Ok(unique.into_iter().collect())
    }

    async fn read_bounded_opened(
        &self,
        fid: Fid,
        max_bytes: usize,
    ) -> std::result::Result<Vec<u8>, ReadBoundedError> {
        let mut out = Vec::new();
        let mut offset = 0u64;
        loop {
            let remaining = max_bytes.saturating_sub(out.len());
            let want = remaining.saturating_add(1).min(IO_CHUNK_BYTES) as u32;
            let chunk = self
                .read_at(fid, offset, want)
                .await
                .map_err(ReadBoundedError::Protocol)?;
            if chunk.is_empty() {
                break;
            }
            if chunk.len() > remaining {
                return Err(ReadBoundedError::TooLarge);
            }
            offset += chunk.len() as u64;
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    async fn stat_raw(&self, path: &str) -> std::result::Result<Stat, ErrorCode> {
        let (fid, _) = self.walk_to(path).await?;
        let res = self.stat_fid(fid).await;
        self.clunk_quietly(fid).await;
        res
    }

    async fn walk_to(&self, path: &str) -> std::result::Result<(Fid, Qid), ErrorCode> {
        let fid = alloc_fid();
        match self
            .transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: split_path(path),
            })
            .await?
        {
            Response::Walk { qid } => Ok((fid, qid)),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn stat_fid(&self, fid: Fid) -> std::result::Result<Stat, ErrorCode> {
        match self.transport.call(Request::Stat { fid }).await? {
            Response::Stat { stat } => Ok(stat),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn open_fid(&self, fid: Fid, mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
        match self.transport.call(Request::Open { fid, mode }).await? {
            Response::Open { qid } => Ok(qid),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn read_at(
        &self,
        fid: Fid,
        offset: u64,
        count: u32,
    ) -> std::result::Result<Vec<u8>, ErrorCode> {
        match self
            .transport
            .call(Request::Read { fid, offset, count })
            .await?
        {
            Response::Read { data } => Ok(data),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn clunk(&self, fid: Fid) -> std::result::Result<(), ErrorCode> {
        match self.transport.call(Request::Clunk { fid }).await? {
            Response::Clunk => Ok(()),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn clunk_quietly(&self, fid: Fid) {
        let _ = self.transport.call(Request::Clunk { fid }).await;
    }
}

fn alloc_fid() -> Fid {
    Fid(NEXT_TOOL_FID.fetch_add(1, Ordering::Relaxed))
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn join_components(components: &[String]) -> String {
    if components.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", components.join("/"))
    }
}

fn join_child(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{parent}/{child}")
    }
}

fn ensure_regular_file(path: &str, kind: FileKind) -> Result<()> {
    match kind {
        FileKind::File => Ok(()),
        FileKind::Dir => bail!("Path '{path}' is a directory, not a regular file"),
        FileKind::Stream => {
            bail!("Path '{path}' is a live stream and cannot be accessed as a regular file")
        }
        FileKind::Clone => {
            bail!("Path '{path}' is a clone control file and cannot be accessed as a regular file")
        }
    }
}

fn file_kind_label(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Dir => "directory",
        FileKind::File => "file",
        FileKind::Stream => "stream",
        FileKind::Clone => "clone",
    }
}
