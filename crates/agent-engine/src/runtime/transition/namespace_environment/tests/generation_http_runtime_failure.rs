//! Exact accepted-input settlement through the public Runtime, real HTTP and stores.
use super::*;
use crate::rollout::{RolloutItem, RolloutRecorder};
use crate::runtime::{AgentProcessConfig, spawn_with_namespace_environment};
use alan_agent_protocol::{Submission, UiEvent, UiInputStatus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn local_http_openrouter401_runtime_exact_input_failed() {
    local_http_runtime_exact_input(HttpOutcome::Unauthorized).await;
}

#[tokio::test]
async fn local_http_anthropic_refusal_runtime_exact_input_failed() {
    local_http_runtime_exact_input(HttpOutcome::Refusal).await;
}

#[tokio::test]
async fn local_http_openrouter408_runtime_retries_exact_input() {
    local_http_runtime_exact_input(HttpOutcome::TimeoutThenSuccess).await;
}

#[tokio::test]
async fn local_http_openrouter404_runtime_does_not_retry() {
    local_http_runtime_exact_input(HttpOutcome::NotFound).await;
}

#[derive(Clone, Copy)]
enum HttpOutcome {
    Unauthorized,
    Refusal,
    TimeoutThenSuccess,
    NotFound,
}

async fn local_http_runtime_exact_input(outcome: HttpOutcome) {
    let anthropic_refusal = matches!(outcome, HttpOutcome::Refusal);
    let retries = matches!(outcome, HttpOutcome::TimeoutThenSuccess);
    let expected_reason = match outcome {
        HttpOutcome::Unauthorized => "stream_error:authentication",
        HttpOutcome::Refusal => "stream_error:safety",
        HttpOutcome::TimeoutThenSuccess => "stream_error:timeout",
        HttpOutcome::NotFound => "stream_error:http",
    };
    let provider = if anthropic_refusal {
        "anthropic_messages"
    } else {
        "openrouter"
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Runtime HTTP fixture bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let stop = CancellationToken::new();
    let server_stop = stop.clone();
    // Serve every attempted request, so a retry defect fails the exact-count
    // assertion rather than masquerading as an HTTP timeout/connection error.
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = tokio::select! {
                _ = server_stop.cancelled() => break,
                accepted = listener.accept() => accepted.unwrap(),
            };
            let attempt = count.fetch_add(1, AtomicOrdering::SeqCst);
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let n = tokio::select! {
                    _ = server_stop.cancelled() => return,
                    read = socket.read(&mut chunk) => read.unwrap(),
                };
                assert!(n > 0, "incomplete Runtime HTTP request");
                request.extend_from_slice(&chunk[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let (status, content_type, body) = if anthropic_refusal {
                (
                    "200 OK",
                    "text/event-stream",
                    concat!(
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"test\",\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"unexecuted\",\"name\":\"Bash\",\"input\":{}}}\n\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\\\"echo unexecuted-refusal-tool\\\"}\"}}\n\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"},\"usage\":{\"output_tokens\":5}}\n\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                    ),
                )
            } else if retries && attempt > 0 {
                (
                    "200 OK",
                    "text/event-stream",
                    concat!(
                        "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"recovered-http-timeout\"},\"finish_reason\":\"stop\"}]}\n\n",
                        "data: [DONE]\n\n",
                    ),
                )
            } else {
                (
                    match outcome {
                        HttpOutcome::TimeoutThenSuccess => "408 Request Timeout",
                        HttpOutcome::NotFound => "404 Not Found",
                        _ => "401 Unauthorized",
                    },
                    "text/plain",
                    "secretbodymarker https://private.example",
                )
            };
            socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    let stores = tempfile::tempdir().unwrap();
    let llmfs = Arc::new(LlmFs::new());
    if anthropic_refusal {
        llmfs.register_connection(
            "default",
            Box::new(alan_llm::AnthropicMessagesClient::with_params(
                "test", &url, "test",
            )),
        );
    } else {
        llmfs.register_connection(
            "default",
            Box::new(alan_llm::OpenRouterClient::with_params("test", &url, "test").unwrap()),
        );
    }
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(AgentFs::new())),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
    let shell = Shell::new(root);
    let mut core = if anthropic_refusal {
        crate::Config::for_anthropic_messages("test", Some(&url), Some("test"))
    } else {
        let mut config = crate::Config::default();
        config.llm_provider = crate::config::LlmProvider::OpenRouter;
        config.openrouter_model = "test".into();
        config
    };
    core.memory.enabled = false;
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: stores.path().join("rollouts"),
                checkpoints: stores.path().join("checkpoints"),
                cache: stores.path().join("cache"),
                tmp: stores.path().join("tmp"),
                metadata: stores.path().join("metadata"),
            }),
            ..Default::default()
        },
        environment,
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let input = Submission::new(Op::Input {
        parts: vec![ContentPart::text(
            "HTTP terminal failure must settle this input",
        )],
        mode: InputMode::FollowUp,
    });
    let id = input.id.clone();
    runtime
        .handle
        .submission_tx
        .send(input.clone())
        .await
        .unwrap();
    // Capture assertions only after normal shutdown so durable writer flush and
    // HTTP server completion are part of the owning lifecycle, not task aborts.
    let observed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let ui = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
            let events: Vec<UiEvent> = ui.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
            if events.iter().any(|event| matches!(event, UiEvent::InputCompleted { submission_ids, .. } if submission_ids == std::slice::from_ref(&id)))
                && (retries || events.iter().any(|event| matches!(event, UiEvent::Error { .. })))
                && matches!(events.last(), Some(UiEvent::Activity { snapshot }) if snapshot.state == alan_agent_protocol::UiActivityState::Idle)
            { break (ui, events); }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await;
    let shutdown =
        tokio::time::timeout(std::time::Duration::from_secs(5), runtime.shutdown()).await;
    stop.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .expect("bounded HTTP fixture cleanup")
        .unwrap();
    shutdown.expect("bounded Runtime shutdown").unwrap();
    let (ui, events) = observed.expect("exact Runtime input terminal and idle");
    let completed: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            UiEvent::InputCompleted {
                submission_ids,
                status,
                error,
            } if submission_ids == std::slice::from_ref(&id) => Some((status, error)),
            _ => None,
        })
        .collect();
    assert_eq!(completed.len(), 1, "one exact-ID terminal");
    assert_eq!(
        *completed[0].0,
        if retries {
            UiInputStatus::Completed
        } else {
            UiInputStatus::Failed
        }
    );
    if retries {
        assert!(completed[0].1.is_none());
    } else {
        assert!(
            completed[0].1.as_ref().unwrap().contains(expected_reason),
            "terminal={:?}",
            completed[0]
        );
    }
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, UiEvent::Error { .. }))
            .count(),
        usize::from(!retries)
    );
    let history = RolloutRecorder::load_history(&path).await.unwrap();
    let admission: Vec<_> = history
        .iter()
        .filter_map(|item| match item {
            RolloutItem::Event(event)
                if event.event_type == "machine_input_admitted_v1" && event.payload["id"] == id =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(admission.len(), 1);
    assert_eq!(
        admission[0].payload["op"],
        serde_json::to_value(&input.op).unwrap()
    );
    assert_eq!(
        admission[0].payload["callable_binding"]["provider"],
        provider
    );
    assert_eq!(history.iter().filter(|item| matches!(item,
        RolloutItem::Event(event) if event.event_type == "machine_input_dispatched_v1" && event.payload["submission_id"] == id
    )).count(), 1);
    // Dispatch is this executed input's durable exclusion owner; removal is
    // reserved for inputs excluded without dispatch. Do not synthesize either.
    assert!(!history.iter().any(|item| matches!(item, RolloutItem::Event(event)
        if (event.event_type == "machine_inputs_removed_v1" && event.payload["submission_ids"].as_array().is_some_and(|ids| ids.iter().any(|value| value == &id)))
        || (event.event_type == "machine_input_removed_v1" && event.payload["submission_id"] == id))));
    let recovered = crate::agent_machine::AgentMachine::load_from_rollout_in_dir(
        &path,
        "/agent/recovered",
        "test",
        stores.path(),
    )
    .await
    .unwrap();
    {
        let queue = recovered.input_queue();
        let queue = queue.lock().unwrap();
        assert!(queue.admitted_ids.contains(&id));
        assert!(queue.settled_ids.contains(&id));
        assert!(queue.pending.is_empty());
        assert!(!queue.bindings.contains_key(&id));
    }
    let durable = tokio::fs::read_to_string(&path).await.unwrap();
    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    if retries {
        assert!(tape.contains("recovered-http-timeout"));
        assert_eq!(history.iter().filter(|item| matches!(item, RolloutItem::Message(message) if message.role == "assistant")).count(), 1);
        assert!(
            !history
                .iter()
                .any(|item| matches!(item, RolloutItem::ToolCall(_)))
        );
    } else {
        assert!(
            !tape.lines().any(
                |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["role"]
                    == "assistant"
            ),
            "no text/fallback execution"
        );
        assert!(
            !history.iter().any(
                |item| matches!(item, RolloutItem::Message(message) if message.role == "assistant")
                    || matches!(item, RolloutItem::ToolCall(_))
                    || matches!(item, RolloutItem::Event(event) if event.event_type == "text_delta")
            ),
            "no assistant TextDelta, fallback or Tool execution"
        );
    }
    let status = String::from_utf8(
        shell
            .cat("/mnt/llm/connections/default/g0/status")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&status).unwrap()["status"],
        "error"
    );
    let generation_events = String::from_utf8(
        shell
            .cat("/mnt/llm/connections/default/g0/events")
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        !generation_events
            .lines()
            .any(
                |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["text"].is_string()
            )
    );
    assert!(generation_events.contains(expected_reason));
    if anthropic_refusal {
        assert!(
            generation_events.contains("unexecuted-refusal-tool"),
            "fixture delivered Tool arguments before refusal"
        );
    }
    for projection in [&ui, &durable, &tape, &status, &generation_events] {
        assert!(!projection.contains("secretbodymarker"));
        assert!(!projection.contains("private.example"));
        assert!(!projection.contains(&url));
        for raw_status in ["401 Unauthorized", "408 Request Timeout", "404 Not Found"] {
            assert!(!projection.contains(raw_status));
        }
    }
    assert_eq!(
        requests.load(AtomicOrdering::SeqCst),
        if retries { 2 } else { 1 },
        "exact HTTP attempt count"
    );
    if retries {
        let retry_status = String::from_utf8(
            shell
                .cat("/mnt/llm/connections/default/g1/status")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&retry_status).unwrap()["status"],
            "done"
        );
        let retry_events = String::from_utf8(
            shell
                .cat("/mnt/llm/connections/default/g1/events")
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(retry_events.contains("recovered-http-timeout"));
        for projection in [&retry_status, &retry_events] {
            assert!(!projection.contains("secretbodymarker"));
            assert!(!projection.contains("private.example"));
            assert!(!projection.contains(&url));
            assert!(!projection.contains("408 Request Timeout"));
        }
        assert!(
            shell
                .cat("/mnt/llm/connections/default/g2/status")
                .await
                .is_err(),
            "no extra generation"
        );
    } else {
        assert!(
            shell
                .cat("/mnt/llm/connections/default/g1/status")
                .await
                .is_err(),
            "one generation"
        );
    }
}
