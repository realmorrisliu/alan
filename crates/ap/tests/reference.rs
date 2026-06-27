//! End-to-end conformance of the aP conventions against the reference in-memory
//! server (`alan_ap::reference::MemFs`). The reference server is the worked
//! example that gives the fid lifecycle (§5.2), clone-via-open (§5.4), and the
//! three-phase error model (§5.5) teeth, and the template downstream file
//! servers (and alan-shell's M1 echo milestone) build from.

use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{ErrorCode, Fid, InProcessTransport, OpenMode, Request, Response};

fn transport() -> InProcessTransport {
    InProcessTransport::new(Arc::new(MemFs::new()))
}

// §5.2 — fid lifecycle: walk allocates a fid, clunk releases it, and a released
// fid is no longer usable.
#[tokio::test]
async fn fid_is_allocated_by_walk_and_released_by_clunk() {
    let t = transport();

    // walk from the well-known root binds a fresh fid to "greeting".
    let walked = t
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(10),
            names: vec!["greeting".into()],
        })
        .await;
    assert!(
        matches!(walked, Ok(Response::Walk { .. })),
        "walk should allocate the fid: {walked:?}"
    );

    t.call(Request::Open {
        fid: Fid(10),
        mode: OpenMode::Read,
    })
    .await
    .unwrap();
    let read = t
        .call(Request::Read {
            fid: Fid(10),
            offset: 0,
            count: 64,
        })
        .await;
    assert_eq!(
        read,
        Ok(Response::Read {
            data: b"hi".to_vec()
        })
    );

    // After clunk the fid is gone; reusing it is an error, not stale data.
    assert_eq!(
        t.call(Request::Clunk { fid: Fid(10) }).await,
        Ok(Response::Clunk)
    );
    let after = t
        .call(Request::Read {
            fid: Fid(10),
            offset: 0,
            count: 64,
        })
        .await;
    assert_eq!(after, Err(ErrorCode::NotFound));
}

// §5.4 — clone-via-open: opening the clone file allocates a new resource and its
// name is read back; two callers get independent resources.
#[tokio::test]
async fn clone_via_open_yields_independent_resources() {
    let t = transport();

    let open_clone = |fid: Fid| {
        let t = t.clone();
        async move {
            t.call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: vec!["clone".into()],
            })
            .await
            .unwrap();
            t.call(Request::Open {
                fid,
                mode: OpenMode::Read,
            })
            .await
            .unwrap();
            // Reading the clone fid returns the allocated resource's name.
            let Response::Read { data } = t
                .call(Request::Read {
                    fid,
                    offset: 0,
                    count: 64,
                })
                .await
                .unwrap()
            else {
                panic!("expected read response");
            };
            String::from_utf8(data).unwrap()
        }
    };

    let first = open_clone(Fid(20)).await;
    let second = open_clone(Fid(21)).await;
    assert_ne!(
        first, second,
        "two clone opens must allocate distinct resources"
    );

    // Each allocated resource is a real, walkable child carrying its own id.
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(30),
        names: vec![first.clone(), "id".into()],
    })
    .await
    .unwrap();
    t.call(Request::Open {
        fid: Fid(30),
        mode: OpenMode::Read,
    })
    .await
    .unwrap();
    let Response::Read { data } = t
        .call(Request::Read {
            fid: Fid(30),
            offset: 0,
            count: 64,
        })
        .await
        .unwrap()
    else {
        panic!("expected read response");
    };
    assert_eq!(String::from_utf8(data).unwrap(), first);
}

// §5.5 — dial-time failure: a denied/absent open returns an operation error and
// starts no interaction.
#[tokio::test]
async fn dial_time_failure_returns_open_error() {
    let t = transport();
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(40),
        names: vec!["nope".into()],
    })
    .await
    .expect_err("walking a missing name fails");
}

// §5.2 — a fid is a handle to one interaction: walk must not rebind the reserved
// root or an already-live fid (PR #573 review).
#[tokio::test]
async fn walk_rejects_rebinding_reserved_or_live_fids() {
    let t = transport();

    // Rebinding the well-known root is rejected (would corrupt the whole server).
    assert_eq!(
        t.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid::ROOT,
            names: vec!["greeting".into()]
        })
        .await,
        Err(ErrorCode::BadRequest)
    );

    // A second walk onto an already-live fid is rejected, not a silent clobber.
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(60),
        names: vec!["greeting".into()],
    })
    .await
    .unwrap();
    assert_eq!(
        t.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(60),
            names: vec!["clone".into()]
        })
        .await,
        Err(ErrorCode::BadRequest)
    );
}

// §5.5 — a write without write authority must be rejected, not silently buffered
// and skipped at commit (PR #573 review).
#[tokio::test]
async fn write_without_write_intent_is_rejected() {
    let t = transport();
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(70),
        names: vec!["submit".into()],
    })
    .await
    .unwrap();

    // No open at all: a write is denied.
    assert_eq!(
        t.call(Request::Write {
            fid: Fid(70),
            offset: 0,
            data: b"{}".to_vec()
        })
        .await,
        Err(ErrorCode::NoAccess)
    );

    // Opened read-only: still denied (cannot escalate to write).
    t.call(Request::Open {
        fid: Fid(70),
        mode: OpenMode::Read,
    })
    .await
    .unwrap();
    assert_eq!(
        t.call(Request::Write {
            fid: Fid(70),
            offset: 0,
            data: b"{}".to_vec()
        })
        .await,
        Err(ErrorCode::NoAccess)
    );
}

// §5.2 — reopening a live fid is rejected, so a second open cannot downgrade
// write intent and bypass commit-time validation (PR #573 review).
#[tokio::test]
async fn reopening_a_live_fid_is_rejected() {
    let t = transport();
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(80),
        names: vec!["submit".into()],
    })
    .await
    .unwrap();
    t.call(Request::Open {
        fid: Fid(80),
        mode: OpenMode::Write,
    })
    .await
    .unwrap();
    t.call(Request::Write {
        fid: Fid(80),
        offset: 0,
        data: b"{ truncated".to_vec(),
    })
    .await
    .unwrap();

    // A second open (e.g. read) on the same fid must not succeed and clobber mode.
    assert_eq!(
        t.call(Request::Open {
            fid: Fid(80),
            mode: OpenMode::Read
        })
        .await,
        Err(ErrorCode::BadRequest)
    );
    // The malformed document is still rejected at clunk — validation not bypassed.
    assert_eq!(
        t.call(Request::Clunk { fid: Fid(80) }).await,
        Err(ErrorCode::BadRequest)
    );
}

// The document write honors the byte offset, so overwriting a chunk at a lower
// offset changes the committed document (PR #573 review).
#[tokio::test]
async fn document_writes_honor_offset() {
    let t = transport();
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(90),
        names: vec!["submit".into()],
    })
    .await
    .unwrap();
    t.call(Request::Open {
        fid: Fid(90),
        mode: OpenMode::Write,
    })
    .await
    .unwrap();
    // Placeholder `{"ok":zzzz}` (11 bytes), then overwrite the 4-byte value at
    // offset 6 with `true` → `{"ok":true}`.
    t.call(Request::Write {
        fid: Fid(90),
        offset: 0,
        data: br#"{"ok":zzzz}"#.to_vec(),
    })
    .await
    .unwrap();
    t.call(Request::Write {
        fid: Fid(90),
        offset: 6,
        data: b"true".to_vec(),
    })
    .await
    .unwrap();
    // Offset-correct overwrite yields valid JSON → commits cleanly. (An append-
    // only buffer would instead build `{"ok":zzzz}true` and fail validation.)
    assert_eq!(
        t.call(Request::Clunk { fid: Fid(90) }).await,
        Ok(Response::Clunk)
    );
}

// A huge/overflowing write offset returns an aP error instead of panicking the
// in-process server (PR #573 review).
#[tokio::test]
async fn overflowing_write_offset_is_rejected_not_panicked() {
    let t = transport();
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(100),
        names: vec!["submit".into()],
    })
    .await
    .unwrap();
    t.call(Request::Open {
        fid: Fid(100),
        mode: OpenMode::Write,
    })
    .await
    .unwrap();
    assert_eq!(
        t.call(Request::Write {
            fid: Fid(100),
            offset: u64::MAX,
            data: b"x".to_vec()
        })
        .await,
        Err(ErrorCode::BadRequest)
    );
}

// Root is the reusable anchor: opening then clunking it must leave it openable
// again, not locked out for the server's lifetime (PR #573 review).
#[tokio::test]
async fn root_fid_can_be_opened_again_after_clunk() {
    let t = transport();
    t.call(Request::Open {
        fid: Fid::ROOT,
        mode: OpenMode::Read,
    })
    .await
    .unwrap();
    assert_eq!(
        t.call(Request::Clunk { fid: Fid::ROOT }).await,
        Ok(Response::Clunk)
    );
    // Reopening root succeeds — its per-open state was cleared on clunk.
    t.call(Request::Open {
        fid: Fid::ROOT,
        mode: OpenMode::Read,
    })
    .await
    .expect("root remains the reusable anchor after clunk");
    // And it still resolves walks afterward.
    t.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(101),
        names: vec!["greeting".into()],
    })
    .await
    .expect("root still anchors walks");
}

// §5.5 — commit-time failure: a document write commits on clunk and a malformed
// document is rejected at clunk, distinct from a dial-time error.
#[tokio::test]
async fn commit_time_failure_is_reported_at_clunk() {
    let t = transport();

    // "submit" is a commit-on-clunk document file expecting valid JSON.
    let walk_submit = |fid: Fid| {
        let t = t.clone();
        async move {
            t.call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: vec!["submit".into()],
            })
            .await
            .unwrap();
            t.call(Request::Open {
                fid,
                mode: OpenMode::Write,
            })
            .await
            .unwrap();
        }
    };

    // Valid document: writes across two chunks, commits cleanly on clunk.
    walk_submit(Fid(50)).await;
    t.call(Request::Write {
        fid: Fid(50),
        offset: 0,
        data: b"{\"ok\":".to_vec(),
    })
    .await
    .unwrap();
    t.call(Request::Write {
        fid: Fid(50),
        offset: 6,
        data: b"true}".to_vec(),
    })
    .await
    .unwrap();
    assert_eq!(
        t.call(Request::Clunk { fid: Fid(50) }).await,
        Ok(Response::Clunk)
    );

    // Malformed document: writes succeed, the rejection lands at clunk.
    walk_submit(Fid(51)).await;
    t.call(Request::Write {
        fid: Fid(51),
        offset: 0,
        data: b"{ truncated".to_vec(),
    })
    .await
    .expect("partial writes are accepted; commit has not happened yet");
    assert_eq!(
        t.call(Request::Clunk { fid: Fid(51) }).await,
        Err(ErrorCode::BadRequest)
    );
}
