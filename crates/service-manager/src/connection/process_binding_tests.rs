//! Startup classification never confuses missing managed metadata with injection.
use super::*;
use crate::connection::process_binding::ProcessConnection;
use alan_agent_engine::runtime::model_binding::ConnectionAuthority;

#[tokio::test]
async fn managed_none_catalog_exact_restore_preserves_full_authority() {
    for provider in [
        ProviderId::Chatgpt,
        ProviderId::OpenRouter,
        ProviderId::GoogleGeminiGenerateContent,
        ProviderId::AnthropicMessages,
    ] {
        let service = ConnectionService::ephemeral("test");
        let factory = Arc::new(TestLlmClientFactory::default());
        service
            .attach_callable_registry(
                Arc::new(alan_llmfs::LlmFs::new()),
                factory.clone(),
                Config::default(),
                None,
            )
            .await
            .unwrap();
        let mut connections = service.metadata();
        let kind = ConnectionsFile::profile_descriptor(provider).credential_kind;
        connections.credentials.insert(
            "managed-ref".into(),
            crate::connection_profile::ConnectionCredential {
                kind,
                provider_family: provider,
                label: "test reference".into(),
                backend: crate::connection_profile::default_credential_backend(kind).into(),
            },
        );
        let mut managed = profile();
        managed.provider = provider;
        managed.credential_id = Some("managed-ref".into());
        managed.settings = crate::connection_profile::normalize_profile_settings(
            provider,
            &BTreeMap::from([
                ("model".into(), "original-managed-model".into()),
                ("project_id".into(), "test-project".into()),
                ("location".into(), "us-central1".into()),
            ]),
        );
        managed.settings.retain(|key, _| {
            let descriptor = ConnectionsFile::profile_descriptor(provider);
            descriptor.required_settings.contains(&key.as_str())
                || descriptor.optional_settings.contains(&key.as_str())
        });
        connections.profiles.insert("managed".into(), managed);
        service
            .apply(ConnectionCommand::ReplaceMetadata {
                expected: service.metadata().fingerprint().unwrap(),
                connections,
            })
            .await
            .unwrap();
        let authority = ProcessConnection {
            service: service.clone(),
            profile: "managed".into(),
            namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        };
        let captured = authority.capture_initial().await.unwrap().unwrap();
        assert!(captured.config.effective_model_info().is_none());
        assert!(authority.catalog().await.is_err());
        assert!(
            authority
                .capture(Some("arbitrary-new-model"))
                .await
                .is_err()
        );
        assert_eq!(
            authority
                .restore(&captured.identity)
                .await
                .unwrap()
                .identity,
            captured.identity,
            "unchanged managed {provider:?} capture must restore"
        );
        assert!(authority.catalog().await.is_err());
        for field in ["model", "provider", "credential", "revision", "profile"] {
            let mut wrong = captured.identity.clone();
            match field {
                "model" => wrong.model = "arbitrary-new-model".into(),
                "provider" => wrong.provider = "wrong-provider".into(),
                "credential" => wrong.credential_ref = Some("wrong-ref".into()),
                "revision" => wrong.revision.push_str("changed"),
                "profile" => wrong.profile = "outside-authority".into(),
                _ => unreachable!(),
            }
            assert!(authority.restore(&wrong).await.is_err(), "{field}");
        }
        factory.unavailable.lock().unwrap().insert("managed".into());
        service
            .callables
            .lock()
            .await
            .as_mut()
            .unwrap()
            .published_profiles
            .remove("managed");
        assert!(
            authority.restore(&captured.identity).await.is_err(),
            "unchanged identity cannot restore an unpublished callable"
        );
        let mut unavailable = service.metadata();
        unavailable.profiles.get_mut("managed").unwrap().label = Some("unavailable".into());
        service
            .apply(ConnectionCommand::ReplaceMetadata {
                expected: service.metadata().fingerprint().unwrap(),
                connections: unavailable,
            })
            .await
            .unwrap();
        assert!(
            authority.capture(None).await.is_err(),
            "unavailable changed-label profile cannot capture its ordinary callable"
        );
        assert!(authority.restore(&captured.identity).await.is_err());
    }
}

#[tokio::test]
async fn process_initial_binding_classifies_injection_and_preserves_managed_validation() {
    let service = ConnectionService::ephemeral("test");
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    service
        .attach_callable_registry(
            llmfs.clone(),
            Arc::new(TestLlmClientFactory::default()),
            Config::default(),
            Some(("injected".into(), LlmClient::new(MockLlmProvider::new()))),
        )
        .await
        .unwrap();
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(
            service.capture_connection("injected").await.unwrap(),
        )),
        alan_kernel::Access::ReadWrite,
    );
    let ns = alan_kernel::LiveNamespace::new(ns);
    let injected = ProcessConnection {
        service: service.clone(),
        profile: "injected".into(),
        namespace: ns.clone(),
    };
    let captured = injected.capture_initial().await.unwrap().unwrap();
    assert_eq!(captured.identity.revision, "injected-namespace-callable");
    assert_eq!(
        injected.restore(&captured.identity).await.unwrap().identity,
        captured.identity
    );
    assert!(injected.capture(Some("unauthorized-model")).await.is_err());
    let catalog = injected.catalog().await.unwrap();
    assert_eq!(catalog["profile"], "injected");
    assert_eq!(
        catalog["models"].as_array().unwrap().len(),
        1,
        "injected fallback only authorizes its original callable"
    );
    assert_eq!(catalog["models"][0]["model"], captured.identity.model);
    assert!(catalog.get("credential_ref").is_none());
    assert!(catalog.get("revision").is_none());
    let missing = ProcessConnection {
        service: service.clone(),
        profile: "missing".into(),
        namespace: ns.clone(),
    };
    assert!(missing.capture_initial().await.unwrap().is_none());
    assert!(missing.capture(None).await.is_err());
    assert!(missing.restore(&captured.identity).await.is_err());
    let mut connections = service.metadata();
    connections.credentials.insert(
        "openai-main".into(),
        crate::connection_profile::ConnectionCredential {
            kind: crate::connection_profile::CredentialKind::SecretString,
            provider_family: ProviderId::OpenAiResponses,
            label: "test reference".into(),
            backend: crate::connection_profile::default_credential_backend(
                crate::connection_profile::CredentialKind::SecretString,
            )
            .into(),
        },
    );
    service
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: service.metadata().fingerprint().unwrap(),
            connections,
        })
        .await
        .unwrap();
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".into(),
            profile: profile(),
        })
        .await
        .unwrap();
    let managed = ProcessConnection {
        service: service.clone(),
        profile: "main".into(),
        namespace: ns,
    };
    let captured = managed.capture_initial().await.unwrap().unwrap();
    assert_ne!(captured.identity.revision, "injected-namespace-callable");
    assert_eq!(
        managed.restore(&captured.identity).await.unwrap().identity,
        captured.identity
    );
    let changed_model = "gpt-5.2";
    let changed = managed.capture(Some(changed_model)).await.unwrap();
    assert_ne!(changed.identity.model, captured.identity.model);
    assert_eq!(
        managed.restore(&changed.identity).await.unwrap().identity,
        changed.identity
    );
    let mut uncatalogued = captured.identity.clone();
    uncatalogued.model = "arbitrary-uncatalogued-model".into();
    assert!(managed.restore(&uncatalogued).await.is_err());
    let mut wrong = captured.identity;
    wrong.revision.push_str("changed");
    assert!(managed.restore(&wrong).await.is_err());
}

#[tokio::test]
async fn catalog_observes_published_authority_without_recapturing_on_idle_polls() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = ConnectionStoreBindings::new(temp.path().join("connections.toml")).unwrap();
    let service = ConnectionService::open("test", &bindings).unwrap();
    let writer = ConnectionService::open("test", &bindings).unwrap();
    service
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            Arc::new(TestLlmClientFactory::default()),
            Config::default(),
            Some(("main".into(), LlmClient::new(MockLlmProvider::new()))),
        )
        .await
        .unwrap();
    let managed = profile();
    let mut connections = service.metadata();
    connections.credentials.insert(
        "openai-main".into(),
        crate::connection_profile::ConnectionCredential {
            kind: crate::connection_profile::CredentialKind::SecretString,
            provider_family: ProviderId::OpenAiResponses,
            label: "test reference".into(),
            backend: crate::connection_profile::default_credential_backend(
                crate::connection_profile::CredentialKind::SecretString,
            )
            .into(),
        },
    );
    service
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: service.metadata().fingerprint().unwrap(),
            connections,
        })
        .await
        .unwrap();
    let authority = ProcessConnection {
        service: service.clone(),
        profile: "main".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
    };
    let published = authority.catalog().await.unwrap();
    let bytes = std::fs::read(&bindings.metadata_path).unwrap();
    // This sentinel proves observation performs no disk read. It is not a fresh
    // external metadata assertion: only the already-published callable is observed.
    std::fs::write(&bindings.metadata_path, "invalid = [").unwrap();
    for _ in 0..100 {
        assert_eq!(authority.catalog().await.unwrap(), published);
    }
    assert!(
        authority.capture(None).await.is_err(),
        "capture still validates durable metadata"
    );
    std::fs::write(&bindings.metadata_path, bytes).unwrap();
    writer.refresh().await.unwrap();
    writer
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".into(),
            profile: managed.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        authority.catalog().await.unwrap(),
        published,
        "external change waits for an existing refresh boundary"
    );
    let captured = authority.capture(None).await.unwrap();
    assert_ne!(
        captured.identity.revision, "injected-namespace-callable",
        "capture refreshes the external managed profile"
    );
    assert_eq!(
        authority.catalog().await.unwrap_err().to_string(),
        "model catalog unavailable",
        "no invented static catalog"
    );
    service
        .apply(ConnectionCommand::RemoveProfile {
            profile_id: "main".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        authority.catalog().await.unwrap_err().to_string(),
        "Process profile unavailable",
        "same-service removal publishes immediately"
    );
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "main".into(),
            profile: managed,
        })
        .await
        .unwrap();
    assert_eq!(
        authority.catalog().await.unwrap_err().to_string(),
        "model catalog unavailable"
    );
    service
        .apply(ConnectionCommand::RequestNative {
            request: NativeConnectionRequest {
                id: "logout-main".into(),
                profile_id: "main".into(),
                action: NativeConnectionAction::Logout,
            },
        })
        .await
        .unwrap();
    assert_eq!(
        authority.catalog().await.unwrap_err().to_string(),
        "Process profile is not callable",
        "pending native request removes callable authority"
    );
    service
        .respond_native(NativeConnectionResponse {
            request_id: "logout-main".into(),
            opaque_credential_ref: None,
            status: "logged_out".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        authority.catalog().await.unwrap_err().to_string(),
        "Process profile is not callable",
        "logout must not retain a catalog grant"
    );
}
