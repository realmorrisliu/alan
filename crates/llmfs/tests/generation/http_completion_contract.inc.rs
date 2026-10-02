// Real loopback fixtures: retained for the controller, not run in the author sandbox.
#[tokio::test]
async fn local_http_completion_failure_lifecycle() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let delta = "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"stop\"}]}\n\n";
    let is_terminal = |record: &serde_json::Value| {
        record["error"].is_string()
            || record["done"] == true
            || record["aborted"] == true
            || record["rejected"] == true
    };
    for router in [false, true] {
        // EOF, malformed JSON, and a short Content-Length body, before/after delta.
        for prefix in ["", delta] {
            for mode in ["eof", "parse", "body", "done"] {
                let case = format!(
                    "router={router} mode={mode} prefix={}",
                    if prefix.is_empty() { "empty" } else { "delta" }
                );
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .unwrap_or_else(|error| panic!("{case}: bind: {error}"));
                let url = format!("http://{}", listener.local_addr().unwrap());
                let body = format!(
                    "{prefix}{}",
                    match mode {
                        "parse" => "data: secret-token https://private.example malformed\n\n",
                        "done" => "data: [DONE]\n\n",
                        _ => "",
                    }
                );
                let length = body.len() + if mode == "body" { 100 } else { 0 };
                let server_case = case.clone();
                let server = tokio::spawn(async move {
                    let (mut socket, _) = listener
                        .accept()
                        .await
                        .unwrap_or_else(|error| panic!("{server_case}: accept: {error}"));
                    let mut request = [0; 8192];
                    assert!(
                        socket
                            .read(&mut request)
                            .await
                            .unwrap_or_else(|error| panic!("{server_case}: request read: {error}"))
                            > 0,
                        "{server_case}: empty request"
                    );
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}").as_bytes()).await.unwrap_or_else(|error| panic!("{server_case}: response write: {error}"));
                    socket
                        .shutdown()
                        .await
                        .unwrap_or_else(|error| panic!("{server_case}: shutdown: {error}"));
                });
                let fs = if router {
                    llmfs_with(
                        alan_llm::OpenRouterClient::with_params("test", &url, "test").unwrap(),
                    )
                } else {
                    llmfs_with(alan_llm::OpenAiChatCompletionsClient::official_with_params(
                        "test", &url, "test",
                    ))
                };
                let g = clone_gen(&fs, Fid(1)).await;
                commit_request(&fs, &g, Fid(2), SIMPLE_REQUEST)
                    .await
                    .unwrap_or_else(|error| panic!("{case}: commit: {error:?}"));
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
                .unwrap_or_else(|error| panic!("{case}: walk events: {error:?}"));
                fs.open(Fid(3), OpenMode::Read)
                    .await
                    .unwrap_or_else(|error| panic!("{case}: open events: {error:?}"));
                let mut events = String::new();
                loop {
                    let bytes = tokio::time::timeout(
                        Duration::from_secs(2),
                        fs.read(Fid(3), events.len() as u64, 65536),
                    )
                    .await
                    .unwrap_or_else(|error| {
                        panic!(
                            "{case}: timed out awaiting terminal event: {error}; events={events:?}"
                        )
                    })
                    .unwrap_or_else(|error| panic!("{case}: read events: {error:?}"));
                    assert!(
                        !bytes.is_empty(),
                        "{case}: empty read before terminal: {events}"
                    );
                    events.push_str(
                        std::str::from_utf8(&bytes)
                            .unwrap_or_else(|error| panic!("{case}: event UTF-8: {error}")),
                    );
                    // Only complete JSON lines are records; the live edge is not EOF.
                    if events
                        .split_inclusive('\n')
                        .filter(|line| line.ends_with('\n'))
                        .any(|line| {
                            let record: serde_json::Value = serde_json::from_str(line)
                                .unwrap_or_else(|error| {
                                    panic!("{case}: event JSON: {error}; line={line:?}")
                                });
                            is_terminal(&record)
                        })
                    {
                        break;
                    }
                }
                // Terminal observation ends live tailing. Drain only bytes already
                // appended according to stat; never read again at the live edge.
                let terminal_length = fs
                    .stat(Fid(3))
                    .await
                    .unwrap_or_else(|error| panic!("{case}: stat events: {error:?}"))
                    .length;
                assert!(
                    terminal_length >= events.len() as u64,
                    "{case}: events length shrank"
                );
                while (events.len() as u64) < terminal_length {
                    let remaining = (terminal_length - events.len() as u64).min(65536) as u32;
                    let bytes = tokio::time::timeout(
                        Duration::from_secs(2),
                        fs.read(Fid(3), events.len() as u64, remaining),
                    )
                    .await
                    .unwrap_or_else(|error| {
                        panic!("{case}: timed out draining stat-bounded events: {error}")
                    })
                    .unwrap_or_else(|error| panic!("{case}: drain events: {error:?}"));
                    assert!(!bytes.is_empty(), "{case}: empty read before stat length");
                    events
                        .push_str(std::str::from_utf8(&bytes).unwrap_or_else(|error| {
                            panic!("{case}: drained event UTF-8: {error}")
                        }));
                }
                assert!(
                    events.ends_with('\n'),
                    "{case}: incomplete event record: {events}"
                );
                let records: Vec<serde_json::Value> = events
                    .lines()
                    .map(|line| {
                        serde_json::from_str(line).unwrap_or_else(|error| {
                            panic!("{case}: event JSON: {error}; line={line:?}")
                        })
                    })
                    .collect();
                assert_eq!(
                    records.iter().filter(|record| is_terminal(record)).count(),
                    1,
                    "{case}: exactly one terminal: {events}"
                );
                let errors: Vec<_> = records
                    .iter()
                    .filter(|record| record["error"].is_string())
                    .collect();
                let done = records
                    .iter()
                    .filter(|record| record["done"] == true)
                    .count();
                let payload: String = records
                    .iter()
                    .filter_map(|record| record["text"].as_str())
                    .collect();
                assert_eq!(
                    payload,
                    if prefix.is_empty() { "" } else { "partial" },
                    "{case}: exact payload: {events}"
                );
                if !prefix.is_empty() {
                    let payload_index = records
                        .iter()
                        .position(|record| record["text"] == "partial")
                        .unwrap_or_else(|| panic!("{case}: missing partial payload"));
                    let terminal_index = records
                        .iter()
                        .position(is_terminal)
                        .unwrap_or_else(|| panic!("{case}: missing terminal"));
                    assert!(
                        payload_index < terminal_index,
                        "{case}: payload must precede terminal: {events}"
                    );
                }
                assert!(
                    !events.contains("secret-token") && !events.contains("private.example"),
                    "{case}: sensitive sentinel leaked: {events}"
                );
                if mode == "done" {
                    assert!(errors.is_empty(), "{case}: normal completion: {events}");
                    assert_eq!(done, 1, "{case}: single done: {events}");
                    assert_eq!(status_of(&fs, &g, Fid(4)).await, "done", "{case}: status");
                } else {
                    let expected = match mode {
                        "parse" => "stream_error:parse",
                        "body" => "stream_error:body",
                        _ => "stream_error:closed",
                    };
                    assert_eq!(errors.len(), 1, "{case}: single error: {events}");
                    assert_eq!(
                        errors[0]["error"], expected,
                        "{case}: error category: {events}"
                    );
                    assert_eq!(done, 0, "{case}: no successful terminal: {events}");
                    assert_eq!(
                        records
                            .iter()
                            .filter(|record| record["finish_reason"] == expected)
                            .count(),
                        1,
                        "{case}: single finish projection: {events}"
                    );
                    assert_eq!(status_of(&fs, &g, Fid(4)).await, "error", "{case}: status");
                }
                server
                    .await
                    .unwrap_or_else(|error| panic!("{case}: server task: {error}"));
            }
        }
    }
}
