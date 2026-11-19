use std::sync::Arc;

use super::*;
use alan_agent_engine::LlmClient;
use alan_llm::{GenerationResponse, MockLlmProvider, StreamChunk};
use tokio::sync::Notify;

#[path = "v2_tests.rs"]
mod v2_tests;

pub(super) fn mock_response(text: &str) -> GenerationResponse {
    GenerationResponse {
        content: text.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: Some("stop".to_string()),
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

pub(super) fn stream_chunk(text: Option<&str>, is_finished: bool) -> StreamChunk {
    StreamChunk {
        text: text.map(str::to_string),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        usage: None,
        provider_response_id: None,
        provider_response_status: None,
        sequence_number: None,
        tool_call_delta: None,
        is_finished,
        finish_reason: is_finished.then(|| "stop".to_string()),
    }
}

pub(super) async fn wait_for_tail_contains(tail: &mut alan_shell::Tail, expected: &str) -> String {
    let mut collected = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !collected.contains(expected) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for `{expected}`, got `{collected}`"
        );
        let chunk = tokio::time::timeout(remaining, tail.read(4096))
            .await
            .expect("timed out waiting for tail chunk")
            .expect("tail read failed");
        if chunk.is_empty() {
            break;
        }
        collected.push_str(&String::from_utf8_lossy(&chunk));
    }
    collected
}

pub(super) async fn wait_for_submission(client: &RootAgentClient, submission_id: &str) {
    let path = format!("{}/machine/ui/events", client.agent_path());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let body = client.shell().cat(&path).await.unwrap_or_default();
        if String::from_utf8_lossy(&body).contains(submission_id) {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for {submission_id} in {path}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub(super) async fn next_embed_event(stream: &mut EmbedEventStream) -> EmbedEvent {
    tokio::time::timeout(std::time::Duration::from_secs(5), stream.next_event())
        .await
        .expect("timed out waiting for embed event")
        .expect("embed event")
}

pub(super) async fn read_until_submission(
    stream: &mut EmbedEventStream,
    submission_id: &str,
) -> Vec<EmbedEvent> {
    let mut events = Vec::new();
    loop {
        let event = next_embed_event(stream).await;
        let done = matches!(
            &event,
            EmbedEvent::TurnFinished {
                submission_id: id,
                ..
            } if id == submission_id
        );
        events.push(event);
        if done {
            return events;
        }
    }
}

pub(super) fn collect_text(events: &[EmbedEvent]) -> String {
    let mut text = String::new();
    for event in events {
        if let EmbedEvent::TextDelta {
            chunk,
            is_final: false,
            ..
        } = event
        {
            text.push_str(chunk);
        }
    }
    text
}

#[tokio::test]
async fn ephemeral_embedded_instance_executes_turn_and_has_no_host_tools() {
    let provider = MockLlmProvider::new().with_response(mock_response("hello from embedded alan"));
    let config = EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(provider)));
    let app = EmbeddedAlan::boot(config).await.unwrap();

    // Default embedded instance mounts no host directories under /mnt
    let mnt_entries = app.shell().ls("/mnt").await.unwrap();
    assert!(!mnt_entries.iter().any(|entry| entry == "host"));

    let client = app.root_agent();
    let mut output = client.tail_output().await.unwrap();
    let _submission_id = client.submit_text("say hi").await.unwrap();

    let text = wait_for_tail_contains(&mut output, "hello from embedded alan").await;
    assert!(text.contains("hello from embedded alan"), "got: {text}");

    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn ephemeral_embedded_instance_has_writable_memory_and_bound_descriptor() {
    let provider = MockLlmProvider::new().with_response(mock_response("ok"));
    let config = EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(provider)));
    let (sm_config, _) = config.into_service_manager_config().unwrap();
    assert!(sm_config.in_memory_stores);
    assert!(
        sm_config
            .launch_context
            .descriptor(alan_agent_engine::MEMORY_STORE_DESCRIPTOR)
            .is_some()
    );

    use alan_ap::{Fid, Request};
    let resolved = sm_config
        .launch_context
        .namespace
        .resolve("/memory")
        .expect("/memory must be mounted in ephemeral mode");
    assert_eq!(resolved.access, Access::ReadWrite);
    let ns_transport = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
        sm_config.launch_context.namespace,
    )));
    ns_transport
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: vec!["memory".to_string()],
        })
        .await
        .unwrap();
    ns_transport
        .call(Request::Create {
            fid: Fid(1),
            newfid: Fid(2),
            name: "note.txt".to_string(),
            kind: alan_ap::FileKind::File,
        })
        .await
        .unwrap();
    ns_transport
        .call(Request::Open {
            fid: Fid(2),
            mode: alan_ap::OpenMode::Write,
        })
        .await
        .unwrap();
    ns_transport
        .call(Request::Write {
            fid: Fid(2),
            offset: 0,
            data: b"ephemeral-memory".to_vec(),
        })
        .await
        .unwrap();
    ns_transport
        .call(Request::Clunk { fid: Fid(2) })
        .await
        .unwrap();
    ns_transport
        .call(Request::Clunk { fid: Fid(1) })
        .await
        .unwrap();

    let shell = Shell::new(ns_transport);
    let stored = shell.cat("/memory/note.txt").await.unwrap();
    assert_eq!(stored, b"ephemeral-memory");

    let provider = MockLlmProvider::new().with_response(mock_response("ok"));
    let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(provider),
    )))
    .await
    .unwrap();
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn isolated_instances_do_not_interfere_and_support_custom_mounts() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let fs_a = Arc::new(alan_ap::reference::MemFs::with_read_only_file(
        "note.txt",
        b"only-a".to_vec(),
    ));
    let fs_b = Arc::new(alan_ap::reference::MemFs::with_read_only_file(
        "note.txt",
        b"only-b".to_vec(),
    ));

    let provider_a = MockLlmProvider::new().with_response(mock_response("reply A"));
    let provider_b = MockLlmProvider::new().with_response(mock_response("reply B"));

    let app_a = EmbeddedAlan::boot(
        EmbeddedConfig::isolated(dir_a.path(), LlmSource::Client(LlmClient::new(provider_a)))
            .with_mount("/mnt/app-a", fs_a, Access::ReadOnly),
    )
    .await
    .unwrap();

    let app_b = EmbeddedAlan::boot(
        EmbeddedConfig::isolated(dir_b.path(), LlmSource::Client(LlmClient::new(provider_b)))
            .with_mount("/mnt/app-b", fs_b, Access::ReadOnly),
    )
    .await
    .unwrap();

    assert_ne!(app_a.boot_id(), app_b.boot_id());

    assert_eq!(
        app_a.shell().cat("/mnt/app-a/note.txt").await.unwrap(),
        b"only-a"
    );
    assert!(app_b.shell().ls("/mnt/app-a").await.is_err());

    let mut out_a = app_a.root_agent().tail_output().await.unwrap();
    let mut out_b = app_b.root_agent().tail_output().await.unwrap();
    app_a.root_agent().submit_text("hi a").await.unwrap();
    app_b.root_agent().submit_text("hi b").await.unwrap();

    wait_for_tail_contains(&mut out_a, "reply A").await;
    wait_for_tail_contains(&mut out_b, "reply B").await;

    assert!(
        dir_a
            .path()
            .join("services/agent-runtime/rollouts")
            .exists()
    );
    assert!(
        dir_b
            .path()
            .join("services/agent-runtime/rollouts")
            .exists()
    );

    app_a.shutdown().await.unwrap();
    assert!(app_b.shell().ls("/agent/root").await.is_ok());
    app_b.shutdown().await.unwrap();
}

#[tokio::test]
async fn event_stream_yields_text_final_usage_and_turn_finished_for_submission() {
    let reply = "hello from embedded alan";
    let provider = MockLlmProvider::new().with_response(mock_response(reply));
    let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(provider),
    )))
    .await
    .unwrap();
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let submission_id = client.submit_text("say hi").await.unwrap();

    let events = read_until_submission(&mut stream, &submission_id).await;
    assert_eq!(collect_text(&events), reply, "{events:?}");
    assert!(
        events.iter().any(|event| matches!(
            event,
            EmbedEvent::TextDelta {
                submission_id: Some(id),
                chunk,
                is_final: true
            } if id == &submission_id && chunk.is_empty()
        )),
        "expected terminal TextDelta(is_final: true): {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            EmbedEvent::Usage { submission_id: Some(id), .. } if id == &submission_id
        )),
        "expected Usage event before TurnFinished: {events:?}"
    );
    match events.last() {
        Some(EmbedEvent::TurnFinished {
            submission_id: id,
            status,
            error,
        }) => {
            assert_eq!(id, &submission_id);
            assert_eq!(*status, UiInputStatus::Completed);
            assert!(error.is_none());
        }
        other => panic!("expected TurnFinished, got {other:?}"),
    }

    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn event_stream_keeps_each_turns_text_with_its_submission() {
    let provider =
        MockLlmProvider::new().with_responses(vec![mock_response("ONE"), mock_response("TWO")]);
    let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(provider),
    )))
    .await
    .unwrap();
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let first = client.submit_text("first").await.unwrap();
    wait_for_submission(&client, &first).await;
    let second = client.submit_text("second").await.unwrap();
    wait_for_submission(&client, &second).await;

    let events = read_until_submission(&mut stream, &second).await;
    let first_at = events.iter().position(|event| {
        matches!(
            event,
            EmbedEvent::TurnFinished { submission_id, .. } if submission_id == &first
        )
    });
    let Some(first_at) = first_at else {
        panic!("missing TurnFinished for {first}: {events:?}");
    };
    let first_text = collect_text(&events[..first_at]);
    let second_text = collect_text(&events[first_at + 1..]);
    assert_eq!(first_text, "ONE", "{events:?}");
    assert_eq!(second_text, "TWO", "{events:?}");
    app.shutdown().await.unwrap();
}

struct GatedMockProvider {
    inner: MockLlmProvider,
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl alan_llm::LlmProvider for GatedMockProvider {
    async fn generate(
        &mut self,
        request: alan_llm::GenerationRequest,
    ) -> Result<GenerationResponse> {
        self.inner.generate(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.inner.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: alan_llm::GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.started.notify_one();
        let response = self.inner.generate(request).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let release = Arc::clone(&self.release);
        let content = response.content;
        tokio::spawn(async move {
            release.notified().await;
            if !content.is_empty() {
                let _ = tx.send(stream_chunk(Some(&content), false)).await;
            }
            let _ = tx.send(stream_chunk(None, true)).await;
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

#[tokio::test]
async fn event_stream_reports_interrupted_turn() {
    let reply = "do-not-leak-interrupt-reply";
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let provider = GatedMockProvider {
        inner: MockLlmProvider::new().with_response(mock_response(reply)),
        started: Arc::clone(&started),
        release: Arc::clone(&release),
    };
    let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(provider),
    )))
    .await
    .unwrap();
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let submission_id = client.submit_text("interrupt this turn").await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
        .await
        .expect("generation did not start");
    client.interrupt(Some(&submission_id)).await.unwrap();

    let events = read_until_submission(&mut stream, &submission_id).await;
    let finished = events.last().expect("turn event");
    match finished {
        EmbedEvent::TurnFinished {
            submission_id: id,
            status,
            ..
        } => {
            assert_eq!(id, &submission_id);
            assert_eq!(*status, UiInputStatus::Cancelled);
        }
        other => panic!("expected TurnFinished, got {other:?}"),
    }
    assert!(
        !events.iter().any(|event| matches!(
            event,
            EmbedEvent::TextDelta { chunk, .. } if chunk.contains(reply)
        )),
        "interrupted turn leaked the mock reply: {events:?}"
    );

    release.notify_one();
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn isolated_resume_root_restores_prior_turn() {
    let dir = tempfile::tempdir().unwrap();
    let marker = "resume-marker-amber";
    let provider = MockLlmProvider::new().with_response(mock_response(marker));
    let app = EmbeddedAlan::boot(EmbeddedConfig::isolated(
        dir.path(),
        LlmSource::Client(LlmClient::new(provider)),
    ))
    .await
    .unwrap();
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let submission_id = client.submit_text("remember the marker").await.unwrap();
    let _events = read_until_submission(&mut stream, &submission_id).await;
    app.shutdown().await.unwrap();

    let resumed = MockLlmProvider::new().with_response(mock_response("resumed-reply"));
    let app = EmbeddedAlan::boot(
        EmbeddedConfig::isolated(
            dir.path(),
            LlmSource::Client(LlmClient::new(resumed.clone())),
        )
        .with_root_resume(true),
    )
    .await
    .expect("resume boot");
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let submission_id = client.submit_text("what did you store").await.unwrap();
    let events = read_until_submission(&mut stream, &submission_id).await;
    assert_eq!(collect_text(&events), "resumed-reply");
    match events.last() {
        Some(EmbedEvent::TurnFinished {
            submission_id: id,
            status,
            error,
        }) => {
            assert!(error.is_none());
            assert_eq!(id, &submission_id);
            assert_eq!(*status, UiInputStatus::Completed);
        }
        other => panic!("expected TurnFinished, got {other:?}"),
    }
    let saw_marker = resumed.recorded_requests().iter().any(|request| {
        request
            .system_prompt
            .as_deref()
            .is_some_and(|prompt| prompt.contains(marker))
            || request
                .messages
                .iter()
                .any(|message| message.content.contains(marker))
    });
    assert!(
        saw_marker,
        "resumed generation did not include prior marker {marker}"
    );
    app.shutdown().await.unwrap();
}
