use super::tests::build_jwt;
use super::*;
use serde_json::json;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn independent_refreshers_share_rotation_and_logout_wins_over_inflight_refresh() {
    use tokio::io::AsyncReadExt;
    for logout in [false, true] {
        let temp = TempDir::new().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = ChatgptAuthConfig {
            storage_path: temp.path().join("auth.json"),
            issuer: format!("http://{}", listener.local_addr().unwrap()),
            client_id: "test".into(),
            browser_callback_port: 1455,
        };
        let make_manager = || {
            let mut manager = ChatgptAuthManager::new(config.clone()).unwrap();
            std::sync::Arc::get_mut(&mut manager.inner).unwrap().client =
                reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(2))
                    .build()
                    .unwrap();
            manager
        };
        let first = make_manager();
        let second = make_manager();
        first
            .import_token_bundle(
                ImportedChatgptTokenBundle {
                    id_token: build_jwt(
                        json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-test"}}),
                    ),
                    access_token: build_jwt(json!({"exp": 1})),
                    refresh_token: "old-refresh".into(),
                },
                None,
            )
            .unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let new_token = build_jwt(json!({"exp": 4_102_444_800_i64}));
        let response = json!({"access_token": new_token, "refresh_token": "rotated"}).to_string();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0; 2048];
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                let raw = String::from_utf8_lossy(&request);
                if let Some(end) = raw.find("\r\n\r\n") {
                    let length = raw[..end]
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            started_tx.send(()).unwrap();
            release_rx.await.unwrap();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).as_bytes()).await.unwrap();
        });
        let refresh = tokio::spawn(async move { first.force_refresh_auth().await });
        tokio::time::timeout(Duration::from_secs(2), started_rx)
            .await
            .unwrap()
            .unwrap();
        let storage = AuthStorage::new(config.storage_path).unwrap();
        let concurrent = if logout {
            storage.clear_chatgpt().unwrap();
            None
        } else {
            let refresh = tokio::spawn(async move { second.request_auth().await });
            tokio::task::yield_now().await;
            Some(refresh)
        };
        release_tx.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(3), refresh)
            .await
            .unwrap()
            .unwrap();
        server.await.unwrap();
        if let Some(concurrent) = concurrent {
            let next = tokio::time::timeout(Duration::from_secs(3), concurrent)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(result.unwrap().access_token, new_token);
            assert_eq!(next.access_token, new_token);
            assert_eq!(
                storage
                    .load()
                    .unwrap()
                    .chatgpt
                    .unwrap()
                    .tokens
                    .refresh_token,
                "rotated"
            );
        } else {
            assert!(result.is_err());
            assert!(storage.load().unwrap().chatgpt.is_none());
        }
    }
}
