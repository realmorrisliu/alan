//! aP wire-shape conformance (substrate §5.7).
//!
//! Proves a dumb byte transport could carry every value type and every
//! operation unchanged: each type survives a JSON serialize → deserialize round
//! trip. If any aP type stops being wire-shaped (borrows, non-serializable
//! fields), these tests fail before any wire transport exists.

use std::io::Cursor;

use alan_ap::{
    ErrorCode, Fid, FileKind, MAX_WIRE_FRAME_BYTES, OpenMode, Qid, Request, Response, Stat,
    WireError, encode_request_frame, read_request_frame,
};
use tokio::io::BufReader;

fn roundtrip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let bytes = serde_json::to_vec(value).expect("serialize");
    serde_json::from_slice(&bytes).expect("deserialize")
}

#[test]
fn qid_survives_roundtrip() {
    let qid = Qid {
        kind: FileKind::Stream,
        version: 7,
        path: 0xdead_beef,
    };
    assert_eq!(roundtrip(&qid), qid);
}

#[test]
fn file_kinds_survive_roundtrip() {
    for kind in [
        FileKind::Dir,
        FileKind::File,
        FileKind::Stream,
        FileKind::Clone,
    ] {
        assert_eq!(roundtrip(&kind), kind);
    }
}

#[test]
fn open_modes_survive_roundtrip() {
    for mode in [OpenMode::Read, OpenMode::Write, OpenMode::ReadWrite] {
        assert_eq!(roundtrip(&mode), mode);
    }
}

#[test]
fn error_codes_survive_roundtrip() {
    for code in [
        ErrorCode::NotFound,
        ErrorCode::NoAccess,
        ErrorCode::RateLimited,
        ErrorCode::BadRequest,
        ErrorCode::NotDirectory,
        ErrorCode::IsDirectory,
        ErrorCode::Unsupported,
        ErrorCode::Io,
    ] {
        assert_eq!(roundtrip(&code), code);
    }
}

#[test]
fn stat_survives_roundtrip() {
    let stat = Stat {
        name: "output".to_string(),
        qid: Qid {
            kind: FileKind::Stream,
            version: 1,
            path: 42,
        },
        length: 1024,
        writable: false,
    };
    assert_eq!(roundtrip(&stat), stat);
}

#[test]
fn every_request_operation_survives_roundtrip() {
    let requests = [
        Request::Walk {
            fid: Fid(1),
            newfid: Fid(2),
            names: vec!["proc".into(), "7".into()],
        },
        Request::Open {
            fid: Fid(2),
            mode: OpenMode::Read,
        },
        Request::Read {
            fid: Fid(2),
            offset: 0,
            count: 4096,
        },
        Request::Write {
            fid: Fid(2),
            offset: 0,
            data: b"hello".to_vec(),
        },
        Request::Stat { fid: Fid(2) },
        Request::Create {
            fid: Fid(2),
            newfid: Fid(3),
            name: "child".into(),
            kind: FileKind::File,
        },
        Request::Remove { fid: Fid(2) },
        Request::Clunk { fid: Fid(2) },
    ];
    for req in requests {
        assert_eq!(roundtrip(&req), req);
    }
}

#[test]
fn every_response_operation_survives_roundtrip() {
    let qid = Qid {
        kind: FileKind::File,
        version: 0,
        path: 9,
    };
    let responses = [
        Response::Walk { qid },
        Response::Open { qid },
        Response::Read {
            data: b"hello".to_vec(),
        },
        Response::Write { count: 5 },
        Response::Stat {
            stat: Stat {
                name: "child".into(),
                qid,
                length: 5,
                writable: true,
            },
        },
        Response::Create { qid },
        Response::Remove,
        Response::Clunk,
    ];
    for resp in responses {
        assert_eq!(roundtrip(&resp), resp);
    }
}

#[tokio::test]
async fn oversized_request_frame_without_newline_is_rejected() {
    let mut reader = BufReader::new(Cursor::new(vec![b'x'; MAX_WIRE_FRAME_BYTES + 1]));

    assert!(matches!(
        read_request_frame(&mut reader).await,
        Err(WireError::FrameTooLarge {
            max: MAX_WIRE_FRAME_BYTES
        })
    ));
}

#[tokio::test]
async fn oversized_request_frame_with_newline_is_rejected() {
    let mut frame = vec![b'x'; MAX_WIRE_FRAME_BYTES + 1];
    frame.push(b'\n');
    let mut reader = BufReader::new(Cursor::new(frame));

    assert!(matches!(
        read_request_frame(&mut reader).await,
        Err(WireError::FrameTooLarge {
            max: MAX_WIRE_FRAME_BYTES
        })
    ));
}

#[tokio::test]
async fn request_frame_without_newline_is_rejected() {
    let mut frame = encode_request_frame(&Request::Stat { fid: Fid(1) }).unwrap();
    assert_eq!(frame.pop(), Some(b'\n'));
    let mut reader = BufReader::new(Cursor::new(frame));

    assert!(matches!(
        read_request_frame(&mut reader).await,
        Err(WireError::TruncatedFrame)
    ));
}
