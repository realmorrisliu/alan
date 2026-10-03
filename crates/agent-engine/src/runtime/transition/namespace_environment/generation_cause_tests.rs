use super::*;
use alan_ap::{Fid, FileKind, FileServer, InProcessTransport, Qid, Stat};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Notify;

struct CauseFs {
    data: Vec<u8>,
    length: u64,
    mode: &'static str,
    reads: AtomicUsize,
    clunks: AtomicUsize,
    started: Notify,
}
impl CauseFs {
    fn new(data: &[u8], length: u64, mode: &'static str) -> Arc<Self> {
        Arc::new(Self {
            data: data.to_vec(),
            length,
            mode,
            reads: AtomicUsize::new(0),
            clunks: AtomicUsize::new(0),
            started: Notify::new(),
        })
    }
    fn qid() -> Qid {
        Qid {
            kind: FileKind::Stream,
            version: 0,
            path: 1,
        }
    }
    fn client(self: &Arc<Self>) -> NamespaceClient {
        NamespaceClient::new(InProcessTransport::new(self.clone()))
    }
}
#[async_trait::async_trait]
impl FileServer for CauseFs {
    async fn walk(&self, _: Fid, _: Fid, names: &[String]) -> std::result::Result<Qid, ErrorCode> {
        if self.mode == "missing"
            || (self.mode == "missing_events" && names.last().is_some_and(|name| name == "events"))
        {
            Err(ErrorCode::NotFound)
        } else {
            Ok(Self::qid())
        }
    }
    async fn open(&self, _: Fid, _: OpenMode) -> std::result::Result<Qid, ErrorCode> {
        Ok(Self::qid())
    }
    async fn read(
        &self,
        _: Fid,
        offset: u64,
        count: u32,
    ) -> std::result::Result<Vec<u8>, ErrorCode> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        if self.mode == "pending" {
            return std::future::pending().await;
        }
        if self.mode == "error" {
            return Err(ErrorCode::Io);
        }
        // Deliberately short reads; assert the consumer never passes stat's live edge.
        assert!(offset < self.length);
        if self.mode != "stream" {
            assert!(offset + u64::from(count) <= self.length);
        }
        let start = offset as usize;
        let end = (start + 3).min(self.data.len()).min(start + count as usize);
        Ok(self.data.get(start..end).unwrap_or_default().to_vec())
    }
    async fn write(&self, _: Fid, _: u64, _: &[u8]) -> std::result::Result<u32, ErrorCode> {
        Err(ErrorCode::Io)
    }
    async fn stat(&self, _: Fid) -> std::result::Result<Stat, ErrorCode> {
        Ok(Stat {
            name: "events".into(),
            qid: Self::qid(),
            length: self.length,
            executable: false,
            writable: false,
        })
    }
    async fn create(
        &self,
        _: Fid,
        _: Fid,
        _: &str,
        _: FileKind,
    ) -> std::result::Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> std::result::Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn clunk(&self, _: Fid) -> std::result::Result<(), ErrorCode> {
        self.clunks.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
async fn assert_clunks(fs: &CauseFs, expected: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while fs.clunks.load(Ordering::SeqCst) < expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fs.clunks.load(Ordering::SeqCst), expected);
}
const VALID: &[u8] = b"{\"version\":1,\"error\":\"stream_error:authentication\"}\n";

#[tokio::test]
async fn stat_bounded_missing_oversize_bad_and_short_published_causes() {
    for (data, length, mode, reason, reads, clunks) in [
        (
            VALID,
            VALID.len() as u64,
            "missing",
            "stream_error:unknown",
            false,
            0,
        ),
        (
            VALID,
            VALID.len() as u64,
            "missing_events",
            "stream_error:unknown",
            false,
            1,
        ),
        (VALID, 0, "normal", "stream_error:unknown", false, 2),
        (
            VALID,
            MAX_CAUSE_BYTES + 1,
            "normal",
            "stream_error:unknown",
            false,
            2,
        ),
        (
            b"bad\n".as_slice(),
            4,
            "normal",
            "stream_error:unknown",
            true,
            2,
        ),
        (
            VALID,
            VALID.len() as u64 + 1,
            "normal",
            "stream_error:unknown",
            true,
            2,
        ),
        (
            VALID,
            VALID.len() as u64,
            "normal",
            "stream_error:authentication",
            true,
            2,
        ),
    ] {
        let fs = CauseFs::new(data, length, mode);
        let error = commit_generation(&fs.client(), "default", "g0", b"request")
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("llmfs generation failed: {reason}")
        );
        assert_eq!(
            error.downcast_ref::<ErrorCode>(),
            Some(&if mode == "missing" {
                ErrorCode::NotFound
            } else {
                ErrorCode::Io
            })
        );
        assert_eq!(
            crate::retry::is_retryable(&error.context("temporary stream context")),
            reason != "stream_error:authentication"
        );
        assert_eq!(fs.reads.load(Ordering::SeqCst) > 0, reads);
        assert_clunks(&fs, clunks).await;
    }
}

#[tokio::test]
async fn cause_read_error_and_100ms_timeout_clunk_fid_preserving_io() {
    for mode in ["error", "pending"] {
        let fs = CauseFs::new(VALID, VALID.len() as u64, mode);
        let started = tokio::time::Instant::now();
        let error = commit_generation(&fs.client(), "default", "g0", b"request")
            .await
            .unwrap_err();
        if mode == "pending" {
            assert!(started.elapsed() >= CAUSE_TIMEOUT);
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(
            error.to_string(),
            "llmfs generation failed: stream_error:unknown"
        );
        assert_eq!(error.downcast_ref::<ErrorCode>(), Some(&ErrorCode::Io));
        assert_clunks(&fs, 2).await;
    }
}

#[tokio::test]
async fn cancellation_wins_pending_cause_and_clunks_owned_fid() {
    let fs = CauseFs::new(VALID, VALID.len() as u64, "pending");
    let cancel = tokio_util::sync::CancellationToken::new();
    let task = tokio::spawn({
        let fs = fs.clone();
        let cancel = cancel.clone();
        async move {
            let client = fs.client();
            super::super::run_generation_step_with_controls(
                commit_generation(&client, "default", "g0", b"request"),
                &client,
                "default",
                "g0",
                1,
                &cancel,
            )
            .await
        }
    });
    fs.started.notified().await;
    cancel.cancel();
    let error = tokio::time::timeout(CAUSE_TIMEOUT, task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.to_string(), "LLM request cancelled");
    // failed commit data, cancelled cause events, best-effort abort ctl
    assert_clunks(&fs, 3).await;
}

#[tokio::test]
async fn published_event_error_carries_same_typed_retry_decision() {
    for (reason, retryable) in [
        ("authentication", false),
        ("http", false),
        ("safety", false),
        ("recitation", false),
        ("rate_limit", true),
        ("unavailable", true),
        ("timeout", true),
        ("connect", true),
        ("body", true),
        ("closed", true),
        ("parse", true),
        ("unknown", true),
    ] {
        let bytes = format!("{{\"version\":1,\"error\":\"stream_error:{reason}\"}}\n");
        let fs = CauseFs::new(bytes.as_bytes(), bytes.len() as u64, "stream");
        let error = super::super::read_generation_response(&fs.client(), "default", "g0")
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("llmfs generation failed: stream_error:{reason}")
        );
        assert_eq!(error.downcast_ref::<ErrorCode>(), Some(&ErrorCode::Io));
        assert_eq!(
            crate::retry::is_retryable(&error.context("temporary stream")),
            retryable
        );
        assert_clunks(&fs, 1).await;
    }
}

#[test]
fn published_cause_accepts_only_safe_complete_error_events() {
    assert_eq!(published_cause(VALID), "stream_error:authentication");
    for bytes in [
        b"{\"version\":1,\"error\":\"secret https://private.example\"}\n".as_slice(),
        b"{\"version\":1,\"error\":\"stop\"}\n",
        b"{\"version\":2,\"error\":\"stream_error:parse\"}\n",
        b"{\"version\":1,\"error\":\"stream_error:parse\"}",
        b"invalid\n",
        b"",
    ] {
        assert_eq!(published_cause(bytes), "stream_error:unknown");
    }
}
