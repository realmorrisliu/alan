use super::*;

#[tokio::test]
async fn read_file_accepts_regular_file_shrinking_after_stat() {
    let fs = Arc::new(ScriptedReadFs::shrinking_file(b"exited\n", 8));
    let client = NamespaceClient::new(InProcessTransport::new(fs.clone()));

    assert_eq!(
        client.read_file("/proc/10/status").await.unwrap(),
        b"exited\n"
    );
    assert_eq!(fs.clunk_count.load(AtomicOrdering::SeqCst), 1);
}

#[tokio::test]
async fn read_file_stops_at_reported_regular_file_length() {
    let fs = Arc::new(ScriptedReadFs::shrinking_file(b"ready\nignored", 6));
    let client = NamespaceClient::new(InProcessTransport::new(fs.clone()));

    assert_eq!(
        client.read_file("/proc/10/status").await.unwrap(),
        b"ready\n"
    );
    assert_eq!(fs.clunk_count.load(AtomicOrdering::SeqCst), 1);
}

#[tokio::test]
async fn read_stream_clunks_open_fid_when_cancelled() {
    let fs = Arc::new(BlockingReadFs::new());
    let client = NamespaceClient::new(InProcessTransport::new(fs.clone()));
    let task = tokio::spawn({
        let client = client.clone();
        async move { client.read_stream_from("/agent/1/io/input", 0).await }
    });

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        fs.read_started.notified(),
    )
    .await
    .expect("read should start");
    task.abort();
    let _ = task.await;
    tokio::time::timeout(std::time::Duration::from_secs(1), fs.clunked.notified())
        .await
        .expect("cancelled read should clunk the fid");

    assert_eq!(fs.clunk_count.load(AtomicOrdering::SeqCst), 1);
}

#[tokio::test]
async fn llmfs_event_error_clunks_events_fid() {
    let fs = Arc::new(ScriptedReadFs::new(
        b"{\"version\":1,\"error\":\"temporary 503\"}\n".as_slice(),
    ));
    let client = NamespaceClient::new(InProcessTransport::new(fs.clone()));
    let mut ignore = |_event: Event| async {};

    let err = read_generation_response_with_text_events(&client, "default", "gen-1", &mut ignore)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("llmfs generation failed"),
        "{err:#}"
    );
    assert_eq!(fs.clunk_count.load(AtomicOrdering::SeqCst), 1);
}

#[tokio::test]
async fn controlled_generation_aborts_llmfs_on_cancel() {
    let started = Arc::new(Notify::new());
    let llmfs = Arc::new(LlmFs::new());
    llmfs.register_connection(
        "default",
        Box::new(BlockingStreamProvider {
            started: Arc::clone(&started),
        }),
    );
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let environment = environment.clone();
        let cancel = cancel.clone();
        async move {
            let request = GenerationRequest::new().with_user_message("hello");
            let mut ignore = |_event: Event| async {};
            environment
                .generate_with_text_events_controlled(&request, &mut ignore, 30, &cancel)
                .await
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
        .await
        .expect("provider stream should start");
    cancel.cancel();
    let err = task.await.unwrap().unwrap_err();
    let err = format!("{err:#}");
    assert!(
        err.contains("cancelled") || err.contains("aborted"),
        "{err}"
    );

    let status = String::from_utf8(
        shell
            .cat("/mnt/llm/connections/default/g0/status")
            .await
            .unwrap(),
    )
    .unwrap();
    let status: serde_json::Value = serde_json::from_str(&status).unwrap();
    assert_eq!(status["status"], "aborted");
}
