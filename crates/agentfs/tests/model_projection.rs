//! Model observations are ordinary watched documents, not Connection restore evidence.
use alan_agentfs::AgentFs;
use alan_ap::{Fid, FileServer, OpenMode};

#[tokio::test]
async fn model_surface_is_unknown_until_clunk_and_emits_watch_record() {
    let fs = AgentFs::new();
    let names = ["machine".into(), "ui".into(), "models".into()];
    fs.walk(Fid::ROOT, Fid(1), &names).await.unwrap();
    let before = fs.stat(Fid(1)).await.unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    let initial = String::from_utf8(fs.read(Fid(1), 0, 4096).await.unwrap()).unwrap();
    assert!(initial.contains("\"known\":false"));
    assert!(initial.contains("\"selected_next\":null"));
    fs.walk(Fid::ROOT, Fid(2), &names).await.unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    let next = br#"{"version":1,"publication_version":1,"process_path":"/proc/1","known":true,"catalog":null,"selected_next":null,"active":null,"admitted":[]}"#;
    fs.write(Fid(2), 0, next).await.unwrap();
    assert_eq!(
        fs.stat(Fid(1)).await.unwrap().qid.version,
        before.qid.version
    );
    fs.clunk(Fid(2)).await.unwrap();
    assert_ne!(
        fs.stat(Fid(1)).await.unwrap().qid.version,
        before.qid.version
    );
    fs.clunk(Fid(1)).await.unwrap();
    fs.walk(Fid::ROOT, Fid(3), &names).await.unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(3), 0, 4096).await.unwrap(), next);
    fs.walk(Fid::ROOT, Fid(4), &["events".into()])
        .await
        .unwrap();
    fs.open(Fid(4), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(4), 0, 4096).await.unwrap(), b"ui:models\n");
}
