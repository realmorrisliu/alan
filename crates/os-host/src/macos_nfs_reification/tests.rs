//! Unit tests for XDR, ONC RPC v2, read-only NFSv3/MOUNT v3, and dynamic aP snapshots.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use alan_ap::reference::MemFs;
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};
use async_trait::async_trait;
use tokio::sync::Mutex;

use super::nfs_server::{
    ACCESS3_DELETE, ACCESS3_EXECUTE, ACCESS3_EXTEND, ACCESS3_LOOKUP, ACCESS3_MODIFY, ACCESS3_READ,
    MNT3_OK, MNT3ERR_NOENT, NF3DIR, NF3REG, NFS3_OK, NFS3ERR_ACCES, NFS3ERR_BADHANDLE,
    NFS3ERR_INVAL, NFS3ERR_NOENT, NFS3ERR_ROFS, NFS3ERR_STALE, NFS3ERR_TOOSMALL, NfsServerConfig,
    NfsService, ROOT_FILE_ID, serve_rpc_stream,
};
use super::rpc::{
    ACCEPT_PROC_UNAVAIL, ACCEPT_PROG_MISMATCH, ACCEPT_PROG_UNAVAIL, ACCEPT_SUCCESS, AUTH_NONE,
    AUTH_UNIX, AuthUnix, MOUNT_PROGRAM, MOUNT_VERSION, MSG_ACCEPTED, MSG_DENIED, MSG_REPLY,
    MountProc, NFS_PROGRAM, NFS_VERSION, NfsProc, REJECT_AUTH_ERROR, REJECT_RPC_MISMATCH, RpcAuth,
    RpcError, decode_rpc_call, encode_rpc_call, read_rpc_record, write_rpc_record,
};
use super::xdr::{MAX_XDR_RECORD_BYTES, XdrError, XdrReader, XdrWriter, xdr_padded_len};

/// Test aP server exposing static files, a live `FileKind::Stream`, a dynamic
/// `status` node whose `stat.length` is 0, and a `FileKind::Clone` side-effect detector.
pub(super) struct DynamicTestFs {
    stream: Stream,
    status_text: Vec<u8>,
    clone_opens: Arc<AtomicUsize>,
    fids: Mutex<HashMap<Fid, (String, Option<OpenMode>)>>,
}

impl DynamicTestFs {
    pub(super) async fn new() -> Self {
        let stream = Stream::new();
        stream.append(b"event-1: ready\nevent-2: ok\n").await;
        Self {
            stream,
            status_text: b"state=healthy\nmode=ro\n".to_vec(),
            clone_opens: Arc::new(AtomicUsize::new(0)),
            fids: Mutex::new(HashMap::from([(Fid::ROOT, (String::new(), None))])),
        }
    }

    fn qid_for(path: &str) -> Result<Qid, ErrorCode> {
        match path {
            "" => Ok(Qid {
                kind: FileKind::Dir,
                version: 1,
                path: 1,
            }),
            "sub" => Ok(Qid {
                kind: FileKind::Dir,
                version: 1,
                path: 2,
            }),
            "hello.txt" => Ok(Qid {
                kind: FileKind::File,
                version: 3,
                path: 3,
            }),
            "sub/nested.txt" => Ok(Qid {
                kind: FileKind::File,
                version: 4,
                path: 4,
            }),
            "events" => Ok(Qid {
                kind: FileKind::Stream,
                version: 1,
                path: 5,
            }),
            "status" => Ok(Qid {
                kind: FileKind::File,
                version: 7,
                path: 6,
            }),
            "clone" => Ok(Qid {
                kind: FileKind::Clone,
                version: 1,
                path: 7,
            }),
            _ => Err(ErrorCode::NotFound),
        }
    }
}

#[async_trait]
impl FileServer for DynamicTestFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut fids = self.fids.lock().await;
        let base = fids.get(&fid).ok_or(ErrorCode::NotFound)?.0.clone();
        let mut path = base;
        for name in names {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(name);
        }
        let qid = Self::qid_for(&path)?;
        fids.insert(newfid, (path, None));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut fids = self.fids.lock().await;
        let entry = fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        let qid = Self::qid_for(&entry.0)?;
        if qid.kind == FileKind::Clone {
            self.clone_opens.fetch_add(1, Ordering::SeqCst);
        }
        entry.1 = Some(mode);
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let path = {
            let fids = self.fids.lock().await;
            let entry = fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if !matches!(entry.1, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                return Err(ErrorCode::NoAccess);
            }
            entry.0.clone()
        };
        if path == "events" {
            return Ok(self.stream.read(offset, count).await);
        }
        let bytes: Vec<u8> = match path.as_str() {
            "" => b"hello.txt\nsub\nevents\nstatus\nclone".to_vec(),
            "sub" => b"nested.txt".to_vec(),
            "hello.txt" => b"hello alan9 nfs\nline two needle\n".to_vec(),
            "sub/nested.txt" => b"deep nested content\n".to_vec(),
            "status" => self.status_text.clone(),
            _ => return Err(ErrorCode::Unsupported),
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let path = self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?
            .0
            .clone();
        let qid = Self::qid_for(&path)?;
        let length = match path.as_str() {
            "hello.txt" => b"hello alan9 nfs\nline two needle\n".len() as u64,
            "sub/nested.txt" => b"deep nested content\n".len() as u64,
            "events" => self.stream.len().await,
            // Intentionally report 0 for dynamic `status` node to verify snapshot probing.
            "status" | "clone" | "" | "sub" => 0,
            _ => return Err(ErrorCode::NotFound),
        };
        Ok(Stat {
            name: path,
            qid,
            length,
            executable: false,
            writable: false,
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
        if fid != Fid::ROOT {
            self.fids.lock().await.remove(&fid);
        }
        Ok(())
    }
}

pub(super) fn parse_accepted_reply(reply: &[u8], expected_xid: u32) -> XdrReader<'_> {
    let mut r = XdrReader::new(reply).expect("valid reply XDR");
    assert_eq!(r.read_u32().unwrap(), expected_xid);
    assert_eq!(r.read_u32().unwrap(), MSG_REPLY);
    assert_eq!(r.read_u32().unwrap(), MSG_ACCEPTED);
    assert_eq!(r.read_u32().unwrap(), AUTH_NONE);
    assert!(r.read_opaque().unwrap().is_empty());
    assert_eq!(r.read_u32().unwrap(), ACCEPT_SUCCESS);
    r
}

fn skip_fattr3(r: &mut XdrReader<'_>) -> (u32, u32, u64, u64) {
    let ftype = r.read_u32().unwrap();
    let mode = r.read_u32().unwrap();
    let _nlink = r.read_u32().unwrap();
    let _uid = r.read_u32().unwrap();
    let _gid = r.read_u32().unwrap();
    let size = r.read_u64().unwrap();
    let _used = r.read_u64().unwrap();
    let _rdev1 = r.read_u32().unwrap();
    let _rdev2 = r.read_u32().unwrap();
    let _fsid = r.read_u64().unwrap();
    let fileid = r.read_u64().unwrap();
    for _ in 0..3 {
        let _sec = r.read_u32().unwrap();
        let _nsec = r.read_u32().unwrap();
    }
    (ftype, mode, size, fileid)
}

fn skip_post_op_attr(r: &mut XdrReader<'_>) -> Option<(u32, u32, u64, u64)> {
    if r.read_bool().unwrap() {
        Some(skip_fattr3(r))
    } else {
        None
    }
}

async fn call_nfs(service: &NfsService, xid: u32, proc_id: NfsProc, args: &[u8]) -> Vec<u8> {
    let req = encode_rpc_call(
        xid,
        NFS_PROGRAM,
        NFS_VERSION,
        proc_id as u32,
        &RpcAuth::None,
        args,
    )
    .unwrap();
    service.handle_rpc_record(&req).await.unwrap()
}

async fn call_mount(service: &NfsService, xid: u32, proc_id: MountProc, args: &[u8]) -> Vec<u8> {
    let req = encode_rpc_call(
        xid,
        MOUNT_PROGRAM,
        MOUNT_VERSION,
        proc_id as u32,
        &RpcAuth::None,
        args,
    )
    .unwrap();
    service.handle_rpc_record(&req).await.unwrap()
}

#[test]
fn xdr_roundtrip_padding_and_bounds_checks() {
    assert_eq!(xdr_padded_len(0), Some(0));
    assert_eq!(xdr_padded_len(1), Some(4));
    assert_eq!(xdr_padded_len(2), Some(4));
    assert_eq!(xdr_padded_len(3), Some(4));
    assert_eq!(xdr_padded_len(4), Some(4));
    assert_eq!(xdr_padded_len(5), Some(8));

    let mut w = XdrWriter::new();
    w.write_u32(0x1234_5678).unwrap();
    w.write_i32(-42).unwrap();
    w.write_u64(0x0102_0304_0506_0708).unwrap();
    w.write_bool(true).unwrap();
    w.write_bool(false).unwrap();
    w.write_fixed_opaque(b"a").unwrap();
    w.write_opaque(b"ab").unwrap();
    w.write_string("abc").unwrap();
    w.write_string("abcd").unwrap();

    let encoded = w.into_bytes();
    assert_eq!(encoded.len() % 4, 0);

    let mut r = XdrReader::new(&encoded).unwrap();
    assert_eq!(r.read_u32().unwrap(), 0x1234_5678);
    assert_eq!(r.read_i32().unwrap(), -42);
    assert_eq!(r.read_u64().unwrap(), 0x0102_0304_0506_0708);
    assert!(r.read_bool().unwrap());
    assert!(!r.read_bool().unwrap());
    assert_eq!(r.read_fixed_opaque(1).unwrap(), b"a");
    assert_eq!(r.read_opaque().unwrap(), b"ab");
    assert_eq!(r.read_string().unwrap(), "abc");
    assert_eq!(r.read_string().unwrap(), "abcd");
    assert_eq!(r.remaining(), 0);
    assert_eq!(r.read_u32(), Err(XdrError::UnexpectedEof));

    // Invalid boolean value.
    let bad_bool_bytes = 2u32.to_be_bytes();
    let mut bad_bool = XdrReader::new(&bad_bool_bytes).unwrap();
    assert_eq!(bad_bool.read_bool(), Err(XdrError::InvalidBool(2)));

    // Invalid UTF-8 string.
    let mut bad_utf8_w = XdrWriter::new();
    bad_utf8_w.write_opaque(&[0xFF, 0xFE]).unwrap();
    let bad_utf8_bytes = bad_utf8_w.into_bytes();
    let mut bad_utf8_r = XdrReader::new(&bad_utf8_bytes).unwrap();
    assert_eq!(bad_utf8_r.read_string(), Err(XdrError::InvalidUtf8));

    // 2 MiB cap enforced on both reader and writer.
    let oversized = vec![0u8; MAX_XDR_RECORD_BYTES + 4];
    assert!(matches!(
        XdrReader::new(&oversized),
        Err(XdrError::RecordTooLarge { .. })
    ));
    let mut capped = XdrWriter::with_limit(16);
    assert!(matches!(
        capped.write_opaque(&[0u8; 20]),
        Err(XdrError::RecordTooLarge { .. })
    ));
}

#[tokio::test]
async fn rpc_multi_fragment_framing_auth_and_dispatch_errors() {
    // Multi-fragment TCP record reassembly + AUTH_UNIX decode.
    let auth = RpcAuth::Unix(AuthUnix {
        stamp: 99,
        machine_name: "mac-local".to_string(),
        uid: 501,
        gid: 20,
        aux_gids: vec![12, 61],
    });
    let call_bytes = encode_rpc_call(77, NFS_PROGRAM, NFS_VERSION, 0, &auth, &[]).unwrap();
    let split = call_bytes.len() / 2;

    let (mut client, mut server) = tokio::io::duplex(4096);
    let frag1_hdr = (split as u32).to_be_bytes(); // last_fragment = 0
    let frag2_hdr = (0x8000_0000u32 | ((call_bytes.len() - split) as u32)).to_be_bytes();
    tokio::io::AsyncWriteExt::write_all(&mut client, &frag1_hdr)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut client, &call_bytes[..split])
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut client, &frag2_hdr)
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut client, &call_bytes[split..])
        .await
        .unwrap();
    drop(client);

    let reassembled = read_rpc_record(&mut server).await.unwrap().unwrap();
    assert_eq!(reassembled, call_bytes);
    let (hdr, _) = decode_rpc_call(&reassembled).unwrap();
    assert_eq!(hdr.xid, 77);
    assert_eq!(hdr.cred, auth);
    assert!(read_rpc_record(&mut server).await.unwrap().is_none());

    // Record > 2 MiB rejected before allocating full payload.
    let (mut client2, mut server2) = tokio::io::duplex(64);
    let huge_hdr = (0x8000_0000u32 | ((MAX_XDR_RECORD_BYTES + 16) as u32)).to_be_bytes();
    tokio::io::AsyncWriteExt::write_all(&mut client2, &huge_hdr)
        .await
        .unwrap();
    assert!(matches!(
        read_rpc_record(&mut server2).await,
        Err(RpcError::RecordTooLarge { .. })
    ));

    // Dispatch error replies: RPC version mismatch, unknown prog, version mismatch, unknown proc, unknown auth.
    let svc = NfsService::new(Arc::new(MemFs::empty()), NfsServerConfig::default());

    let mut bad_rpc_ver =
        encode_rpc_call(1, NFS_PROGRAM, NFS_VERSION, 0, &RpcAuth::None, &[]).unwrap();
    bad_rpc_ver[8..12].copy_from_slice(&99u32.to_be_bytes());
    let rep = svc.handle_rpc_record(&bad_rpc_ver).await.unwrap();
    let mut r = XdrReader::new(&rep).unwrap();
    assert_eq!(r.read_u32().unwrap(), 1);
    assert_eq!(r.read_u32().unwrap(), MSG_REPLY);
    assert_eq!(r.read_u32().unwrap(), MSG_DENIED);
    assert_eq!(r.read_u32().unwrap(), REJECT_RPC_MISMATCH);

    let bad_auth = encode_rpc_call(
        2,
        NFS_PROGRAM,
        NFS_VERSION,
        0,
        &RpcAuth::Other {
            flavor: 6,
            body: vec![1, 2, 3],
        },
        &[],
    )
    .unwrap();
    let rep = svc.handle_rpc_record(&bad_auth).await.unwrap();
    let mut r = XdrReader::new(&rep).unwrap();
    assert_eq!(r.read_u32().unwrap(), 2);
    assert_eq!(r.read_u32().unwrap(), MSG_REPLY);
    assert_eq!(r.read_u32().unwrap(), MSG_DENIED);
    assert_eq!(r.read_u32().unwrap(), REJECT_AUTH_ERROR);

    let bad_prog = encode_rpc_call(3, 999_999, 3, 0, &RpcAuth::None, &[]).unwrap();
    let rep = svc.handle_rpc_record(&bad_prog).await.unwrap();
    let mut r = XdrReader::new(&rep).unwrap();
    assert_eq!(r.read_u32().unwrap(), 3);
    assert_eq!(r.read_u32().unwrap(), MSG_REPLY);
    assert_eq!(r.read_u32().unwrap(), MSG_ACCEPTED);
    assert_eq!(r.read_u32().unwrap(), AUTH_NONE);
    let _ = r.read_opaque().unwrap();
    assert_eq!(r.read_u32().unwrap(), ACCEPT_PROG_UNAVAIL);

    let bad_vers = encode_rpc_call(4, NFS_PROGRAM, 2, 0, &RpcAuth::None, &[]).unwrap();
    let rep = svc.handle_rpc_record(&bad_vers).await.unwrap();
    let mut r = XdrReader::new(&rep).unwrap();
    for _ in 0..5 {
        let _ = r.read_u32().unwrap();
    }
    assert_eq!(r.read_u32().unwrap(), ACCEPT_PROG_MISMATCH);
    assert_eq!(r.read_u32().unwrap(), 3);
    assert_eq!(r.read_u32().unwrap(), 3);

    let bad_proc = encode_rpc_call(5, NFS_PROGRAM, NFS_VERSION, 99, &RpcAuth::None, &[]).unwrap();
    let rep = svc.handle_rpc_record(&bad_proc).await.unwrap();
    let mut r = XdrReader::new(&rep).unwrap();
    for _ in 0..5 {
        let _ = r.read_u32().unwrap();
    }
    assert_eq!(r.read_u32().unwrap(), ACCEPT_PROC_UNAVAIL);
}

#[tokio::test]
async fn mount_and_nfs_readonly_procedures_and_dotdot_traversal() {
    let fs = Arc::new(DynamicTestFs::new().await);
    let clone_opens = Arc::clone(&fs.clone_opens);
    let svc = NfsService::new(fs, NfsServerConfig::default());

    // MOUNT NULL, EXPORT, MNT("/"), MNT("/bad"), UMNT, UMNTALL, DUMP.
    let rep = call_mount(&svc, 10, MountProc::Null, &[]).await;
    assert_eq!(parse_accepted_reply(&rep, 10).remaining(), 0);

    let rep = call_mount(&svc, 11, MountProc::Export, &[]).await;
    let mut r = parse_accepted_reply(&rep, 11);
    assert!(r.read_bool().unwrap());
    assert_eq!(r.read_string().unwrap(), "/");
    assert!(!r.read_bool().unwrap());
    assert!(!r.read_bool().unwrap());

    let mut mnt_args = XdrWriter::new();
    mnt_args.write_string("/").unwrap();
    let rep = call_mount(&svc, 12, MountProc::Mnt, mnt_args.as_slice()).await;
    let mut r = parse_accepted_reply(&rep, 12);
    assert_eq!(r.read_u32().unwrap(), MNT3_OK);
    let root_fh = r.read_opaque().unwrap().to_vec();
    let n_flavors = r.read_u32().unwrap();
    assert_eq!(n_flavors, 2);
    assert_eq!(r.read_u32().unwrap(), AUTH_UNIX);
    assert_eq!(r.read_u32().unwrap(), AUTH_NONE);

    let mut bad_mnt = XdrWriter::new();
    bad_mnt.write_string("/nonexistent").unwrap();
    let rep = call_mount(&svc, 13, MountProc::Mnt, bad_mnt.as_slice()).await;
    assert_eq!(
        parse_accepted_reply(&rep, 13).read_u32().unwrap(),
        MNT3ERR_NOENT
    );

    let _ = call_mount(&svc, 14, MountProc::Umnt, mnt_args.as_slice()).await;
    let _ = call_mount(&svc, 15, MountProc::UmntAll, &[]).await;
    let rep = call_mount(&svc, 16, MountProc::Dump, &[]).await;
    assert!(!parse_accepted_reply(&rep, 16).read_bool().unwrap());

    // NFS NULL & GETATTR(root).
    let rep = call_nfs(&svc, 20, NfsProc::Null, &[]).await;
    assert_eq!(parse_accepted_reply(&rep, 20).remaining(), 0);

    let mut fh_arg = XdrWriter::new();
    fh_arg.write_opaque(&root_fh).unwrap();
    let rep = call_nfs(&svc, 21, NfsProc::GetAttr, fh_arg.as_slice()).await;
    let mut r = parse_accepted_reply(&rep, 21);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    let (ftype, mode, _size, fileid) = skip_fattr3(&mut r);
    assert_eq!(ftype, NF3DIR);
    assert_eq!(mode, 0o555);
    assert_eq!(fileid, ROOT_FILE_ID);

    // LOOKUP root/sub, root/sub/nested.txt, and ".." back to root, plus root/.. stays at root.
    let lookup = |dir: &[u8], name: &str| {
        let mut w = XdrWriter::new();
        w.write_opaque(dir).unwrap();
        w.write_string(name).unwrap();
        w.into_bytes()
    };

    let rep = call_nfs(&svc, 22, NfsProc::Lookup, &lookup(&root_fh, "sub")).await;
    let mut r = parse_accepted_reply(&rep, 22);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    let sub_fh = r.read_opaque().unwrap().to_vec();
    let (sub_type, _, _, sub_id) = skip_post_op_attr(&mut r).unwrap();
    assert_eq!(sub_type, NF3DIR);
    assert_ne!(sub_id, ROOT_FILE_ID);

    // sub/.. returns root_fh!
    let rep = call_nfs(&svc, 23, NfsProc::Lookup, &lookup(&sub_fh, "..")).await;
    let mut r = parse_accepted_reply(&rep, 23);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    assert_eq!(r.read_opaque().unwrap(), root_fh.as_slice());

    // root/.. clamps to root_fh!
    let rep = call_nfs(&svc, 24, NfsProc::Lookup, &lookup(&root_fh, "..")).await;
    let mut r = parse_accepted_reply(&rep, 24);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    assert_eq!(r.read_opaque().unwrap(), root_fh.as_slice());

    // Invalid slash & missing entry.
    let rep = call_nfs(
        &svc,
        25,
        NfsProc::Lookup,
        &lookup(&root_fh, "sub/nested.txt"),
    )
    .await;
    assert_eq!(
        parse_accepted_reply(&rep, 25).read_u32().unwrap(),
        NFS3ERR_INVAL
    );
    let rep = call_nfs(&svc, 26, NfsProc::Lookup, &lookup(&root_fh, "missing")).await;
    assert_eq!(
        parse_accepted_reply(&rep, 26).read_u32().unwrap(),
        NFS3ERR_NOENT
    );

    // LOOKUP nested file & READ slices.
    let rep = call_nfs(&svc, 27, NfsProc::Lookup, &lookup(&sub_fh, "nested.txt")).await;
    let mut r = parse_accepted_reply(&rep, 27);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    let nested_fh = r.read_opaque().unwrap().to_vec();
    let (nested_type, nested_mode, nested_size, _) = skip_post_op_attr(&mut r).unwrap();
    assert_eq!(nested_type, NF3REG);
    assert_eq!(nested_mode, 0o444);
    assert_eq!(nested_size, b"deep nested content\n".len() as u64);

    let mut read_arg = XdrWriter::new();
    read_arg.write_opaque(&nested_fh).unwrap();
    read_arg.write_u64(5).unwrap();
    read_arg.write_u32(6).unwrap();
    let rep = call_nfs(&svc, 28, NfsProc::Read, read_arg.as_slice()).await;
    let mut r = parse_accepted_reply(&rep, 28);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    assert!(skip_post_op_attr(&mut r).is_some());
    assert_eq!(r.read_u32().unwrap(), 6);
    assert!(!r.read_bool().unwrap());
    assert_eq!(r.read_opaque().unwrap(), b"nested");

    // ACCESS never grants MODIFY/EXTEND/DELETE.
    let mut acc_arg = XdrWriter::new();
    acc_arg.write_opaque(&nested_fh).unwrap();
    acc_arg
        .write_u32(
            ACCESS3_READ
                | ACCESS3_LOOKUP
                | ACCESS3_MODIFY
                | ACCESS3_EXTEND
                | ACCESS3_DELETE
                | ACCESS3_EXECUTE,
        )
        .unwrap();
    let rep = call_nfs(&svc, 29, NfsProc::Access, acc_arg.as_slice()).await;
    let mut r = parse_accepted_reply(&rep, 29);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    assert!(skip_post_op_attr(&mut r).is_some());
    assert_eq!(r.read_u32().unwrap(), ACCESS3_READ);

    // READLINK -> NFS3ERR_INVAL.
    let rep = call_nfs(&svc, 30, NfsProc::ReadLink, fh_arg.as_slice()).await;
    assert_eq!(
        parse_accepted_reply(&rep, 30).read_u32().unwrap(),
        NFS3ERR_INVAL
    );

    // READDIR + pagination + NFS3ERR_TOOSMALL.
    let mut rd_small = XdrWriter::new();
    rd_small.write_opaque(&root_fh).unwrap();
    rd_small.write_u64(0).unwrap();
    rd_small.write_fixed_opaque(&[0u8; 8]).unwrap();
    rd_small.write_u32(64).unwrap();
    let rep = call_nfs(&svc, 31, NfsProc::Readdir, rd_small.as_slice()).await;
    assert_eq!(
        parse_accepted_reply(&rep, 31).read_u32().unwrap(),
        NFS3ERR_TOOSMALL
    );

    let mut cookie = 0u64;
    let mut verf = [0u8; 8];
    let mut seen_names = Vec::new();
    loop {
        let mut rd = XdrWriter::new();
        rd.write_opaque(&root_fh).unwrap();
        rd.write_u64(cookie).unwrap();
        rd.write_fixed_opaque(&verf).unwrap();
        rd.write_u32(180).unwrap(); // forces multi-page pagination
        let rep = call_nfs(&svc, 32, NfsProc::Readdir, rd.as_slice()).await;
        let mut r = parse_accepted_reply(&rep, 32);
        assert_eq!(r.read_u32().unwrap(), NFS3_OK);
        assert!(skip_post_op_attr(&mut r).is_some());
        verf.copy_from_slice(r.read_fixed_opaque(8).unwrap());
        while r.read_bool().unwrap() {
            let _id = r.read_u64().unwrap();
            let name = r.read_string().unwrap();
            cookie = r.read_u64().unwrap();
            seen_names.push(name);
        }
        if r.read_bool().unwrap() {
            break;
        }
    }
    assert_eq!(
        seen_names,
        vec![".", "..", "clone", "events", "hello.txt", "status", "sub"]
    );

    // READDIRPLUS returns names + attrs + handles.
    let mut rdp = XdrWriter::new();
    rdp.write_opaque(&root_fh).unwrap();
    rdp.write_u64(0).unwrap();
    rdp.write_fixed_opaque(&[0u8; 8]).unwrap();
    rdp.write_u32(4096).unwrap();
    rdp.write_u32(8192).unwrap();
    let rep = call_nfs(&svc, 33, NfsProc::ReaddirPlus, rdp.as_slice()).await;
    let mut r = parse_accepted_reply(&rep, 33);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    assert!(skip_post_op_attr(&mut r).is_some());
    let _ = r.read_fixed_opaque(8).unwrap();
    let mut plus_entries = BTreeMap::new();
    while r.read_bool().unwrap() {
        let _id = r.read_u64().unwrap();
        let name = r.read_string().unwrap();
        let _c = r.read_u64().unwrap();
        let attr = skip_post_op_attr(&mut r).unwrap();
        assert!(r.read_bool().unwrap());
        let fh = r.read_opaque().unwrap().to_vec();
        plus_entries.insert(name, (attr, fh));
    }
    assert!(r.read_bool().unwrap());
    assert!(plus_entries.contains_key("hello.txt"));
    assert!(plus_entries.contains_key("events"));
    assert!(plus_entries.contains_key("status"));
    // Dynamic status node reported size 0 on stat, probed non-blockingly to actual length.
    assert_eq!(
        plus_entries["status"].0.2,
        b"state=healthy\nmode=ro\n".len() as u64
    );

    // Dynamic Stream node: non-blocking snapshot read returns retained bytes + EOF without hanging!
    let events_fh = &plus_entries["events"].1;
    let mut stream_read = XdrWriter::new();
    stream_read.write_opaque(events_fh).unwrap();
    stream_read.write_u64(0).unwrap();
    stream_read.write_u32(4096).unwrap();
    let rep = tokio::time::timeout(
        Duration::from_secs(1),
        call_nfs(&svc, 34, NfsProc::Read, stream_read.as_slice()),
    )
    .await
    .expect("stream snapshot read must not block");
    let mut r = parse_accepted_reply(&rep, 34);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    assert!(skip_post_op_attr(&mut r).is_some());
    let count = r.read_u32().unwrap();
    let eof = r.read_bool().unwrap();
    let data = r.read_opaque().unwrap();
    assert!(eof);
    assert_eq!(count as usize, data.len());
    assert_eq!(data, b"event-1: ready\nevent-2: ok\n");

    // Stream read at retained edge returns empty + EOF immediately.
    let mut edge_read = XdrWriter::new();
    edge_read.write_opaque(events_fh).unwrap();
    edge_read.write_u64(data.len() as u64).unwrap();
    edge_read.write_u32(1024).unwrap();
    let rep = tokio::time::timeout(
        Duration::from_secs(1),
        call_nfs(&svc, 35, NfsProc::Read, edge_read.as_slice()),
    )
    .await
    .expect("stream edge read must not block");
    let mut r = parse_accepted_reply(&rep, 35);
    assert_eq!(r.read_u32().unwrap(), NFS3_OK);
    let _ = skip_post_op_attr(&mut r);
    assert_eq!(r.read_u32().unwrap(), 0);
    assert!(r.read_bool().unwrap());
    assert!(r.read_opaque().unwrap().is_empty());

    // Clone node must NEVER be opened on GETATTR/READ (no side effects).
    let clone_fh = &plus_entries["clone"].1;
    let mut clone_read = XdrWriter::new();
    clone_read.write_opaque(clone_fh).unwrap();
    clone_read.write_u64(0).unwrap();
    clone_read.write_u32(64).unwrap();
    let rep = call_nfs(&svc, 36, NfsProc::Read, clone_read.as_slice()).await;
    assert_eq!(
        parse_accepted_reply(&rep, 36).read_u32().unwrap(),
        NFS3ERR_ACCES
    );
    assert_eq!(clone_opens.load(Ordering::SeqCst), 0);

    // FSSTAT, FSINFO, PATHCONF succeed.
    for (xid, proc_id) in [
        (37, NfsProc::FsStat),
        (38, NfsProc::FsInfo),
        (39, NfsProc::PathConf),
    ] {
        let rep = call_nfs(&svc, xid, proc_id, fh_arg.as_slice()).await;
        assert_eq!(parse_accepted_reply(&rep, xid).read_u32().unwrap(), NFS3_OK);
    }

    // Bad handle & stale handle errors.
    let mut bad_fh = XdrWriter::new();
    bad_fh.write_opaque(&[1, 2, 3]).unwrap();
    let rep = call_nfs(&svc, 40, NfsProc::GetAttr, bad_fh.as_slice()).await;
    assert_eq!(
        parse_accepted_reply(&rep, 40).read_u32().unwrap(),
        NFS3ERR_BADHANDLE
    );

    let mut stale_fh = XdrWriter::new();
    stale_fh.write_opaque(&[0xFF; 16]).unwrap();
    let rep = call_nfs(&svc, 41, NfsProc::GetAttr, stale_fh.as_slice()).await;
    assert_eq!(
        parse_accepted_reply(&rep, 41).read_u32().unwrap(),
        NFS3ERR_STALE
    );
}

#[tokio::test]
async fn all_mutating_nfs_procedures_return_nfs3err_rofs() {
    let svc = NfsService::new(
        Arc::new(MemFs::with_read_only_file("a.txt", b"hello")),
        NfsServerConfig::default(),
    );
    let mutating = [
        (NfsProc::SetAttr, 12usize),
        (NfsProc::Write, 12),
        (NfsProc::Create, 12),
        (NfsProc::Mkdir, 12),
        (NfsProc::Symlink, 12),
        (NfsProc::Mknod, 12),
        (NfsProc::Remove, 12),
        (NfsProc::Rmdir, 12),
        (NfsProc::Rename, 20), // status(4) + 2 * wcc_data(8)
        (NfsProc::Link, 16),   // status(4) + post_op_attr(4) + wcc_data(8)
        (NfsProc::Commit, 12),
    ];

    for (idx, (proc_id, expected_body_len)) in mutating.into_iter().enumerate() {
        assert!(proc_id.is_mutating());
        let xid = 100 + idx as u32;
        let rep = call_nfs(&svc, xid, proc_id, &[]).await;
        let mut r = parse_accepted_reply(&rep, xid);
        assert_eq!(r.remaining(), expected_body_len, "proc {proc_id:?}");
        assert_eq!(r.read_u32().unwrap(), NFS3ERR_ROFS, "proc {proc_id:?}");
    }
}

#[tokio::test]
async fn rpc_stream_drains_inflight_replies_on_client_half_close() {
    let svc = Arc::new(NfsService::new(
        Arc::new(MemFs::with_read_only_file("hi.txt", b"ok")),
        NfsServerConfig::default(),
    ));
    let (client, server) = tokio::io::duplex(8192);
    let (mut c_read, mut c_write) = tokio::io::split(client);
    let (s_read, s_write) = tokio::io::split(server);
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(serve_rpc_stream(s_read, s_write, svc, rx));

    let mut mnt = XdrWriter::new();
    mnt.write_string("/").unwrap();
    let c1 = encode_rpc_call(
        1,
        MOUNT_PROGRAM,
        MOUNT_VERSION,
        MountProc::Mnt as u32,
        &RpcAuth::None,
        mnt.as_slice(),
    )
    .unwrap();
    let c2 = encode_rpc_call(
        2,
        NFS_PROGRAM,
        NFS_VERSION,
        NfsProc::Null as u32,
        &RpcAuth::None,
        &[],
    )
    .unwrap();
    write_rpc_record(&mut c_write, &c1).await.unwrap();
    write_rpc_record(&mut c_write, &c2).await.unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut c_write)
        .await
        .unwrap();
    drop(c_write); // immediate half-close before reading replies

    let mut seen_xids = Vec::new();
    while let Some(rep) = read_rpc_record(&mut c_read).await.unwrap() {
        let mut r = XdrReader::new(&rep).unwrap();
        seen_xids.push(r.read_u32().unwrap());
    }
    seen_xids.sort();
    assert_eq!(seen_xids, vec![1, 2]);
    task.await.unwrap();
}
