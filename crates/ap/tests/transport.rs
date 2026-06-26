//! In-process fast-path transport (substrate §5.6): the transport dispatches an
//! aP `Request` to a `FileServer` and returns its `Response` without
//! serializing, so high-rate streams pay no protocol cost. These tests pin the
//! dispatch mapping (each `Request` variant → the matching typed method → the
//! matching `Response`) and that typed errors propagate as `ErrorCode`.

use std::sync::Arc;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Request,
    Response, Stat,
};

/// Minimal server: one readable file at fid 1 holding `b"hi"`; everything else
/// is `NotFound`. Just enough to prove the transport's dispatch wiring.
struct OneFile;

#[async_trait::async_trait]
impl FileServer for OneFile {
    async fn walk(&self, _fid: Fid, _newfid: Fid, _names: &[String]) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid(1) {
            Ok(Qid {
                kind: FileKind::File,
                version: 0,
                path: 1,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
    }
    async fn read(&self, fid: Fid, offset: Offset, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        if fid == Fid(1) {
            Ok(b"hi".to_vec().split_off(offset as usize))
        } else {
            Err(ErrorCode::NotFound)
        }
    }
    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Err(ErrorCode::NotFound)
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
    async fn clunk(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

#[tokio::test]
async fn transport_dispatches_read_to_the_server() {
    let transport = InProcessTransport::new(Arc::new(OneFile));

    let opened = transport
        .call(Request::Open {
            fid: Fid(1),
            mode: OpenMode::Read,
        })
        .await;
    assert_eq!(
        opened,
        Ok(Response::Open {
            qid: Qid {
                kind: FileKind::File,
                version: 0,
                path: 1
            }
        })
    );

    let read = transport
        .call(Request::Read {
            fid: Fid(1),
            offset: 0,
            count: 16,
        })
        .await;
    assert_eq!(
        read,
        Ok(Response::Read {
            data: b"hi".to_vec()
        })
    );
}

#[tokio::test]
async fn transport_propagates_typed_errors() {
    let transport = InProcessTransport::new(Arc::new(OneFile));

    let denied = transport
        .call(Request::Write {
            fid: Fid(1),
            offset: 0,
            data: b"x".to_vec(),
        })
        .await;
    assert_eq!(denied, Err(ErrorCode::NoAccess));

    let missing = transport
        .call(Request::Open {
            fid: Fid(9),
            mode: OpenMode::Read,
        })
        .await;
    assert_eq!(missing, Err(ErrorCode::NotFound));
}
