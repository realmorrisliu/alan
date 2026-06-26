//! Generation as a clone-via-open connection directory (add-llm-file-server §4,
//! the minimal callable slice brought into the Plan 9 core). A caller opens
//! `connections/<conn>/clone` (allocating a Generation directory), writes one
//! request document to `data` (committed on clunk), and reads the streamed token
//! records from `events`. Backed by the mock provider — no real API key.

use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_llm::mock::MockLlmProvider;
use alan_llmfs::LlmFs;

fn llmfs() -> LlmFs {
    let fs = LlmFs::new();
    fs.register_connection("default", Box::new(MockLlmProvider::new()));
    fs
}

async fn read_all(fs: &LlmFs, path: &[&str], fid: Fid) -> Vec<u8> {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.expect("walk");
    fs.open(fid, OpenMode::Read).await.expect("open");
    fs.read(fid, 0, 65536).await.expect("read")
}

/// Tail `events` until a terminal `done` record appears, accumulating the text.
async fn drain_events(fs: &LlmFs, gen_id: &str, fid: Fid) -> String {
    let path = [
        "connections".to_string(),
        "default".to_string(),
        gen_id.to_string(),
        "events".to_string(),
    ];
    fs.walk(Fid::ROOT, fid, &path).await.unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    let mut acc = String::new();
    let mut offset = 0u64;
    loop {
        let chunk = tokio::time::timeout(Duration::from_millis(500), fs.read(fid, offset, 65536))
            .await
            .expect("events stream stalled")
            .unwrap();
        offset += chunk.len() as u64;
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("\"done\"") {
            break;
        }
    }
    acc
}

#[tokio::test]
async fn clone_open_allocates_a_generation_directory() {
    let fs = llmfs();
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let gen_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    assert!(!gen_id.is_empty());

    // The allocated generation is a real, walkable directory with its files.
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "default", &gen_id], Fid(2)).await)
            .unwrap();
    for f in ["data", "events", "ctl", "status"] {
        assert!(
            listing.lines().any(|l| l == f),
            "generation dir lists {f}: {listing:?}"
        );
    }
}

async fn open_clone(fs: &LlmFs, fid: Fid) -> String {
    fs.walk(
        Fid::ROOT,
        fid,
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(fid, OpenMode::ReadWrite).await.unwrap();
    String::from_utf8(fs.read(fid, 0, 64).await.unwrap()).unwrap()
}

#[tokio::test]
async fn two_clone_opens_allocate_independent_generations() {
    let fs = llmfs();
    let a = open_clone(&fs, Fid(1)).await;
    let b = open_clone(&fs, Fid(2)).await;
    assert_ne!(a, b, "each clone-open allocates a distinct Generation");
}

#[tokio::test]
async fn writing_the_request_streams_tokens_to_events() {
    let fs = llmfs();

    // clone-via-open → generation id
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let gen_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    // write the request document to data; commit on clunk starts the generation
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            gen_id.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, br#"{"user":"hi"}"#).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();

    // events carries the streamed tokens, ending with a done record
    let events = drain_events(&fs, &gen_id, Fid(3)).await;
    assert!(
        events.contains("Mock response"),
        "events should carry the model tokens: {events:?}"
    );
    assert!(
        events.contains("\"done\""),
        "events should end with a terminal record"
    );
}

#[tokio::test]
async fn a_malformed_request_is_rejected_at_clunk() {
    let fs = llmfs();
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let gen_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            gen_id,
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"{ truncated").await.unwrap();
    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::BadRequest));
}
