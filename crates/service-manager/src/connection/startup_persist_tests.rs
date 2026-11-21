//! Non-blocking startup discovery, warm catalog disk cache, and Root `/model` persistence tests.

use super::*;
use crate::connection::catalog_cache::cache_path_for_metadata;
use crate::connection::process_binding::ProcessConnection;
use alan_agent_engine::runtime::model_binding::ConnectionAuthority;
use alan_llm::{ProviderModel, ReasoningEffort};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
struct ControlledCatalogFactory {
    models_by_profile: Arc<Mutex<BTreeMap<String, Option<Vec<ProviderModel>>>>>,
    account_by_profile: Arc<Mutex<BTreeMap<String, Option<String>>>>,
    gate: Arc<Mutex<Option<Arc<tokio::sync::Semaphore>>>>,
    started: Arc<tokio::sync::Notify>,
    discovery_calls: Arc<AtomicUsize>,
}

impl LlmClientFactory for ControlledCatalogFactory {
    fn create(
        &self,
        _base: &Config,
        selected: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        let profile_id = selected.unwrap_or("default").to_string();
        let account = self
            .account_by_profile
            .lock()
            .unwrap()
            .get(&profile_id)
            .cloned()
            .flatten();
        Ok(LlmClient::new(ControlledCatalogClient {
            profile_id,
            models_by_profile: self.models_by_profile.clone(),
            account,
            gate: self.gate.lock().unwrap().clone(),
            started: self.started.clone(),
            discovery_calls: self.discovery_calls.clone(),
            mock: MockLlmProvider::new(),
        }))
    }
}

struct ControlledCatalogClient {
    profile_id: String,
    models_by_profile: Arc<Mutex<BTreeMap<String, Option<Vec<ProviderModel>>>>>,
    account: Option<String>,
    gate: Option<Arc<tokio::sync::Semaphore>>,
    started: Arc<tokio::sync::Notify>,
    discovery_calls: Arc<AtomicUsize>,
    mock: MockLlmProvider,
}

#[async_trait::async_trait]
impl LlmProvider for ControlledCatalogClient {
    fn account_identity(&self) -> Option<&str> {
        self.account.as_deref()
    }

    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        self.discovery_calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_waiters();
        if let Some(gate) = &self.gate {
            gate.acquire().await.unwrap().forget();
        }
        Ok(self
            .models_by_profile
            .lock()
            .unwrap()
            .get(&self.profile_id)
            .cloned()
            .flatten())
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
        "controlled"
    }
}

fn sample_model(slug: &str) -> ProviderModel {
    ProviderModel {
        slug: slug.into(),
        context_window_tokens: 128_000,
        supported_reasoning_efforts: vec![ReasoningEffort::Low, ReasoningEffort::High],
        default_reasoning_effort: Some(ReasoningEffort::High),
    }
}

async fn read_llmfs_json(llmfs: &Arc<alan_llmfs::LlmFs>, path: &str) -> serde_json::Value {
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs.clone()),
        alan_kernel::Access::ReadOnly,
    );
    let shell = Shell::new(InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(ns),
    )));
    serde_json::from_slice(&shell.cat(path).await.unwrap()).unwrap()
}

fn write_two_profile_connections(conn_path: &std::path::Path) -> ConnectionsFile {
    let mut file = ConnectionsFile::default();
    let chatgpt_kind = ConnectionsFile::profile_descriptor(ProviderId::Chatgpt).credential_kind;
    file.credentials.insert(
        "chatgpt-cred".into(),
        crate::connection_profile::ConnectionCredential {
            kind: chatgpt_kind,
            provider_family: ProviderId::Chatgpt,
            label: "ChatGPT".into(),
            backend: crate::connection_profile::default_credential_backend(chatgpt_kind).into(),
        },
    );
    let mut chatgpt = profile();
    chatgpt.provider = ProviderId::Chatgpt;
    chatgpt.credential_id = Some("chatgpt-cred".into());
    chatgpt.settings = crate::connection_profile::normalize_profile_settings(
        ProviderId::Chatgpt,
        &BTreeMap::from([("model".into(), "gpt-5.3-codex".into())]),
    );
    file.profiles.insert("chatgpt-main".into(), chatgpt);

    let mut magpie = profile();
    magpie.provider = ProviderId::Magpie;
    magpie.credential_id = None;
    magpie.settings = crate::connection_profile::normalize_profile_settings(
        ProviderId::Magpie,
        &BTreeMap::from([
            ("base_url".into(), "http://127.0.0.1:3425/v1".into()),
            ("model".into(), "auto/deepseek-v4".into()),
        ]),
    );
    file.profiles.insert("magpie-main".into(), magpie);
    file.default_profile = Some("chatgpt-main".into());
    file.save_to_path(conn_path).unwrap();
    file
}

#[tokio::test]
async fn slow_remote_catalog_discovery_does_not_block_boot_and_populates_concurrently() {
    let dir = tempfile::tempdir().unwrap();
    let conn_path = dir.path().join("connections.toml");
    write_two_profile_connections(&conn_path);

    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let factory = Arc::new(ControlledCatalogFactory::default());
    *factory.gate.lock().unwrap() = Some(gate.clone());
    factory
        .account_by_profile
        .lock()
        .unwrap()
        .insert("chatgpt-main".into(), Some("acct-fast-boot".into()));
    factory.models_by_profile.lock().unwrap().insert(
        "chatgpt-main".into(),
        Some(vec![sample_model("gpt-5.3-codex"), sample_model("gpt-5.4")]),
    );
    factory.models_by_profile.lock().unwrap().insert(
        "magpie-main".into(),
        Some(vec![
            sample_model("auto/deepseek-v4"),
            sample_model("auto/qwen-3.5"),
        ]),
    );

    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let service = ConnectionService::open(&ConnectionStoreBindings {
        metadata_path: conn_path.clone(),
    })
    .unwrap();

    // Boot/refresh must return immediately (<200ms) even while remote /models are stalled.
    let started = Instant::now();
    service
        .attach_callable_registry(llmfs.clone(), factory.clone(), Config::default(), None)
        .await
        .unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(200),
        "boot blocked on remote catalog discovery for {elapsed:?}"
    );

    // Release both stalled remote responses concurrently and await settlement.
    gate.add_permits(2);
    tokio::time::timeout(Duration::from_secs(2), service.wait_for_catalog_discovery())
        .await
        .unwrap();

    let chatgpt_doc = read_llmfs_json(&llmfs, "/mnt/llm/connections/chatgpt-main/models").await;
    assert_eq!(chatgpt_doc["source"], "live");
    assert_eq!(chatgpt_doc["models"].as_array().unwrap().len(), 2);

    let magpie_doc = read_llmfs_json(&llmfs, "/mnt/llm/connections/magpie-main/models").await;
    assert_eq!(magpie_doc["source"], "live");
    assert_eq!(magpie_doc["models"].as_array().unwrap().len(), 2);

    // Disk cache is written alongside connections.toml without any secrets.
    let cache_path = cache_path_for_metadata(&conn_path);
    assert!(cache_path.is_file(), "catalog-cache.json should be written");
    let raw_cache = std::fs::read_to_string(&cache_path).unwrap();
    assert!(raw_cache.contains("gpt-5.4"));
    assert!(raw_cache.contains("auto/qwen-3.5"));
}

#[tokio::test]
async fn warm_disk_cache_hydrates_immediately_before_network_and_respects_account_identity() {
    let dir = tempfile::tempdir().unwrap();
    let conn_path = dir.path().join("connections.toml");
    write_two_profile_connections(&conn_path);

    // Cold boot: populate disk cache from live discovery.
    let factory_cold = Arc::new(ControlledCatalogFactory::default());
    factory_cold
        .account_by_profile
        .lock()
        .unwrap()
        .insert("chatgpt-main".into(), Some("acct-a".into()));
    factory_cold.models_by_profile.lock().unwrap().insert(
        "chatgpt-main".into(),
        Some(vec![sample_model("gpt-5.3-codex"), sample_model("gpt-5.4")]),
    );
    factory_cold.models_by_profile.lock().unwrap().insert(
        "magpie-main".into(),
        Some(vec![sample_model("auto/deepseek-v4")]),
    );

    let service_cold = ConnectionService::open(&ConnectionStoreBindings {
        metadata_path: conn_path.clone(),
    })
    .unwrap();
    service_cold
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            factory_cold,
            Config::default(),
            None,
        )
        .await
        .unwrap();
    service_cold.wait_for_catalog_discovery().await;

    // Warm boot with completely stalled network: cached models must hydrate immediately.
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let factory_warm = Arc::new(ControlledCatalogFactory::default());
    *factory_warm.gate.lock().unwrap() = Some(gate.clone());
    factory_warm
        .account_by_profile
        .lock()
        .unwrap()
        .insert("chatgpt-main".into(), Some("acct-a".into()));
    factory_warm.models_by_profile.lock().unwrap().insert(
        "chatgpt-main".into(),
        Some(vec![
            sample_model("gpt-5.3-codex"),
            sample_model("gpt-5.4"),
            sample_model("gpt-5.5-live"),
        ]),
    );
    factory_warm.models_by_profile.lock().unwrap().insert(
        "magpie-main".into(),
        Some(vec![sample_model("auto/deepseek-v4")]),
    );

    let warm_llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let service_warm = ConnectionService::open(&ConnectionStoreBindings {
        metadata_path: conn_path.clone(),
    })
    .unwrap();
    let started = Instant::now();
    service_warm
        .attach_callable_registry(
            warm_llmfs.clone(),
            factory_warm.clone(),
            Config::default(),
            None,
        )
        .await
        .unwrap();
    assert!(started.elapsed() < Duration::from_millis(200));

    let root = ProcessConnection {
        service: service_warm.clone(),
        profile: "chatgpt-main".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: true,
        selected_profile: std::sync::Mutex::new(None),
    };

    // Catalog is immediately available from warm disk cache before network responds!
    let cached_cat = tokio::time::timeout(Duration::from_millis(100), root.catalog())
        .await
        .unwrap()
        .unwrap();
    let cached_slugs: Vec<&str> = cached_cat["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["model"].as_str())
        .collect();
    assert_eq!(cached_slugs, vec!["gpt-5.3-codex", "gpt-5.4"]);

    let warm_doc = read_llmfs_json(&warm_llmfs, "/mnt/llm/connections/chatgpt-main/models").await;
    assert_eq!(warm_doc["models"].as_array().unwrap().len(), 2);

    // Once background network responds, catalog upgrades seamlessly to include gpt-5.5-live.
    gate.add_permits(2);
    service_warm.wait_for_catalog_discovery().await;
    let live_cat = root.catalog().await.unwrap();
    let live_slugs: Vec<&str> = live_cat["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["model"].as_str())
        .collect();
    assert_eq!(live_slugs, vec!["gpt-5.3-codex", "gpt-5.4", "gpt-5.5-live"]);

    // Account mismatch must NOT hydrate account A's cached catalog for account B.
    let gate_b = Arc::new(tokio::sync::Semaphore::new(0));
    let factory_b = Arc::new(ControlledCatalogFactory::default());
    *factory_b.gate.lock().unwrap() = Some(gate_b.clone());
    factory_b
        .account_by_profile
        .lock()
        .unwrap()
        .insert("chatgpt-main".into(), Some("acct-b".into()));
    let service_b = ConnectionService::open(&ConnectionStoreBindings {
        metadata_path: conn_path,
    })
    .unwrap();
    service_b
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            factory_b,
            Config::default(),
            None,
        )
        .await
        .unwrap();
    let root_b = ProcessConnection {
        service: service_b,
        profile: "chatgpt-main".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    assert!(
        root_b.catalog().await.is_err(),
        "account B must not hydrate account A's disk cache"
    );
    gate_b.add_permits(2);
}

#[tokio::test]
async fn root_model_selection_persists_across_restarts_while_child_and_restore_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let conn_path = dir.path().join("connections.toml");
    write_two_profile_connections(&conn_path);

    let factory = Arc::new(ControlledCatalogFactory::default());
    factory
        .account_by_profile
        .lock()
        .unwrap()
        .insert("chatgpt-main".into(), Some("acct-persist".into()));
    factory.models_by_profile.lock().unwrap().insert(
        "chatgpt-main".into(),
        Some(vec![sample_model("gpt-5.3-codex"), sample_model("gpt-5.4")]),
    );
    factory.models_by_profile.lock().unwrap().insert(
        "magpie-main".into(),
        Some(vec![
            sample_model("auto/deepseek-v4"),
            sample_model("auto/qwen-3.5"),
        ]),
    );

    let service = ConnectionService::open(&ConnectionStoreBindings {
        metadata_path: conn_path.clone(),
    })
    .unwrap();
    service
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            factory.clone(),
            Config::default(),
            None,
        )
        .await
        .unwrap();
    service.wait_for_catalog_discovery().await;

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

    // 1. Restricted child model selection MUST NOT persist to connections.toml.
    let initial_root = root.capture_initial().await.unwrap().unwrap();
    assert_eq!(initial_root.identity.model, "gpt-5.3-codex");
    let child_selected = child.capture(Some("gpt-5.4")).await.unwrap();
    assert_eq!(child_selected.identity.model, "gpt-5.4");
    let on_disk_after_child = ConnectionsFile::load_from_path(&conn_path).unwrap().0;
    assert_eq!(
        on_disk_after_child.profiles["chatgpt-main"].settings["model"],
        "gpt-5.3-codex"
    );

    // 2. Root same-profile model selection persists settings["model"] without bumping reload_epoch
    //    or dropping live catalogs.
    let epoch_before = service
        .callables
        .lock()
        .await
        .as_ref()
        .unwrap()
        .reload_epoch;
    let root_same_profile = root.capture(Some("gpt-5.4")).await.unwrap();
    assert_eq!(root_same_profile.identity.profile, "chatgpt-main");
    assert_eq!(root_same_profile.identity.model, "gpt-5.4");
    let epoch_after = service
        .callables
        .lock()
        .await
        .as_ref()
        .unwrap()
        .reload_epoch;
    assert_eq!(epoch_before, epoch_after);
    let on_disk_same = ConnectionsFile::load_from_path(&conn_path).unwrap().0;
    assert_eq!(
        on_disk_same.default_profile.as_deref(),
        Some("chatgpt-main")
    );
    assert_eq!(
        on_disk_same.profiles["chatgpt-main"].settings["model"],
        "gpt-5.4"
    );

    // Earlier admitted binding (from before /model) still restores cleanly!
    assert_eq!(
        root.restore(&initial_root.identity).await.unwrap().identity,
        initial_root.identity
    );

    // 3. Root cross-profile `profile::model` selection persists both default_profile and model.
    let root_cross = root
        .capture(Some("magpie-main::auto/qwen-3.5"))
        .await
        .unwrap();
    assert_eq!(root_cross.identity.profile, "magpie-main");
    assert_eq!(root_cross.identity.model, "auto/qwen-3.5");
    let on_disk_cross = ConnectionsFile::load_from_path(&conn_path).unwrap().0;
    assert_eq!(
        on_disk_cross.default_profile.as_deref(),
        Some("magpie-main")
    );
    assert_eq!(
        on_disk_cross.profiles["magpie-main"].settings["model"],
        "auto/qwen-3.5"
    );

    // 4. `restore()` (used by `/resume`) stays session-local and MUST NOT overwrite disk defaults.
    let restored = root.restore(&initial_root.identity).await.unwrap();
    assert_eq!(restored.identity, initial_root.identity);
    let on_disk_after_restore = ConnectionsFile::load_from_path(&conn_path).unwrap().0;
    assert_eq!(
        on_disk_after_restore.default_profile.as_deref(),
        Some("magpie-main")
    );
    assert_eq!(
        on_disk_after_restore.profiles["magpie-main"].settings["model"],
        "auto/qwen-3.5"
    );

    // 5. Fresh `ConnectionService::open(...)` boots directly into the persisted profile + model.
    let restarted = ConnectionService::open(&ConnectionStoreBindings {
        metadata_path: conn_path,
    })
    .unwrap();
    restarted
        .attach_callable_registry(
            Arc::new(alan_llmfs::LlmFs::new()),
            factory,
            Config::default(),
            None,
        )
        .await
        .unwrap();
    let default_profile = restarted.default_profile().unwrap();
    assert_eq!(default_profile, "magpie-main");
    let restarted_root = ProcessConnection {
        service: restarted,
        profile: default_profile,
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: true,
        selected_profile: std::sync::Mutex::new(None),
    };
    let booted = restarted_root.capture_initial().await.unwrap().unwrap();
    assert_eq!(booted.identity.profile, "magpie-main");
    assert_eq!(booted.identity.model, "auto/qwen-3.5");
}
