use std::collections::BTreeMap;
use std::sync::Arc;

use alan_agent_engine::Config;
use alan_agent_engine::LlmProvider as ProviderId;
use alan_agent_engine::runtime::model_binding::ConnectionAuthority;
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, Credentials, LiveNamespace, Namespace, ProcessRunner};
use alan_shell::Shell;
use chrono::Utc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::connection::ConnectionCommand;
use crate::connection::process_binding::ProcessConnection;
use crate::connection::provider_packages;
use crate::connection_profile::{
    ConnectionCredential, ConnectionProfile, CredentialKind, default_credential_backend,
    first_party_provider_catalog,
};
use crate::quartermaster::{QUARTERMASTER_EXECUTABLE, QuartermasterProcessRunner};
use crate::{ConnectionService, PackageService};

async fn spawn_mock_anthropic_http_server(expected_api_key: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected_key = expected_api_key.to_string();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let expected = expected_key.clone();
            tokio::spawn(async move {
                let mut raw = vec![0u8; 32 * 1024];
                let mut filled = 0usize;
                loop {
                    let n = socket.read(&mut raw[filled..]).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    filled += n;
                    let text = String::from_utf8_lossy(&raw[..filled]);
                    if let Some(header_end) = text.find("\r\n\r\n") {
                        let headers = &text[..header_end];
                        let content_len = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.trim().eq_ignore_ascii_case("content-length") {
                                    value.trim().parse::<usize>().ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0);
                        if filled >= header_end + 4 + content_len {
                            let seen_key = headers.lines().find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                if name.trim().eq_ignore_ascii_case("x-api-key") {
                                    Some(value.trim().to_string())
                                } else {
                                    None
                                }
                            });
                            let body_text = String::from_utf8_lossy(
                                &raw[header_end + 4..header_end + 4 + content_len],
                            );
                            let response = if seen_key.as_deref() == Some(expected.as_str()) {
                                if body_text.contains("\"stream\":true") {
                                    let sse = concat!(
                                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-4-6\",\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n",
                                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello from anthropic package\"}}\n\n",
                                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
                                        "data: {\"type\":\"message_stop\"}\n\n",
                                    );
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        sse.len(),
                                        sse
                                    )
                                } else {
                                    let body = serde_json::json!({
                                        "id": "msg_test_1",
                                        "type": "message",
                                        "role": "assistant",
                                        "model": "claude-sonnet-4-6",
                                        "content": [{ "type": "text", "text": "hello from anthropic package" }],
                                        "stop_reason": "end_turn",
                                        "usage": { "input_tokens": 5, "output_tokens": 4 }
                                    })
                                    .to_string();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                        body.len(),
                                        body
                                    )
                                }
                            } else {
                                let body = "{\"error\":{\"type\":\"authentication_error\",\"message\":\"invalid api key\"}}";
                                format!(
                                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                    body.len(),
                                    body
                                )
                            };
                            let _ = socket.write_all(response.as_bytes()).await;
                            let _ = socket.shutdown().await;
                            return;
                        }
                    }
                }
            });
        }
    });

    format!("http://{addr}/v1")
}

#[derive(Debug)]
struct HostSecretBackedFactory {
    secrets: std::sync::Mutex<BTreeMap<String, String>>,
}

impl crate::LlmClientFactory for HostSecretBackedFactory {
    fn create(
        &self,
        base_config: &Config,
        profile_id: Option<&str>,
        connections: &crate::ConnectionsFile,
    ) -> anyhow::Result<alan_agent_engine::LlmClient> {
        let mut config = base_config.clone();
        let resolved = connections.apply_profile_metadata_to_config(profile_id, &mut config)?;
        if let Some(credential_id) = resolved.credential_id.as_deref()
            && let Some(secret) = self.secrets.lock().unwrap().get(credential_id)
        {
            match resolved.provider {
                ProviderId::AnthropicMessages => {
                    config.anthropic_messages_api_key = Some(secret.clone());
                }
                ProviderId::OpenAiResponses => {
                    config.openai_responses_api_key = Some(secret.clone());
                }
                _ => {}
            }
        }
        alan_agent_engine::LlmClient::from_config(config.to_provider_config()?)
    }
}

#[tokio::test]
async fn bundled_anthropic_provider_install_bind_invoke_and_uninstall_revokes_availability() {
    assert!(
        !first_party_provider_catalog()
            .iter()
            .any(|descriptor| descriptor.provider_id == ProviderId::AnthropicMessages),
        "Anthropic must stay out of first_party_provider_catalog"
    );

    let package_dir =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/anthropic-provider");
    assert!(package_dir.join("alan-package.toml").is_file());

    let packages = PackageService::ephemeral().unwrap();
    let connections = ConnectionService::ephemeral().unwrap();
    connections.attach_package_service(Arc::clone(&packages));

    // Before installing `anthropic-provider`, both `pkg:anthropic-provider` and `anthropic` fail!
    assert!(
        provider_packages::resolve_provider_selection("pkg:anthropic-provider", Some(&packages))
            .is_err()
    );
    assert!(provider_packages::resolve_provider_selection("anthropic", Some(&packages)).is_err());

    // Install bundled `anthropic-provider` cleanly through `/bin/q` + `/mnt/package`.
    let q_res = crate::run_q_command_with_package_service(
        &packages,
        Some(&package_dir),
        &[
            "install".to_string(),
            "--name".to_string(),
            "anthropic-provider".to_string(),
            crate::CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        q_res.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&q_res.output)
    );

    // Verify package contains NO embedded secret and binds via ConnectionProfile + Host secret.
    let (selected_provider, selected_pkg) =
        provider_packages::resolve_provider_selection("pkg:anthropic-provider", Some(&packages))
            .unwrap();
    assert_eq!(selected_provider, ProviderId::AnthropicMessages);
    let (pkg_id, pkg_export) = selected_pkg.unwrap();
    assert_eq!(pkg_id, "anthropic-provider");

    let mock_url = spawn_mock_anthropic_http_server("host-secret-anthropic-key").await;
    let factory = Arc::new(HostSecretBackedFactory {
        secrets: std::sync::Mutex::new(BTreeMap::from([(
            "claude-cred".to_string(),
            "host-secret-anthropic-key".to_string(),
        )])),
    });
    connections
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            factory,
            Config::default(),
            None,
        )
        .await
        .unwrap();

    let mut metadata = connections.metadata();
    metadata.credentials.insert(
        "claude-cred".to_string(),
        ConnectionCredential {
            kind: CredentialKind::SecretString,
            provider_family: ProviderId::AnthropicMessages,
            label: "Host Anthropic key".to_string(),
            backend: default_credential_backend(CredentialKind::SecretString).to_string(),
        },
    );
    let settings = provider_packages::normalize_settings_with_provider_package(
        ProviderId::AnthropicMessages,
        Some(&pkg_export),
        &BTreeMap::from([("base_url".to_string(), mock_url)]),
    )
    .unwrap();
    metadata.profiles.insert(
        "claude-main".to_string(),
        ConnectionProfile {
            provider: ProviderId::AnthropicMessages,
            label: Some("Anthropic Package Profile".to_string()),
            credential_id: Some("claude-cred".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source: provider_packages::provider_package_source("anthropic-provider"),
            settings,
        },
    );
    metadata.default_profile = Some("claude-main".to_string());
    connections
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: connections.metadata().fingerprint().unwrap(),
            connections: metadata,
        })
        .await
        .unwrap();

    // Verify ready validation and invoke cleanly through ProcessConnection -> /mnt/llm!
    let mut conn_ns = Namespace::new();
    conn_ns.mount(
        "/mnt/connections",
        InProcessTransport::new(connections.file_server()),
        Access::ReadOnly,
    );
    conn_ns.mount(
        QUARTERMASTER_EXECUTABLE,
        InProcessTransport::new(Arc::new(MemFs::empty())),
        Access::ReadOnly,
    );
    conn_ns.mount(
        "/mnt/package",
        InProcessTransport::new(packages.file_server()),
        Access::ReadWrite,
    );
    let conn_shell = Shell::new(InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(conn_ns.clone()),
    )));
    let validation: BTreeMap<String, String> =
        serde_json::from_slice(&conn_shell.cat("/mnt/connections/validation").await.unwrap())
            .unwrap();
    assert_eq!(
        validation.get("claude-main").map(String::as_str),
        Some("ready")
    );

    let authority = ProcessConnection {
        service: Arc::clone(&connections),
        profile: "claude-main".to_string(),
        namespace: LiveNamespace::new(conn_ns),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    let captured = authority.capture_initial().await.unwrap().unwrap();
    assert!(
        captured.config.anthropic_messages_api_key.is_none(),
        "Host API secrets must NEVER leak into Process config"
    );
    let proc_shell = Shell::new(captured.root.clone());
    assert!(
        proc_shell
            .ls("/mnt/llm/connections")
            .await
            .unwrap()
            .contains(&captured.connection)
    );
    // Invoke generation directly through Alan's `/mnt/llm` file server boundary:
    let clone_fid = alan_ap::Fid(701);
    let alan_ap::Response::Walk { .. } = captured
        .root
        .call(alan_ap::Request::Walk {
            fid: alan_ap::Fid::ROOT,
            newfid: clone_fid,
            names: vec![
                "mnt".into(),
                "llm".into(),
                "connections".into(),
                captured.connection.clone(),
                "clone".into(),
            ],
        })
        .await
        .unwrap()
    else {
        panic!("walk to clone failed");
    };
    captured
        .root
        .call(alan_ap::Request::Open {
            fid: clone_fid,
            mode: alan_ap::OpenMode::ReadWrite,
        })
        .await
        .unwrap();
    let alan_ap::Response::Read { data } = captured
        .root
        .call(alan_ap::Request::Read {
            fid: clone_fid,
            offset: 0,
            count: 64,
        })
        .await
        .unwrap()
    else {
        panic!("read clone id failed");
    };
    let gen_id = String::from_utf8(data).unwrap().trim().to_string();

    let data_path = format!("/mnt/llm/connections/{}/{gen_id}/data", captured.connection);
    let req_bytes =
        br#"{"version":2,"messages":[{"role":"user","content":"Say hi via Anthropic"}]}"#;
    proc_shell.write(&data_path, req_bytes).await.unwrap();

    let status_path = format!(
        "/mnt/llm/connections/{}/{gen_id}/status",
        captured.connection
    );
    let final_status = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let status_doc: serde_json::Value =
                serde_json::from_slice(&proc_shell.cat(&status_path).await.unwrap()).unwrap();
            let state = status_doc["status"].as_str().unwrap_or("");
            if state != "running" && state != "open" {
                break status_doc;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        final_status["status"], "done",
        "unexpected status: {final_status}"
    );
    let events_fid = alan_ap::Fid(702);
    captured
        .root
        .call(alan_ap::Request::Walk {
            fid: alan_ap::Fid::ROOT,
            newfid: events_fid,
            names: vec![
                "mnt".into(),
                "llm".into(),
                "connections".into(),
                captured.connection.clone(),
                gen_id.clone(),
                "events".into(),
            ],
        })
        .await
        .unwrap();
    captured
        .root
        .call(alan_ap::Request::Open {
            fid: events_fid,
            mode: alan_ap::OpenMode::Read,
        })
        .await
        .unwrap();
    let alan_ap::Response::Read { data: event_bytes } = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        captured.root.call(alan_ap::Request::Read {
            fid: events_fid,
            offset: 0,
            count: 65536,
        }),
    )
    .await
    .unwrap()
    .unwrap() else {
        panic!("read events failed");
    };
    let events_text = String::from_utf8(event_bytes).unwrap();
    assert!(
        events_text.contains("hello from anthropic package"),
        "expected Anthropic response in /mnt/llm events: {events_text}"
    );
    let _ = captured
        .root
        .call(alan_ap::Request::Clunk { fid: events_fid })
        .await;
    let _ = captured
        .root
        .call(alan_ap::Request::Clunk { fid: clone_fid })
        .await;

    // Uninstall `anthropic-provider` via `/bin/q` -> availability MUST disappear immediately!
    let uninstall_res = QuartermasterProcessRunner
        .run(alan_kernel::ProcessInvocation {
            pid: alan_kernel::Pid(9),
            parent: None,
            credentials: Credentials::user("test"),
            namespace: authority.namespace.snapshot(),
            exec: alan_kernel::ExecSpec {
                executable: QUARTERMASTER_EXECUTABLE.to_string(),
                args: vec!["uninstall".to_string(), "anthropic-provider".to_string()],
                namespace: alan_kernel::ExecNamespaceManifest::default(),
                descriptors: BTreeMap::new(),
            },
        })
        .await;
    assert_eq!(
        uninstall_res.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&uninstall_res.output)
    );

    let validation_after: BTreeMap<String, String> =
        serde_json::from_slice(&conn_shell.cat("/mnt/connections/validation").await.unwrap())
            .unwrap();
    assert_eq!(
        validation_after.get("claude-main").map(String::as_str),
        Some("unavailable")
    );
    assert!(authority.capture_initial().await.unwrap().is_none());
    assert!(authority.capture(None).await.is_err());
    assert!(provider_packages::resolve_provider_selection("anthropic", Some(&packages)).is_err());
}
