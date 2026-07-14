//! alan-hostfs — host directory file server for Alan OS.
//!
//! `HostDirFs` exposes one canonical host directory as an ordinary aP file tree.
//! It is intentionally a file server, not kernel state: namespace access is still
//! owned by `alan-kernel`, while this crate only maps a declared host root to aP
//! fid operations and rejects any path that escapes that root.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::ffi::{CStr, CString};
use std::hash::{Hash, Hasher};
use std::io::{Read as _, SeekFrom, Write as _};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::Mutex;

const MAX_BUFFERED_FILE_BYTES: usize = 64 * 1024 * 1024;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    root_dir: std::fs::File,
    access: HostDirAccess,
    fids: Mutex<HashMap<Fid, HostFid>>,
}

struct HostFid {
    rel: Vec<String>,
    mode: Option<OpenMode>,
    write_identity: Option<HostFileIdentity>,
    write_buf: Vec<u8>,
    wrote: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HostFileIdentity {
    dev: u64,
    ino: u64,
    mtime: i64,
    mtime_nsec: i64,
    len: u64,
}

impl HostFileIdentity {
    fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            mtime: metadata.mtime(),
            mtime_nsec: metadata.mtime_nsec(),
            len: metadata.len(),
        }
    }
}

impl HostDirFs {
    /// Export an existing host directory as an aP tree.
    pub fn new(root: impl AsRef<Path>, access: HostDirAccess) -> Result<Self, ErrorCode> {
        let root = std::fs::canonicalize(root).map_err(|_| ErrorCode::NotFound)?;
        if !root.is_dir() {
            return Err(ErrorCode::NotDirectory);
        }
        let root_dir = open_root_dir(&root)?;
        Ok(Self {
            root,
            root_dir,
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

    fn existing_handle(&self, rel: &[String]) -> Result<HostHandle, ErrorCode> {
        open_existing_handle(&self.root_dir, rel, libc::O_RDONLY)
    }

    fn existing_write_handle(&self, rel: &[String]) -> Result<HostHandle, ErrorCode> {
        open_existing_handle(&self.root_dir, rel, libc::O_WRONLY)
    }

    fn parent_handle(&self, rel: &[String], name: &str) -> Result<HostHandle, ErrorCode> {
        validate_name(name)?;
        let parent = self.existing_handle(rel)?;
        if !parent.metadata.is_dir() {
            return Err(ErrorCode::NotDirectory);
        }
        Ok(parent)
    }

    fn qid_for_metadata(&self, metadata: &std::fs::Metadata) -> Result<Qid, ErrorCode> {
        let kind = file_kind(metadata)?;
        let version = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(qid_version_from_duration)
            .unwrap_or(0);
        Ok(Qid {
            kind,
            version,
            path: qid_path(metadata),
        })
    }

    fn symlink_qid_for_rel(&self, rel: &[String]) -> Result<Qid, ErrorCode> {
        if !self.access.writable() {
            return Err(ErrorCode::NoAccess);
        }
        let (parent, name) = open_parent_for_entry(&self.root_dir, rel)?;
        match entry_kind_at(parent.file.as_raw_fd(), &name)? {
            HostEntryKind::Symlink => Ok(Qid {
                kind: FileKind::File,
                version: 0,
                path: qid_path_for_rel(rel),
            }),
            HostEntryKind::Dir | HostEntryKind::File => Err(ErrorCode::NoAccess),
        }
    }

    fn qid_for_rel_no_follow(&self, rel: &[String]) -> Result<Qid, ErrorCode> {
        if rel.is_empty() {
            return self.qid_for_metadata(&self.root_dir.metadata().map_err(|_| ErrorCode::Io)?);
        }
        let (parent, name) = open_parent_for_entry(&self.root_dir, rel)?;
        let stat = entry_stat_at(parent.file.as_raw_fd(), &name)?;
        qid_for_entry_stat(&stat)
    }

    fn entry_kind_for_rel(&self, rel: &[String]) -> Result<HostEntryKind, ErrorCode> {
        validate_rel(rel)?;
        if rel.is_empty() {
            return Ok(HostEntryKind::Dir);
        }
        let (parent, name) = open_parent_for_entry(&self.root_dir, rel)?;
        entry_kind_at(parent.file.as_raw_fd(), &name)
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
        let base = self.existing_handle(&rel)?;
        if !names.is_empty() && !base.is_dir() {
            return Err(ErrorCode::NotDirectory);
        }
        rel.extend(names.iter().cloned());
        let qid = match self.existing_handle(&rel) {
            Ok(handle) => self.qid_for_metadata(&handle.metadata)?,
            Err(ErrorCode::NoAccess) => match self.qid_for_rel_no_follow(&rel) {
                Ok(qid) => qid,
                Err(ErrorCode::NoAccess) => self.symlink_qid_for_rel(&rel)?,
                Err(error) => return Err(error),
            },
            Err(error) => return Err(error),
        };
        fids.insert(
            newfid,
            HostFid {
                rel,
                mode: None,
                write_identity: None,
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
        let mut write_seed = None;
        let mut write_identity = None;
        let qid = match mode {
            OpenMode::Read => {
                let handle = self.existing_handle(&rel)?;
                self.qid_for_metadata(&handle.metadata)?
            }
            OpenMode::Write => match self.entry_kind_for_rel(&rel)? {
                HostEntryKind::Dir => return Err(ErrorCode::IsDirectory),
                HostEntryKind::Symlink => return Err(ErrorCode::NoAccess),
                HostEntryKind::File => {
                    let handle = self.existing_write_handle(&rel)?;
                    reject_multiply_linked_file(&handle.metadata)?;
                    write_identity = Some(HostFileIdentity::from_metadata(&handle.metadata));
                    self.qid_for_metadata(&handle.metadata)?
                }
            },
            OpenMode::ReadWrite => {
                let handle = self.existing_handle(&rel)?;
                let qid = self.qid_for_metadata(&handle.metadata)?;
                if qid.kind == FileKind::Dir {
                    return Err(ErrorCode::IsDirectory);
                }
                let read_identity = HostFileIdentity::from_metadata(&handle.metadata);
                let write_handle = self.existing_write_handle(&rel)?;
                reject_multiply_linked_file(&write_handle.metadata)?;
                ensure_same_file_identity(&write_handle.metadata, read_identity)?;
                drop(write_handle);
                write_seed = Some(read_file_for_write_seed(handle.file)?);
                write_identity = Some(read_identity);
                qid
            }
        };
        if let Some(fid_state) = fids.get_mut(&fid) {
            if fid_state.mode.is_some() {
                return Err(ErrorCode::BadRequest);
            }
            fid_state.mode = Some(mode);
            fid_state.write_identity = write_identity;
            if let Some(seed) = write_seed {
                fid_state.write_buf = seed;
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

        let handle = self.existing_handle(&rel)?;
        let bytes = if handle.metadata.is_dir() {
            directory_listing(handle.file)?
        } else if handle.metadata.is_file() {
            return read_file_range(handle.file, offset, count).await;
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
        match self.entry_kind_for_rel(&fid_state.rel)? {
            HostEntryKind::File => {}
            HostEntryKind::Dir => return Err(ErrorCode::IsDirectory),
            HostEntryKind::Symlink => return Err(ErrorCode::NoAccess),
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

        if rel.is_empty() {
            let handle = self.existing_handle(&rel)?;
            let qid = self.qid_for_metadata(&handle.metadata)?;
            let length = directory_listing(handle.file)?.len() as u64;
            return Ok(Stat {
                name: String::new(),
                qid,
                length,
                writable: false,
            });
        }

        let stat = entry_stat_for_rel_no_follow(&self.root_dir, &rel)?;
        let kind = entry_kind_from_stat(&stat)?;
        let qid = qid_for_entry_stat(&stat)?;
        let length = match kind {
            HostEntryKind::File => u64::try_from(stat.st_size).map_err(|_| ErrorCode::Io)?,
            HostEntryKind::Dir => {
                let handle = self.existing_handle(&rel)?;
                directory_listing(handle.file)?.len() as u64
            }
            HostEntryKind::Symlink => return Err(ErrorCode::NoAccess),
        };
        Ok(Stat {
            name: rel.last().cloned().unwrap_or_default(),
            qid,
            length,
            writable: self.access.writable() && qid.kind == FileKind::File,
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
        let parent = self.parent_handle(&parent_rel, name)?;
        match kind {
            FileKind::Dir => mkdir_child(parent.file.as_raw_fd(), name)?,
            FileKind::File => {
                create_child_file(parent.file.as_raw_fd(), name)?;
            }
            FileKind::Stream | FileKind::Clone => return Err(ErrorCode::Unsupported),
        }
        let mut rel = parent_rel;
        rel.push(name.to_string());
        let handle = self.existing_handle(&rel)?;
        let qid = self.qid_for_metadata(&handle.metadata)?;
        fids.insert(
            newfid,
            HostFid {
                rel,
                mode: None,
                write_identity: None,
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
        remove_entry(&self.root_dir, &rel)?;
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
            let identity = fid_state.write_identity.ok_or(ErrorCode::BadRequest)?;
            atomic_replace_file(
                &self.root_dir,
                &fid_state.rel,
                identity,
                &fid_state.write_buf,
            )?;
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
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\n')
        || name.contains('\0')
    {
        return Err(ErrorCode::BadRequest);
    }
    Ok(())
}

struct HostHandle {
    file: std::fs::File,
    metadata: std::fs::Metadata,
}

impl HostHandle {
    fn is_dir(&self) -> bool {
        self.metadata.is_dir()
    }
}

fn open_existing_handle(
    root_dir: &std::fs::File,
    rel: &[String],
    final_access: libc::c_int,
) -> Result<HostHandle, ErrorCode> {
    validate_rel(rel)?;
    // `try_clone`/`dup` shares the directory cursor with `root_dir`. A listing
    // would therefore consume the anchor's cursor and make later root listings
    // appear empty. Re-open `.` relative to the stable anchor so every operation
    // owns an independent open-file description and directory cursor.
    let mut current = reopen_root_dir(root_dir)?;
    let mut metadata = current.metadata().map_err(|_| ErrorCode::Io)?;
    if rel.is_empty() {
        return Ok(HostHandle {
            file: current,
            metadata,
        });
    }

    for (index, name) in rel.iter().enumerate() {
        if !metadata.is_dir() {
            return Err(ErrorCode::NotDirectory);
        }
        let is_last = index + 1 == rel.len();
        let flags = if is_last {
            match entry_kind_at(current.as_raw_fd(), name)? {
                HostEntryKind::Dir => {
                    if final_access != libc::O_RDONLY {
                        return Err(ErrorCode::IsDirectory);
                    }
                    final_access | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_DIRECTORY
                }
                HostEntryKind::File => final_access | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                HostEntryKind::Symlink => return Err(ErrorCode::NoAccess),
            }
        } else {
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_DIRECTORY
        };
        let child = openat_file(current.as_raw_fd(), name, flags, 0)?;
        metadata = child.metadata().map_err(|_| ErrorCode::Io)?;
        current = child;
    }

    Ok(HostHandle {
        file: current,
        metadata,
    })
}

fn open_parent_for_entry(
    root_dir: &std::fs::File,
    rel: &[String],
) -> Result<(HostHandle, String), ErrorCode> {
    let (name, parent_rel) = rel.split_last().ok_or(ErrorCode::Unsupported)?;
    validate_name(name)?;
    let parent = open_existing_handle(root_dir, parent_rel, libc::O_RDONLY)?;
    if !parent.metadata.is_dir() {
        return Err(ErrorCode::NotDirectory);
    }
    Ok((parent, name.clone()))
}

fn mkdir_child(parent_fd: RawFd, name: &str) -> Result<(), ErrorCode> {
    let name = c_name(name)?;
    let result = unsafe { libc::mkdirat(parent_fd, name.as_ptr(), 0o777) };
    if result == 0 {
        Ok(())
    } else {
        Err(map_create_error(std::io::Error::last_os_error()))
    }
}

fn create_child_file(parent_fd: RawFd, name: &str) -> Result<(), ErrorCode> {
    let file = create_child_file_with_mode(parent_fd, name, 0o666)?;
    drop(file);
    Ok(())
}

fn create_child_file_with_mode(
    parent_fd: RawFd,
    name: &str,
    mode: libc::mode_t,
) -> Result<std::fs::File, ErrorCode> {
    let name = c_name(name)?;
    let fd = unsafe {
        libc::openat(
            parent_fd,
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(map_create_error(std::io::Error::last_os_error()));
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

fn remove_entry(root_dir: &std::fs::File, rel: &[String]) -> Result<(), ErrorCode> {
    let (parent, name) = open_parent_for_entry(root_dir, rel)?;
    match entry_kind_at(parent.file.as_raw_fd(), &name)? {
        HostEntryKind::File | HostEntryKind::Symlink => {
            unlink_child(parent.file.as_raw_fd(), &name, 0)
        }
        HostEntryKind::Dir => unlink_child(parent.file.as_raw_fd(), &name, libc::AT_REMOVEDIR),
    }
}

fn unlink_child(parent_fd: RawFd, name: &str, flags: libc::c_int) -> Result<(), ErrorCode> {
    let name = c_name(name)?;
    let result = unsafe { libc::unlinkat(parent_fd, name.as_ptr(), flags) };
    if result == 0 {
        Ok(())
    } else {
        Err(map_remove_error(std::io::Error::last_os_error()))
    }
}

fn rename_child(parent_fd: RawFd, from: &str, to: &str) -> Result<(), ErrorCode> {
    let from = c_name(from)?;
    let to = c_name(to)?;
    let result = unsafe { libc::renameat(parent_fd, from.as_ptr(), parent_fd, to.as_ptr()) };
    if result == 0 {
        Ok(())
    } else {
        Err(map_create_error(std::io::Error::last_os_error()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostEntryKind {
    Dir,
    File,
    Symlink,
}

fn entry_kind_at(parent_fd: RawFd, name: &str) -> Result<HostEntryKind, ErrorCode> {
    let stat = entry_stat_at(parent_fd, name)?;
    entry_kind_from_stat(&stat)
}

fn entry_stat_at(parent_fd: RawFd, name: &str) -> Result<libc::stat, ErrorCode> {
    let name = c_name(name)?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            parent_fd,
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        return Err(map_open_error(std::io::Error::last_os_error()));
    }
    Ok(unsafe { stat.assume_init() })
}

fn entry_stat_for_rel_no_follow(
    root_dir: &std::fs::File,
    rel: &[String],
) -> Result<libc::stat, ErrorCode> {
    let (parent, name) = open_parent_for_entry(root_dir, rel)?;
    entry_stat_at(parent.file.as_raw_fd(), &name)
}

fn entry_kind_from_stat(stat: &libc::stat) -> Result<HostEntryKind, ErrorCode> {
    let mode = stat.st_mode as libc::mode_t;
    match mode & libc::S_IFMT {
        libc::S_IFDIR => Ok(HostEntryKind::Dir),
        libc::S_IFREG => Ok(HostEntryKind::File),
        libc::S_IFLNK => Ok(HostEntryKind::Symlink),
        _ => Err(ErrorCode::Unsupported),
    }
}

fn qid_for_entry_stat(stat: &libc::stat) -> Result<Qid, ErrorCode> {
    let kind = match entry_kind_from_stat(stat)? {
        HostEntryKind::Dir => FileKind::Dir,
        HostEntryKind::File => FileKind::File,
        HostEntryKind::Symlink => return Err(ErrorCode::NoAccess),
    };
    Ok(Qid {
        kind,
        version: 0,
        path: qid_path_from_stat(stat),
    })
}

fn openat_file(
    parent_fd: RawFd,
    name: &str,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> Result<std::fs::File, ErrorCode> {
    let name = c_name(name)?;
    let fd = unsafe { libc::openat(parent_fd, name.as_ptr(), flags, mode as libc::c_uint) };
    if fd < 0 {
        return Err(map_open_error(std::io::Error::last_os_error()));
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

fn open_root_dir(path: &Path) -> Result<std::fs::File, ErrorCode> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| ErrorCode::BadRequest)?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(map_open_error(std::io::Error::last_os_error()));
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

fn reopen_root_dir(root_dir: &std::fs::File) -> Result<std::fs::File, ErrorCode> {
    let dot = CString::new(".").expect("a literal dot has no NUL byte");
    let fd = unsafe {
        libc::openat(
            root_dir.as_raw_fd(),
            dot.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0,
        )
    };
    if fd < 0 {
        return Err(map_open_error(std::io::Error::last_os_error()));
    }
    Ok(unsafe { std::fs::File::from_raw_fd(fd) })
}

fn c_name(name: &str) -> Result<CString, ErrorCode> {
    validate_name(name)?;
    CString::new(name).map_err(|_| ErrorCode::BadRequest)
}

fn map_open_error(error: std::io::Error) -> ErrorCode {
    match error.raw_os_error() {
        Some(code) if code == libc::ENOENT => ErrorCode::NotFound,
        Some(code) if code == libc::ENOTDIR => ErrorCode::NotDirectory,
        Some(code) if code == libc::ELOOP => ErrorCode::NoAccess,
        Some(code) if code == libc::EACCES || code == libc::EPERM => ErrorCode::NoAccess,
        _ => ErrorCode::Io,
    }
}

fn map_create_error(error: std::io::Error) -> ErrorCode {
    match error.raw_os_error() {
        Some(code) if code == libc::EEXIST => ErrorCode::BadRequest,
        Some(code) if code == libc::ENOENT => ErrorCode::NotFound,
        Some(code) if code == libc::ENOTDIR => ErrorCode::NotDirectory,
        Some(code) if code == libc::ELOOP => ErrorCode::NoAccess,
        Some(code) if code == libc::EACCES || code == libc::EPERM => ErrorCode::NoAccess,
        _ => ErrorCode::Io,
    }
}

fn map_remove_error(error: std::io::Error) -> ErrorCode {
    match error.raw_os_error() {
        Some(code) if code == libc::ENOENT => ErrorCode::NotFound,
        Some(code) if code == libc::ENOTDIR => ErrorCode::NotDirectory,
        Some(code) if code == libc::EACCES || code == libc::EPERM => ErrorCode::NoAccess,
        _ => ErrorCode::Io,
    }
}

fn directory_listing(file: std::fs::File) -> Result<Vec<u8>, ErrorCode> {
    let fd = file.into_raw_fd();
    let dir = unsafe { libc::fdopendir(fd) };
    if dir.is_null() {
        unsafe {
            libc::close(fd);
        }
        return Err(ErrorCode::Io);
    }
    let mut names = Vec::new();
    loop {
        let entry = unsafe { libc::readdir(dir) };
        if entry.is_null() {
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        names.push(String::from_utf8_lossy(name.to_bytes()).to_string());
    }
    let close_result = unsafe { libc::closedir(dir) };
    if close_result != 0 {
        return Err(ErrorCode::Io);
    }
    names.sort();
    Ok(names.join("\n").into_bytes())
}

async fn read_file_range(
    file: std::fs::File,
    offset: Offset,
    count: u32,
) -> Result<Vec<u8>, ErrorCode> {
    let count = count as usize;
    if count > MAX_BUFFERED_FILE_BYTES {
        return Err(ErrorCode::BadRequest);
    }
    let mut file = tokio::fs::File::from_std(file);
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(|_| ErrorCode::Io)?;
    let mut bytes = vec![0; count];
    let read = file.read(&mut bytes).await.map_err(|_| ErrorCode::Io)?;
    bytes.truncate(read);
    Ok(bytes)
}

fn read_file_for_write_seed(file: std::fs::File) -> Result<Vec<u8>, ErrorCode> {
    let mut limited = file.take(MAX_BUFFERED_FILE_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).map_err(|_| ErrorCode::Io)?;
    if bytes.len() > MAX_BUFFERED_FILE_BYTES {
        return Err(ErrorCode::BadRequest);
    }
    Ok(bytes)
}

fn file_kind(metadata: &std::fs::Metadata) -> Result<FileKind, ErrorCode> {
    if metadata.is_dir() {
        Ok(FileKind::Dir)
    } else if metadata.is_file() {
        Ok(FileKind::File)
    } else {
        Err(ErrorCode::Unsupported)
    }
}

fn reject_multiply_linked_file(metadata: &std::fs::Metadata) -> Result<(), ErrorCode> {
    if metadata.is_file() && metadata.nlink() > 1 {
        Err(ErrorCode::NoAccess)
    } else {
        Ok(())
    }
}

fn ensure_same_file_identity(
    metadata: &std::fs::Metadata,
    expected: HostFileIdentity,
) -> Result<(), ErrorCode> {
    if HostFileIdentity::from_metadata(metadata) == expected {
        Ok(())
    } else {
        Err(ErrorCode::NoAccess)
    }
}

fn atomic_replace_file(
    root_dir: &std::fs::File,
    rel: &[String],
    expected_identity: HostFileIdentity,
    bytes: &[u8],
) -> Result<(), ErrorCode> {
    let (parent, name) = open_parent_for_entry(root_dir, rel)?;
    let existing = openat_file(
        parent.file.as_raw_fd(),
        &name,
        libc::O_WRONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        0,
    )?;
    let existing_metadata = existing.metadata().map_err(|_| ErrorCode::Io)?;
    if !existing_metadata.is_file() {
        return Err(ErrorCode::Unsupported);
    }
    reject_multiply_linked_file(&existing_metadata)?;
    ensure_same_file_identity(&existing_metadata, expected_identity)?;
    let mode = existing_metadata.mode() & 0o7777;
    drop(existing);

    let (temp_name, mut temp_file) = create_replacement_temp_file(parent.file.as_raw_fd(), mode)?;
    let write_result = temp_file
        .set_permissions(std::fs::Permissions::from_mode(mode))
        .and_then(|_| temp_file.write_all(bytes))
        .and_then(|_| temp_file.sync_all());
    if write_result.is_err() {
        drop(temp_file);
        let _ = unlink_child(parent.file.as_raw_fd(), &temp_name, 0);
        return Err(ErrorCode::Io);
    }
    drop(temp_file);
    if let Err(error) = rename_child(parent.file.as_raw_fd(), &temp_name, &name) {
        let _ = unlink_child(parent.file.as_raw_fd(), &temp_name, 0);
        return Err(error);
    }
    Ok(())
}

fn create_replacement_temp_file(
    parent_fd: RawFd,
    mode: u32,
) -> Result<(String, std::fs::File), ErrorCode> {
    for _ in 0..16 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(".alan-hostfs-replace-{}-{counter}", std::process::id());
        match create_child_file_with_mode(parent_fd, &name, mode as libc::mode_t) {
            Ok(file) => return Ok((name, file)),
            Err(ErrorCode::BadRequest) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(ErrorCode::Io)
}

fn qid_path(metadata: &std::fs::Metadata) -> u64 {
    let mut hasher = DefaultHasher::new();
    metadata.dev().hash(&mut hasher);
    metadata.ino().hash(&mut hasher);
    hasher.finish()
}

fn qid_path_from_stat(stat: &libc::stat) -> u64 {
    let mut hasher = DefaultHasher::new();
    stat.st_dev.hash(&mut hasher);
    stat.st_ino.hash(&mut hasher);
    hasher.finish()
}

fn qid_path_for_rel(rel: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    rel.hash(&mut hasher);
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

    #[cfg(unix)]
    fn running_as_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

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

    #[cfg(unix)]
    #[tokio::test]
    async fn walk_rejects_fifo_without_blocking() {
        use std::os::unix::ffi::OsStrExt;

        let temp = tempfile::tempdir().unwrap();
        let fifo = temp.path().join("pipe");
        let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        let walked = tokio::time::timeout(
            Duration::from_millis(100),
            fs.walk(Fid::ROOT, Fid(1), &["pipe".to_string()]),
        )
        .await
        .unwrap();
        assert_eq!(walked.unwrap_err(), ErrorCode::Unsupported);
    }

    #[tokio::test]
    async fn lists_directory_in_stable_order() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("b.txt"), "").unwrap();
        std::fs::write(temp.path().join("a.txt"), "").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        let listing = fs.read(Fid::ROOT, 0, 1024).await.unwrap();
        assert_eq!(String::from_utf8(listing).unwrap(), "a.txt\nb.txt");

        let repeated = fs.read(Fid::ROOT, 0, 1024).await.unwrap();
        assert_eq!(String::from_utf8(repeated).unwrap(), "a.txt\nb.txt");
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

    #[cfg(unix)]
    #[tokio::test]
    async fn write_intent_open_rejects_host_file_without_write_permission() {
        use std::os::unix::fs::PermissionsExt;

        if running_as_root() {
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("readonly.txt");
        std::fs::write(&path, "readonly").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["readonly.txt".to_string()])
            .await
            .unwrap();
        assert_eq!(
            fs.open(Fid(1), OpenMode::Write).await.unwrap_err(),
            ErrorCode::NoAccess
        );

        fs.walk(Fid::ROOT, Fid(2), &["readonly.txt".to_string()])
            .await
            .unwrap();
        assert_eq!(
            fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap_err(),
            ErrorCode::NoAccess
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_open_allows_host_file_without_read_permission() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("writeonly.txt");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["writeonly.txt".to_string()])
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Write).await.unwrap();
        fs.write(Fid(1), 0, b"new").await.unwrap();
        fs.clunk(Fid(1)).await.unwrap();

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "new");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stat_allows_host_file_without_read_permission() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("writeonly.txt");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["writeonly.txt".to_string()])
            .await
            .unwrap();
        let stat = fs.stat(Fid(1)).await.unwrap();

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(stat.name, "writeonly.txt");
        assert_eq!(stat.qid.kind, FileKind::File);
        assert_eq!(stat.length, 3);
        assert!(stat.writable);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_intent_open_rejects_multiply_linked_host_file() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = temp.path().join("linked.txt");
        let outside_alias = outside.path().join("alias.txt");
        std::fs::write(&path, "linked").unwrap();
        std::fs::hard_link(&path, &outside_alias).unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["linked.txt".to_string()])
            .await
            .unwrap();
        assert_eq!(
            fs.open(Fid(1), OpenMode::Write).await.unwrap_err(),
            ErrorCode::NoAccess
        );

        fs.walk(Fid::ROOT, Fid(2), &["linked.txt".to_string()])
            .await
            .unwrap();
        assert_eq!(
            fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap_err(),
            ErrorCode::NoAccess
        );
        assert_eq!(std::fs::read_to_string(outside_alias).unwrap(), "linked");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn clunk_rejects_multiply_linked_host_file_created_after_open() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = temp.path().join("target.txt");
        let outside_alias = outside.path().join("alias.txt");
        std::fs::write(&path, "safe").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["target.txt".to_string()])
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Write).await.unwrap();
        fs.write(Fid(1), 0, b"changed").await.unwrap();
        std::fs::hard_link(&path, &outside_alias).unwrap();

        assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "safe");
        assert_eq!(std::fs::read_to_string(outside_alias).unwrap(), "safe");
    }

    #[tokio::test]
    async fn clunk_rejects_host_file_replaced_after_open() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("target.txt");
        std::fs::write(&path, "safe").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["target.txt".to_string()])
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Write).await.unwrap();
        fs.write(Fid(1), 0, b"changed").await.unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "external").unwrap();

        assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_clunk_staging_preserves_original_host_file() {
        use std::os::unix::fs::PermissionsExt;

        if running_as_root() {
            return;
        }

        let temp = tempfile::tempdir().unwrap();
        let mounted = temp.path().join("mounted");
        std::fs::create_dir(&mounted).unwrap();
        let path = mounted.join("target.txt");
        std::fs::write(&path, "safe").unwrap();
        let fs = HostDirFs::new(&mounted, HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["target.txt".to_string()])
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Write).await.unwrap();
        fs.write(Fid(1), 0, b"changed").await.unwrap();
        std::fs::set_permissions(&mounted, std::fs::Permissions::from_mode(0o555)).unwrap();

        assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
        std::fs::set_permissions(&mounted, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "safe");
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

    #[cfg(unix)]
    #[tokio::test]
    async fn read_rejects_symlink_replacement_after_open() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("notes.txt"), "safe").unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["notes.txt".to_string()])
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Read).await.unwrap();
        std::fs::remove_file(temp.path().join("notes.txt")).unwrap();
        symlink(
            outside.path().join("secret.txt"),
            temp.path().join("notes.txt"),
        )
        .unwrap();

        assert_eq!(
            fs.read(Fid(1), 0, 1024).await.unwrap_err(),
            ErrorCode::NoAccess
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn clunk_rejects_symlink_replacement_before_commit() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_secret = outside.path().join("secret.txt");
        std::fs::write(temp.path().join("draft.txt"), "safe").unwrap();
        std::fs::write(&outside_secret, "secret").unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["draft.txt".to_string()])
            .await
            .unwrap();
        fs.open(Fid(1), OpenMode::Write).await.unwrap();
        fs.write(Fid(1), 0, b"changed").await.unwrap();
        std::fs::remove_file(temp.path().join("draft.txt")).unwrap();
        symlink(&outside_secret, temp.path().join("draft.txt")).unwrap();

        assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
        assert_eq!(std::fs::read_to_string(outside_secret).unwrap(), "secret");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn create_rejects_symlink_parent_replacement() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("dir")).unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["dir".to_string()])
            .await
            .unwrap();
        std::fs::remove_dir(temp.path().join("dir")).unwrap();
        symlink(outside.path(), temp.path().join("dir")).unwrap();

        assert_eq!(
            fs.create(Fid(1), Fid(2), "escaped.txt", FileKind::File)
                .await
                .unwrap_err(),
            ErrorCode::NoAccess
        );
        assert!(!outside.path().join("escaped.txt").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn remove_unlinks_symlink_entry_without_removing_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target.txt");
        let alias = temp.path().join("alias.txt");
        std::fs::write(&target, "target").unwrap();
        symlink("target.txt", &alias).unwrap();
        let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

        fs.walk(Fid::ROOT, Fid(1), &["alias.txt".to_string()])
            .await
            .unwrap();
        fs.remove(Fid(1)).await.unwrap();

        assert!(!alias.exists());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "target");
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
