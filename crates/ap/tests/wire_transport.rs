//! aP byte transport: framed requests/results, export loop, and imported trees.

use std::sync::Arc;
use std::time::Duration;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, ImportedFileServer, MAX_WIRE_FRAME_BYTES, Offset,
    OpenMode, Qid, Request, Response, Stat, Stream, decode_request_frame, decode_response_frame,
    encode_request_frame, encode_response_frame, export_file_server,
};
use tokio::io::{BufReader, duplex};
use tokio::sync::{Mutex, Notify};

fn qid(kind: FileKind, path: u64) -> Qid {
    Qid {
        kind,
        version: 0,
        path,
    }
}

#[test]
fn every_request_and_response_result_survives_byte_framing() {
    let requests = [
        Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: vec!["agent".into(), "root".into()],
        },
        Request::Open {
            fid: Fid(1),
            mode: OpenMode::Read,
        },
        Request::Read {
            fid: Fid(1),
            offset: 7,
            count: 128,
        },
        Request::Write {
            fid: Fid(1),
            offset: 0,
            data: b"payload".to_vec(),
        },
        Request::Stat { fid: Fid(1) },
        Request::Create {
            fid: Fid::ROOT,
            newfid: Fid(2),
            name: "child".into(),
            kind: FileKind::File,
        },
        Request::Remove { fid: Fid(2) },
        Request::Clunk { fid: Fid(1) },
    ];

    for request in requests {
        let frame = encode_request_frame(&request).expect("encode request frame");
        assert_eq!(frame.last(), Some(&b'\n'));
        assert_eq!(
            decode_request_frame(&frame).expect("decode request frame"),
            request
        );
    }

    let stat = Stat {
        name: "file".into(),
        qid: qid(FileKind::File, 1),
        length: 7,
        writable: true,
    };
    let responses = [
        Ok(Response::Walk {
            qid: qid(FileKind::Dir, 10),
        }),
        Ok(Response::Open {
            qid: qid(FileKind::File, 1),
        }),
        Ok(Response::Read {
            data: b"payload".to_vec(),
        }),
        Ok(Response::Write { count: 7 }),
        Ok(Response::Stat { stat }),
        Ok(Response::Create {
            qid: qid(FileKind::File, 2),
        }),
        Ok(Response::Remove),
        Ok(Response::Clunk),
        Err(ErrorCode::NoAccess),
        Err(ErrorCode::BadRequest),
        Err(ErrorCode::Io),
    ];

    for response in responses {
        let frame = encode_response_frame(&response).expect("encode response frame");
        assert_eq!(frame.last(), Some(&b'\n'));
        assert_eq!(
            decode_response_frame(&frame).expect("decode response frame"),
            response
        );
    }
}

struct OneFile;

#[async_trait::async_trait]
impl FileServer for OneFile {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT && newfid == Fid(1) && names == ["file"] {
            Ok(qid(FileKind::File, 1))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid(1) {
            Ok(qid(FileKind::File, 1))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if fid != Fid(1) {
            return Err(ErrorCode::NotFound);
        }
        let bytes = b"hello over aP";
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if fid == Fid(1) {
            Ok(Stat {
                name: "file".into(),
                qid: qid(FileKind::File, 1),
                length: b"hello over aP".len() as u64,
                writable: false,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
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
async fn imported_tree_dispatches_to_exported_server() {
    let (client_stream, server_stream) = duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let server: Arc<dyn FileServer> = Arc::new(OneFile);
    let server_task = tokio::spawn(export_file_server(
        server,
        BufReader::new(server_read),
        server_write,
    ));
    let imported = ImportedFileServer::new(BufReader::new(client_read), client_write);

    let walked = imported
        .walk(Fid::ROOT, Fid(1), &["file".to_string()])
        .await;
    assert_eq!(walked, Ok(qid(FileKind::File, 1)));

    let opened = imported.open(Fid(1), OpenMode::Read).await;
    assert_eq!(opened, Ok(qid(FileKind::File, 1)));

    let read = imported.read(Fid(1), 6, 128).await;
    assert_eq!(read, Ok(b"over aP".to_vec()));

    let denied = imported.write(Fid(1), 0, b"x").await;
    assert_eq!(denied, Err(ErrorCode::NoAccess));

    drop(imported);
    server_task.abort();
}

struct StreamFile {
    stream: Stream,
    read_started: Option<Arc<Notify>>,
}

#[async_trait::async_trait]
impl FileServer for StreamFile {
    async fn walk(&self, _fid: Fid, _newfid: Fid, _names: &[String]) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NotFound)
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid(1) {
            Ok(qid(FileKind::Stream, 1))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if fid == Fid(1) {
            if let Some(read_started) = &self.read_started {
                read_started.notify_one();
            }
            Ok(self.stream.read(offset, count).await)
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if fid == Fid(1) {
            Ok(Stat {
                name: "stream".into(),
                qid: qid(FileKind::Stream, 1),
                length: self.stream.len().await,
                writable: false,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
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
async fn imported_stream_read_blocks_until_remote_stream_has_data() {
    let (client_stream, server_stream) = duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let stream = Stream::new();
    let server: Arc<dyn FileServer> = Arc::new(StreamFile {
        stream: stream.clone(),
        read_started: None,
    });
    let server_task = tokio::spawn(export_file_server(
        server,
        BufReader::new(server_read),
        server_write,
    ));
    let imported = Arc::new(ImportedFileServer::new(
        BufReader::new(client_read),
        client_write,
    ));

    imported.open(Fid(1), OpenMode::Read).await.unwrap();

    let reader = Arc::clone(&imported);
    let handle = tokio::spawn(async move { reader.read(Fid(1), 0, 64).await });

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !handle.is_finished(),
        "imported stream read returned before remote stream had data"
    );

    stream.append(b"late bytes").await;
    let got = tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .expect("imported stream read did not wake")
        .expect("reader task panicked");
    assert_eq!(got, Ok(b"late bytes".to_vec()));

    drop(imported);
    server_task.abort();
}

#[tokio::test]
async fn cancelled_in_flight_call_does_not_desynchronize_next_request() {
    let (client_stream, server_stream) = duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let stream = Stream::new();
    let read_started = Arc::new(Notify::new());
    let server: Arc<dyn FileServer> = Arc::new(StreamFile {
        stream: stream.clone(),
        read_started: Some(Arc::clone(&read_started)),
    });
    let server_task = tokio::spawn(export_file_server(
        server,
        BufReader::new(server_read),
        server_write,
    ));
    let imported = Arc::new(ImportedFileServer::new(
        BufReader::new(client_read),
        client_write,
    ));

    imported.open(Fid(1), OpenMode::Read).await.unwrap();
    let reader = Arc::clone(&imported);
    let handle = tokio::spawn(async move { reader.read(Fid(1), 0, 64).await });
    tokio::time::timeout(Duration::from_millis(500), read_started.notified())
        .await
        .expect("remote read request did not reach exported server");

    handle.abort();
    assert!(
        handle
            .await
            .expect_err("reader task should have been cancelled")
            .is_cancelled()
    );

    stream.append(b"abandoned response").await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(imported.stat(Fid(1)).await, Err(ErrorCode::Io));

    drop(imported);
    server_task.abort();
}

struct BlockingReadFile {
    read_started: Arc<Notify>,
    read_dropped: Arc<Notify>,
}

#[async_trait::async_trait]
impl FileServer for BlockingReadFile {
    async fn walk(&self, _fid: Fid, _newfid: Fid, _names: &[String]) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NotFound)
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: Offset, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        struct DropNotify(Arc<Notify>);

        impl Drop for DropNotify {
            fn drop(&mut self) {
                self.0.notify_one();
            }
        }

        self.read_started.notify_one();
        let _drop_notify = DropNotify(Arc::clone(&self.read_dropped));
        std::future::pending().await
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
async fn export_cancels_blocking_call_when_peer_disconnects() {
    let (client_stream, server_stream) = duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let read_started = Arc::new(Notify::new());
    let read_dropped = Arc::new(Notify::new());
    let server: Arc<dyn FileServer> = Arc::new(BlockingReadFile {
        read_started: Arc::clone(&read_started),
        read_dropped: Arc::clone(&read_dropped),
    });
    let server_task = tokio::spawn(export_file_server(
        server,
        BufReader::new(server_read),
        server_write,
    ));
    let imported = Arc::new(ImportedFileServer::new(
        BufReader::new(client_read),
        client_write,
    ));

    let reader = Arc::clone(&imported);
    let read_task = tokio::spawn(async move { reader.read(Fid(1), 0, 64).await });
    tokio::time::timeout(Duration::from_millis(500), read_started.notified())
        .await
        .expect("remote read request did not reach exported server");

    drop(imported);
    read_task.abort();
    let _ = read_task.await;

    tokio::time::timeout(Duration::from_millis(500), read_dropped.notified())
        .await
        .expect("exported blocking read was not cancelled after peer disconnect");
    let result = tokio::time::timeout(Duration::from_millis(500), server_task)
        .await
        .expect("export task did not exit after peer disconnect")
        .expect("export task panicked");
    assert!(result.is_ok(), "{result:?}");
}

struct RecordingWriteFile {
    writes: Mutex<Vec<(Offset, Vec<u8>)>>,
}

#[async_trait::async_trait]
impl FileServer for RecordingWriteFile {
    async fn walk(&self, _fid: Fid, _newfid: Fid, _names: &[String]) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NotFound)
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: Offset, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn write(&self, _fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.writes.lock().await.push((offset, data.to_vec()));
        Ok(data.len() as u32)
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
async fn imported_large_write_is_split_into_bounded_remote_frames() {
    let (client_stream, server_stream) = duplex(1024 * 1024);
    let (client_read, client_write) = tokio::io::split(client_stream);
    let (server_read, server_write) = tokio::io::split(server_stream);

    let server = Arc::new(RecordingWriteFile {
        writes: Mutex::new(Vec::new()),
    });
    let server_task = tokio::spawn(export_file_server(
        server.clone(),
        BufReader::new(server_read),
        server_write,
    ));
    let imported = ImportedFileServer::new(BufReader::new(client_read), client_write);
    let data = vec![255; MAX_WIRE_FRAME_BYTES];

    assert_eq!(
        imported.write(Fid(1), 7, &data).await,
        Ok(data.len() as u32)
    );

    let writes = server.writes.lock().await;
    assert!(
        writes.len() > 1,
        "large imported write should be split into multiple bounded frames"
    );
    let mut next_offset = 7;
    let mut reconstructed = Vec::new();
    for (offset, chunk) in writes.iter() {
        assert_eq!(*offset, next_offset);
        next_offset += chunk.len() as u64;
        reconstructed.extend_from_slice(chunk);
    }
    assert_eq!(reconstructed, data);

    drop(imported);
    server_task.abort();
}
