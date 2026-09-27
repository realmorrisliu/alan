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

#[tokio::test]
async fn failed_metadata_commit_preserves_profiles_and_dependent_state() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    let service = ConnectionService::open("test", &bindings).unwrap();
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    service
        .apply(ConnectionCommand::SetDefault {
            profile_id: "main".into(),
        })
        .await
        .unwrap();
    service.select(7, "main").unwrap();
    service
        .apply(ConnectionCommand::RequestNative {
            request: NativeConnectionRequest {
                id: "login".into(),
                profile_id: "main".into(),
                action: NativeConnectionAction::BrowserLogin,
            },
        })
        .await
        .unwrap();
    let before = service.metadata();
    let disk = std::fs::read(&bindings.metadata_path).unwrap();
    let backup = temp.path().join("committed.toml");
    std::fs::rename(&bindings.metadata_path, &backup).unwrap();
    std::fs::create_dir(&bindings.metadata_path).unwrap();
    for command in [
        ConnectionCommand::AddProfile {
            profile_id: "other".into(),
            profile: profile(),
        },
        ConnectionCommand::RemoveProfile {
            profile_id: "main".into(),
        },
        ConnectionCommand::SetDefault {
            profile_id: "main".into(),
        },
        ConnectionCommand::ClearDefault,
        ConnectionCommand::ReplaceMetadata {
            expected: before.fingerprint().unwrap(),
            connections: ConnectionsFile::default(),
        },
    ] {
        assert!(service.apply(command).await.is_err());
        assert_eq!(service.metadata(), before);
        let state = service.state.lock().unwrap();
        assert_eq!(state.selections.get(&7).map(String::as_str), Some("main"));
        assert!(state.requests.contains_key("login"));
        assert!(state.validation.contains_key("main"));
        assert_eq!(std::fs::read(&backup).unwrap(), disk);
    }
    std::fs::remove_dir(&bindings.metadata_path).unwrap();
    std::fs::rename(backup, &bindings.metadata_path).unwrap();
    service
        .apply(ConnectionCommand::RemoveProfile {
            profile_id: "main".into(),
        })
        .await
        .unwrap();
    assert!(service.metadata().profiles.is_empty());
    assert!(service.state.lock().unwrap().selections.is_empty());
    assert!(service.state.lock().unwrap().requests.is_empty());
    assert_eq!(
        ConnectionsFile::load_from_path(&bindings.metadata_path)
            .unwrap()
            .0,
        service.metadata()
    );
}

#[tokio::test]
async fn post_replace_error_publishes_visible_metadata_and_dependent_state() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    let service = ConnectionService::open("test", &bindings).unwrap();
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    service.select(7, "main").unwrap();
    {
        let mut committed = service.state.lock().unwrap();
        let mut candidate = committed.clone();
        candidate.connections.profiles.clear();
        candidate.selections.clear();
        candidate.validation.clear();
        candidate
            .connections
            .save_to_path(&bindings.metadata_path)
            .unwrap();
        // Inject the error at the directory-sync boundary, after publication.
        let failed_sync = Err(anyhow::anyhow!("sync parent directory failed"));
        assert!(service.publish_saved_state(&mut committed, candidate, &failed_sync));
        assert!(failed_sync.is_err());
        assert!(committed.connections.profiles.is_empty());
        assert!(committed.selections.is_empty());
        assert!(committed.validation.is_empty());
    }
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "next".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    let disk = ConnectionsFile::load_from_path(&bindings.metadata_path)
        .unwrap()
        .0;
    assert_eq!(disk, service.metadata());
    assert!(disk.profiles.contains_key("next"));
    assert!(!disk.profiles.contains_key("main"));
}

#[tokio::test]
async fn metadata_replacement_rejects_stale_clients_without_losing_the_first_update() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    let service = ConnectionService::open("test", &bindings).unwrap();
    let first = Shell::new(InProcessTransport::new(service.file_server()));
    let second = Shell::new(InProcessTransport::new(service.file_server()));
    let expected: ConnectionsFile =
        serde_json::from_slice(&first.cat("/metadata").await.unwrap()).unwrap();
    let stale: ConnectionsFile =
        serde_json::from_slice(&second.cat("/metadata").await.unwrap()).unwrap();
    let mut first_update = expected.clone();
    first_update.profiles.insert("first".into(), profile());
    // A metadata document above half the write limit must remain editable.
    first_update.profiles.get_mut("first").unwrap().label = Some("x".repeat(600_000));
    let replace = |expected: &ConnectionsFile, connections: &ConnectionsFile| {
        serde_json::to_vec(&serde_json::json!({ "op": "replace_metadata", "expected": expected.fingerprint().unwrap(), "connections": connections })).unwrap()
    };
    first
        .write("/ctl", &replace(&expected, &first_update))
        .await
        .unwrap();
    let mut stale_update = stale.clone();
    stale_update.profiles.insert("second".into(), profile());
    assert!(
        second
            .write("/ctl", &replace(&stale, &stale_update))
            .await
            .is_err()
    );
    assert_eq!(service.metadata(), first_update);
    assert_eq!(
        ConnectionsFile::load_from_path(&bindings.metadata_path)
            .unwrap()
            .0,
        first_update
    );
    let refreshed: ConnectionsFile =
        serde_json::from_slice(&second.cat("/metadata").await.unwrap()).unwrap();
    let mut merged = refreshed.clone();
    merged.profiles.insert("second".into(), profile());
    second
        .write("/ctl", &replace(&refreshed, &merged))
        .await
        .unwrap();
    assert_eq!(service.metadata(), merged);
    let unguarded = serde_json::to_vec(
        &serde_json::json!({ "op": "replace_metadata", "connections": expected }),
    )
    .unwrap();
    assert!(first.write("/ctl", &unguarded).await.is_err());
    assert_eq!(service.metadata(), merged);
}

#[tokio::test]
async fn independent_services_reject_stale_writes_and_refresh_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    let first = ConnectionService::open("test", &bindings).unwrap();
    let second = ConnectionService::open("test", &bindings).unwrap();
    first
        .apply(ConnectionCommand::AddProfile {
            profile_id: "first".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    assert!(
        second
            .apply(ConnectionCommand::AddProfile {
                profile_id: "second".into(),
                profile: profile()
            })
            .await
            .is_err()
    );
    assert_eq!(second.metadata(), first.metadata());
    assert_eq!(
        second
            .state
            .lock()
            .unwrap()
            .validation
            .get("first")
            .map(String::as_str),
        Some("unavailable")
    );
    second
        .apply(ConnectionCommand::AddProfile {
            profile_id: "second".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    assert!(
        first
            .apply(ConnectionCommand::RemoveProfile {
                profile_id: "first".into()
            })
            .await
            .is_err()
    );
    assert_eq!(first.metadata(), second.metadata());
    second.select(7, "first").unwrap();
    first
        .apply(ConnectionCommand::RemoveProfile {
            profile_id: "first".into(),
        })
        .await
        .unwrap();
    assert!(
        second
            .apply(ConnectionCommand::SetDefault {
                profile_id: "first".into()
            })
            .await
            .is_err()
    );
    assert!(second.state.lock().unwrap().selections.is_empty());
    let disk = ConnectionsFile::load_from_path(&bindings.metadata_path)
        .unwrap()
        .0;
    assert_eq!(disk, second.metadata());
    assert!(!disk.profiles.contains_key("first"));
    assert!(disk.profiles.contains_key("second"));
}

#[tokio::test]
async fn legacy_timestamps_are_stable_and_replaced_profiles_discard_native_state() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    std::fs::write(
        &bindings.metadata_path,
        r#"version = 1
[profiles.main]
provider = "openai_responses"
[profiles.main.settings]
base_url = "https://api.openai.com/v1"
model = "gpt-5.4"
"#,
    )
    .unwrap();
    let first = ConnectionService::open("test", &bindings).unwrap();
    let second = ConnectionService::open("test", &bindings).unwrap();
    assert_eq!(first.metadata(), second.metadata());
    first
        .apply(ConnectionCommand::SetDefault {
            profile_id: "main".into(),
        })
        .await
        .unwrap();
    let second = ConnectionService::open("test", &bindings).unwrap();
    {
        let mut state = second.state.lock().unwrap();
        state
            .native_status
            .insert("main".into(), "logged_out".into());
        state.requests.insert(
            "old".into(),
            NativeConnectionRequest {
                id: "old".into(),
                profile_id: "main".into(),
                action: NativeConnectionAction::SecretEntry,
            },
        );
    }
    let mut changed = first.metadata();
    changed.profiles.get_mut("main").unwrap().credential_id = Some("new-secret".into());
    first
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: first.metadata().fingerprint().unwrap(),
            connections: changed,
        })
        .await
        .unwrap();
    assert!(
        second
            .apply(ConnectionCommand::SetDefault {
                profile_id: "main".into()
            })
            .await
            .is_err()
    );
    let state = second.state.lock().unwrap();
    assert!(!state.requests.contains_key("old"));
    assert!(!state.native_status.contains_key("main"));
}

#[tokio::test]
async fn independent_reader_refreshes_callables_and_preserves_open_snapshot() {
    use alan_ap::{Fid, OpenMode};
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    let writer = ConnectionService::open("test", &bindings).unwrap();
    let reader = ConnectionService::open("test", &bindings).unwrap();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    reader
        .attach_callable_registry(
            llmfs.clone(),
            Arc::new(TestLlmClientFactory::default()),
            Config::default(),
            None,
        )
        .await
        .unwrap();
    let callable = Shell::new(InProcessTransport::new(llmfs));
    let fs = reader.file_server();
    writer
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    let fid = Fid(42);
    fs.walk(Fid::ROOT, fid, &["metadata".into()]).await.unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    assert_eq!(callable.ls("/connections").await.unwrap(), ["main"]);
    reader.select(7, "main").unwrap();
    let expected = serde_json::to_vec(&writer.metadata()).unwrap();
    let mut bytes = fs.read(fid, 0, 8).await.unwrap();
    writer
        .apply(ConnectionCommand::RemoveProfile {
            profile_id: "main".into(),
        })
        .await
        .unwrap();
    let fresh = Shell::new(InProcessTransport::new(fs.clone()));
    let metadata: ConnectionsFile =
        serde_json::from_slice(&fresh.cat("/metadata").await.unwrap()).unwrap();
    assert!(metadata.profiles.is_empty());
    assert!(callable.ls("/connections").await.unwrap().is_empty());
    assert!(reader.selected_profile(7).is_none());
    assert_eq!(fs.stat(fid).await.unwrap().length, expected.len() as u64);
    bytes.extend(fs.read(fid, 8, u32::MAX).await.unwrap());
    assert_eq!(bytes, expected);
    fs.clunk(fid).await.unwrap();
    std::fs::write(&bindings.metadata_path, "invalid = [").unwrap();
    assert!(fresh.cat("/metadata").await.is_err());
}
