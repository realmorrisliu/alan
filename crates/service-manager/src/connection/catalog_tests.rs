//! Provider metadata publication and capture share one Connection-owned catalog.
use super::*;
use crate::connection::process_binding::ProcessConnection;
use alan_agent_engine::runtime::model_binding::ConnectionAuthority;
use alan_llm::{ProviderModel, ReasoningEffort};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct CatalogFactory {
    models: Arc<Mutex<Option<Vec<ProviderModel>>>>,
    reads: Arc<AtomicUsize>,
    creates: AtomicUsize,
    account: Mutex<Option<String>>,
    blocked: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    started: Arc<tokio::sync::Notify>,
}

impl Default for CatalogFactory {
    fn default() -> Self {
        Self {
            models: Default::default(),
            reads: Default::default(),
            creates: Default::default(),
            account: Mutex::new(Some("account-a".into())),
            blocked: Default::default(),
            started: Default::default(),
        }
    }
}

impl LlmClientFactory for CatalogFactory {
    fn create(&self, _: &Config, _: Option<&str>, _: &ConnectionsFile) -> Result<LlmClient> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Ok(LlmClient::new(CatalogClient {
            models: self.models.clone(),
            reads: self.reads.clone(),
            mock: MockLlmProvider::new(),
            account: self.account.lock().unwrap().clone(),
            blocked: self.blocked.lock().unwrap().clone(),
            started: self.started.clone(),
        }))
    }
}

struct CatalogClient {
    models: Arc<Mutex<Option<Vec<ProviderModel>>>>,
    reads: Arc<AtomicUsize>,
    mock: MockLlmProvider,
    account: Option<String>,
    blocked: Option<Arc<tokio::sync::Semaphore>>,
    started: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl LlmProvider for CatalogClient {
    fn account_identity(&self) -> Option<&str> {
        self.account.as_deref()
    }

    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let models = self.models.lock().unwrap().clone();
        if let Some(blocked) = &self.blocked {
            self.started.notify_one();
            blocked.acquire().await.unwrap().forget();
        }
        Ok(models)
    }
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.mock.generate(request).await
    }
    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.mock.chat(system, user).await
    }
    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.mock.generate_stream(request).await
    }
    fn provider_name(&self) -> &'static str {
        "chatgpt"
    }
}

fn model(slug: &str) -> ProviderModel {
    ProviderModel {
        slug: slug.into(),
        context_window_tokens: 100000,
        supported_reasoning_efforts: vec![ReasoningEffort::Medium, ReasoningEffort::High],
        default_reasoning_effort: Some(ReasoningEffort::Medium),
    }
}

async fn managed_connection(
    factory: Arc<CatalogFactory>,
) -> (Arc<ConnectionService>, ProcessConnection) {
    let service = ConnectionService::ephemeral().unwrap();
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
    let kind = ConnectionsFile::profile_descriptor(ProviderId::Chatgpt).credential_kind;
    connections.credentials.insert(
        "managed-ref".into(),
        crate::connection_profile::ConnectionCredential {
            kind,
            provider_family: ProviderId::Chatgpt,
            label: "test".into(),
            backend: crate::connection_profile::default_credential_backend(kind).into(),
        },
    );
    let mut managed = profile();
    managed.provider = ProviderId::Chatgpt;
    managed.credential_id = Some("managed-ref".into());
    managed.settings = BTreeMap::from([
        ("model".into(), "model-a".into()),
        ("base_url".into(), "https://api.openai.com/v1".into()),
    ]);
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
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    (service, authority)
}

#[tokio::test]
async fn chatgpt_catalog_publication_preserves_captured_bindings_and_retries_unavailable_metadata()
{
    let factory = Arc::new(CatalogFactory::default());
    let (service, authority) = managed_connection(factory.clone()).await;
    service.wait_for_catalog_discovery().await;
    let original = authority.capture_initial().await.unwrap().unwrap();
    assert_eq!(
        factory.reads.load(Ordering::SeqCst),
        1,
        "startup must not retry unavailable discovery"
    );
    assert!(authority.catalog().await.is_err());
    assert_eq!(
        authority
            .restore(&original.identity)
            .await
            .unwrap()
            .identity,
        original.identity
    );

    *factory.models.lock().unwrap() = Some(vec![model("model-a"), model("model-b")]);
    let selected = authority.capture(Some("model-b")).await.unwrap();
    assert_eq!(selected.config.effective_model(), "model-b");
    assert_eq!(
        selected
            .config
            .effective_model_info()
            .unwrap()
            .default_reasoning_effort,
        Some(ReasoningEffort::Medium)
    );
    assert_eq!(selected.config.effective_context_window_tokens(), 100000);
    let published = authority.catalog().await.unwrap();
    assert_eq!(published["models"].as_array().unwrap().len(), 2);
    let reads = factory.reads.load(Ordering::SeqCst);
    let creates = factory.creates.load(Ordering::SeqCst);
    for _ in 0..100 {
        assert_eq!(authority.catalog().await.unwrap(), published);
    }
    assert_eq!(factory.reads.load(Ordering::SeqCst), reads);
    assert_eq!(factory.creates.load(Ordering::SeqCst), creates);
    assert!(authority.capture(Some("not-authorized")).await.is_err());
    assert_eq!(factory.creates.load(Ordering::SeqCst), creates);
    assert_eq!(original.config.effective_model(), "model-a");
    assert_eq!(
        authority
            .restore(&selected.identity)
            .await
            .unwrap()
            .identity,
        selected.identity
    );
    assert_eq!(
        service.metadata().profiles["managed"].settings["model"],
        "model-a"
    );

    // Replacing profile publication removes the previous account's catalog.
    *factory.models.lock().unwrap() = None;
    let mut changed = service.metadata();
    changed.profiles.get_mut("managed").unwrap().label = Some("replacement".into());
    service
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: service.metadata().fingerprint().unwrap(),
            connections: changed,
        })
        .await
        .unwrap();
    assert!(authority.catalog().await.is_err());
    assert!(authority.restore(&selected.identity).await.is_err());
    assert_eq!(selected.config.effective_model(), "model-b");
    assert_eq!(
        selected.config.effective_model_info().unwrap().slug,
        "model-b"
    );
}

#[tokio::test]
async fn implicit_account_replacement_cannot_select_or_restore_with_old_catalog_authority() {
    let factory = Arc::new(CatalogFactory::default());
    *factory.models.lock().unwrap() = Some(vec![model("model-a"), model("model-b")]);
    *factory.account.lock().unwrap() = Some("account-a".into());
    let (service, authority) = managed_connection(factory.clone()).await;
    let original = authority.capture_initial().await.unwrap().unwrap();
    let selected = authority.capture(Some("model-b")).await.unwrap();
    assert_eq!(
        original.config.chatgpt_account_id.as_deref(),
        Some("account-a")
    );
    let original_catalog = authority.catalog().await.unwrap();
    let metadata = service.metadata();
    assert!(
        !metadata.profiles["managed"]
            .settings
            .contains_key("account_id")
    );

    *factory.account.lock().unwrap() = Some("account-b".into());
    let error = authority.capture(Some("model-b")).await.err().unwrap();
    assert!(error.to_string().contains("provider account changed"));
    assert_eq!(authority.catalog().await.unwrap(), original_catalog);
    assert_eq!(
        authority.capture(None).await.unwrap().identity,
        original.identity
    );

    // A new invocation with identical profile metadata binds B, so exact A recovery
    // must fail rather than reassigning old work to the newly logged-in account.
    let replacement = ConnectionService::ephemeral().unwrap();
    replacement
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            factory,
            Config::default(),
            None,
        )
        .await
        .unwrap();
    replacement
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: replacement.metadata().fingerprint().unwrap(),
            connections: metadata,
        })
        .await
        .unwrap();
    let next = ProcessConnection {
        service: replacement,
        profile: "managed".into(),
        namespace: authority.namespace.clone(),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    let rebound = next.capture_initial().await.unwrap().unwrap();
    assert_eq!(
        rebound.config.chatgpt_account_id.as_deref(),
        Some("account-b")
    );
    assert_ne!(original.identity.revision, rebound.identity.revision);
    assert!(next.restore(&original.identity).await.is_err());
    assert!(next.restore(&selected.identity).await.is_err());
}

#[tokio::test]
async fn slow_discovery_does_not_block_published_observation_or_publish_into_replaced_profile() {
    let factory = Arc::new(CatalogFactory::default());
    *factory.models.lock().unwrap() = Some(vec![model("model-a")]);
    let (service, authority) = managed_connection(factory.clone()).await;
    service.wait_for_catalog_discovery().await;
    let original = authority.catalog().await.unwrap();
    let blocked = Arc::new(tokio::sync::Semaphore::new(0));
    *factory.blocked.lock().unwrap() = Some(blocked.clone());
    let profile = service.metadata().profiles["managed"].clone();
    let writer = service.clone();
    let slow_profile = profile.clone();
    let update = tokio::spawn(async move {
        writer
            .apply(ConnectionCommand::AddProfile {
                profile_id: "slow".into(),
                profile: slow_profile,
            })
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        factory.started.notified(),
    )
    .await
    .unwrap();
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_millis(250), authority.catalog())
            .await
            .unwrap()
            .unwrap(),
        original
    );
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            authority.capture(None)
        )
        .await
        .unwrap()
        .is_ok()
    );

    service
        .apply(ConnectionCommand::RemoveProfile {
            profile_id: "slow".into(),
        })
        .await
        .unwrap();
    *factory.blocked.lock().unwrap() = None;
    *factory.models.lock().unwrap() = Some(vec![model("replacement-model")]);
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "slow".into(),
            profile,
        })
        .await
        .unwrap();
    service.wait_for_catalog_discovery().await;
    blocked.add_permits(1);
    tokio::time::timeout(std::time::Duration::from_secs(2), update)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let replaced = ProcessConnection {
        service,
        profile: "slow".into(),
        namespace: authority.namespace.clone(),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    let catalog = replaced.catalog().await.unwrap();
    assert_eq!(catalog["models"][0]["model"], "replacement-model");
    assert_eq!(catalog["models"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn magpie_and_openrouter_live_catalogs_and_static_provider_catalogs_publish_cleanly() {
    for provider in [ProviderId::Magpie, ProviderId::OpenRouter] {
        let factory = Arc::new(CatalogFactory::default());
        *factory.account.lock().unwrap() = None;
        *factory.models.lock().unwrap() = Some(vec![model("live-model-1"), model("live-model-2")]);
        let service = ConnectionService::ephemeral().unwrap();
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
        let mut prof = profile();
        prof.provider = provider;
        if !ConnectionsFile::profile_descriptor(provider).credential_optional {
            let kind = ConnectionsFile::profile_descriptor(provider).credential_kind;
            connections.credentials.insert(
                "openrouter-cred".into(),
                crate::connection_profile::ConnectionCredential {
                    kind,
                    provider_family: provider,
                    label: "test".into(),
                    backend: crate::connection_profile::default_credential_backend(kind).into(),
                },
            );
            prof.credential_id = Some("openrouter-cred".into());
        } else {
            prof.credential_id = None;
        }
        prof.settings = crate::connection_profile::normalize_profile_settings(
            provider,
            &BTreeMap::from([("model".into(), "live-model-1".into())]),
        );
        connections.profiles.insert("p".into(), prof);
        service
            .apply(ConnectionCommand::ReplaceMetadata {
                expected: service.metadata().fingerprint().unwrap(),
                connections,
            })
            .await
            .unwrap();
        service.wait_for_catalog_discovery().await;
        let authority = ProcessConnection {
            service: service.clone(),
            profile: "p".into(),
            namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
            evaluator: None,
            guardian_profile: None,
            allow_profile_switch: false,
            selected_profile: std::sync::Mutex::new(None),
        };
        let cat = authority.catalog().await.unwrap();
        let slugs: Vec<&str> = cat["models"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["model"].as_str())
            .collect();
        assert_eq!(slugs, vec!["live-model-1", "live-model-2"]);
        let selected = authority.capture(Some("live-model-2")).await.unwrap();
        assert_eq!(selected.config.effective_model(), "live-model-2");
        assert!(authority.capture(Some("not-in-catalog")).await.is_err());
    }

    for (provider, expected_model) in [
        (ProviderId::OpenAiResponses, "gpt-5.4"),
        (ProviderId::GoogleGeminiGenerateContent, "gemini-2.5-flash"),
        (ProviderId::AnthropicMessages, "claude-sonnet-4-6"),
    ] {
        let factory = Arc::new(CatalogFactory::default());
        *factory.account.lock().unwrap() = None;
        let service = ConnectionService::ephemeral().unwrap();
        service
            .attach_callable_registry(
                Arc::new(alan_llmfs::LlmFs::new()),
                factory,
                Config::default(),
                None,
            )
            .await
            .unwrap();
        let mut connections = service.metadata();
        let kind = ConnectionsFile::profile_descriptor(provider).credential_kind;
        connections.credentials.insert(
            "cred-ref".into(),
            crate::connection_profile::ConnectionCredential {
                kind,
                provider_family: provider,
                label: "test".into(),
                backend: crate::connection_profile::default_credential_backend(kind).into(),
            },
        );
        let mut prof = profile();
        prof.provider = provider;
        prof.credential_id = Some("cred-ref".into());
        prof.settings = crate::connection_profile::normalize_profile_settings(
            provider,
            &BTreeMap::from([
                ("model".into(), expected_model.into()),
                ("project_id".into(), "proj-1".into()),
                ("location".into(), "global".into()),
            ]),
        );
        prof.settings.retain(|key, _| {
            let d = ConnectionsFile::profile_descriptor(provider);
            d.required_settings.contains(&key.as_str())
                || d.optional_settings.contains(&key.as_str())
        });
        connections.profiles.insert("p".into(), prof);
        service
            .apply(ConnectionCommand::ReplaceMetadata {
                expected: service.metadata().fingerprint().unwrap(),
                connections,
            })
            .await
            .unwrap();
        service.wait_for_catalog_discovery().await;
        let authority = ProcessConnection {
            service,
            profile: "p".into(),
            namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
            evaluator: None,
            guardian_profile: None,
            allow_profile_switch: false,
            selected_profile: std::sync::Mutex::new(None),
        };
        let cat = authority.catalog().await.unwrap();
        let slugs: Vec<&str> = cat["models"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m["model"].as_str())
            .collect();
        assert!(slugs.contains(&expected_model), "{provider:?}: {slugs:?}");
    }
}

#[tokio::test]
async fn reload_connections_redirects_unconfigured_default_after_live_login() {
    let dir = tempfile::tempdir().unwrap();
    let conn_path = dir.path().join("connections.toml");
    let bindings = ConnectionStoreBindings {
        metadata_path: conn_path.clone(),
    };
    let factory = Arc::new(CatalogFactory::default());
    *factory.account.lock().unwrap() = None;
    *factory.models.lock().unwrap() = Some(vec![model("moonshotai/kimi-k2.6")]);

    let service = ConnectionService::open(&bindings).unwrap();
    service
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            factory,
            Config::default(),
            None,
        )
        .await
        .unwrap();

    let authority = ProcessConnection {
        service: service.clone(),
        profile: "default".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    assert!(authority.capture_initial().await.unwrap().is_none());

    let mut updated = ConnectionsFile::default();
    updated.credentials.insert(
        "openrouter-main".into(),
        crate::connection_profile::ConnectionCredential {
            kind: crate::connection_profile::CredentialKind::SecretString,
            provider_family: ProviderId::OpenRouter,
            label: "OpenRouter".into(),
            backend: crate::connection_profile::default_credential_backend(
                crate::connection_profile::CredentialKind::SecretString,
            )
            .into(),
        },
    );
    let mut prof = profile();
    prof.provider = ProviderId::OpenRouter;
    prof.credential_id = Some("openrouter-main".into());
    prof.settings = BTreeMap::from([("model".into(), "moonshotai/kimi-k2.6".into())]);
    updated.profiles.insert("openrouter-main".into(), prof);
    updated.default_profile = Some("openrouter-main".into());
    updated.save_to_path(&conn_path).unwrap();

    service.reload_connections().await.unwrap();
    let captured = authority
        .capture(Some("moonshotai/kimi-k2.6"))
        .await
        .unwrap();
    assert_eq!(captured.identity.profile, "openrouter-main");
    assert_eq!(captured.config.effective_model(), "moonshotai/kimi-k2.6");
    let cat = authority.catalog().await.unwrap();
    assert_eq!(cat["models"][0]["model"], "moonshotai/kimi-k2.6");
}

#[derive(Debug)]
struct MultiProviderCatalogFactory;

impl LlmClientFactory for MultiProviderCatalogFactory {
    fn create(
        &self,
        _base: &Config,
        selected: Option<&str>,
        connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        let Some(profile_id) = selected else {
            return Ok(LlmClient::new(MockLlmProvider::new()));
        };
        let provider = connections.profiles[profile_id].provider;
        let (models, account, name) = match provider {
            ProviderId::Chatgpt => (
                Some(vec![model("gpt-5.3-codex"), model("gpt-5.4")]),
                Some("acct-1".to_string()),
                "chatgpt",
            ),
            ProviderId::Magpie => (
                Some(vec![ProviderModel {
                    slug: "auto/deepseek-v4.1-flash".to_string(),
                    context_window_tokens: 1_048_576,
                    supported_reasoning_efforts: vec![
                        ReasoningEffort::Low,
                        ReasoningEffort::High,
                        ReasoningEffort::Max,
                    ],
                    default_reasoning_effort: Some(ReasoningEffort::High),
                }]),
                None,
                "magpie",
            ),
            _ => (None, None, "mock"),
        };
        Ok(LlmClient::new(NamedCatalogClient {
            name,
            account,
            models,
            mock: MockLlmProvider::new(),
        }))
    }
}

struct NamedCatalogClient {
    name: &'static str,
    account: Option<String>,
    models: Option<Vec<ProviderModel>>,
    mock: MockLlmProvider,
}

#[async_trait::async_trait]
impl LlmProvider for NamedCatalogClient {
    fn account_identity(&self) -> Option<&str> {
        self.account.as_deref()
    }
    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        Ok(self.models.clone())
    }
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.mock.generate(request).await
    }
    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.mock.chat(system, user).await
    }
    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.mock.generate_stream(request).await
    }
    fn provider_name(&self) -> &'static str {
        self.name
    }
}

#[tokio::test]
async fn root_profile_switch_redirects_after_live_login_publishes_all_profiles_and_restores_across_profiles()
 {
    let dir = tempfile::tempdir().unwrap();
    let conn_path = dir.path().join("connections.toml");
    let mut initial = ConnectionsFile::default();
    let kind = ConnectionsFile::profile_descriptor(ProviderId::Chatgpt).credential_kind;
    initial.credentials.insert(
        "chatgpt-main".into(),
        crate::connection_profile::ConnectionCredential {
            kind,
            provider_family: ProviderId::Chatgpt,
            label: "ChatGPT".into(),
            backend: crate::connection_profile::default_credential_backend(kind).into(),
        },
    );
    let mut chatgpt = profile();
    chatgpt.provider = ProviderId::Chatgpt;
    chatgpt.credential_id = Some("chatgpt-main".into());
    chatgpt.settings = crate::connection_profile::normalize_profile_settings(
        ProviderId::Chatgpt,
        &BTreeMap::from([("model".into(), "gpt-5.3-codex".into())]),
    );
    initial.profiles.insert("chatgpt-main".into(), chatgpt);
    initial.default_profile = Some("chatgpt-main".into());
    initial.save_to_path(&conn_path).unwrap();

    let service = ConnectionService::open(&ConnectionStoreBindings {
        metadata_path: conn_path.clone(),
    })
    .unwrap();
    service
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            Arc::new(MultiProviderCatalogFactory),
            Config::default(),
            None,
        )
        .await
        .unwrap();

    let root = ProcessConnection {
        service: service.clone(),
        profile: "chatgpt-main".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: true,
        selected_profile: std::sync::Mutex::new(None),
    };
    let child = ProcessConnection {
        service: service.clone(),
        profile: "chatgpt-main".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };

    let initial_captured = root.capture_initial().await.unwrap().unwrap();
    assert_eq!(initial_captured.identity.profile, "chatgpt-main");

    let mut magpie = profile();
    magpie.provider = ProviderId::Magpie;
    magpie.credential_id = None;
    magpie.settings = crate::connection_profile::normalize_profile_settings(
        ProviderId::Magpie,
        &BTreeMap::from([
            ("base_url".into(), "http://127.0.0.1:3425/v1".into()),
            ("model".into(), "auto/deepseek-v4.1-flash".into()),
        ]),
    );
    initial.profiles.insert("magpie-main".into(), magpie);
    initial.default_profile = Some("magpie-main".into());
    initial.save_to_path(&conn_path).unwrap();
    service.reload_connections().await.unwrap();
    service.wait_for_catalog_discovery().await;

    assert!(
        child
            .capture(Some("magpie-main::auto/deepseek-v4.1-flash"))
            .await
            .is_err()
    );

    let magpie_captured = root
        .capture(Some("magpie-main::auto/deepseek-v4.1-flash"))
        .await
        .unwrap();
    assert_eq!(magpie_captured.identity.profile, "magpie-main");
    assert_eq!(magpie_captured.identity.model, "auto/deepseek-v4.1-flash");
    assert_eq!(
        magpie_captured.config.effective_context_window_tokens(),
        1_048_576
    );

    let cat = root.catalog().await.unwrap();
    assert_eq!(cat["profile"], "magpie-main");
    let profile_ids: Vec<&str> = cat["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["profile"].as_str())
        .collect();
    assert_eq!(profile_ids, vec!["magpie-main", "chatgpt-main"]);

    let back_to_gpt = root.capture(Some("chatgpt-main::gpt-5.4")).await.unwrap();
    assert_eq!(back_to_gpt.identity.profile, "chatgpt-main");
    assert_eq!(back_to_gpt.identity.model, "gpt-5.4");
    assert_eq!(
        root.restore(&magpie_captured.identity)
            .await
            .unwrap()
            .identity,
        magpie_captured.identity
    );

    ConnectionsFile::default().save_to_path(&conn_path).unwrap();
    service.reload_connections().await.unwrap();
    assert!(root.capture_initial().await.unwrap().is_none());
}
