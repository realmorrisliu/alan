use alan_agentfs::AgentFs;
use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};

const KNOWN: &[u8] = br#"{"version":1,"publication_version":7,"process_path":"/proc/3","known":true,"mentionable_skill_ids":["release-check"]}"#;

async fn open(fs: &AgentFs, fid: u64, mode: OpenMode) {
    fs.walk(
        Fid::ROOT,
        Fid(fid),
        &["machine".into(), "ui".into(), "skills".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(fid), mode).await.unwrap();
}

#[tokio::test]
async fn public_skill_writes_replace_entire_projection_and_invalidate() {
    for (payload, overflow, buffered) in [
        (b"{".to_vec(), false, false),
        (br#"{"host_path":"private"}"#.to_vec(), false, false),
        (br#"{"version":1,"publication_version":7,"process_path":"/proc/3","known":"true","mentionable_skill_ids":[]}"#.to_vec(), false, false),
        (vec![0xff], false, false),
        (vec![b'x'; (1 << 20) + 1], true, false),
        (vec![b'x'; (1 << 20) + 1], true, true),
    ] {
        let fs = AgentFs::new();
        open(&fs, 1, OpenMode::Read).await;
        open(&fs, 2, OpenMode::Write).await;
        fs.write(Fid(2), 0, KNOWN).await.unwrap();
        fs.clunk(Fid(2)).await.unwrap();
        let known: serde_json::Value = serde_json::from_slice(&fs.read(Fid(1), 0, 4096).await.unwrap()).unwrap();
        assert_eq!(known["known"], true);
        assert_eq!(known["mentionable_skill_ids"][0], "release-check");
        let before = fs.stat(Fid(1)).await.unwrap().qid.version;
        open(&fs, 3, OpenMode::Write).await;
        if buffered { fs.write(Fid(3), 0, KNOWN).await.unwrap(); }
        let result = fs.write(Fid(3), 0, &payload).await;
        if overflow { assert_eq!(result, Err(ErrorCode::BadRequest)); } else { result.unwrap(); }
        // Buffered ordinary writes do not publish before clunk, even on overflow.
        assert_eq!(fs.stat(Fid(1)).await.unwrap().qid.version, before);
        fs.clunk(Fid(3)).await.unwrap();
        assert!(fs.stat(Fid(1)).await.unwrap().qid.version > before);
        let bytes = fs.read(Fid(1), 0, 4096).await.unwrap();
        let unknown: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(unknown["known"], false);
        assert_eq!(unknown["mentionable_skill_ids"], serde_json::json!([]));
        assert!(!String::from_utf8(bytes).unwrap().contains("private"));
        fs.walk(Fid::ROOT, Fid(4), &["events".into()]).await.unwrap();
        fs.open(Fid(4), OpenMode::Read).await.unwrap();
        assert_eq!(fs.read(Fid(4), 0, 4096).await.unwrap(), b"ui:skills\nui:skills\n");
    }
}

#[tokio::test]
async fn public_skill_readwrite_preserves_valid_buffer_and_known_empty() {
    let fs = AgentFs::new();
    open(&fs, 1, OpenMode::Write).await;
    fs.write(Fid(1), 0, KNOWN).await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();
    open(&fs, 2, OpenMode::ReadWrite).await;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs.read(Fid(2), 0, 4096).await.unwrap())
            .unwrap(),
        serde_json::from_slice::<serde_json::Value>(KNOWN).unwrap()
    );
    fs.write(Fid(2), 0, KNOWN).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    open(&fs, 3, OpenMode::Read).await;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs.read(Fid(3), 0, 4096).await.unwrap())
            .unwrap(),
        serde_json::from_slice::<serde_json::Value>(KNOWN).unwrap()
    );
    open(&fs, 4, OpenMode::Write).await;
    let empty = br#"{"version":1,"publication_version":8,"process_path":"/proc/3","known":true,"mentionable_skill_ids":[]}"#;
    fs.write(Fid(4), 0, empty).await.unwrap();
    fs.clunk(Fid(4)).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs.read(Fid(3), 0, 4096).await.unwrap())
            .unwrap(),
        serde_json::from_slice::<serde_json::Value>(empty).unwrap()
    );
}
