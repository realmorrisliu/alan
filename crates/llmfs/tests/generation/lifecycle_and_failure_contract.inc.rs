
#[tokio::test]
async fn reading_requires_a_read_open() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "status".into(),
        ],
    )
    .await
    .unwrap();
    // No open: read is denied.
    assert_eq!(fs.read(Fid(2), 0, 64).await, Err(ErrorCode::NoAccess));
}

#[tokio::test]
async fn writing_requires_a_write_open() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &["connections".into(), "default".into(), g, "data".into()],
    )
    .await
    .unwrap();
    // Walked but not opened for write: a write is refused.
    assert_eq!(fs.write(Fid(2), 0, b"{}").await, Err(ErrorCode::NoAccess));
}

#[tokio::test]
async fn read_open_of_a_write_only_file_is_refused() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    // `data` and `ctl` are write-only sinks: a read-intent open fails at dial time.
    for leaf in ["data", "ctl"] {
        fs.walk(
            Fid::ROOT,
            Fid(2),
            &[
                "connections".into(),
                "default".into(),
                g.clone(),
                leaf.into(),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            fs.open(Fid(2), OpenMode::Read).await,
            Err(ErrorCode::NoAccess)
        );
        fs.clunk(Fid(2)).await.ok();
    }
}

#[tokio::test]
async fn an_empty_request_is_rejected() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    // Zero bytes written: an empty request is malformed.
    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::BadRequest));
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "rejected");
}

#[tokio::test]
async fn invalid_v2_request_is_rejected_before_running() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fs = llmfs_with(RecordingProvider {
        requests: Arc::clone(&requests),
    });
    let g = clone_gen(&fs, Fid(1)).await;

    assert_eq!(
        commit_request(&fs, &g, Fid(2), br#"{"version":2,"messages":[]}"#).await,
        Err(ErrorCode::BadRequest)
    );
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "rejected");
    assert!(
        requests.lock().unwrap().is_empty(),
        "invalid v2 documents must not reach the provider"
    );

    let events =
        String::from_utf8(read_all(&fs, &["connections", "default", &g, "events"], Fid(4)).await)
            .unwrap();
    assert!(
        events.contains("\"rejected\""),
        "invalid v2 documents append a rejected event: {events:?}"
    );
}

#[tokio::test]
async fn retired_or_invalid_request_documents_never_reach_the_provider() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fs = llmfs_with(RecordingProvider {
        requests: Arc::clone(&requests),
    });
    let rejected: [(&str, &[u8]); 4] = [
        (
            "retired unversioned shape",
            br#"{"system":"system prompt","user":"hello"}"#,
        ),
        (
            "missing version discriminator",
            br#"{"messages":[{"role":"user","content":"hello"}]}"#,
        ),
        ("malformed JSON", br#"{"version":2,"messages":["#),
        (
            "unknown version",
            br#"{"version":3,"messages":[{"role":"user","content":"hello"}]}"#,
        ),
    ];

    for (index, (case, body)) in rejected.into_iter().enumerate() {
        let base = 10 + index as u64 * 3;
        let generation = clone_gen(&fs, Fid(base)).await;
        assert_eq!(
            commit_request(&fs, &generation, Fid(base + 1), body).await,
            Err(ErrorCode::BadRequest),
            "{case} must fail at commit"
        );
        assert_eq!(
            status_of(&fs, &generation, Fid(base + 2)).await,
            "rejected",
            "{case} must leave a terminal rejected generation"
        );
        assert!(
            requests.lock().unwrap().is_empty(),
            "{case} must fail before provider dispatch"
        );
    }
}

#[tokio::test]
async fn unknown_request_fields_are_rejected() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    assert_eq!(
        commit_request(
            &fs,
            &g,
            Fid(2),
            br#"{"version":2,"messages":[{"role":"user","content":"hi"}],"unknown":true}"#,
        )
        .await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_started_generation_cannot_be_restarted() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
        .await
        .unwrap();
    // A second request document into the same Generation is refused.
    assert_eq!(
        commit_request(&fs, &g, Fid(3), AGAIN_REQUEST).await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_connection_lists_its_generations() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "default"], Fid(2)).await).unwrap();
    assert!(listing.lines().any(|l| l == "clone"));
    assert!(
        listing.lines().any(|l| l == g),
        "connection lists gen {g}: {listing:?}"
    );
}

#[tokio::test]
async fn distinct_generations_get_distinct_status_qids() {
    let fs = llmfs();
    let a = clone_gen(&fs, Fid(1)).await;
    let b = clone_gen(&fs, Fid(2)).await;
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &["connections".into(), "default".into(), a, "status".into()],
    )
    .await
    .unwrap();
    let pa = fs.stat(Fid(3)).await.unwrap().qid.path;
    fs.walk(
        Fid::ROOT,
        Fid(4),
        &["connections".into(), "default".into(), b, "status".into()],
    )
    .await
    .unwrap();
    let pb = fs.stat(Fid(4)).await.unwrap().qid.path;
    assert_ne!(
        pa, pb,
        "distinct generations must have distinct status qids"
    );
}

#[tokio::test]
async fn status_qid_version_bumps_as_the_generation_advances() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "status".into(),
        ],
    )
    .await
    .unwrap();
    let v0 = fs.stat(Fid(2)).await.unwrap().qid.version;
    commit_request(&fs, &g, Fid(3), SIMPLE_REQUEST)
        .await
        .unwrap();
    drain_events(&fs, &g, Fid(4)).await;
    fs.walk(
        Fid::ROOT,
        Fid(5),
        &["connections".into(), "default".into(), g, "status".into()],
    )
    .await
    .unwrap();
    let v1 = fs.stat(Fid(5)).await.unwrap().qid.version;
    assert_ne!(v0, v1, "status qid version must change as status advances");
}

#[tokio::test]
async fn stat_reports_real_status_length() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "status".into(),
        ],
    )
    .await
    .unwrap();
    let stat_len = fs.stat(Fid(2)).await.unwrap().length;
    let status =
        String::from_utf8(read_all(&fs, &["connections", "default", &g, "status"], Fid(3)).await)
            .unwrap();
    assert_eq!(
        stat_len,
        status.len() as u64,
        "status stat length matches the readable status document"
    );
}

#[tokio::test]
async fn status_exposes_progress_tokens_and_cost() {
    let fs = llmfs_with(UsageProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(5),
        &["connections".into(), "default".into(), "meter".into()],
    )
    .await
    .unwrap();
    let meter_v0 = fs.stat(Fid(5)).await.unwrap().qid.version;

    commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
        .await
        .unwrap();
    drain_events(&fs, &g, Fid(3)).await;

    let status = status_doc_of(&fs, &g, Fid(4)).await;
    assert_eq!(status["version"], 1);
    assert_eq!(status["generation"], g);
    assert_eq!(status["connection"], "default");
    assert_eq!(status["status"], "done");
    assert_eq!(status["progress"]["terminal"], true);
    assert_eq!(status["tokens"]["available"], true);
    assert_eq!(status["tokens"]["prompt_tokens"], 11);
    assert_eq!(status["tokens"]["cached_prompt_tokens"], 3);
    assert_eq!(status["tokens"]["completion_tokens"], 7);
    assert_eq!(status["tokens"]["total_tokens"], 18);
    assert_eq!(status["tokens"]["reasoning_tokens"], 5);
    assert_eq!(status["cost"]["currency"], "USD");
    assert_eq!(status["cost"]["amount_microusd"], 0);
    assert_eq!(status["cost"]["metered"], false);
    let meter_v1 = fs.stat(Fid(5)).await.unwrap().qid.version;
    assert_ne!(
        meter_v0, meter_v1,
        "meter qid version changes when usage updates token totals"
    );

    let meter: serde_json::Value =
        serde_json::from_slice(&read_all(&fs, &["connections", "default", "meter"], Fid(6)).await)
            .unwrap();
    assert_eq!(meter["meter"]["generation_starts"], 1);
    assert_eq!(meter["meter"]["total_tokens"], 18);
    assert_eq!(meter["meter"]["total_cost_microusd"], 0);
}

#[tokio::test]
async fn terminal_generations_are_reaped_by_retention_policy() {
    let fs = llmfs();
    let mut ids = Vec::new();
    for i in 0..17 {
        let gen_id = clone_gen(&fs, Fid(10 + i * 3)).await;
        commit_request(&fs, &gen_id, Fid(11 + i * 3), SIMPLE_REQUEST)
            .await
            .unwrap();
        drain_events(&fs, &gen_id, Fid(12 + i * 3)).await;
        ids.push(gen_id);
    }

    let open_gen = clone_gen(&fs, Fid(1000)).await;
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "default"], Fid(1001)).await).unwrap();
    assert!(
        !listing.lines().any(|line| line == ids[0]),
        "oldest terminal Generation should be reaped: {listing:?}"
    );
    assert!(
        listing.lines().any(|line| line == ids[1]),
        "recent terminal Generation should stay retained: {listing:?}"
    );
    assert!(
        listing.lines().any(|line| line == open_gen),
        "new open Generation should not be reaped: {listing:?}"
    );
    assert_eq!(
        fs.walk(
            Fid::ROOT,
            Fid(1002),
            &[
                "connections".into(),
                "default".into(),
                ids[0].clone(),
                "status".into(),
            ],
        )
        .await,
        Err(ErrorCode::NotFound),
        "reaped Generation is no longer walkable"
    );
}

#[tokio::test]
async fn opened_events_stat_survives_retention_reap() {
    let fs = llmfs();
    let mut oldest = String::new();
    let oldest_events_fid = Fid(12);

    for i in 0..17 {
        let gen_id = clone_gen(&fs, Fid(10 + i * 3)).await;
        commit_request(&fs, &gen_id, Fid(11 + i * 3), SIMPLE_REQUEST)
            .await
            .unwrap();
        drain_events(&fs, &gen_id, Fid(12 + i * 3)).await;
        if i == 0 {
            oldest = gen_id;
        }
    }

    let _open_gen = clone_gen(&fs, Fid(1000)).await;
    assert_eq!(
        fs.walk(
            Fid::ROOT,
            Fid(1001),
            &[
                "connections".into(),
                "default".into(),
                oldest.clone(),
                "status".into(),
            ],
        )
        .await,
        Err(ErrorCode::NotFound),
        "oldest terminal Generation should be reaped"
    );

    let events = fs.read(oldest_events_fid, 0, 65536).await.unwrap();
    assert!(
        String::from_utf8_lossy(&events).contains("\"done\""),
        "opened events fid still reads retained terminal bytes"
    );
    assert_eq!(
        fs.stat(oldest_events_fid).await.unwrap().length,
        events.len() as u64,
        "opened events fid stat must report retained stream length after reap"
    );
}

#[tokio::test]
async fn a_reused_fid_is_rejected() {
    let fs = llmfs();
    fs.walk(Fid::ROOT, Fid(1), &["connections".into()])
        .await
        .unwrap();
    // Fid(1) is live; rebinding it must be refused.
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(1), &["connections".into()]).await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_repeated_clone_open_is_rejected() {
    let fs = llmfs();
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    // A second open of the same clone fid must not allocate a second Generation.
    assert_eq!(
        fs.open(Fid(1), OpenMode::ReadWrite).await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn ctl_accepts_a_newline_terminated_abort() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    // `echo abort > ctl` delivers "abort\n".
    fs.write(Fid(2), 0, b"abort\n").await.unwrap();
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "aborted");
}

#[tokio::test]
async fn abort_before_commit_makes_the_later_commit_fail() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    // Abort while still open.
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"abort").await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    // The aborted Generation cannot then be started by committing a request.
    assert_eq!(
        commit_request(&fs, &g, Fid(3), SIMPLE_REQUEST).await,
        Err(ErrorCode::BadRequest)
    );
    // And a watcher sees a terminal record rather than blocking forever.
    let events =
        String::from_utf8(read_all(&fs, &["connections", "default", &g, "events"], Fid(4)).await)
            .unwrap();
    assert!(
        events.contains("aborted"),
        "events has a terminal abort record: {events:?}"
    );
}

#[tokio::test]
async fn abort_after_done_is_rejected() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
        .await
        .unwrap();
    drain_events(&fs, &g, Fid(3)).await; // runs to done
    fs.walk(
        Fid::ROOT,
        Fid(4),
        &["connections".into(), "default".into(), g, "ctl".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(4), OpenMode::Write).await.unwrap();
    assert_eq!(
        fs.write(Fid(4), 0, b"abort").await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_running_generation_can_be_aborted() {
    let fs = llmfs_with(HangingProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
        .await
        .unwrap();
    // The provider never finishes; abort settles the Generation and records it.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Write).await.unwrap();
    fs.write(Fid(3), 0, b"abort").await.unwrap();
    assert_eq!(status_of(&fs, &g, Fid(4)).await, "aborted");
}

#[tokio::test]
async fn unregister_connection_aborts_active_generations() {
    let fs = llmfs_with(HangingProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
        .await
        .unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "events".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();
    fs.unregister_connection("default").await;

    let events = String::from_utf8(fs.read(Fid(3), 0, 65536).await.unwrap()).unwrap();
    assert!(
        events.contains("\"aborted\""),
        "unregister appends a terminal aborted event: {events:?}"
    );
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(4), &["connections".into(), "default".into()])
            .await,
        Err(ErrorCode::NotFound),
        "the connection endpoint is removed"
    );

    fs.register_connection("default", Box::new(MockLlmProvider::new()));
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "default"], Fid(5)).await).unwrap();
    assert!(
        !listing.lines().any(|line| line == g),
        "old aborted generation must not reappear under a re-registered connection: {listing:?}"
    );
}

#[tokio::test]
async fn unregister_connection_makes_in_flight_data_commit_fail() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;

    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, SIMPLE_REQUEST).await.unwrap();

    fs.unregister_connection("default").await;

    assert_eq!(
        fs.clunk(Fid(2)).await,
        Err(ErrorCode::BadRequest),
        "commit-on-clunk must not report success after unregister discards the Generation"
    );
}

#[tokio::test]
async fn an_early_closed_stream_ends_with_a_terminal_error() {
    let fs = llmfs_with(EarlyCloseProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
        .await
        .unwrap();
    // Tail events: a stream that closes without a finished chunk must still reach a
    // terminal record (error), not block the reader forever.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "events".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();
    let mut acc = String::new();
    let mut offset = 0u64;
    loop {
        let chunk =
            tokio::time::timeout(Duration::from_millis(500), fs.read(Fid(3), offset, 65536))
                .await
                .expect("events stalled without a terminal record")
                .unwrap();
        offset += chunk.len() as u64;
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("error") {
            break;
        }
    }
    assert!(
        acc.contains("partial"),
        "the partial text was recorded: {acc:?}"
    );
    assert_eq!(status_of(&fs, &g, Fid(5)).await, "error");
}

#[tokio::test]
async fn a_startup_failure_is_terminal() {
    let fs = llmfs_with(StartupFailProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    // generate_stream errors before a receiver: the commit fails and the
    // Generation is left in a terminal error state (not stuck open).
    assert_eq!(
        commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "error");
}

/// A provider whose `generate_stream` startup takes a while before returning a
/// receiver, so a test can abort *during* startup.
struct SlowStartupProvider;

#[async_trait::async_trait]
impl LlmProvider for SlowStartupProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx.send(text_chunk("late", true)).await;
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "slow-startup"
    }
}

#[tokio::test]
async fn abort_during_provider_startup_cancels_it() {
    let fs = Arc::new(llmfs_with(SlowStartupProvider));
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, SIMPLE_REQUEST).await.unwrap();

    // Commit in a task: it parks awaiting the slow provider startup.
    let committer = fs.clone();
    let commit = tokio::spawn(async move { committer.clunk(Fid(2)).await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Abort while startup is still in flight.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Write).await.unwrap();
    fs.write(Fid(3), 0, b"abort").await.unwrap();

    // The commit returns promptly (the provider future is dropped), well before
    // the 5s startup would finish, and the Generation is aborted.
    let r = tokio::time::timeout(Duration::from_millis(500), commit)
        .await
        .expect("commit did not cancel on abort")
        .unwrap();
    assert_eq!(r, Ok(()));
    assert_eq!(status_of(&fs, &g, Fid(4)).await, "aborted");
}

/// Emits some text then a *finished* chunk whose finish_reason is `stream_error`
/// (an upstream failure after partial output), like the real adapters.
struct StreamErrorProvider;

#[async_trait::async_trait]
impl LlmProvider for StreamErrorProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx.send(text_chunk("partial", false)).await;
            let mut err = text_chunk("", true);
            err.text = None;
            err.finish_reason = Some("stream_error".to_string());
            let _ = tx.send(err).await;
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "stream-error"
    }
}

#[tokio::test]
async fn a_stream_error_finish_reason_is_terminal_error_not_done() {
    let fs = llmfs_with(StreamErrorProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
        .await
        .unwrap();
    // Tail events to a terminal record.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "events".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();
    let mut acc = String::new();
    let mut offset = 0u64;
    loop {
        let chunk =
            tokio::time::timeout(Duration::from_millis(500), fs.read(Fid(3), offset, 65536))
                .await
                .expect("events stalled")
                .unwrap();
        offset += chunk.len() as u64;
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("error") {
            break;
        }
    }
    assert!(
        !acc.contains("\"done\""),
        "a stream error must not record done: {acc:?}"
    );
    assert_eq!(status_of(&fs, &g, Fid(4)).await, "error");
}
