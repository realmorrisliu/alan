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

fn evaluation_connections() -> ConnectionsFile {
    serde_json::from_value(serde_json::json!({"version":1,
        "profiles":{"evaluation":{"provider":"typesafe","credential_id":"eval-key",
            "settings":{"model":"jev-1.13.0"}}},
        "credentials":{"eval-key":{"kind":"secret_string","provider_family":"typesafe",
            "label":"test","backend":"host_credential_store"}}
    }))
    .unwrap()
}

#[tokio::test]
async fn evaluation_wrappers_preserve_capability_and_recheck_revoked_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let store = SecretStore::from_directory(temp.path()).unwrap();
    store.save("eval-key", "fixture-secret-not-sent").unwrap();
    let connections = evaluation_connections();
    let factory = ProductLlmClientFactory {
        credentials_dir: temp.path().into(),
        keychain_service: None,
        managed_auth: None,
    };
    let mut captured = factory
        .create(&Config::default(), Some("evaluation"), &connections)
        .unwrap();
    assert!(!captured.supports_generation());
    assert!(captured.supports_choice_evaluation());
    let mut config = Config::default();
    apply_profile_to_config(&connections, Some("evaluation"), &store, &mut config).unwrap();
    let serialized = toml::to_string(&config).unwrap();
    assert!(!serialized.contains("fixture-secret-not-sent"));
    assert!(!serialized.contains("typesafe_api_key"));
    store.delete("eval-key").unwrap();
    let result = captured
        .evaluate_choice(alan_llm::ChoiceEvaluationRequest {
            input: "test".into(),
            candidates: vec![alan_llm::EvaluationCandidate {
                id: "a".into(),
                description: "test".into(),
            }],
        })
        .await;
    assert!(result.unwrap_err().to_string().contains("missing a secret"));
}

#[tokio::test]
#[ignore = "requires explicitly supplied TYPESAFE_API_KEY; live mounted Connection probe"]
async fn live_typesafe_profile_through_mounted_connection() {
    use alan_ap::{Fid, FileServer, OpenMode};
    let temp = tempfile::tempdir().unwrap();
    let store = SecretStore::from_directory(&temp.path().join("host-credentials")).unwrap();
    store
        .save(
            "eval-key",
            &std::env::var("TYPESAFE_API_KEY").expect("TYPESAFE_API_KEY required"),
        )
        .unwrap();
    let path = temp.path().join("connections.toml");
    evaluation_connections().save_to_path(&path).unwrap();
    let service = alan_service_manager::ConnectionService::open(
        &alan_service_manager::ConnectionStoreBindings::new(path).unwrap(),
    )
    .unwrap();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    service
        .attach_callable_registry(
            llmfs.clone(),
            Arc::new(ProductLlmClientFactory {
                credentials_dir: temp.path().join("host-credentials"),
                keychain_service: None,
                managed_auth: None,
            }),
            Config::default(),
            None,
        )
        .await
        .unwrap();
    let names = |tail: &str| vec!["connections".into(), "evaluation".into(), tail.into()];
    llmfs
        .walk(Fid::ROOT, Fid(1), &names("evaluate"))
        .await
        .unwrap();
    llmfs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(llmfs.read(Fid(1), 0, 100).await.unwrap()).unwrap();
    let path = |tail: &str| {
        vec![
            "connections".into(),
            "evaluation".into(),
            id.clone(),
            tail.into(),
        ]
    };
    llmfs.walk(Fid::ROOT, Fid(2), &path("data")).await.unwrap();
    llmfs.open(Fid(2), OpenMode::Write).await.unwrap();
    let body = serde_json::to_vec(&serde_json::json!({"version":1,"schema":"choice.v1",
        "input":"fn main() {}", "candidates":[{"id":"rust","description":"Rust source code"},
        {"id":"python","description":"Python source code"}],"deadline_ms":10000}))
    .unwrap();
    llmfs.write(Fid(2), 0, &body).await.unwrap();
    llmfs.clunk(Fid(2)).await.unwrap();
    llmfs
        .walk(Fid::ROOT, Fid(3), &path("events"))
        .await
        .unwrap();
    llmfs.open(Fid(3), OpenMode::Read).await.unwrap();
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(12),
        llmfs.read(Fid(3), 0, 65536),
    )
    .await
    .unwrap()
    .unwrap();
    let event: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(event["done"], true, "{event}");
    assert_eq!(event["evaluation"]["selection"]["id"], "rust");
    assert_eq!(event["evaluation"]["provider"], "typesafe");
    assert_eq!(event["evaluation"]["model"], "jev-1.13.0");
    assert!(service.default_profile().is_none());
    eprintln!("Mounted TypeSafe profile probe: {event}");
}

#[tokio::test]
#[ignore = "requires explicitly supplied TYPESAFE_API_KEY; live Root Machine probe"]
async fn live_typesafe_profile_through_root_machine() {
    use alan_agent_engine::{InputIntent, InputMode, UserInputRecord, runtime::EvaluationSurface};
    use alan_llm::MockLlmProvider;
    use std::time::Duration;

    #[derive(Debug)]
    struct Factory(ProductLlmClientFactory, MockLlmProvider);
    impl LlmClientFactory for Factory {
        fn create(
            &self,
            config: &Config,
            profile: Option<&str>,
            connections: &ConnectionsFile,
        ) -> Result<LlmClient> {
            if profile == Some("main") {
                Ok(LlmClient::new(self.1.clone()))
            } else {
                self.0.create(config, profile, connections)
            }
        }
    }

    let temp = tempfile::tempdir().unwrap();
    let credentials = temp.path().join("host-credentials");
    SecretStore::from_directory(&credentials)
        .unwrap()
        .save(
            "eval-key",
            &std::env::var("TYPESAFE_API_KEY").expect("TYPESAFE_API_KEY required"),
        )
        .unwrap();
    let mut connections = evaluation_connections();
    connections.profiles.insert(
        "main".into(),
        serde_json::from_value(serde_json::json!({
            "provider":"openai_responses", "credential_id":"main-key", "settings":{"model":"gpt-5.4"}
        }))
        .unwrap(),
    );
    connections.credentials.insert(
        "main-key".into(),
        serde_json::from_value(serde_json::json!({
            "kind":"secret_string", "provider_family":"openai_responses",
            "label":"mock generation", "backend":"host_credential_store"
        }))
        .unwrap(),
    );
    connections.default_profile = Some("main".into());
    let metadata = temp.path().join("connections.toml");
    connections.save_to_path(&metadata).unwrap();
    let generation = MockLlmProvider::new();
    let mut host = HostBootConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        LlmClient::new(generation.clone()),
        ToolRegistry::new(),
    )
    .with_input_shadow("evaluation", EvaluationSurface::Redirected);
    host.0.process.agent_config.core_config.memory.enabled = false;
    host.0.process.store_bindings = Some(alan_agent_engine::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("runtime-metadata"),
    });
    host.0.connection_store =
        Some(alan_service_manager::ConnectionStoreBindings::new(metadata).unwrap());
    host.0.llm_factory = Arc::new(Factory(
        ProductLlmClientFactory {
            credentials_dir: credentials,
            keychain_service: None,
            managed_auth: None,
        },
        generation.clone(),
    ));
    let manager = host.boot_foreground().await.unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
    let input = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, "pwd");
    shell
        .write("/agent/root/io/input", &input.encode_payload().unwrap())
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(35), async {
        loop {
            let snapshot: serde_json::Value =
                serde_json::from_slice(&shell.cat("/agent/root/machine/evaluation").await.unwrap())
                    .unwrap();
            let observation = &snapshot["observation"];
            if !observation.is_null()
                && observation["outcome"]["state"] != "started"
                && !generation.recorded_requests().is_empty()
            {
                break observation.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    let selected = manager.connection().selected_profile(manager.root_pid().0);
    let default_profile = manager.connection().default_profile();
    manager.shutdown().await.unwrap();
    let observation = result.expect("Root shadow evaluation did not settle");
    assert_eq!(
        observation["identity"]["submission_id"],
        input.submission_id
    );
    assert_eq!(observation["identity"]["callable"]["profile"], "evaluation");
    assert_eq!(observation["identity"]["callable"]["model"], "jev-1.13.0");
    assert_eq!(observation["outcome"]["state"], "selected", "{observation}");
    assert_eq!(observation["outcome"]["candidate_id"], "command");
    assert_eq!(selected.as_deref(), Some("main"));
    assert_eq!(default_profile.as_deref(), Some("main"));
    eprintln!("Root Machine TypeSafe probe: {observation}");
}
