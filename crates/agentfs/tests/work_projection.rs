//! Work completion is a runtime-owned snapshot, never a writable decision file.
use alan_agentfs::AgentFs;
use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use serde_json::json;

#[tokio::test]
async fn work_projection_is_read_only_bounded_and_snapshot_consistent() {
    let fs = AgentFs::new();
    let names = ["machine".into(), "work".into()];
    fs.walk(Fid::ROOT, Fid(1), &names).await.unwrap();
    let initial_version = fs.stat(Fid(1)).await.unwrap().qid.version;
    assert!(!fs.stat(Fid(1)).await.unwrap().writable);
    assert_eq!(
        fs.write(Fid(1), 0, b"forged").await,
        Err(ErrorCode::NoAccess)
    );
    for mode in [OpenMode::Write, OpenMode::ReadWrite] {
        assert_eq!(fs.open(Fid(1), mode).await, Err(ErrorCode::NoAccess));
    }
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    let initial = fs.read(Fid(1), 0, 4096).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&initial).unwrap(),
        json!({"version":1,"work":null})
    );
    fs.publish_work(None).await.unwrap();
    assert_eq!(fs.stat(Fid(1)).await.unwrap().qid.version, initial_version);
    assert_eq!(
        fs.write(Fid(1), 0, b"forged").await,
        Err(ErrorCode::NoAccess)
    );

    // Partial reads retain one snapshot even while the runtime publishes another.
    let prefix = fs.read(Fid(1), 0, 7).await.unwrap();
    let work = json!({"id":"source:input-1", "state":"interrupted"});
    fs.publish_work(Some(work.clone())).await.unwrap();
    let mut old_read = prefix;
    old_read.extend(fs.read(Fid(1), 7, 4096).await.unwrap());
    assert_eq!(old_read, initial);
    let current = fs.read(Fid(1), 0, 4096).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&current).unwrap(),
        json!({"version":1,"work":work})
    );
    let published_version = fs.stat(Fid(1)).await.unwrap().qid.version;
    assert_ne!(published_version, initial_version);
    fs.publish_work(Some(work)).await.unwrap();
    assert_eq!(
        fs.stat(Fid(1)).await.unwrap().qid.version,
        published_version
    );

    // Rejected publications cannot erase the last acknowledged work.
    for value in [json!("invalid"), json!({"data":"x".repeat(1 << 20)})] {
        assert_eq!(
            fs.publish_work(Some(value)).await,
            Err(ErrorCode::BadRequest)
        );
    }
    assert_eq!(fs.read(Fid(1), 0, 4096).await.unwrap(), current);
    assert_eq!(
        fs.stat(Fid(1)).await.unwrap().qid.version,
        published_version
    );
    fs.walk(Fid::ROOT, Fid(2), &["events".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(2), 0, 4096).await.unwrap(), b"work\n");
    fs.clunk(Fid(1)).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
}

#[tokio::test]
async fn restored_request_preserves_identity_and_never_overwrites_a_response() {
    let fs = AgentFs::new();
    fs.restore_pending_request("r7", "structured_input", "choose", "{}")
        .await
        .unwrap();
    fs.restore_pending_request("r7", "structured_input", "choose", "{}")
        .await
        .unwrap();
    assert_eq!(
        fs.restore_pending_request("r7", "structured_input", "changed", "{}")
            .await,
        Err(ErrorCode::BadRequest)
    );
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["requests".into(), "r7".into(), "response".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"hostfs").await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();
    fs.restore_pending_request("r7", "structured_input", "choose", "{}")
        .await
        .unwrap();
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &["requests".into(), "r7".into(), "status".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(2), 0, 100).await.unwrap(), b"answered");
    fs.walk(Fid::ROOT, Fid(3), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(3), OpenMode::ReadWrite).await.unwrap();
    assert_eq!(fs.read(Fid(3), 0, 100).await.unwrap(), b"r8");
    for id in ["r07", "r18446744073709551615", "r-1", "r7/response"] {
        assert_eq!(
            fs.restore_pending_request(id, "structured_input", "choose", "{}")
                .await,
            Err(ErrorCode::BadRequest)
        );
    }
}
