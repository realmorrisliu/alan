use super::*;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn captured_clients_read_rotated_secrets_and_reject_logout_on_every_request_path() {
    let temp = tempfile::tempdir().unwrap();
    let store = SecretStore::from_directory(temp.path()).unwrap();
    store.save("secret", "initial-key").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for key in ["chat-key", "generate-key", "stream-key"] {
            let (socket, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains(&format!("authorization: bearer {key}\r\n"))
            );
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            reader.read_exact(&mut vec![0; length]).await.unwrap();
            reader
                .get_mut()
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                )
                .await
                .unwrap();
        }
    });
    let connections: ConnectionsFile = serde_json::from_value(serde_json::json!({
        "version": 1,
        "profiles": {"main": {"provider": "openai_chat_completions", "credential_id": "secret",
            "settings": {"base_url": format!("http://{address}"), "model": "gpt-5.4"}}},
        "credentials": {"secret": {"kind": "secret_string", "provider_family": "openai_chat_completions",
            "label": "test", "backend": "host_credential_store"}}
    })).unwrap();
    let factory = ProductLlmClientFactory {
        credentials_dir: temp.path().to_path_buf(),
        keychain_service: None,
        managed_auth: None,
    };
    let mut captured = factory
        .create(&Config::default(), Some("main"), &connections)
        .unwrap();
    let request = || GenerationRequest::new().with_user_message("test");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        // This independent writer stands in for another CLI/instance using the shared store.
        let writer = SecretStore::from_directory(temp.path()).unwrap();
        writer.save("secret", "chat-key").unwrap();
        assert!(captured.chat(None, "test").await.is_err());
        writer.save("secret", "generate-key").unwrap();
        assert!(captured.generate(request()).await.is_err());
        writer.save("secret", "stream-key").unwrap();
        if let Ok(mut stream) = captured.generate_stream(request()).await {
            while stream.recv().await.is_some() {}
        }
        server.await.unwrap();
        writer.delete("secret").unwrap();
        assert!(
            captured
                .chat(None, "test")
                .await
                .unwrap_err()
                .to_string()
                .contains("missing a secret")
        );
        assert!(
            captured
                .generate(request())
                .await
                .unwrap_err()
                .to_string()
                .contains("missing a secret")
        );
        assert!(
            captured
                .generate_stream(request())
                .await
                .unwrap_err()
                .to_string()
                .contains("missing a secret")
        );
    })
    .await
    .unwrap();
}
