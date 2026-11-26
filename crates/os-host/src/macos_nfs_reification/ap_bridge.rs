//! aP-to-NFSv3 adapter helpers: NFSv3/MOUNT v3 constants, inode table, `fattr3` encoding,
//! and non-blocking bounded snapshot reads for dynamic `Stream`, status, and search nodes.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, OpenMode, Stat};

use super::xdr::{XdrError, XdrWriter};

/// Root directory inode number (never zero).
pub const ROOT_FILE_ID: u64 = 1;
/// Fixed length of encoded NFSv3 file handles (`boot_gen: u64` + `fileid: u64`).
pub const NFS_FH_LEN: usize = 16;
/// RFC 1813 maximum NFSv3 file handle length.
pub const NFS3_MAX_FH_LEN: usize = 64;
/// RFC 1813 maximum filename component length.
pub const NFS3_MAX_NAME_LEN: usize = 255;
/// RFC 1813 maximum MOUNT path length.
pub const MOUNT3_MAX_PATH_LEN: usize = 1024;

// MOUNT v3 status codes (RFC 1813 Appendix I).
pub const MNT3_OK: u32 = 0;
pub const MNT3ERR_NOENT: u32 = 2;
pub const MNT3ERR_ACCES: u32 = 13;
pub const MNT3ERR_INVAL: u32 = 22;

// NFS v3 status codes (RFC 1813 Section 2.6).
pub const NFS3_OK: u32 = 0;
pub const NFS3ERR_NOENT: u32 = 2;
pub const NFS3ERR_IO: u32 = 5;
pub const NFS3ERR_ACCES: u32 = 13;
pub const NFS3ERR_NOTDIR: u32 = 20;
pub const NFS3ERR_ISDIR: u32 = 21;
pub const NFS3ERR_INVAL: u32 = 22;
pub const NFS3ERR_ROFS: u32 = 30;
pub const NFS3ERR_NAMETOOLONG: u32 = 63;
pub const NFS3ERR_STALE: u32 = 70;
pub const NFS3ERR_BADHANDLE: u32 = 10_001;
pub const NFS3ERR_NOTSUPP: u32 = 10_004;
pub const NFS3ERR_TOOSMALL: u32 = 10_005;
pub const NFS3ERR_SERVERFAULT: u32 = 10_006;

// NFS v3 file types (`ftype3`).
pub const NF3REG: u32 = 1;
pub const NF3DIR: u32 = 2;

// NFS v3 ACCESS permission bits.
pub const ACCESS3_READ: u32 = 0x0001;
pub const ACCESS3_LOOKUP: u32 = 0x0002;
pub const ACCESS3_MODIFY: u32 = 0x0004;
pub const ACCESS3_EXTEND: u32 = 0x0008;
pub const ACCESS3_DELETE: u32 = 0x0010;
pub const ACCESS3_EXECUTE: u32 = 0x0020;

/// High fid range to avoid colliding with `alan_shell`'s low process-global fid sequence.
static NEXT_NFS_FID: AtomicU64 = AtomicU64::new(0x4000_0000_0000_0000);

/// Configuration for [`super::nfs_server::NfsLoopbackServer`] and [`super::nfs_server::NfsService`].
#[derive(Debug, Clone)]
pub struct NfsServerConfig {
    /// Maximum wait for a dynamic `read` chunk before returning a bounded snapshot.
    pub snapshot_read_timeout: Duration,
    /// Maximum wait for metadata operations (`walk`, `stat`, `open`).
    pub metadata_op_timeout: Duration,
    /// Maximum bytes snapshotted from a dynamic `Stream` or zero-length synthetic file.
    pub max_snapshot_bytes: usize,
    /// Maximum byte count returned in a single NFS `READ` reply.
    pub max_read_bytes: u32,
    /// Maximum directory listing byte payload read from an aP directory.
    pub max_dir_listing_bytes: usize,
    /// Maximum path-to-inode entries retained in the inode table.
    pub max_inodes: usize,
    /// Maximum concurrent in-flight RPC calls per TCP connection.
    pub max_concurrent_rpcs_per_conn: usize,
}

impl Default for NfsServerConfig {
    fn default() -> Self {
        Self {
            snapshot_read_timeout: Duration::from_millis(250),
            metadata_op_timeout: Duration::from_secs(2),
            max_snapshot_bytes: 1024 * 1024,
            max_read_bytes: 65_536,
            max_dir_listing_bytes: 512 * 1024,
            max_inodes: 131_072,
            max_concurrent_rpcs_per_conn: 64,
        }
    }
}

/// Path-to-inode table backing compact 16-byte NFSv3 file handles.
pub(crate) struct InodeTable {
    boot_gen: u64,
    next_id: u64,
    max_inodes: usize,
    id_to_path: HashMap<u64, Vec<String>>,
    path_to_id: HashMap<Vec<String>, u64>,
}

impl InodeTable {
    pub(crate) fn new(boot_gen: u64, max_inodes: usize) -> Self {
        let mut id_to_path = HashMap::new();
        let mut path_to_id = HashMap::new();
        id_to_path.insert(ROOT_FILE_ID, Vec::new());
        path_to_id.insert(Vec::new(), ROOT_FILE_ID);
        Self {
            boot_gen,
            next_id: ROOT_FILE_ID + 1,
            max_inodes: max_inodes.max(2),
            id_to_path,
            path_to_id,
        }
    }

    pub(crate) fn encode_fh(&self, fileid: u64) -> [u8; NFS_FH_LEN] {
        let mut fh = [0u8; NFS_FH_LEN];
        fh[..8].copy_from_slice(&self.boot_gen.to_be_bytes());
        fh[8..].copy_from_slice(&fileid.to_be_bytes());
        fh
    }

    pub(crate) fn decode_fh(&self, raw: &[u8]) -> Result<u64, u32> {
        if raw.len() != NFS_FH_LEN {
            return Err(NFS3ERR_BADHANDLE);
        }
        let fh_gen = u64::from_be_bytes([
            raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
        ]);
        let fileid = u64::from_be_bytes([
            raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
        ]);
        if fh_gen != self.boot_gen || !self.id_to_path.contains_key(&fileid) {
            return Err(NFS3ERR_STALE);
        }
        Ok(fileid)
    }

    pub(crate) fn path_of(&self, fileid: u64) -> Option<Vec<String>> {
        self.id_to_path.get(&fileid).cloned()
    }

    pub(crate) fn intern(&mut self, path: Vec<String>) -> Result<u64, u32> {
        if let Some(&existing) = self.path_to_id.get(&path) {
            return Ok(existing);
        }
        if self.id_to_path.len() >= self.max_inodes {
            return Err(NFS3ERR_SERVERFAULT);
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(NFS3ERR_SERVERFAULT)?;
        self.path_to_id.insert(path.clone(), id);
        self.id_to_path.insert(id, path);
        Ok(id)
    }
}

/// Encodable RFC 1813 `fattr3` structure (84 bytes on the wire).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fattr3 {
    pub(crate) ftype: u32,
    pub(crate) mode: u32,
    pub(crate) nlink: u32,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) size: u64,
    pub(crate) used: u64,
    pub(crate) fsid: u64,
    pub(crate) fileid: u64,
    pub(crate) sec: u32,
    pub(crate) nsec: u32,
}

impl Fattr3 {
    pub(crate) fn encode(&self, writer: &mut XdrWriter) -> Result<(), XdrError> {
        writer.write_u32(self.ftype)?;
        writer.write_u32(self.mode)?;
        writer.write_u32(self.nlink)?;
        writer.write_u32(self.uid)?;
        writer.write_u32(self.gid)?;
        writer.write_u64(self.size)?;
        writer.write_u64(self.used)?;
        writer.write_u32(0)?; // rdev.specdata1
        writer.write_u32(0)?; // rdev.specdata2
        writer.write_u64(self.fsid)?;
        writer.write_u64(self.fileid)?;
        for _ in 0..3 {
            writer.write_u32(self.sec)?;
            writer.write_u32(self.nsec)?;
        }
        Ok(())
    }
}

/// Non-blocking bounded snapshot adapter over an aP [`FileServer`].
pub(crate) struct ApSnapshotReader {
    server: Arc<dyn FileServer>,
    config: NfsServerConfig,
}

impl ApSnapshotReader {
    pub(crate) fn new(server: Arc<dyn FileServer>, config: NfsServerConfig) -> Self {
        Self { server, config }
    }

    pub(crate) fn config(&self) -> &NfsServerConfig {
        &self.config
    }

    fn alloc_fid() -> Fid {
        Fid(NEXT_NFS_FID.fetch_add(1, Ordering::Relaxed))
    }

    async fn clunk_quietly(&self, fid: Fid) {
        let _ = tokio::time::timeout(self.config.metadata_op_timeout, self.server.clunk(fid)).await;
    }

    /// Walk `path` from `Fid::ROOT`, stat it, and resolve an accurate read-only snapshot length.
    pub(crate) async fn walk_and_stat(&self, path: &[String]) -> Result<(Stat, u64), u32> {
        let fid = Self::alloc_fid();
        match tokio::time::timeout(
            self.config.metadata_op_timeout,
            self.server.walk(Fid::ROOT, fid, path),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(map_ap_error(err)),
            Err(_) => return Err(NFS3ERR_IO),
        }

        let stat = match tokio::time::timeout(
            self.config.metadata_op_timeout,
            self.server.stat(fid),
        )
        .await
        {
            Ok(Ok(stat)) => stat,
            Ok(Err(err)) => {
                self.clunk_quietly(fid).await;
                return Err(map_ap_error(err));
            }
            Err(_) => {
                self.clunk_quietly(fid).await;
                return Err(NFS3ERR_IO);
            }
        };

        let effective_size = match stat.qid.kind {
            FileKind::Dir => stat.length.max(512),
            FileKind::Stream => stat.length.min(self.config.max_snapshot_bytes as u64),
            FileKind::Clone => 0,
            FileKind::File if stat.length > 0 => stat.length,
            FileKind::File => self.probe_dynamic_file_length(fid).await,
        };
        self.clunk_quietly(fid).await;
        Ok((stat, effective_size))
    }

    async fn probe_dynamic_file_length(&self, fid: Fid) -> u64 {
        let opened = tokio::time::timeout(
            self.config.snapshot_read_timeout,
            self.server.open(fid, OpenMode::Read),
        )
        .await;
        if !matches!(opened, Ok(Ok(_))) {
            return 0;
        }
        let mut total = 0usize;
        let cap = self.config.max_snapshot_bytes;
        while total < cap {
            let want = (cap - total).min(16_384) as u32;
            match tokio::time::timeout(
                self.config.snapshot_read_timeout,
                self.server.read(fid, total as u64, want),
            )
            .await
            {
                Ok(Ok(chunk)) if !chunk.is_empty() => {
                    total = total.saturating_add(chunk.len()).min(cap);
                }
                _ => break,
            }
        }
        total as u64
    }

    /// Read and sort a directory's child names with bounded total size and timeout.
    pub(crate) async fn read_dir_names(&self, path: &[String]) -> Result<(Stat, Vec<String>), u32> {
        let fid = Self::alloc_fid();
        match tokio::time::timeout(
            self.config.metadata_op_timeout,
            self.server.walk(Fid::ROOT, fid, path),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(map_ap_error(err)),
            Err(_) => return Err(NFS3ERR_IO),
        }

        let result = async {
            let stat = tokio::time::timeout(self.config.metadata_op_timeout, self.server.stat(fid))
                .await
                .map_err(|_| NFS3ERR_IO)?
                .map_err(map_ap_error)?;
            if stat.qid.kind != FileKind::Dir {
                return Err(NFS3ERR_NOTDIR);
            }
            tokio::time::timeout(
                self.config.metadata_op_timeout,
                self.server.open(fid, OpenMode::Read),
            )
            .await
            .map_err(|_| NFS3ERR_IO)?
            .map_err(map_ap_error)?;

            let mut raw = Vec::new();
            let limit = self.config.max_dir_listing_bytes;
            while raw.len() < limit {
                let want = (limit - raw.len()).min(16_384) as u32;
                let chunk = tokio::time::timeout(
                    self.config.snapshot_read_timeout,
                    self.server.read(fid, raw.len() as u64, want),
                )
                .await
                .map_err(|_| NFS3ERR_IO)?
                .map_err(map_ap_error)?;
                if chunk.is_empty() {
                    break;
                }
                raw.extend_from_slice(&chunk);
            }
            let text = String::from_utf8(raw).map_err(|_| NFS3ERR_IO)?;
            let mut dedup = BTreeSet::new();
            for line in text.lines() {
                if !line.is_empty()
                    && line != "."
                    && line != ".."
                    && !line.contains('/')
                    && !line.contains('\0')
                {
                    dedup.insert(line.to_string());
                }
            }
            Ok((stat, dedup.into_iter().collect()))
        }
        .await;

        self.clunk_quietly(fid).await;
        result
    }

    /// Perform a non-blocking bounded slice read for a regular file or `FileKind::Stream`.
    pub(crate) async fn read_file_slice(
        &self,
        path: &[String],
        offset: u64,
        count: u32,
    ) -> Result<(Stat, u64, Vec<u8>, bool), u32> {
        let fid = Self::alloc_fid();
        match tokio::time::timeout(
            self.config.metadata_op_timeout,
            self.server.walk(Fid::ROOT, fid, path),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(map_ap_error(err)),
            Err(_) => return Err(NFS3ERR_IO),
        }

        let result = async {
            let stat = tokio::time::timeout(self.config.metadata_op_timeout, self.server.stat(fid))
                .await
                .map_err(|_| NFS3ERR_IO)?
                .map_err(map_ap_error)?;

            match stat.qid.kind {
                FileKind::Dir => Err(NFS3ERR_ISDIR),
                FileKind::Clone => Err(NFS3ERR_ACCES),
                FileKind::Stream => self.read_stream_snapshot(fid, stat, offset, count).await,
                FileKind::File => self.read_regular_snapshot(fid, stat, offset, count).await,
            }
        }
        .await;

        self.clunk_quietly(fid).await;
        result
    }

    async fn read_stream_snapshot(
        &self,
        fid: Fid,
        stat: Stat,
        offset: u64,
        count: u32,
    ) -> Result<(Stat, u64, Vec<u8>, bool), u32> {
        let retained = stat.length.min(self.config.max_snapshot_bytes as u64);
        if offset >= retained {
            return Ok((stat, retained, Vec::new(), true));
        }
        tokio::time::timeout(
            self.config.metadata_op_timeout,
            self.server.open(fid, OpenMode::Read),
        )
        .await
        .map_err(|_| NFS3ERR_IO)?
        .map_err(map_ap_error)?;

        let want = (retained - offset)
            .min(u64::from(count))
            .min(u64::from(self.config.max_read_bytes)) as usize;
        let mut buf = Vec::with_capacity(want);
        while buf.len() < want {
            let req = (want - buf.len()).min(16_384) as u32;
            match tokio::time::timeout(
                self.config.snapshot_read_timeout,
                self.server.read(fid, offset + buf.len() as u64, req),
            )
            .await
            {
                Ok(Ok(chunk)) if !chunk.is_empty() => buf.extend_from_slice(&chunk),
                Ok(Ok(_)) | Err(_) => break,
                Ok(Err(err)) => return Err(map_ap_error(err)),
            }
        }
        let eof = offset.saturating_add(buf.len() as u64) >= retained;
        Ok((stat, retained, buf, eof))
    }

    async fn read_regular_snapshot(
        &self,
        fid: Fid,
        stat: Stat,
        offset: u64,
        count: u32,
    ) -> Result<(Stat, u64, Vec<u8>, bool), u32> {
        tokio::time::timeout(
            self.config.metadata_op_timeout,
            self.server.open(fid, OpenMode::Read),
        )
        .await
        .map_err(|_| NFS3ERR_IO)?
        .map_err(map_ap_error)?;

        let cap = self.config.max_snapshot_bytes as u64;
        if stat.length == 0 && offset >= cap {
            return Ok((stat, cap, Vec::new(), true));
        }
        let want = count.min(self.config.max_read_bytes) as usize;
        let mut buf = Vec::with_capacity(want);
        let mut hit_eof = false;
        while buf.len() < want {
            let req = (want - buf.len()).min(16_384) as u32;
            match tokio::time::timeout(
                self.config.snapshot_read_timeout,
                self.server.read(fid, offset + buf.len() as u64, req),
            )
            .await
            {
                Ok(Ok(chunk)) if !chunk.is_empty() => buf.extend_from_slice(&chunk),
                Ok(Ok(_)) | Err(_) => {
                    hit_eof = true;
                    break;
                }
                Ok(Err(err)) => return Err(map_ap_error(err)),
            }
        }
        let end = offset.saturating_add(buf.len() as u64);
        let eof =
            hit_eof || (stat.length > 0 && end >= stat.length) || (stat.length == 0 && end >= cap);
        let effective_size = stat.length.max(end);
        Ok((stat, effective_size, buf, eof))
    }
}

pub(crate) fn parent_path(path: &[String]) -> Vec<String> {
    if path.is_empty() {
        Vec::new()
    } else {
        path[..path.len() - 1].to_vec()
    }
}

pub(crate) fn map_ap_error(err: ErrorCode) -> u32 {
    match err {
        ErrorCode::NotFound => NFS3ERR_NOENT,
        ErrorCode::NoAccess => NFS3ERR_ACCES,
        ErrorCode::NotDirectory => NFS3ERR_NOTDIR,
        ErrorCode::IsDirectory => NFS3ERR_ISDIR,
        ErrorCode::BadRequest => NFS3ERR_INVAL,
        ErrorCode::Unsupported => NFS3ERR_NOTSUPP,
        ErrorCode::RateLimited | ErrorCode::Io => NFS3ERR_IO,
    }
}

#[cfg(unix)]
pub(crate) fn current_uid_gid() -> (u32, u32) {
    // SAFETY: `geteuid` and `getegid` are thread-safe POSIX queries with no pointer arguments.
    unsafe { (libc::geteuid(), libc::getegid()) }
}

#[cfg(not(unix))]
pub(crate) fn current_uid_gid() -> (u32, u32) {
    (0, 0)
}
