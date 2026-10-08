//! Evaluation advice is a runtime-owned snapshot, never a writable decision file.
use alan_agentfs::AgentFs;
use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use serde_json::json;

#[tokio::test]
async fn evaluation_projection_is_read_only_bounded_and_snapshot_consistent() {
    let fs = AgentFs::new();
    let names = ["machine".into(), "evaluation".into()];
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
        json!({"version":1,"observation":null})
    );
    fs.publish_evaluation_observation(None).await.unwrap();
    assert_eq!(fs.stat(Fid(1)).await.unwrap().qid.version, initial_version);
    assert_eq!(
        fs.write(Fid(1), 0, b"forged").await,
        Err(ErrorCode::NoAccess)
    );

    // Partial reads retain one snapshot even while the runtime publishes another.
    let prefix = fs.read(Fid(1), 0, 7).await.unwrap();
    let observation = json!({"id":"source:input-1", "state":"interrupted"});
    fs.publish_evaluation_observation(Some(observation.clone()))
        .await
        .unwrap();
    let mut old_read = prefix;
    old_read.extend(fs.read(Fid(1), 7, 4096).await.unwrap());
    assert_eq!(old_read, initial);
    let current = fs.read(Fid(1), 0, 4096).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&current).unwrap(),
        json!({"version":1,"observation":observation})
    );
    let published_version = fs.stat(Fid(1)).await.unwrap().qid.version;
    assert_ne!(published_version, initial_version);
    fs.publish_evaluation_observation(Some(observation))
        .await
        .unwrap();
    assert_eq!(
        fs.stat(Fid(1)).await.unwrap().qid.version,
        published_version
    );

    // Rejected publications cannot erase the last acknowledged observation.
    for value in [json!("invalid"), json!({"data":"x".repeat(1 << 20)})] {
        assert_eq!(
            fs.publish_evaluation_observation(Some(value)).await,
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
    assert_eq!(fs.read(Fid(2), 0, 4096).await.unwrap(), b"evaluation\n");
    fs.clunk(Fid(1)).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
}
