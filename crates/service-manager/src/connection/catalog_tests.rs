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
    let service = ConnectionService::ephemeral("test");
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
        (
            "base_url".into(),
            "https://chatgpt.com/backend-api/codex".into(),
        ),
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
    };
    (service, authority)
}

#[tokio::test]
async fn chatgpt_catalog_publication_preserves_captured_bindings_and_retries_unavailable_metadata()
{
    let factory = Arc::new(CatalogFactory::default());
    let (service, authority) = managed_connection(factory.clone()).await;
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
    let replacement = ConnectionService::ephemeral("test");
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
    };
    let catalog = replaced.catalog().await.unwrap();
    assert_eq!(catalog["models"][0]["model"], "replacement-model");
    assert_eq!(catalog["models"].as_array().unwrap().len(), 1);
}
