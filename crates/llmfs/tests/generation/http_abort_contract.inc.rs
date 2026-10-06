// Public LLMFS abort must reach a silent, still-open HTTP body.
#[tokio::test]
async fn local_http_abort_releases_silent_body() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    for provider in ["anthropic", "responses"] {
        for prefix in [false, true] {
            let case = format!("{provider} prefix={prefix}");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .unwrap_or_else(|error| panic!("{case}: bind: {error}"));
            let url = format!("http://{}", listener.local_addr().unwrap());
            let (ready_tx, ready) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                // Read the complete request, so remaining request bytes cannot
                // be mistaken for body cancellation below.
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert!(count > 0, "request closed before headers");
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n").await.unwrap();
                if prefix {
                    let event = if provider == "anthropic" {
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}\n\n"
                    } else {
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n"
                    };
                    socket.write_all(event.as_bytes()).await.unwrap();
                }
                ready_tx.send(()).unwrap();
                // No more bytes or EOF are sent by the fixture: only client
                // cancellation can end this pending transport.
                let count = socket.read(&mut buffer).await;
                assert!(
                    matches!(count, Ok(0)) || count.is_err(),
                    "unexpected client bytes"
                );
            });
            let fs = if provider == "anthropic" {
                llmfs_with(alan_llm::AnthropicMessagesClient::with_params(
                    "test", &url, "test",
                ))
            } else {
                llmfs_with(alan_llm::OpenAiResponsesClient::with_params(
                    "test", &url, "test",
                ))
            };
            let generation = clone_gen(&fs, Fid(1)).await;
            commit_request(&fs, &generation, Fid(2), SIMPLE_REQUEST)
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), ready)
                .await
                .unwrap_or_else(|error| panic!("{case}: HTTP fixture not ready: {error}"))
                .unwrap();
            fs.walk(
                Fid::ROOT,
                Fid(3),
                &[
                    "connections".into(),
                    "default".into(),
                    generation.clone(),
                    "events".into(),
                ],
            )
            .await
            .unwrap();
            fs.open(Fid(3), OpenMode::Read).await.unwrap();
            let mut events = String::new();
            if prefix {
                while !events.contains("partial") {
                    let bytes = tokio::time::timeout(
                        Duration::from_secs(2),
                        fs.read(Fid(3), events.len() as u64, 65536),
                    )
                    .await
                    .unwrap_or_else(|error| panic!("{case}: payload not projected: {error}"))
                    .unwrap();
                    events.push_str(std::str::from_utf8(&bytes).unwrap());
                }
            }
            fs.walk(
                Fid::ROOT,
                Fid(4),
                &[
                    "connections".into(),
                    "default".into(),
                    generation.clone(),
                    "ctl".into(),
                ],
            )
            .await
            .unwrap();
            fs.open(Fid(4), OpenMode::Write).await.unwrap();
            fs.write(Fid(4), 0, b"abort").await.unwrap();
            assert_eq!(
                status_of(&fs, &generation, Fid(5)).await,
                "aborted",
                "{case}"
            );
            tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .unwrap_or_else(|error| panic!("{case}: abort retained silent HTTP body: {error}"))
                .unwrap();
            let length = fs.stat(Fid(3)).await.unwrap().length;
            while (events.len() as u64) < length {
                let bytes = fs
                    .read(
                        Fid(3),
                        events.len() as u64,
                        (length - events.len() as u64) as u32,
                    )
                    .await
                    .unwrap();
                assert!(!bytes.is_empty(), "{case}: stat-bounded read empty");
                events.push_str(std::str::from_utf8(&bytes).unwrap());
            }
            let records: Vec<serde_json::Value> = events
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(
                records
                    .iter()
                    .filter(|record| record["aborted"] == true)
                    .count(),
                1,
                "{case}: {events}"
            );
            assert!(
                !records
                    .iter()
                    .any(|record| record["done"] == true || record["error"].is_string()),
                "{case}: {events}"
            );
        }
    }
}
