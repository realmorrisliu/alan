//! alan-hostfs — host directory file server for Alan OS.
//!
//! `HostDirFs` exposes one canonical host directory as an ordinary aP file tree.
//! It is intentionally a file server, not kernel state: namespace access is still
//! owned by `alan-kernel`, while this crate only maps a declared host root to aP
//! fid operations and rejects any path that escapes that root.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Read as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::Mutex;

const MAX_BUFFERED_FILE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostDirAccess {
    ReadOnly,
    ReadWrite,
}

impl HostDirAccess {
    pub const fn writable(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

/// Host-directory-backed aP file server.
pub struct HostDirFs {
    root: PathBuf,
    access: HostDirAccess,
    fids: Mutex<HashMap<Fid, HostFid>>,
}

struct HostFid {
    rel: Vec<String>,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
    wrote: bool,
}

impl HostDirFs {
    /// Export an existing host directory as an aP tree.
    pub fn new(root: impl AsRef<Path>, access: HostDirAccess) -> Result<Self, ErrorCode> {
        let root = std::fs::canonicalize(root).map_err(|_| ErrorCode::NotFound)?;
        if !root.is_dir() {
            return Err(ErrorCode::NotDirectory);
        }
        Ok(Self {
            root,
            access,
            fids: Mutex::new(HashMap::new()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub const fn access(&self) -> HostDirAccess {
        self.access
    }

    fn rel_for_fid(fids: &HashMap<Fid, HostFid>, fid: Fid) -> Result<Vec<String>, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Vec::new());
        }
        fids.get(&fid)
            .map(|fid| fid.rel.clone())
            .ok_or(ErrorCode::NotFound)
    }

    fn candidate_path(&self, rel: &[String]) -> Result<PathBuf, ErrorCode> {
        validate_rel(rel)?;
        Ok(rel
            .iter()
            .fold(self.root.clone(), |path, name| path.join(name)))
    }

    fn existing_path(&self, rel: &[String]) -> Result<PathBuf, ErrorCode> {
        let candidate = self.candidate_path(rel)?;
        let resolved = std::fs::canonicalize(candidate).map_err(|_| ErrorCode::NotFound)?;
        ensure_under_root(&self.root, &resolved)?;
        Ok(resolved)
    }

    fn create_path(&self, parent: &[String], name: &str) -> Result<PathBuf, ErrorCode> {
        validate_name(name)?;
        let parent_path = self.existing_path(parent)?;
        if !parent_path.is_dir() {
            return Err(ErrorCode::NotDirectory);
        }
        let candidate = parent_path.join(name);
        if candidate.exists() {
            return Err(ErrorCode::BadRequest);
        }
        ensure_under_root(&self.root, &candidate)?;
        Ok(candidate)
    }

    fn qid_for_path(&self, path: &Path) -> Result<Qid, ErrorCode> {
        let metadata = std::fs::metadata(path).map_err(|_| ErrorCode::NotFound)?;
        let kind = if metadata.is_dir() {
            FileKind::Dir
        } else if metadata.is_file() {
            FileKind::File
        } else {
            return Err(ErrorCode::Unsupported);
        };
        let version = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(qid_version_from_duration)
            .unwrap_or(0);
        Ok(Qid {
            kind,
            version,
            path: qid_path(path),
        })
    }
}

#[async_trait]
impl FileServer for HostDirFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut fids = self.fids.lock().await;
        if newfid == Fid::ROOT || fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut rel = Self::rel_for_fid(&fids, fid)?;
        let base = self.existing_path(&rel)?;
        if !names.is_empty() && !base.is_dir() {
            return Err(ErrorCode::NotDirectory);
        }
        rel.extend(names.iter().cloned());
        let path = self.existing_path(&rel)?;
        let qid = self.qid_for_path(&path)?;
        fids.insert(
            newfid,
            HostFid {
                rel,
                mode: None,
                write_buf: Vec::new(),
                wrote: false,
            },
        );
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) && !self.access.writable() {
            return Err(ErrorCode::NoAccess);
        }
        let mut fids = self.fids.lock().await;
        let rel = Self::rel_for_fid(&fids, fid)?;
        let path = self.existing_path(&rel)?;
        let qid = self.qid_for_path(&path)?;
        if qid.kind == FileKind::Dir && matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            return Err(ErrorCode::IsDirectory);
        }
        if let Some(fid_state) = fids.get_mut(&fid) {
            if fid_state.mode.is_some() {
                return Err(ErrorCode::BadRequest);
            }
            fid_state.mode = Some(mode);
            if matches!(mode, OpenMode::ReadWrite) && qid.kind == FileKind::File {
                fid_state.write_buf = read_file_for_write_seed(&path)?;
            }
        }
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let fids = self.fids.lock().await;
        if fid != Fid::ROOT {
            let fid_state = fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if !matches!(fid_state.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                return Err(ErrorCode::NoAccess);
            }
        }
        let rel = Self::rel_for_fid(&fids, fid)?;
        drop(fids);

        let path = self.existing_path(&rel)?;
        let bytes = if path.is_dir() {
            directory_listing(&path).await?
        } else if path.is_file() {
            return read_file_range(&path, offset, count).await;
        } else {
            return Err(ErrorCode::Unsupported);
        };
        Ok(slice(bytes, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        if !self.access.writable() {
            return Err(ErrorCode::NoAccess);
        }
        let mut fids = self.fids.lock().await;
        let fid_state = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(fid_state.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        let path = self.existing_path(&fid_state.rel)?;
        if !path.is_file() {
            return Err(ErrorCode::Unsupported);
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > MAX_BUFFERED_FILE_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        if fid_state.write_buf.len() < end {
            fid_state.write_buf.resize(end, 0);
        }
        fid_state.write_buf[start..end].copy_from_slice(data);
        fid_state.wrote = true;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let fids = self.fids.lock().await;
        let rel = Self::rel_for_fid(&fids, fid)?;
        drop(fids);

        let path = self.existing_path(&rel)?;
        let metadata = std::fs::metadata(&path).map_err(|_| ErrorCode::NotFound)?;
        let qid = self.qid_for_path(&path)?;
        let length = if metadata.is_file() {
            metadata.len()
        } else {
            directory_listing(&path).await?.len() as u64
        };
        Ok(Stat {
            name: rel.last().cloned().unwrap_or_default(),
            qid,
            length,
            writable: self.access.writable() && metadata.is_file(),
        })
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if !self.access.writable() {
            return Err(ErrorCode::NoAccess);
        }
        let mut fids = self.fids.lock().await;
        if newfid == Fid::ROOT || fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let parent_rel = Self::rel_for_fid(&fids, fid)?;
        let path = self.create_path(&parent_rel, name)?;
        match kind {
            FileKind::Dir => std::fs::create_dir(&path).map_err(|_| ErrorCode::Io)?,
            FileKind::File => {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|_| ErrorCode::Io)?;
            }
            FileKind::Stream | FileKind::Clone => return Err(ErrorCode::Unsupported),
        }
        let mut rel = parent_rel;
        rel.push(name.to_string());
        let qid = self.qid_for_path(&path)?;
        fids.insert(
            newfid,
            HostFid {
                rel,
                mode: None,
                write_buf: Vec::new(),
                wrote: false,
            },
        );
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Err(ErrorCode::Unsupported);
        }
        if !self.access.writable() {
            return Err(ErrorCode::NoAccess);
        }
        let mut fids = self.fids.lock().await;
        let rel = Self::rel_for_fid(&fids, fid)?;
        let path = self.existing_path(&rel)?;
        if path.is_dir() {
            std::fs::remove_dir(&path).map_err(|_| ErrorCode::Io)?;
        } else if path.is_file() {
            std::fs::remove_file(&path).map_err(|_| ErrorCode::Io)?;
        } else {
            return Err(ErrorCode::Unsupported);
        }
        fids.remove(&fid);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let mut fids = self.fids.lock().await;
        let fid_state = fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        if fid_state.wrote {
            let path = self.existing_path(&fid_state.rel)?;
            if !path.is_file() {
                return Err(ErrorCode::Unsupported);
            }
            tokio::fs::write(path, fid_state.write_buf)
                .await
                .map_err(|_| ErrorCode::Io)?;
        }
        Ok(())
    }
}

fn validate_rel(rel: &[String]) -> Result<(), ErrorCode> {
    for name in rel {
        validate_name(name)?;
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), ErrorCode> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\n') {
        return Err(ErrorCode::BadRequest);
    }
    Ok(())
}

fn ensure_under_root(root: &Path, path: &Path) -> Result<(), ErrorCode> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err(ErrorCode::NoAccess)
    }
}

async fn directory_listing(path: &Path) -> Result<Vec<u8>, ErrorCode> {
    let mut entries = tokio::fs::read_dir(path).await.map_err(|_| ErrorCode::Io)?;
    let mut names = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|_| ErrorCode::Io)? {
        let name = entry.file_name();
        names.push(name.to_string_lossy().to_string());
    }
    names.sort();
    Ok(names.join("\n").into_bytes())
}

async fn read_file_range(path: &Path, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
    let count = count as usize;
    if count > MAX_BUFFERED_FILE_BYTES {
        return Err(ErrorCode::BadRequest);
    }
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| ErrorCode::Io)?;
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(|_| ErrorCode::Io)?;
    let mut bytes = vec![0; count];
    let read = file.read(&mut bytes).await.map_err(|_| ErrorCode::Io)?;
    bytes.truncate(read);
    Ok(bytes)
}

fn read_file_for_write_seed(path: &Path) -> Result<Vec<u8>, ErrorCode> {
    let file = std::fs::File::open(path).map_err(|_| ErrorCode::Io)?;
    let mut limited = file.take(MAX_BUFFERED_FILE_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).map_err(|_| ErrorCode::Io)?;
    if bytes.len() > MAX_BUFFERED_FILE_BYTES {
        return Err(ErrorCode::BadRequest);
    }
    Ok(bytes)
}

fn qid_path(path: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn qid_version_from_duration(duration: Duration) -> u32 {
    let mut hasher = DefaultHasher::new();
    duration.as_secs().hash(&mut hasher);
    duration.subsec_nanos().hash(&mut hasher);
    hasher.finish() as u32
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_host_file() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("notes")).unwrap();
        std::fs::write(temp.path().join("notes/today.txt"), "hello").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        let qid = fs
            .walk(
                Fid::ROOT,
                Fid(1),
                &["notes".to_string(), "today.txt".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(qid.kind, FileKind::File);
        fs.open(Fid(1), OpenMode::Read).await.unwrap();
        let bytes = fs.read(Fid(1), 0, 1024).await.unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[tokio::test]
    async fn ranged_reads_do_not_require_full_file_buffering() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("large.txt"), b"0123456789abcdef").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["large.txt".to_string()])
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Read).await.unwrap();
        let bytes = fs.read(Fid(1), 10, 3).await.unwrap();
        assert_eq!(bytes, b"abc");
    }

    #[tokio::test]
    async fn lists_directory_in_stable_order() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("b.txt"), "").unwrap();
        std::fs::write(temp.path().join("a.txt"), "").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        let listing = fs.read(Fid::ROOT, 0, 1024).await.unwrap();
        assert_eq!(String::from_utf8(listing).unwrap(), "a.txt\nb.txt");
    }

    #[tokio::test]
    async fn writes_creates_and_removes_files() {
        let temp = tempfile::tempdir().unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.create(Fid::ROOT, Fid(1), "draft.txt", FileKind::File)
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Write).await.unwrap();
        fs.write(Fid(1), 0, b"draft").await.unwrap();
        fs.clunk(Fid(1)).await.unwrap();
        assert_eq!(
            std::fs::read(temp.path().join("draft.txt")).unwrap(),
            b"draft"
        );

        fs.walk(Fid::ROOT, Fid(2), &["draft.txt".to_string()])
            .await
            .unwrap();
        fs.remove(Fid(2)).await.unwrap();
        assert!(!temp.path().join("draft.txt").exists());
    }

    #[tokio::test]
    async fn rejects_sparse_writes_beyond_buffer_limit() {
        let temp = tempfile::tempdir().unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.create(Fid::ROOT, Fid(1), "draft.txt", FileKind::File)
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Write).await.unwrap();
        let offset = Offset::try_from(MAX_BUFFERED_FILE_BYTES + 1).unwrap();
        let err = fs.write(Fid(1), offset, b"x").await.unwrap_err();
        assert_eq!(err, ErrorCode::BadRequest);
    }

    #[tokio::test]
    async fn readwrite_rejects_seed_files_beyond_buffer_limit() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("large.txt");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_BUFFERED_FILE_BYTES as u64 + 1)
            .unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["large.txt".to_string()])
            .await
            .unwrap();
        let err = fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap_err();
        assert_eq!(err, ErrorCode::BadRequest);
    }

    #[test]
    fn qid_versions_include_full_modified_timestamp() {
        let first = qid_version_from_duration(Duration::new(1, 42));
        let same_nanos_later_second = qid_version_from_duration(Duration::new(2, 42));
        let same_second_later_nanos = qid_version_from_duration(Duration::new(1, 43));

        assert_ne!(first, same_nanos_later_second);
        assert_ne!(first, same_second_later_nanos);
    }

    #[tokio::test]
    async fn read_only_rejects_mutation() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a.txt"), "a").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["a.txt".to_string()])
            .await
            .unwrap();
        assert_eq!(
            fs.open(Fid(1), OpenMode::Write).await.unwrap_err(),
            ErrorCode::NoAccess
        );
        assert_eq!(
            fs.create(Fid::ROOT, Fid(2), "b.txt", FileKind::File)
                .await
                .unwrap_err(),
            ErrorCode::NoAccess
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        symlink(
            outside.path().join("secret.txt"),
            temp.path().join("secret-link"),
        )
        .unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        assert_eq!(
            fs.walk(Fid::ROOT, Fid(1), &["secret-link".to_string()])
                .await
                .unwrap_err(),
            ErrorCode::NoAccess
        );
    }

    #[tokio::test]
    async fn rejects_parent_traversal_components() {
        let temp = tempfile::tempdir().unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        assert_eq!(
            fs.walk(Fid::ROOT, Fid(1), &["..".to_string()])
                .await
                .unwrap_err(),
            ErrorCode::BadRequest
        );
    }
}
