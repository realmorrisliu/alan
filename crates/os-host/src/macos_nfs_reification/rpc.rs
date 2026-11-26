//! ONC RPC v2 (RFC 5531) record-marking framing, auth parsing, and MOUNT/NFS v3 dispatch.

use std::fmt;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::xdr::{MAX_XDR_RECORD_BYTES, XdrError, XdrReader, XdrWriter};

/// ONC RPC protocol version supported by this server.
pub const RPC_VERSION: u32 = 2;

/// RPC message direction constants.
pub const MSG_CALL: u32 = 0;
pub const MSG_REPLY: u32 = 1;

/// RPC reply disposition constants.
pub const MSG_ACCEPTED: u32 = 0;
pub const MSG_DENIED: u32 = 1;

/// RPC accepted status codes.
pub const ACCEPT_SUCCESS: u32 = 0;
pub const ACCEPT_PROG_UNAVAIL: u32 = 1;
pub const ACCEPT_PROG_MISMATCH: u32 = 2;
pub const ACCEPT_PROC_UNAVAIL: u32 = 3;
pub const ACCEPT_GARBAGE_ARGS: u32 = 4;
pub const ACCEPT_SYSTEM_ERR: u32 = 5;

/// RPC rejected status codes.
pub const REJECT_RPC_MISMATCH: u32 = 0;
pub const REJECT_AUTH_ERROR: u32 = 1;

/// RPC authentication error status codes.
pub const AUTH_BADCRED: u32 = 1;
pub const AUTH_REJECTEDCRED: u32 = 2;

/// RPC authentication flavors.
pub const AUTH_NONE: u32 = 0;
pub const AUTH_UNIX: u32 = 1;

/// RFC 5531 maximum opaque auth payload length.
pub const MAX_AUTH_BODY_BYTES: usize = 400;
/// Maximum number of fragments accepted for a single RPC record.
pub const MAX_RPC_FRAGMENTS: usize = 1024;
/// Maximum number of auxiliary GIDs accepted in `AUTH_UNIX`.
pub const MAX_AUTH_UNIX_GIDS: usize = 16;

/// ONC RPC program and version numbers for MOUNT v3 and NFS v3.
pub const MOUNT_PROGRAM: u32 = 100_005;
pub const MOUNT_VERSION: u32 = 3;
pub const NFS_PROGRAM: u32 = 100_003;
pub const NFS_VERSION: u32 = 3;

const RECORD_LAST_FRAGMENT_BIT: u32 = 0x8000_0000;
const RECORD_LENGTH_MASK: u32 = 0x7FFF_FFFF;

/// Errors arising from RPC framing or header decoding.
#[derive(Debug)]
pub enum RpcError {
    /// Underlying transport I/O error.
    Io(std::io::Error),
    /// XDR encoding or decoding error.
    Xdr(XdrError),
    /// Record exceeded [`MAX_XDR_RECORD_BYTES`].
    RecordTooLarge { len: usize, max: usize },
    /// Too many record fragments before `last_fragment` was set.
    TooManyFragments,
    /// Message type was not `MSG_CALL` (`0`).
    UnexpectedMessageType(u32),
    /// Malformed `AUTH_UNIX` credential body.
    MalformedAuth(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "RPC transport I/O error: {err}"),
            Self::Xdr(err) => write!(f, "RPC XDR error: {err}"),
            Self::RecordTooLarge { len, max } => {
                write!(f, "RPC record length {len} exceeds maximum {max} bytes")
            }
            Self::TooManyFragments => write!(f, "RPC record exceeded maximum fragment count"),
            Self::UnexpectedMessageType(ty) => write!(f, "expected RPC CALL (0), got {ty}"),
            Self::MalformedAuth(msg) => write!(f, "malformed RPC auth credential: {msg}"),
        }
    }
}

impl std::error::Error for RpcError {}

impl From<std::io::Error> for RpcError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<XdrError> for RpcError {
    fn from(value: XdrError) -> Self {
        Self::Xdr(value)
    }
}

/// Decoded `AUTH_UNIX` (`AUTH_SYS`, flavor 1) credential payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUnix {
    pub stamp: u32,
    pub machine_name: String,
    pub uid: u32,
    pub gid: u32,
    pub aux_gids: Vec<u32>,
}

/// Parsed RPC call credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcAuth {
    /// `AUTH_NONE` (flavor 0).
    None,
    /// `AUTH_UNIX` / `AUTH_SYS` (flavor 1).
    Unix(AuthUnix),
    /// Any other authentication flavor.
    Other { flavor: u32, body: Vec<u8> },
}

/// Raw opaque authentication tuple (`flavor` + `body`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueAuth {
    pub flavor: u32,
    pub body: Vec<u8>,
}

impl OpaqueAuth {
    /// Empty `AUTH_NONE` verifier.
    pub fn none() -> Self {
        Self {
            flavor: AUTH_NONE,
            body: Vec::new(),
        }
    }
}

/// Decoded ONC RPC v2 call header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcCallHeader {
    pub xid: u32,
    pub rpcvers: u32,
    pub prog: u32,
    pub vers: u32,
    pub proc_num: u32,
    pub cred: RpcAuth,
    pub verf: OpaqueAuth,
}

/// Supported MOUNT v3 (program `100005`, version `3`) procedures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MountProc {
    Null = 0,
    Mnt = 1,
    Dump = 2,
    Umnt = 3,
    UmntAll = 4,
    Export = 5,
}

impl MountProc {
    pub fn from_u32(proc_num: u32) -> Option<Self> {
        match proc_num {
            0 => Some(Self::Null),
            1 => Some(Self::Mnt),
            2 => Some(Self::Dump),
            3 => Some(Self::Umnt),
            4 => Some(Self::UmntAll),
            5 => Some(Self::Export),
            _ => None,
        }
    }
}

/// Supported NFS v3 (program `100003`, version `3`) procedures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NfsProc {
    Null = 0,
    GetAttr = 1,
    SetAttr = 2,
    Lookup = 3,
    Access = 4,
    ReadLink = 5,
    Read = 6,
    Write = 7,
    Create = 8,
    Mkdir = 9,
    Symlink = 10,
    Mknod = 11,
    Remove = 12,
    Rmdir = 13,
    Rename = 14,
    Link = 15,
    Readdir = 16,
    ReaddirPlus = 17,
    FsStat = 18,
    FsInfo = 19,
    PathConf = 20,
    Commit = 21,
}

impl NfsProc {
    pub fn from_u32(proc_num: u32) -> Option<Self> {
        match proc_num {
            0 => Some(Self::Null),
            1 => Some(Self::GetAttr),
            2 => Some(Self::SetAttr),
            3 => Some(Self::Lookup),
            4 => Some(Self::Access),
            5 => Some(Self::ReadLink),
            6 => Some(Self::Read),
            7 => Some(Self::Write),
            8 => Some(Self::Create),
            9 => Some(Self::Mkdir),
            10 => Some(Self::Symlink),
            11 => Some(Self::Mknod),
            12 => Some(Self::Remove),
            13 => Some(Self::Rmdir),
            14 => Some(Self::Rename),
            15 => Some(Self::Link),
            16 => Some(Self::Readdir),
            17 => Some(Self::ReaddirPlus),
            18 => Some(Self::FsStat),
            19 => Some(Self::FsInfo),
            20 => Some(Self::PathConf),
            21 => Some(Self::Commit),
            _ => None,
        }
    }

    /// Whether this NFSv3 procedure attempts to mutate filesystem state.
    pub const fn is_mutating(self) -> bool {
        matches!(
            self,
            Self::SetAttr
                | Self::Write
                | Self::Create
                | Self::Mkdir
                | Self::Symlink
                | Self::Mknod
                | Self::Remove
                | Self::Rmdir
                | Self::Rename
                | Self::Link
                | Self::Commit
        )
    }
}

/// Dispatch target for a validated ONC RPC v2 call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcDispatchTarget {
    Mount(MountProc),
    Nfs(NfsProc),
}

/// Pre-dispatch RPC protocol error that maps directly to an ONC RPC error reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcDispatchError {
    RpcMismatch,
    AuthRejected,
    ProgUnavail,
    ProgMismatch { low: u32, high: u32 },
    ProcUnavail,
}

/// Read one complete ONC RPC TCP record using RFC 5531 record marking.
///
/// Returns `Ok(None)` on a clean EOF at a record boundary. Enforces
/// [`MAX_XDR_RECORD_BYTES`] (2 MiB) and [`MAX_RPC_FRAGMENTS`] across fragments.
pub async fn read_rpc_record<R>(reader: &mut R) -> Result<Option<Vec<u8>>, RpcError>
where
    R: AsyncRead + Unpin,
{
    let mut record = Vec::new();
    let mut fragments = 0usize;

    loop {
        let mut header = [0u8; 4];
        match reader.read_exact(&mut header).await {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof && fragments == 0 => {
                return Ok(None);
            }
            Err(err) => return Err(RpcError::Io(err)),
        }

        fragments += 1;
        if fragments > MAX_RPC_FRAGMENTS {
            return Err(RpcError::TooManyFragments);
        }

        let marker = u32::from_be_bytes(header);
        let last_fragment = (marker & RECORD_LAST_FRAGMENT_BIT) != 0;
        let frag_len = (marker & RECORD_LENGTH_MASK) as usize;

        let new_len = record
            .len()
            .checked_add(frag_len)
            .ok_or(RpcError::RecordTooLarge {
                len: usize::MAX,
                max: MAX_XDR_RECORD_BYTES,
            })?;
        if new_len > MAX_XDR_RECORD_BYTES {
            return Err(RpcError::RecordTooLarge {
                len: new_len,
                max: MAX_XDR_RECORD_BYTES,
            });
        }

        if frag_len > 0 {
            let start = record.len();
            record.resize(new_len, 0);
            reader.read_exact(&mut record[start..new_len]).await?;
        }

        if last_fragment {
            return Ok(Some(record));
        }
    }
}

/// Write one complete ONC RPC TCP record as a single final fragment.
pub async fn write_rpc_record<W>(writer: &mut W, payload: &[u8]) -> Result<(), RpcError>
where
    W: AsyncWrite + Unpin,
{
    if payload.len() > MAX_XDR_RECORD_BYTES {
        return Err(RpcError::RecordTooLarge {
            len: payload.len(),
            max: MAX_XDR_RECORD_BYTES,
        });
    }
    let len_u32 = u32::try_from(payload.len()).map_err(|_| RpcError::RecordTooLarge {
        len: payload.len(),
        max: MAX_XDR_RECORD_BYTES,
    })?;
    let marker = RECORD_LAST_FRAGMENT_BIT | len_u32;
    writer.write_all(&marker.to_be_bytes()).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

/// Decode an ONC RPC v2 call header and return the reader positioned at the procedure arguments.
pub fn decode_rpc_call(record: &[u8]) -> Result<(RpcCallHeader, XdrReader<'_>), RpcError> {
    let mut reader = XdrReader::new(record)?;
    let xid = reader.read_u32()?;
    let msg_type = reader.read_u32()?;
    if msg_type != MSG_CALL {
        return Err(RpcError::UnexpectedMessageType(msg_type));
    }
    let rpcvers = reader.read_u32()?;
    let prog = reader.read_u32()?;
    let vers = reader.read_u32()?;
    let proc_num = reader.read_u32()?;
    let cred = decode_cred(&mut reader)?;
    let verf = decode_opaque_auth(&mut reader)?;
    Ok((
        RpcCallHeader {
            xid,
            rpcvers,
            prog,
            vers,
            proc_num,
            cred,
            verf,
        },
        reader,
    ))
}

/// Classify a decoded RPC call header into a MOUNT v3 or NFS v3 procedure or RPC-level error.
pub fn classify_dispatch(header: &RpcCallHeader) -> Result<RpcDispatchTarget, RpcDispatchError> {
    if header.rpcvers != RPC_VERSION {
        return Err(RpcDispatchError::RpcMismatch);
    }
    if matches!(header.cred, RpcAuth::Other { .. }) {
        return Err(RpcDispatchError::AuthRejected);
    }
    match header.prog {
        MOUNT_PROGRAM => {
            if header.vers != MOUNT_VERSION {
                return Err(RpcDispatchError::ProgMismatch {
                    low: MOUNT_VERSION,
                    high: MOUNT_VERSION,
                });
            }
            MountProc::from_u32(header.proc_num)
                .map(RpcDispatchTarget::Mount)
                .ok_or(RpcDispatchError::ProcUnavail)
        }
        NFS_PROGRAM => {
            if header.vers != NFS_VERSION {
                return Err(RpcDispatchError::ProgMismatch {
                    low: NFS_VERSION,
                    high: NFS_VERSION,
                });
            }
            NfsProc::from_u32(header.proc_num)
                .map(RpcDispatchTarget::Nfs)
                .ok_or(RpcDispatchError::ProcUnavail)
        }
        _ => Err(RpcDispatchError::ProgUnavail),
    }
}

/// Encode an accepted `SUCCESS` RPC reply wrapping `procedure_body`.
pub fn encode_accepted_success(xid: u32, procedure_body: &[u8]) -> Result<Vec<u8>, XdrError> {
    let mut writer = XdrWriter::with_capacity(24 + procedure_body.len());
    write_accepted_header(&mut writer, xid, ACCEPT_SUCCESS)?;
    writer.write_raw_bytes(procedure_body)?;
    Ok(writer.into_bytes())
}

/// Encode an accepted RPC error reply (e.g. `PROG_UNAVAIL`, `PROG_MISMATCH`, `PROC_UNAVAIL`,
/// `GARBAGE_ARGS`, `SYSTEM_ERR`).
pub fn encode_accepted_error(
    xid: u32,
    accept_stat: u32,
    mismatch: Option<(u32, u32)>,
) -> Result<Vec<u8>, XdrError> {
    let mut writer = XdrWriter::with_capacity(32);
    write_accepted_header(&mut writer, xid, accept_stat)?;
    if let Some((low, high)) = mismatch {
        writer.write_u32(low)?;
        writer.write_u32(high)?;
    }
    Ok(writer.into_bytes())
}

/// Encode a `MSG_DENIED / RPC_MISMATCH` reply advertising RPC v2 only.
pub fn encode_denied_rpc_mismatch(xid: u32) -> Result<Vec<u8>, XdrError> {
    let mut writer = XdrWriter::with_capacity(24);
    writer.write_u32(xid)?;
    writer.write_u32(MSG_REPLY)?;
    writer.write_u32(MSG_DENIED)?;
    writer.write_u32(REJECT_RPC_MISMATCH)?;
    writer.write_u32(RPC_VERSION)?;
    writer.write_u32(RPC_VERSION)?;
    Ok(writer.into_bytes())
}

/// Encode a `MSG_DENIED / AUTH_ERROR` reply.
pub fn encode_denied_auth_error(xid: u32, auth_stat: u32) -> Result<Vec<u8>, XdrError> {
    let mut writer = XdrWriter::with_capacity(20);
    writer.write_u32(xid)?;
    writer.write_u32(MSG_REPLY)?;
    writer.write_u32(MSG_DENIED)?;
    writer.write_u32(REJECT_AUTH_ERROR)?;
    writer.write_u32(auth_stat)?;
    Ok(writer.into_bytes())
}

/// Encode a dispatch-level error into the corresponding ONC RPC reply record.
pub fn encode_dispatch_error(xid: u32, err: RpcDispatchError) -> Result<Vec<u8>, XdrError> {
    match err {
        RpcDispatchError::RpcMismatch => encode_denied_rpc_mismatch(xid),
        RpcDispatchError::AuthRejected => encode_denied_auth_error(xid, AUTH_REJECTEDCRED),
        RpcDispatchError::ProgUnavail => encode_accepted_error(xid, ACCEPT_PROG_UNAVAIL, None),
        RpcDispatchError::ProgMismatch { low, high } => {
            encode_accepted_error(xid, ACCEPT_PROG_MISMATCH, Some((low, high)))
        }
        RpcDispatchError::ProcUnavail => encode_accepted_error(xid, ACCEPT_PROC_UNAVAIL, None),
    }
}

/// Encode an ONC RPC v2 call message (used by protocol tests and diagnostics).
pub fn encode_rpc_call(
    xid: u32,
    prog: u32,
    vers: u32,
    proc_num: u32,
    cred: &RpcAuth,
    args: &[u8],
) -> Result<Vec<u8>, XdrError> {
    let mut writer = XdrWriter::with_capacity(64 + args.len());
    writer.write_u32(xid)?;
    writer.write_u32(MSG_CALL)?;
    writer.write_u32(RPC_VERSION)?;
    writer.write_u32(prog)?;
    writer.write_u32(vers)?;
    writer.write_u32(proc_num)?;
    match cred {
        RpcAuth::None => {
            writer.write_u32(AUTH_NONE)?;
            writer.write_opaque(&[])?;
        }
        RpcAuth::Unix(unix) => {
            let mut body = XdrWriter::with_limit(MAX_AUTH_BODY_BYTES);
            body.write_u32(unix.stamp)?;
            body.write_string(&unix.machine_name)?;
            body.write_u32(unix.uid)?;
            body.write_u32(unix.gid)?;
            body.write_u32(
                u32::try_from(unix.aux_gids.len()).map_err(|_| XdrError::LengthOverflow)?,
            )?;
            for &gid in &unix.aux_gids {
                body.write_u32(gid)?;
            }
            writer.write_u32(AUTH_UNIX)?;
            writer.write_opaque(body.as_slice())?;
        }
        RpcAuth::Other { flavor, body } => {
            writer.write_u32(*flavor)?;
            writer.write_opaque(body)?;
        }
    }
    // AUTH_NONE verifier
    writer.write_u32(AUTH_NONE)?;
    writer.write_opaque(&[])?;
    writer.write_raw_bytes(args)?;
    Ok(writer.into_bytes())
}

fn write_accepted_header(
    writer: &mut XdrWriter,
    xid: u32,
    accept_stat: u32,
) -> Result<(), XdrError> {
    writer.write_u32(xid)?;
    writer.write_u32(MSG_REPLY)?;
    writer.write_u32(MSG_ACCEPTED)?;
    // AUTH_NONE reply verifier
    writer.write_u32(AUTH_NONE)?;
    writer.write_opaque(&[])?;
    writer.write_u32(accept_stat)?;
    Ok(())
}

fn decode_opaque_auth(reader: &mut XdrReader<'_>) -> Result<OpaqueAuth, RpcError> {
    let flavor = reader.read_u32()?;
    let body = reader.read_opaque_bounded(MAX_AUTH_BODY_BYTES)?.to_vec();
    Ok(OpaqueAuth { flavor, body })
}

fn decode_cred(reader: &mut XdrReader<'_>) -> Result<RpcAuth, RpcError> {
    let raw = decode_opaque_auth(reader)?;
    match raw.flavor {
        AUTH_NONE => {
            if !raw.body.is_empty() {
                return Err(RpcError::MalformedAuth(
                    "AUTH_NONE body must be empty".to_string(),
                ));
            }
            Ok(RpcAuth::None)
        }
        AUTH_UNIX => {
            let mut body_reader = XdrReader::with_limit(&raw.body, MAX_AUTH_BODY_BYTES)?;
            let stamp = body_reader.read_u32()?;
            let machine_name = body_reader.read_string_bounded(255)?;
            let uid = body_reader.read_u32()?;
            let gid = body_reader.read_u32()?;
            let gid_count = body_reader.read_u32()? as usize;
            if gid_count > MAX_AUTH_UNIX_GIDS {
                return Err(RpcError::MalformedAuth(format!(
                    "AUTH_UNIX aux gid count {gid_count} exceeds {MAX_AUTH_UNIX_GIDS}"
                )));
            }
            let mut aux_gids = Vec::with_capacity(gid_count);
            for _ in 0..gid_count {
                aux_gids.push(body_reader.read_u32()?);
            }
            Ok(RpcAuth::Unix(AuthUnix {
                stamp,
                machine_name,
                uid,
                gid,
                aux_gids,
            }))
        }
        flavor => Ok(RpcAuth::Other {
            flavor,
            body: raw.body,
        }),
    }
}
