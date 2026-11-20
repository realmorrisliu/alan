use super::*;
use crate::storage::{AuthStore, StoredChatgptAuth};
use crate::token_data::{ChatgptIdTokenInfo, ChatgptTokenData};
use base64::Engine;
use chrono::{Duration as ChronoDuration, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn fake_id_token_with_email(email: &str) -> String {
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!(r#"{{"email":"{email}"}}"#));
    format!("{header}.{payload}.sig")
}

async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
    }
    let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    while buf.len() < header_end + content_length {
        let n = socket.read(&mut chunk).await.unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    String::from_utf8_lossy(&buf).to_string()
}

#[test]
fn parses_desktop_and_flat_google_oauth_client_json() {
    let dir = TempDir::new().unwrap();
    let desktop_path = dir.path().join("desktop.json");
    fs::write(
        &desktop_path,
        r#"{"installed":{"client_id":"cid-desktop","client_secret":"sec-desktop"}}"#,
    )
    .unwrap();
    let flat_path = dir.path().join("flat.json");
    fs::write(
        &flat_path,
        r#"{"client_id":"cid-flat","client_secret":"sec-flat"}"#,
    )
    .unwrap();

    if std::env::var_os(ENV_GOOGLE_OAUTH_CLIENT_ID).is_none() {
        let desktop = discover_google_oauth_client(Some(&desktop_path)).unwrap();
        assert_eq!(desktop.client_id, "cid-desktop");
        assert_eq!(desktop.client_secret.as_deref(), Some("sec-desktop"));

        let flat = discover_google_oauth_client(Some(&flat_path)).unwrap();
        assert_eq!(flat.client_id, "cid-flat");
        assert_eq!(flat.client_secret.as_deref(), Some("sec-flat"));
    }
}

#[tokio::test]
async fn browser_pkce_flow_persists_google_auth_and_preserves_chatgpt() {
    let dir = TempDir::new().unwrap();
    let storage_path = dir.path().join("auth.json");
    let storage = AuthStorage::new(storage_path.clone()).unwrap();
    storage
        .save(&AuthStore {
            version: 1,
            chatgpt: Some(StoredChatgptAuth {
                tokens: ChatgptTokenData {
                    id_token: ChatgptIdTokenInfo {
                        email: Some("gpt@example.com".to_string()),
                        plan_type: Some("pro".to_string()),
                        account_id: Some("acct_1".to_string()),
                        user_id: Some("usr_1".to_string()),
                        raw_jwt: fake_id_token_with_email("gpt@example.com"),
                    },
                    access_token: fake_id_token_with_email("gpt@example.com"),
                    refresh_token: "gpt-refresh".to_string(),
                },
                account_id: "acct_1".to_string(),
                email: Some("gpt@example.com".to_string()),
                plan_type: Some("pro".to_string()),
                user_id: Some("usr_1".to_string()),
                access_token_expires_at: None,
                last_refresh_at: None,
            }),
            google: None,
        })
        .unwrap();

    let token_listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let token_addr = token_listener.local_addr().unwrap();
    let id_token = fake_id_token_with_email("vertex@example.com");

    let token_server = tokio::spawn(async move {
        let (mut socket, _) = token_listener.accept().await.unwrap();
        let req = read_http_request(&mut socket).await;
        let (_, body) = req.split_once("\r\n\r\n").unwrap();
        let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(
            form.get("grant_type").map(String::as_str),
            Some("authorization_code")
        );
        assert_eq!(
            form.get("code").map(String::as_str),
            Some("google-auth-code")
        );
        assert_eq!(
            form.get("client_id").map(String::as_str),
            Some("test-client-id")
        );
        assert_eq!(
            form.get("client_secret").map(String::as_str),
            Some("test-client-secret")
        );

        let resp_body = format!(
            r#"{{"access_token":"goog-access-1","refresh_token":"goog-refresh-1","expires_in":3600,"id_token":"{id_token}"}}"#
        );
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        socket.write_all(reply.as_bytes()).await.unwrap();
        form.get("code_verifier").cloned().unwrap()
    });

    let manager = GoogleAuthManager::new(GoogleAuthConfig {
        storage_path: storage_path.clone(),
        auth_url: GOOGLE_AUTH_URL.to_string(),
        token_url: format!("http://{token_addr}/token"),
        projects_url: GOOGLE_PROJECTS_URL.to_string(),
        client_id: Some("test-client-id".to_string()),
        client_secret: Some("test-client-secret".to_string()),
        callback_port: 0,
    })
    .unwrap();

    let pending = manager.start_browser_login().await.unwrap();
    let parsed_auth = Url::parse(pending.authorize_url()).unwrap();
    let params: HashMap<_, _> = parsed_auth.query_pairs().into_owned().collect();
    assert_eq!(
        params.get("scope").map(String::as_str),
        Some(GOOGLE_CLOUD_PLATFORM_SCOPE)
    );
    assert_eq!(
        params.get("access_type").map(String::as_str),
        Some("offline")
    );
    assert_eq!(params.get("prompt").map(String::as_str), Some("consent"));
    assert_eq!(
        params.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    let challenge = params.get("code_challenge").unwrap().clone();
    let state = params.get("state").unwrap().clone();

    let callback = format!(
        "{}?code=google-auth-code&state={state}",
        pending.callback_url()
    );
    let browser_client = tokio::spawn(async move {
        let resp = reqwest::get(callback).await.unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
    });

    let success = pending
        .wait_and_exchange(Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(success.email.as_deref(), Some("vertex@example.com"));
    browser_client.await.unwrap();

    let verifier = token_server.await.unwrap();
    let expected_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    assert_eq!(challenge, expected_challenge);

    let persisted = storage.load().unwrap();
    assert!(persisted.chatgpt.is_some());
    let google = persisted.google.unwrap();
    assert_eq!(google.access_token, "goog-access-1");
    assert_eq!(google.refresh_token, "goog-refresh-1");
    assert_eq!(google.email.as_deref(), Some("vertex@example.com"));
    assert_eq!(manager.access_token().await.unwrap(), "goog-access-1");
}

#[tokio::test]
async fn single_flight_refresh_coalesces_concurrent_requests() {
    let dir = TempDir::new().unwrap();
    let storage_path = dir.path().join("auth.json");
    let storage = AuthStorage::new(storage_path.clone()).unwrap();

    let token_listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let token_addr = token_listener.local_addr().unwrap();
    let refresh_calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = refresh_calls.clone();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = token_listener.accept().await {
            let calls = calls_clone.clone();
            tokio::spawn(async move {
                let req = read_http_request(&mut socket).await;
                let (_, body) = req.split_once("\r\n\r\n").unwrap();
                let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
                    .into_owned()
                    .collect();
                assert_eq!(
                    form.get("grant_type").map(String::as_str),
                    Some("refresh_token")
                );
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(40)).await;
                let resp_body = r#"{"access_token":"fresh-access","refresh_token":"rotated-refresh","expires_in":3600}"#;
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_body.len(),
                    resp_body
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            });
        }
    });

    storage
        .save(&AuthStore {
            version: 1,
            chatgpt: None,
            google: Some(StoredGoogleAuth {
                refresh_token: "old-refresh".to_string(),
                access_token: "stale-access".to_string(),
                expires_at: Some(Utc::now() - ChronoDuration::minutes(10)),
                client_id: Some("cid".to_string()),
                client_secret: Some("csec".to_string()),
                token_uri: Some(format!("http://{token_addr}/token")),
                email: Some("vertex@example.com".to_string()),
                last_refresh_at: None,
            }),
        })
        .unwrap();

    let manager = GoogleAuthManager::new(GoogleAuthConfig {
        storage_path: storage_path.clone(),
        auth_url: GOOGLE_AUTH_URL.to_string(),
        token_url: format!("http://{token_addr}/token"),
        projects_url: GOOGLE_PROJECTS_URL.to_string(),
        client_id: Some("cid".to_string()),
        client_secret: Some("csec".to_string()),
        callback_port: 0,
    })
    .unwrap();

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let mgr = manager.clone();
        set.spawn(async move { mgr.access_token().await });
    }
    while let Some(res) = set.join_next().await {
        assert_eq!(res.unwrap().unwrap(), "fresh-access");
    }
    assert_eq!(refresh_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        storage.load().unwrap().google.unwrap().refresh_token,
        "rotated-refresh"
    );
}

#[tokio::test]
async fn adc_authorized_user_file_exchanges_token_and_respects_priority_chain() {
    let dir = TempDir::new().unwrap();
    let adc_path = dir.path().join("adc.json");
    let storage_path = dir.path().join("auth.json");

    let token_listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let token_addr = token_listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = token_listener.accept().await {
            let req = read_http_request(&mut socket).await;
            let (_, body) = req.split_once("\r\n\r\n").unwrap();
            let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
                .into_owned()
                .collect();
            assert_eq!(
                form.get("refresh_token").map(String::as_str),
                Some("adc-refresh-tok")
            );
            let resp_body = r#"{"access_token":"adc-access-tok","expires_in":3600}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        }
    });

    fs::write(
        &adc_path,
        format!(
            r#"{{
                "type": "authorized_user",
                "client_id": "adc-cid",
                "client_secret": "adc-secret",
                "refresh_token": "adc-refresh-tok",
                "token_uri": "http://{token_addr}/token"
            }}"#
        ),
    )
    .unwrap();

    // Without auth.json, priority chain uses ADC file directly without invoking gcloud.
    let resolved =
        resolve_vertex_access_token_with_paths(Some(&storage_path), Some(&adc_path), None, false)
            .await
            .unwrap();
    assert_eq!(resolved.source, GoogleCredentialSource::AdcAuthorizedUser);
    assert_eq!(resolved.access_token, "adc-access-tok");

    // Once auth.json has fresh managed Google OAuth, managed auth takes priority over ADC.
    let storage = AuthStorage::new(storage_path.clone()).unwrap();
    storage
        .save(&AuthStore {
            version: 1,
            chatgpt: None,
            google: Some(StoredGoogleAuth {
                refresh_token: "managed-refresh".to_string(),
                access_token: "managed-access".to_string(),
                expires_at: Some(Utc::now() + ChronoDuration::hours(1)),
                client_id: Some("cid".to_string()),
                client_secret: None,
                token_uri: None,
                email: None,
                last_refresh_at: Some(Utc::now()),
            }),
        })
        .unwrap();

    let resolved =
        resolve_vertex_access_token_with_paths(Some(&storage_path), Some(&adc_path), None, false)
            .await
            .unwrap();
    assert_eq!(resolved.source, GoogleCredentialSource::ManagedOAuth);
    assert_eq!(resolved.access_token, "managed-access");
}

#[tokio::test]
async fn list_active_gcp_projects_follows_pagination_and_sorts() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        for page in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let req = read_http_request(&mut socket).await;
            assert!(
                req.contains("Authorization: Bearer gcp-tok")
                    || req.contains("authorization: Bearer gcp-tok")
            );
            let body = if page == 0 {
                r#"{"projects":[{"projectId":"zeta-proj","name":"Zeta","lifecycleState":"ACTIVE"},{"projectId":"dead-proj","name":"Dead","lifecycleState":"DELETE_REQUESTED"}],"nextPageToken":"p2"}"#
            } else {
                assert!(req.contains("pageToken=p2"));
                r#"{"projects":[{"projectId":"alpha-proj","name":"Alpha","lifecycleState":"ACTIVE"}]}"#
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(resp.as_bytes()).await.unwrap();
        }
    });

    let projects = list_active_gcp_projects_at(
        &format!("http://{addr}/v1/projects?filter=lifecycleState:ACTIVE"),
        "gcp-tok",
    )
    .await
    .unwrap();
    let ids: Vec<&str> = projects.iter().map(|p| p.project_id.as_str()).collect();
    assert_eq!(ids, vec!["alpha-proj", "zeta-proj"]);
}
