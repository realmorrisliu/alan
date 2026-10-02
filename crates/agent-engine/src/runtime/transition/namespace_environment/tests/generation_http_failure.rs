use super::*;

#[tokio::test]
async fn local_http_openrouter401_engine_failed_commit() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for direct in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("HTTP fixture bind");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            count.fetch_add(1, AtomicOrdering::SeqCst);
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
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
            let body = "secretbodymarker https://private.example";
            socket.write_all(format!("HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept())
                    .await
                    .is_err(),
                "unexpected second HTTP request"
            );
        });
        let llmfs = Arc::new(LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(alan_llm::OpenRouterClient::with_params("test", &url, "test").unwrap()),
        );
        let mut ns = Namespace::new();
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
        let request = GenerationRequest::new().with_user_message("hello");
        let mut emitted = Vec::new();
        let error = if direct {
            let client = NamespaceClient::new(environment.root_transport());
            let doc =
                super::super::generation::LlmRequestDoc::from_generation_request(&request).unwrap();
            super::super::generation::start_generation(
                &client,
                "default",
                &serde_json::to_vec(&doc).unwrap(),
            )
            .await
            .unwrap_err()
        } else {
            let mut emit = |event| {
                emitted.push(event);
                async {}
            };
            environment
                .generation()
                .generate_with_text_events_controlled(
                    &request,
                    &mut emit,
                    2,
                    &CancellationToken::new(),
                )
                .await
                .unwrap_err()
        };
        assert_eq!(
            error.to_string(),
            "llmfs generation failed: stream_error:authentication"
        );
        assert_eq!(error.downcast_ref::<ErrorCode>(), Some(&ErrorCode::Io));
        assert!(
            !emitted
                .iter()
                .any(|event| matches!(event, alan_agent_protocol::Event::TextDelta { .. }))
        );
        let shell = Shell::new(root);
        let status = shell
            .cat("/mnt/llm/connections/default/g0/status")
            .await
            .unwrap();
        let status: serde_json::Value = serde_json::from_slice(&status).unwrap();
        assert_eq!(status["status"], "error");
        assert!(
            shell
                .cat("/mnt/llm/connections/default/g1/status")
                .await
                .is_err(),
            "exactly one generation"
        );
        let events = String::from_utf8(
            shell
                .cat("/mnt/llm/connections/default/g0/events")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            events
                .lines()
                .filter(
                    |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["error"]
                        == "stream_error:authentication"
                )
                .count(),
            1
        );
        for projection in [format!("{error:#}"), status.to_string(), events] {
            assert!(!projection.contains("secretbodymarker"));
            assert!(!projection.contains("private.example"));
            assert!(!projection.contains(&url));
        }
        server.await.unwrap();
        assert_eq!(requests.load(AtomicOrdering::SeqCst), 1);
    }
}
