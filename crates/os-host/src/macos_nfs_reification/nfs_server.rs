//! Pure-Rust read-only loopback NFSv3 + MOUNT v3 server exporting any `Arc<dyn FileServer>`.
//!
//! Binds strictly to `127.0.0.1:0`, serves both MOUNT v3 (`100005`) and NFS v3 (`100003`)
//! on the same TCP port (no `rpcbind`/`portmap`, no root privileges), enforces read-only
//! semantics by returning `NFS3ERR_ROFS` on every mutating procedure, and performs
//! non-blocking bounded snapshot reads for dynamic `Stream`, status, and search nodes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use alan_ap::{FileKind, FileServer, Stat};
use anyhow::{Context, Result, ensure};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

pub use super::ap_bridge::{
    ACCESS3_DELETE, ACCESS3_EXECUTE, ACCESS3_EXTEND, ACCESS3_LOOKUP, ACCESS3_MODIFY, ACCESS3_READ,
    MNT3_OK, MNT3ERR_ACCES, MNT3ERR_INVAL, MNT3ERR_NOENT, MOUNT3_MAX_PATH_LEN, NF3DIR, NF3REG,
    NFS_FH_LEN, NFS3_MAX_FH_LEN, NFS3_MAX_NAME_LEN, NFS3_OK, NFS3ERR_ACCES, NFS3ERR_BADHANDLE,
    NFS3ERR_INVAL, NFS3ERR_IO, NFS3ERR_ISDIR, NFS3ERR_NAMETOOLONG, NFS3ERR_NOENT, NFS3ERR_NOTDIR,
    NFS3ERR_NOTSUPP, NFS3ERR_ROFS, NFS3ERR_SERVERFAULT, NFS3ERR_STALE, NFS3ERR_TOOSMALL,
    NfsServerConfig, ROOT_FILE_ID,
};
use super::ap_bridge::{ApSnapshotReader, Fattr3, InodeTable, current_uid_gid, parent_path};
use super::rpc::{
    ACCEPT_GARBAGE_ARGS, AUTH_NONE, AUTH_UNIX, MountProc, NfsProc, RpcDispatchTarget, RpcError,
    classify_dispatch, decode_rpc_call, encode_accepted_error, encode_accepted_success,
    encode_dispatch_error, read_rpc_record, write_rpc_record,
};
use super::xdr::{XdrError, XdrReader, XdrWriter, xdr_padded_len};

const FSF3_HOMOGENEOUS: u32 = 0x0008;

static NEXT_BOOT_GEN: AtomicU64 = AtomicU64::new(1);

enum ProcedureFailure {
    GarbageArgs,
    Xdr(XdrError),
}

impl From<XdrError> for ProcedureFailure {
    fn from(value: XdrError) -> Self {
        Self::Xdr(value)
    }
}

/// RPC dispatcher translating MOUNT v3 and read-only NFS v3 calls onto an aP [`FileServer`].
pub struct NfsService {
    ap: ApSnapshotReader,
    inodes: RwLock<InodeTable>,
    boot_gen: u64,
    uid: u32,
    gid: u32,
    fsid: u64,
    boot_secs: u32,
}

impl NfsService {
    /// Create a read-only NFSv3 service exporting `server`.
    pub fn new(server: Arc<dyn FileServer>, config: NfsServerConfig) -> Self {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let seq = NEXT_BOOT_GEN.fetch_add(1, Ordering::Relaxed);
        let boot_gen = (now_secs.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seq).max(1);
        let (uid, gid) = current_uid_gid();
        let max_inodes = config.max_inodes;
        Self {
            ap: ApSnapshotReader::new(server, config),
            inodes: RwLock::new(InodeTable::new(boot_gen, max_inodes)),
            boot_gen,
            uid,
            gid,
            fsid: boot_gen,
            boot_secs: u32::try_from(now_secs).unwrap_or(u32::MAX),
        }
    }

    /// Root directory NFSv3 file handle for this server instance.
    pub fn root_fh(&self) -> [u8; NFS_FH_LEN] {
        self.inodes
            .read()
            .expect("inode table lock poisoned")
            .encode_fh(ROOT_FILE_ID)
    }

    /// Decode and execute one complete ONC RPC call record, returning the encoded reply record.
    pub async fn handle_rpc_record(&self, record: &[u8]) -> Result<Vec<u8>, RpcError> {
        let (header, mut args) = decode_rpc_call(record)?;
        let target = match classify_dispatch(&header) {
            Ok(target) => target,
            Err(err) => return Ok(encode_dispatch_error(header.xid, err)?),
        };

        let result = match target {
            RpcDispatchTarget::Mount(proc_id) => self.handle_mount(proc_id, &mut args).await,
            RpcDispatchTarget::Nfs(proc_id) => self.handle_nfs(proc_id, &mut args).await,
        };

        match result {
            Ok(body) => Ok(encode_accepted_success(header.xid, &body)?),
            Err(ProcedureFailure::GarbageArgs) => Ok(encode_accepted_error(
                header.xid,
                ACCEPT_GARBAGE_ARGS,
                None,
            )?),
            Err(ProcedureFailure::Xdr(err)) => Err(RpcError::Xdr(err)),
        }
    }

    async fn handle_mount(
        &self,
        proc_id: MountProc,
        args: &mut XdrReader<'_>,
    ) -> Result<Vec<u8>, ProcedureFailure> {
        let mut out = XdrWriter::new();
        match proc_id {
            MountProc::Null | MountProc::UmntAll => {}
            MountProc::Dump => out.write_bool(false)?,
            MountProc::Umnt => {
                let _dirpath = args
                    .read_string_bounded(MOUNT3_MAX_PATH_LEN)
                    .map_err(|_| ProcedureFailure::GarbageArgs)?;
            }
            MountProc::Export => {
                out.write_bool(true)?;
                out.write_string("/")?;
                out.write_bool(false)?;
                out.write_bool(false)?;
            }
            MountProc::Mnt => {
                let dirpath = args
                    .read_string_bounded(MOUNT3_MAX_PATH_LEN)
                    .map_err(|_| ProcedureFailure::GarbageArgs)?;
                if is_root_mount_path(&dirpath) {
                    let fh = self.root_fh();
                    out.write_u32(MNT3_OK)?;
                    out.write_opaque(&fh)?;
                    out.write_u32(2)?;
                    out.write_u32(AUTH_UNIX)?;
                    out.write_u32(AUTH_NONE)?;
                } else if dirpath.contains('\0') {
                    out.write_u32(MNT3ERR_INVAL)?;
                } else {
                    out.write_u32(MNT3ERR_NOENT)?;
                }
            }
        }
        Ok(out.into_bytes())
    }

    async fn handle_nfs(
        &self,
        proc_id: NfsProc,
        args: &mut XdrReader<'_>,
    ) -> Result<Vec<u8>, ProcedureFailure> {
        if proc_id.is_mutating() {
            return encode_rofs_reply(proc_id);
        }
        match proc_id {
            NfsProc::Null => Ok(Vec::new()),
            NfsProc::GetAttr => self.nfs_getattr(args).await,
            NfsProc::Lookup => self.nfs_lookup(args).await,
            NfsProc::Access => self.nfs_access(args).await,
            NfsProc::ReadLink => self.nfs_readlink(args).await,
            NfsProc::Read => self.nfs_read(args).await,
            NfsProc::Readdir => self.nfs_readdir(args).await,
            NfsProc::ReaddirPlus => self.nfs_readdirplus(args).await,
            NfsProc::FsStat => self.nfs_fsstat(args).await,
            NfsProc::FsInfo => self.nfs_fsinfo(args).await,
            NfsProc::PathConf => self.nfs_pathconf(args).await,
            _ => encode_rofs_reply(proc_id),
        }
    }

    async fn nfs_getattr(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let fh = read_fh(args)?;
        let mut out = XdrWriter::with_capacity(96);
        let (fileid, path) = match self.resolve_fh(fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                return Ok(out.into_bytes());
            }
        };
        match self.ap.walk_and_stat(&path).await {
            Ok((stat, size)) => {
                out.write_u32(NFS3_OK)?;
                self.make_fattr(fileid, &stat, size).encode(&mut out)?;
            }
            Err(status) => out.write_u32(status)?,
        }
        Ok(out.into_bytes())
    }

    async fn nfs_lookup(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let dir_fh = read_fh(args)?;
        let name = args
            .read_string_bounded(NFS3_MAX_NAME_LEN * 4)
            .map_err(|_| ProcedureFailure::GarbageArgs)?;
        let mut out = XdrWriter::with_capacity(224);

        let (dir_id, dir_path) = match self.resolve_fh(dir_fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let dir_attr = self
            .ap
            .walk_and_stat(&dir_path)
            .await
            .ok()
            .map(|(st, sz)| self.make_fattr(dir_id, &st, sz));

        if let Some(attr) = dir_attr
            && attr.ftype != NF3DIR
        {
            out.write_u32(NFS3ERR_NOTDIR)?;
            write_post_op_attr(&mut out, Some(&attr))?;
            return Ok(out.into_bytes());
        }
        if name.len() > NFS3_MAX_NAME_LEN {
            out.write_u32(NFS3ERR_NAMETOOLONG)?;
            write_post_op_attr(&mut out, dir_attr.as_ref())?;
            return Ok(out.into_bytes());
        }
        if name.is_empty() || name.contains('/') || name.contains('\0') {
            out.write_u32(NFS3ERR_INVAL)?;
            write_post_op_attr(&mut out, dir_attr.as_ref())?;
            return Ok(out.into_bytes());
        }

        let target_path = if name == "." {
            dir_path.clone()
        } else if name == ".." {
            parent_path(&dir_path)
        } else {
            let mut next = dir_path.clone();
            next.push(name);
            next
        };

        let (target_stat, target_size) = match self.ap.walk_and_stat(&target_path).await {
            Ok(res) => res,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, dir_attr.as_ref())?;
                return Ok(out.into_bytes());
            }
        };
        let (target_id, target_fh) = match self.intern_and_encode_fh(target_path) {
            Ok(pair) => pair,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, dir_attr.as_ref())?;
                return Ok(out.into_bytes());
            }
        };
        let target_attr = self.make_fattr(target_id, &target_stat, target_size);

        out.write_u32(NFS3_OK)?;
        out.write_opaque(&target_fh)?;
        write_post_op_attr(&mut out, Some(&target_attr))?;
        write_post_op_attr(&mut out, dir_attr.as_ref())?;
        Ok(out.into_bytes())
    }

    async fn nfs_access(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let fh = read_fh(args)?;
        let requested = args.read_u32().map_err(|_| ProcedureFailure::GarbageArgs)?;
        let mut out = XdrWriter::with_capacity(108);

        let (fileid, path) = match self.resolve_fh(fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let (stat, size) = match self.ap.walk_and_stat(&path).await {
            Ok(res) => res,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let attr = self.make_fattr(fileid, &stat, size);
        let allowed = match stat.qid.kind {
            FileKind::Dir => ACCESS3_READ | ACCESS3_LOOKUP | ACCESS3_EXECUTE,
            FileKind::File if stat.executable => ACCESS3_READ | ACCESS3_EXECUTE,
            FileKind::File | FileKind::Stream => ACCESS3_READ,
            FileKind::Clone => 0,
        };
        out.write_u32(NFS3_OK)?;
        write_post_op_attr(&mut out, Some(&attr))?;
        out.write_u32(requested & allowed)?;
        Ok(out.into_bytes())
    }

    async fn nfs_readlink(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let fh = read_fh(args)?;
        let mut out = XdrWriter::with_capacity(96);
        let (fileid, path) = match self.resolve_fh(fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let attr = self
            .ap
            .walk_and_stat(&path)
            .await
            .ok()
            .map(|(st, sz)| self.make_fattr(fileid, &st, sz));
        out.write_u32(NFS3ERR_INVAL)?;
        write_post_op_attr(&mut out, attr.as_ref())?;
        Ok(out.into_bytes())
    }

    async fn nfs_read(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let fh = read_fh(args)?;
        let offset = args.read_u64().map_err(|_| ProcedureFailure::GarbageArgs)?;
        let count = args.read_u32().map_err(|_| ProcedureFailure::GarbageArgs)?;
        let max_read = self.ap.config().max_read_bytes;
        let mut out = XdrWriter::with_capacity(128 + count.min(max_read) as usize);

        let (fileid, path) = match self.resolve_fh(fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        match self.ap.read_file_slice(&path, offset, count).await {
            Ok((stat, effective_size, data, eof)) => {
                let attr = self.make_fattr(fileid, &stat, effective_size);
                let data_len = u32::try_from(data.len()).unwrap_or(u32::MAX);
                out.write_u32(NFS3_OK)?;
                write_post_op_attr(&mut out, Some(&attr))?;
                out.write_u32(data_len)?;
                out.write_bool(eof)?;
                out.write_opaque(&data)?;
            }
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
            }
        }
        Ok(out.into_bytes())
    }

    async fn nfs_readdir(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let dir_fh = read_fh(args)?;
        let cookie = args.read_u64().map_err(|_| ProcedureFailure::GarbageArgs)?;
        let _cookieverf = args
            .read_fixed_opaque(8)
            .map_err(|_| ProcedureFailure::GarbageArgs)?;
        let count = args.read_u32().map_err(|_| ProcedureFailure::GarbageArgs)?;

        let mut out = XdrWriter::with_capacity(count.min(65_536) as usize);
        let (dir_id, dir_path) = match self.resolve_fh(dir_fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let (dir_stat, children) = match self.ap.read_dir_names(&dir_path).await {
            Ok(res) => res,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let dir_attr = self.make_fattr(dir_id, &dir_stat, dir_stat.length.max(512));
        let entries = self.build_dir_entry_list(dir_id, &dir_path, &children);
        let start_idx = usize::try_from(cookie).unwrap_or(entries.len());
        let remaining = entries.get(start_idx..).unwrap_or(&[]);

        let mut used_bytes: usize = 108;
        let budget = count as usize;
        let mut selected = Vec::new();
        for (offset, (entry_id, name)) in remaining.iter().enumerate() {
            let entry_cookie = (start_idx + offset + 1) as u64;
            let entry_size = 24 + xdr_padded_len(name.len()).unwrap_or(usize::MAX);
            if used_bytes.saturating_add(entry_size) > budget {
                if selected.is_empty() {
                    out.write_u32(NFS3ERR_TOOSMALL)?;
                    write_post_op_attr(&mut out, Some(&dir_attr))?;
                    return Ok(out.into_bytes());
                }
                break;
            }
            used_bytes += entry_size;
            selected.push((*entry_id, name.as_str(), entry_cookie));
        }

        let eof = start_idx + selected.len() >= entries.len();
        out.write_u32(NFS3_OK)?;
        write_post_op_attr(&mut out, Some(&dir_attr))?;
        out.write_fixed_opaque(&self.boot_gen.to_be_bytes())?;
        for (entry_id, name, entry_cookie) in selected {
            out.write_bool(true)?;
            out.write_u64(entry_id)?;
            out.write_string(name)?;
            out.write_u64(entry_cookie)?;
        }
        out.write_bool(false)?;
        out.write_bool(eof)?;
        Ok(out.into_bytes())
    }

    async fn nfs_readdirplus(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let dir_fh = read_fh(args)?;
        let cookie = args.read_u64().map_err(|_| ProcedureFailure::GarbageArgs)?;
        let _cookieverf = args
            .read_fixed_opaque(8)
            .map_err(|_| ProcedureFailure::GarbageArgs)?;
        let dircount = args.read_u32().map_err(|_| ProcedureFailure::GarbageArgs)?;
        let maxcount = args.read_u32().map_err(|_| ProcedureFailure::GarbageArgs)?;

        let mut out = XdrWriter::with_capacity(maxcount.min(65_536) as usize);
        let (dir_id, dir_path) = match self.resolve_fh(dir_fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let (dir_stat, children) = match self.ap.read_dir_names(&dir_path).await {
            Ok(res) => res,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let dir_attr = self.make_fattr(dir_id, &dir_stat, dir_stat.length.max(512));
        let entries = self.build_dir_entry_list(dir_id, &dir_path, &children);
        let start_idx = usize::try_from(cookie).unwrap_or(entries.len());
        let remaining = entries.get(start_idx..).unwrap_or(&[]);

        let mut used_max: usize = 108;
        let mut used_dir: usize = 0;
        let max_budget = maxcount as usize;
        let dir_budget = dircount as usize;
        let mut selected = Vec::new();
        let mut consumed = 0usize;

        for (offset, (entry_id, name)) in remaining.iter().enumerate() {
            let entry_cookie = (start_idx + offset + 1) as u64;
            let padded_name = xdr_padded_len(name.len()).unwrap_or(usize::MAX);
            let entry_max = 136 + padded_name;
            let entry_dir = 20 + padded_name;
            if used_max.saturating_add(entry_max) > max_budget
                || (dir_budget > 0 && used_dir.saturating_add(entry_dir) > dir_budget)
            {
                if selected.is_empty() && consumed == 0 {
                    out.write_u32(NFS3ERR_TOOSMALL)?;
                    write_post_op_attr(&mut out, Some(&dir_attr))?;
                    return Ok(out.into_bytes());
                }
                break;
            }

            let entry_path = if name == "." {
                dir_path.clone()
            } else if name == ".." {
                parent_path(&dir_path)
            } else {
                let mut child = dir_path.clone();
                child.push(name.clone());
                child
            };

            let attr = if name == "." {
                Some(dir_attr)
            } else {
                match self.ap.walk_and_stat(&entry_path).await {
                    Ok((st, sz)) => Some(self.make_fattr(*entry_id, &st, sz)),
                    Err(NFS3ERR_NOENT) => {
                        consumed += 1;
                        continue;
                    }
                    Err(_) => None,
                }
            };
            let entry_fh = self
                .inodes
                .read()
                .expect("inode table lock poisoned")
                .encode_fh(*entry_id);

            used_max += entry_max;
            used_dir += entry_dir;
            consumed += 1;
            selected.push((*entry_id, name.as_str(), entry_cookie, attr, entry_fh));
        }

        let eof = start_idx + consumed >= entries.len();
        out.write_u32(NFS3_OK)?;
        write_post_op_attr(&mut out, Some(&dir_attr))?;
        out.write_fixed_opaque(&self.boot_gen.to_be_bytes())?;
        for (entry_id, name, entry_cookie, attr, entry_fh) in selected {
            out.write_bool(true)?;
            out.write_u64(entry_id)?;
            out.write_string(name)?;
            out.write_u64(entry_cookie)?;
            write_post_op_attr(&mut out, attr.as_ref())?;
            out.write_bool(true)?;
            out.write_opaque(&entry_fh)?;
        }
        out.write_bool(false)?;
        out.write_bool(eof)?;
        Ok(out.into_bytes())
    }

    async fn nfs_fsstat(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let fh = read_fh(args)?;
        let mut out = XdrWriter::with_capacity(160);
        let (fileid, path) = match self.resolve_fh(fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let attr = self
            .ap
            .walk_and_stat(&path)
            .await
            .ok()
            .map(|(st, sz)| self.make_fattr(fileid, &st, sz));
        out.write_u32(NFS3_OK)?;
        write_post_op_attr(&mut out, attr.as_ref())?;
        out.write_u64(1 << 30)?; // tbytes
        out.write_u64(0)?; // fbytes
        out.write_u64(0)?; // abytes
        out.write_u64(self.ap.config().max_inodes as u64)?; // tfiles
        out.write_u64(0)?; // ffiles
        out.write_u64(0)?; // afiles
        out.write_u32(0)?; // invarsec
        Ok(out.into_bytes())
    }

    async fn nfs_fsinfo(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let fh = read_fh(args)?;
        let mut out = XdrWriter::with_capacity(160);
        let (fileid, path) = match self.resolve_fh(fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let attr = self
            .ap
            .walk_and_stat(&path)
            .await
            .ok()
            .map(|(st, sz)| self.make_fattr(fileid, &st, sz));
        let max_read = self.ap.config().max_read_bytes;
        out.write_u32(NFS3_OK)?;
        write_post_op_attr(&mut out, attr.as_ref())?;
        out.write_u32(max_read)?; // rtmax
        out.write_u32(max_read)?; // rtpref
        out.write_u32(4096)?; // rtmult
        out.write_u32(max_read)?; // wtmax
        out.write_u32(max_read)?; // wtpref
        out.write_u32(4096)?; // wtmult
        out.write_u32(8192)?; // dtpref
        out.write_u64(1u64 << 40)?; // maxfilesize
        out.write_u32(0)?; // time_delta.seconds
        out.write_u32(1)?; // time_delta.nseconds
        out.write_u32(FSF3_HOMOGENEOUS)?; // properties
        Ok(out.into_bytes())
    }

    async fn nfs_pathconf(&self, args: &mut XdrReader<'_>) -> Result<Vec<u8>, ProcedureFailure> {
        let fh = read_fh(args)?;
        let mut out = XdrWriter::with_capacity(128);
        let (fileid, path) = match self.resolve_fh(fh) {
            Ok(resolved) => resolved,
            Err(status) => {
                out.write_u32(status)?;
                write_post_op_attr(&mut out, None)?;
                return Ok(out.into_bytes());
            }
        };
        let attr = self
            .ap
            .walk_and_stat(&path)
            .await
            .ok()
            .map(|(st, sz)| self.make_fattr(fileid, &st, sz));
        out.write_u32(NFS3_OK)?;
        write_post_op_attr(&mut out, attr.as_ref())?;
        out.write_u32(1)?; // linkmax
        out.write_u32(NFS3_MAX_NAME_LEN as u32)?; // name_max
        out.write_bool(true)?; // no_trunc
        out.write_bool(true)?; // chown_restricted
        out.write_bool(false)?; // case_insensitive
        out.write_bool(true)?; // case_preserving
        Ok(out.into_bytes())
    }

    fn resolve_fh(&self, raw_fh: &[u8]) -> Result<(u64, Vec<String>), u32> {
        let table = self.inodes.read().expect("inode table lock poisoned");
        let fileid = table.decode_fh(raw_fh)?;
        let path = table.path_of(fileid).ok_or(NFS3ERR_STALE)?;
        Ok((fileid, path))
    }

    fn intern_and_encode_fh(&self, path: Vec<String>) -> Result<(u64, [u8; NFS_FH_LEN]), u32> {
        let mut table = self.inodes.write().expect("inode table lock poisoned");
        let id = table.intern(path)?;
        let fh = table.encode_fh(id);
        Ok((id, fh))
    }

    fn build_dir_entry_list(
        &self,
        dir_id: u64,
        dir_path: &[String],
        children: &[String],
    ) -> Vec<(u64, String)> {
        let mut table = self.inodes.write().expect("inode table lock poisoned");
        let parent_id = if dir_path.is_empty() {
            ROOT_FILE_ID
        } else {
            table.intern(parent_path(dir_path)).unwrap_or(ROOT_FILE_ID)
        };
        let mut entries = Vec::with_capacity(children.len() + 2);
        entries.push((dir_id, ".".to_string()));
        entries.push((parent_id, "..".to_string()));
        for name in children {
            let mut child_path = dir_path.to_vec();
            child_path.push(name.clone());
            if let Ok(child_id) = table.intern(child_path) {
                entries.push((child_id, name.clone()));
            }
        }
        entries
    }

    fn make_fattr(&self, fileid: u64, stat: &Stat, size: u64) -> Fattr3 {
        let (ftype, mode, nlink) = match stat.qid.kind {
            FileKind::Dir => (NF3DIR, 0o555, 2),
            FileKind::File if stat.executable => (NF3REG, 0o555, 1),
            FileKind::File | FileKind::Stream => (NF3REG, 0o444, 1),
            FileKind::Clone => (NF3REG, 0o000, 1),
        };
        let version = stat.qid.version;
        let sec = self.boot_secs.saturating_add(version / 1_000_000_000);
        let nsec = version % 1_000_000_000;
        Fattr3 {
            ftype,
            mode,
            nlink,
            uid: self.uid,
            gid: self.gid,
            size,
            used: size,
            fsid: self.fsid,
            fileid,
            sec,
            nsec,
        }
    }
}

/// Running read-only loopback NFSv3 + MOUNT v3 TCP server bound strictly to `127.0.0.1:0`.
pub struct NfsLoopbackServer {
    local_addr: SocketAddr,
    service: Arc<NfsService>,
    shutdown_tx: watch::Sender<bool>,
    accept_task: Option<JoinHandle<()>>,
}

impl NfsLoopbackServer {
    /// Start a read-only loopback NFSv3 server exporting `server` with default configuration.
    pub async fn start(server: Arc<dyn FileServer>) -> Result<Self> {
        Self::start_with_config(server, NfsServerConfig::default()).await
    }

    /// Start a read-only loopback NFSv3 server exporting `server` with explicit configuration.
    pub async fn start_with_config(
        server: Arc<dyn FileServer>,
        config: NfsServerConfig,
    ) -> Result<Self> {
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let listener = TcpListener::bind(bind_addr)
            .await
            .context("bind loopback NFSv3 listener on 127.0.0.1:0")?;
        let local_addr = listener
            .local_addr()
            .context("read loopback NFSv3 local_addr")?;
        ensure!(
            local_addr.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST),
            "NFSv3 listener bound to non-loopback address {local_addr}"
        );

        let service = Arc::new(NfsService::new(server, config));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let accept_task =
            tokio::spawn(run_accept_loop(listener, Arc::clone(&service), shutdown_rx));

        Ok(Self {
            local_addr,
            service,
            shutdown_tx,
            accept_task: Some(accept_task),
        })
    }

    /// Bound loopback socket address (`127.0.0.1:<port>`).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Ephemeral TCP port serving both MOUNT v3 and NFS v3.
    pub fn port(&self) -> u16 {
        self.local_addr.port()
    }

    /// Access the underlying stateless RPC service.
    pub fn service(&self) -> Arc<NfsService> {
        Arc::clone(&self.service)
    }

    /// Signal background server tasks to stop without awaiting completion.
    pub fn signal_shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Gracefully stop the accept loop and abort active connections.
    pub async fn shutdown(mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(task) = self.accept_task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for NfsLoopbackServer {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(task) = self.accept_task.take() {
            task.abort();
        }
    }
}

async fn run_accept_loop(
    listener: TcpListener,
    service: Arc<NfsService>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(pair) => pair,
                    Err(error) => {
                        tracing::debug!(%error, "loopback NFSv3 accept failed");
                        continue;
                    }
                };
                if !peer.ip().is_loopback() {
                    tracing::warn!(%peer, "rejected non-loopback peer on NFSv3 server");
                    continue;
                }
                let _ = stream.set_nodelay(true);
                let svc = Arc::clone(&service);
                let conn_shutdown = shutdown_rx.clone();
                connections.spawn(serve_connection(stream, svc, conn_shutdown));
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
}

async fn serve_connection(
    stream: TcpStream,
    service: Arc<NfsService>,
    shutdown_rx: watch::Receiver<bool>,
) {
    let (reader, writer) = stream.into_split();
    serve_rpc_stream(reader, writer, service, shutdown_rx).await;
}

/// Serve pipelined ONC RPC v2 records over any bidirectional byte stream until
/// clean EOF (draining in-flight replies on half-close) or shutdown.
pub(crate) async fn serve_rpc_stream<R, W>(
    mut reader: R,
    mut writer: W,
    service: Arc<NfsService>,
    mut shutdown_rx: watch::Receiver<bool>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let max_in_flight = service.ap.config().max_concurrent_rpcs_per_conn.max(1);
    let semaphore = Arc::new(Semaphore::new(max_in_flight));
    let (reply_tx, mut reply_rx) = mpsc::channel::<Vec<u8>>(max_in_flight * 2);

    let writer_task = tokio::spawn(async move {
        while let Some(reply) = reply_rx.recv().await {
            if write_rpc_record(&mut writer, &reply).await.is_err() {
                break;
            }
        }
        let _ = tokio::io::AsyncWriteExt::shutdown(&mut writer).await;
    });

    let mut handlers = JoinSet::new();
    let mut abort_in_flight = false;
    loop {
        let record = tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    abort_in_flight = true;
                    break;
                }
                continue;
            }
            res = read_rpc_record(&mut reader) => match res {
                Ok(Some(rec)) => rec,
                Ok(None) => break,
                Err(error) => {
                    tracing::debug!(%error, "loopback NFSv3 RPC framing closed");
                    abort_in_flight = true;
                    break;
                }
            }
        };

        let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
            break;
        };
        let svc = Arc::clone(&service);
        let tx = reply_tx.clone();
        handlers.spawn(async move {
            let _permit = permit;
            match svc.handle_rpc_record(&record).await {
                Ok(reply) => {
                    let _ = tx.send(reply).await;
                }
                Err(error) => {
                    tracing::debug!(%error, "loopback NFSv3 RPC dispatch failed");
                }
            }
        });
    }

    if abort_in_flight {
        handlers.abort_all();
    }
    while handlers.join_next().await.is_some() {}
    drop(reply_tx);
    let _ = writer_task.await;
}

fn encode_rofs_reply(proc_id: NfsProc) -> Result<Vec<u8>, ProcedureFailure> {
    let mut out = XdrWriter::with_capacity(24);
    out.write_u32(NFS3ERR_ROFS)?;
    match proc_id {
        NfsProc::Rename => {
            write_empty_wcc(&mut out)?;
            write_empty_wcc(&mut out)?;
        }
        NfsProc::Link => {
            write_post_op_attr(&mut out, None)?;
            write_empty_wcc(&mut out)?;
        }
        _ => write_empty_wcc(&mut out)?,
    }
    Ok(out.into_bytes())
}

fn read_fh<'a>(args: &mut XdrReader<'a>) -> Result<&'a [u8], ProcedureFailure> {
    args.read_opaque_bounded(NFS3_MAX_FH_LEN)
        .map_err(|_| ProcedureFailure::GarbageArgs)
}

fn write_post_op_attr(writer: &mut XdrWriter, attr: Option<&Fattr3>) -> Result<(), XdrError> {
    match attr {
        Some(attr) => {
            writer.write_bool(true)?;
            attr.encode(writer)
        }
        None => writer.write_bool(false),
    }
}

fn write_empty_wcc(writer: &mut XdrWriter) -> Result<(), XdrError> {
    writer.write_bool(false)?;
    writer.write_bool(false)?;
    Ok(())
}

fn is_root_mount_path(dirpath: &str) -> bool {
    !dirpath.is_empty() && dirpath.bytes().all(|b| b == b'/')
}
