use crate::*;
#[tokio::test]
async fn safe_failure_local_http_background_terminal_contract() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for status in [401, 429, 503] {
        for anthropic in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 8192];
                let read = socket.read(&mut request).await.unwrap();
                assert!(read > 0, "empty HTTP request");
                let body = "secret-token account-private https://private.example";
                socket.write_all(format!("HTTP/1.1 {status} Failure\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            let mut client: Box<dyn LlmProvider> = if anthropic {
                Box::new(AnthropicMessagesClient::with_params("test", &url, "test"))
            } else {
                Box::new(OpenAiChatCompletionsClient::official_with_params(
                    "test", &url, "gpt-5.4",
                ))
            };
            let mut rx = client
                .generate_stream(GenerationRequest::new().with_user_message("hello"))
                .await
                .unwrap();
            let terminal = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .expect("failure must emit terminal before payload");
            assert!(terminal.is_finished);
            let expected = match status {
                401 => "stream_error:authentication",
                429 => "stream_error:rate_limit",
                _ => "stream_error:unavailable",
            };
            assert_eq!(terminal.finish_reason.as_deref(), Some(expected));
            assert!(rx.recv().await.is_none(), "duplicate terminal");
            server.await.unwrap();
        }
    }
}
