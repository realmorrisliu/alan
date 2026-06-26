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
