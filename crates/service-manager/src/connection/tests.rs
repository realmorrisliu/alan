use super::*;
use alan_agent_engine::LlmProvider as ProviderId;
use alan_ap::InProcessTransport;
use alan_llm::MockLlmProvider;
use alan_shell::Shell;
use chrono::Utc;

#[derive(Debug, Default)]
struct TestLlmClientFactory {
    unavailable: Mutex<BTreeSet<String>>,
}

impl LlmClientFactory for TestLlmClientFactory {
    fn create(
        &self,
        _base_config: &Config,
        selected_profile: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        let selected_profile = selected_profile.context("missing selected profile")?;
        ensure!(
            !self.unavailable.lock().unwrap().contains(selected_profile),
            "profile is unavailable"
        );
        Ok(LlmClient::new(MockLlmProvider::new()))
    }
}

fn profile() -> ConnectionProfile {
    ConnectionProfile {
        provider: ProviderId::OpenAiResponses,
        label: Some("main".to_string()),
        credential_id: Some("openai-main".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        source: "managed".to_string(),
        settings: [
            (
                "base_url".to_string(),
                "https://api.openai.com/v1".to_string(),
            ),
            ("model".to_string(), "gpt-5.4".to_string()),
        ]
        .into_iter()
        .collect(),
    }
}

#[tokio::test]
async fn metadata_is_persistent_and_secret_bytes_never_enter_files() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    let service = ConnectionService::open("test", &bindings).unwrap();
    let shell = Shell::new(InProcessTransport::new(service.file_server()));
    let command = serde_json::json!({
        "op": "add_profile",
        "profile_id": "openai-main",
        "profile": profile(),
    });
    shell
        .write("/ctl", &serde_json::to_vec(&command).unwrap())
        .await
        .unwrap();
    assert!(bindings.metadata_path.is_file());
    shell
        .write(
            "/ctl",
            &serde_json::to_vec(&serde_json::json!({
                "op": "request_native",
                "request": {
                    "id": "login-1",
                    "profile_id": "openai-main",
                    "action": "secret_entry"
                }
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    service
        .respond_native(NativeConnectionResponse {
            request_id: "login-1".to_string(),
            opaque_credential_ref: Some("host-keychain:openai-main".to_string()),
            status: "ready".to_string(),
        })
        .await
        .unwrap();
    let all = [
        shell.cat("/profiles").await.unwrap(),
        shell.cat("/native-responses").await.unwrap(),
    ]
    .concat();
    assert!(!String::from_utf8(all).unwrap().contains("sk-secret-value"));
}

#[tokio::test]
async fn rejects_secret_material_instead_of_treating_it_as_reference() {
    let service = ConnectionService::ephemeral("test");
    assert!(
        service
            .respond_native(NativeConnectionResponse {
                request_id: "r".to_string(),
                opaque_credential_ref: Some("sk-secret-value".to_string()),
                status: "ready".to_string(),
            })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn native_request_and_response_state_is_bounded() {
    let pending = ConnectionService::ephemeral("test");
    pending
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".to_string(),
            profile: profile(),
        })
        .await
        .unwrap();
    for index in 0..MAX_PENDING_NATIVE_REQUESTS {
        pending
            .apply(ConnectionCommand::RequestNative {
                request: NativeConnectionRequest {
                    id: format!("pending-{index}"),
                    profile_id: "main".to_string(),
                    action: NativeConnectionAction::SecretEntry,
                },
            })
            .await
            .unwrap();
    }
    assert!(
        pending
            .apply(ConnectionCommand::RequestNative {
                request: NativeConnectionRequest {
                    id: "pending-overflow".to_string(),
                    profile_id: "main".to_string(),
                    action: NativeConnectionAction::SecretEntry,
                },
            })
            .await
            .is_err()
    );

    let completed = ConnectionService::ephemeral("test");
    completed
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".to_string(),
            profile: profile(),
        })
        .await
        .unwrap();
    for index in 0..=MAX_NATIVE_RESPONSES {
        let request_id = format!("completed-{index}");
        completed
            .apply(ConnectionCommand::RequestNative {
                request: NativeConnectionRequest {
                    id: request_id.clone(),
                    profile_id: "main".to_string(),
                    action: NativeConnectionAction::SecretEntry,
                },
            })
            .await
            .unwrap();
        completed
            .respond_native(NativeConnectionResponse {
                request_id,
                opaque_credential_ref: None,
                status: "ready".to_string(),
            })
            .await
            .unwrap();
    }
    let state = completed.state.lock().unwrap();
    assert_eq!(state.responses.len(), MAX_NATIVE_RESPONSES);
    assert!(!state.responses.contains_key("completed-0"));
    assert!(
        state
            .responses
            .contains_key(&format!("completed-{MAX_NATIVE_RESPONSES}"))
    );
    assert!(validate_id(&"x".repeat(MAX_IDENTIFIER_BYTES + 1)).is_err());
}

#[tokio::test]
async fn callable_profiles_follow_metadata_and_native_readiness() {
    let service = ConnectionService::ephemeral("test");
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let factory = Arc::new(TestLlmClientFactory::default());
    service
        .attach_callable_registry(
            llmfs.clone(),
            factory.clone(),
            Config::default(),
            Some((
                "default".to_string(),
                LlmClient::new(MockLlmProvider::new()),
            )),
        )
        .await
        .unwrap();
    let control = Shell::new(InProcessTransport::new(service.file_server()));
    let callable = Shell::new(InProcessTransport::new(llmfs));
    assert_eq!(callable.ls("/connections").await.unwrap(), ["default"]);

    control
        .write(
            "/ctl",
            &serde_json::to_vec(&serde_json::json!({
                "op": "add_profile",
                "profile_id": "openai-main",
                "profile": profile(),
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        callable
            .ls("/connections")
            .await
            .unwrap()
            .contains(&"openai-main".to_string())
    );
    control
        .write(
            "/ctl",
            br#"{"op":"set_default","profile_id":"openai-main"}"#,
        )
        .await
        .unwrap();
    {
        let callables = service.callables.lock().await;
        let registry = callables.as_ref().unwrap();
        assert_eq!(registry.published_default.as_deref(), Some("openai-main"));
        assert!(!registry.published_fallbacks.contains("default"));
    }
    assert!(
        callable
            .ls("/connections")
            .await
            .unwrap()
            .contains(&"default".to_string())
    );

    control
        .write(
            "/ctl",
            &serde_json::to_vec(&serde_json::json!({
                "op": "request_native",
                "request": {
                    "id": "login-1",
                    "profile_id": "openai-main",
                    "action": "secret_entry"
                }
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let validation: BTreeMap<String, String> =
        serde_json::from_slice(&control.cat("/validation").await.unwrap()).unwrap();
    assert_eq!(
        validation.get("openai-main").map(String::as_str),
        Some("pending")
    );
    assert!(
        !callable
            .ls("/connections")
            .await
            .unwrap()
            .contains(&"openai-main".to_string())
    );

    control
        .write(
            "/native-responses",
            &serde_json::to_vec(&NativeConnectionResponse {
                request_id: "login-1".to_string(),
                opaque_credential_ref: Some("host-keychain:openai-main".to_string()),
                status: "ready".to_string(),
            })
            .unwrap(),
        )
        .await
        .unwrap();
    let validation: BTreeMap<String, String> =
        serde_json::from_slice(&control.cat("/validation").await.unwrap()).unwrap();
    assert_eq!(
        validation.get("openai-main").map(String::as_str),
        Some("ready")
    );
    assert!(
        callable
            .ls("/connections")
            .await
            .unwrap()
            .contains(&"openai-main".to_string())
    );

    control
        .write(
            "/ctl",
            br#"{"op":"remove_profile","profile_id":"openai-main"}"#,
        )
        .await
        .unwrap();
    assert!(
        !callable
            .ls("/connections")
            .await
            .unwrap()
            .contains(&"openai-main".to_string())
    );

    factory
        .unavailable
        .lock()
        .unwrap()
        .insert("broken".to_string());
    control
        .write(
            "/ctl",
            &serde_json::to_vec(&serde_json::json!({
                "op": "add_profile",
                "profile_id": "broken",
                "profile": profile(),
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let validation: BTreeMap<String, String> =
        serde_json::from_slice(&control.cat("/validation").await.unwrap()).unwrap();
    assert_eq!(
        validation.get("broken").map(String::as_str),
        Some("unavailable")
    );
    assert!(
        !callable
            .ls("/connections")
            .await
            .unwrap()
            .contains(&"broken".to_string())
    );

    control
        .write(
            "/ctl",
            br#"{"op":"request_native","request":{"id":"repair-1","profile_id":"broken","action":"secret_entry"}}"#,
        )
        .await
        .unwrap();
    factory.unavailable.lock().unwrap().remove("broken");
    control
        .write(
            "/native-responses",
            &serde_json::to_vec(&NativeConnectionResponse {
                request_id: "repair-1".to_string(),
                opaque_credential_ref: Some("host-keychain:broken".to_string()),
                status: "ready".to_string(),
            })
            .unwrap(),
        )
        .await
        .unwrap();
    let validation: BTreeMap<String, String> =
        serde_json::from_slice(&control.cat("/validation").await.unwrap()).unwrap();
    assert_eq!(validation.get("broken").map(String::as_str), Some("ready"));
    assert!(
        callable
            .ls("/connections")
            .await
            .unwrap()
            .contains(&"broken".to_string())
    );
}
