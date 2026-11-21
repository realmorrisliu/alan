//! Vertex AI live discovery, custom `gemini-*` authorization, and llmfs live catalog sync tests.

use super::*;
use crate::connection::process_binding::ProcessConnection;
use alan_agent_engine::runtime::model_binding::ConnectionAuthority;
use alan_llm::gemini_catalog::{
    GEMINI_DEFAULT_CONTEXT_WINDOW, VERTEX_BASELINE_MODEL_SLUGS, infer_gemini_provider_model,
};
use alan_llm::{ProviderModel, ReasoningEffort};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default)]
struct VertexCatalogFactory {
    models: Arc<Mutex<Option<Vec<ProviderModel>>>>,
    reads: Arc<AtomicUsize>,
    creates: AtomicUsize,
    blocked: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    started: Arc<tokio::sync::Notify>,
}

impl LlmClientFactory for VertexCatalogFactory {
    fn create(&self, _: &Config, _: Option<&str>, _: &ConnectionsFile) -> Result<LlmClient> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Ok(LlmClient::new(VertexCatalogClient {
            models: self.models.clone(),
            reads: self.reads.clone(),
            mock: MockLlmProvider::new(),
            blocked: self.blocked.lock().unwrap().clone(),
            started: self.started.clone(),
        }))
    }
}

struct VertexCatalogClient {
    models: Arc<Mutex<Option<Vec<ProviderModel>>>>,
    reads: Arc<AtomicUsize>,
    mock: MockLlmProvider,
    blocked: Option<Arc<tokio::sync::Semaphore>>,
    started: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl LlmProvider for VertexCatalogClient {
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
        "google_gemini_generate_content"
    }
}

fn vertex_profile(model: &str) -> ConnectionProfile {
    let mut prof = profile();
    prof.provider = ProviderId::GoogleGeminiGenerateContent;
    prof.credential_id = None;
    prof.settings = crate::connection_profile::normalize_profile_settings(
        ProviderId::GoogleGeminiGenerateContent,
        &BTreeMap::from([
            ("project_id".into(), "demo-gcp-proj".into()),
            ("location".into(), "global".into()),
            ("model".into(), model.into()),
        ]),
    );
    prof
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

#[tokio::test]
async fn vertex_live_discovery_publishes_to_connection_and_llmfs_and_supports_custom_gemini_slugs()
{
    let factory = Arc::new(VertexCatalogFactory::default());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let service = ConnectionService::ephemeral().unwrap();
    service
        .attach_callable_registry(llmfs.clone(), factory.clone(), Config::default(), None)
        .await
        .unwrap();

    // Start with live discovery returning None -> graceful fallback to curated baseline.
    let mut connections = service.metadata();
    connections
        .profiles
        .insert("vertex-main".into(), vertex_profile("gemini-2.5-flash"));
    connections.default_profile = Some("vertex-main".into());
    service
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: service.metadata().fingerprint().unwrap(),
            connections,
        })
        .await
        .unwrap();

    let authority = ProcessConnection {
        service: service.clone(),
        profile: "vertex-main".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };

    // Before live discovery succeeds, both Process catalog and llmfs fall back to baseline.
    let fallback_models = read_llmfs_json(&llmfs, "/mnt/llm/connections/vertex-main/models").await;
    assert_eq!(fallback_models["version"], 1);
    assert_eq!(fallback_models["connection"], "vertex-main");
    assert_eq!(
        fallback_models["provider"],
        "google_gemini_generate_content"
    );
    assert_eq!(fallback_models["source"], "bundled-gemini");
    let fallback_slugs: Vec<&str> = fallback_models["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["slug"].as_str())
        .collect();
    for baseline in VERTEX_BASELINE_MODEL_SLUGS {
        assert!(fallback_slugs.contains(baseline), "missing {baseline}");
    }

    // Explicit custom official `gemini-*` publisher slug is authorized even before live discovery.
    let custom = authority
        .capture(Some("gemini-4.5-pro-preview"))
        .await
        .unwrap();
    assert_eq!(custom.config.effective_model(), "gemini-4.5-pro-preview");
    let custom_info = custom
        .config
        .effective_model_info()
        .expect("custom gemini-* model gets inferred family capabilities");
    assert_eq!(custom_info.family, "gemini-4.5");
    assert_eq!(
        custom_info.context_window_tokens,
        GEMINI_DEFAULT_CONTEXT_WINDOW
    );
    assert_eq!(
        custom_info.supported_reasoning_efforts,
        vec![
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
        ]
    );
    assert_eq!(
        custom_info.default_reasoning_effort,
        Some(ReasoningEffort::High)
    );
    let resolved_controls = alan_agent_engine::resolve_runtime_request_controls(
        &custom.config,
        alan_agent_engine::provider_capabilities_for_config(&custom.config),
        alan_agent_engine::RequestControlIntent::default(),
    )
    .unwrap();
    assert_eq!(
        resolved_controls.reasoning_effort(),
        Some(ReasoningEffort::High)
    );

    // Non-Gemini and unsafe slugs are rejected cleanly.
    for rejected in [
        "arbitrary-new-model",
        "claude-sonnet-4-6",
        "gemini-",
        "gemini-4/../evil",
        "gemini-4/bad",
        "GEMINI-4-PRO",
    ] {
        assert!(
            authority.capture(Some(rejected)).await.is_err(),
            "{rejected} must be rejected"
        );
    }

    // Live discovery recovers with a newly launched Vertex model; explicit selection retries it
    // and publishes live models to both ConnectionService and llmfs (including `default` alias).
    let live_list = vec![
        infer_gemini_provider_model("gemini-4.0-pro-preview"),
        infer_gemini_provider_model("gemini-2.5-flash"),
    ];
    *factory.models.lock().unwrap() = Some(live_list);

    let selected = authority
        .capture(Some("gemini-4.0-pro-preview"))
        .await
        .unwrap();
    assert_eq!(selected.config.effective_model(), "gemini-4.0-pro-preview");
    assert_eq!(
        selected
            .config
            .effective_model_info()
            .unwrap()
            .default_reasoning_effort,
        Some(ReasoningEffort::High)
    );

    let catalog = authority.catalog().await.unwrap();
    let ui_slugs: Vec<&str> = catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["model"].as_str())
        .collect();
    assert_eq!(ui_slugs, vec!["gemini-4.0-pro-preview", "gemini-2.5-flash"]);

    for path in [
        "/mnt/llm/connections/vertex-main/models",
        "/mnt/llm/connections/default/models",
    ] {
        let doc = read_llmfs_json(&llmfs, path).await;
        assert_eq!(doc["source"], "live", "{path}");
        assert_eq!(doc["provider"], "google_gemini_generate_content", "{path}");
        assert_eq!(doc["models"][0]["slug"], "gemini-4.0-pro-preview", "{path}");
    }

    // The captured Process namespace also serves the published live models file.
    let captured_shell = Shell::new(selected.root);
    let captured_doc: serde_json::Value = serde_json::from_slice(
        &captured_shell
            .cat("/mnt/llm/connections/vertex-main/models")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(captured_doc["source"], "live");
    assert_eq!(captured_doc["models"][0]["slug"], "gemini-4.0-pro-preview");
}

#[tokio::test]
async fn superseded_vertex_discovery_does_not_overwrite_replacement_llmfs_models() {
    let factory = Arc::new(VertexCatalogFactory::default());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let service = ConnectionService::ephemeral().unwrap();
    service
        .attach_callable_registry(llmfs.clone(), factory.clone(), Config::default(), None)
        .await
        .unwrap();

    let blocked = Arc::new(tokio::sync::Semaphore::new(0));
    *factory.blocked.lock().unwrap() = Some(blocked.clone());
    *factory.models.lock().unwrap() = Some(vec![infer_gemini_provider_model("gemini-4.0-stale")]);

    let writer = service.clone();
    let slow_add = tokio::spawn(async move {
        writer
            .apply(ConnectionCommand::AddProfile {
                profile_id: "slow-vertex".into(),
                profile: vertex_profile("gemini-2.5-flash"),
            })
            .await
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        factory.started.notified(),
    )
    .await
    .unwrap();

    // Remove and replace `slow-vertex` while the first discovery is blocked in flight.
    service
        .apply(ConnectionCommand::RemoveProfile {
            profile_id: "slow-vertex".into(),
        })
        .await
        .unwrap();
    *factory.blocked.lock().unwrap() = None;
    *factory.models.lock().unwrap() = Some(vec![infer_gemini_provider_model("gemini-4.1-fresh")]);
    service
        .apply(ConnectionCommand::AddProfile {
            profile_id: "slow-vertex".into(),
            profile: vertex_profile("gemini-2.5-flash"),
        })
        .await
        .unwrap();
    service.wait_for_catalog_discovery().await;

    // Release the old in-flight discovery and ensure it cannot pollute llmfs or catalog.
    blocked.add_permits(1);
    tokio::time::timeout(std::time::Duration::from_secs(2), slow_add)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let llmfs_doc = read_llmfs_json(&llmfs, "/mnt/llm/connections/slow-vertex/models").await;
    assert_eq!(llmfs_doc["source"], "live");
    assert_eq!(llmfs_doc["models"].as_array().unwrap().len(), 1);
    assert_eq!(llmfs_doc["models"][0]["slug"], "gemini-4.1-fresh");
}

struct StaticVertexTokenSource;

#[async_trait::async_trait]
impl alan_llm::google_gemini_generate_content::VertexAccessTokenSource for StaticVertexTokenSource {
    async fn access_token(&self) -> Result<String> {
        Ok("vertex-e2e-token".to_string())
    }
}

#[derive(Debug)]
struct RealVertexClientFactory {
    base_url: String,
    express_api_key: Option<String>,
}

impl LlmClientFactory for RealVertexClientFactory {
    fn create(
        &self,
        _base: &Config,
        selected: Option<&str>,
        connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        let profile_id = selected.context("missing selected profile")?;
        let prof = connections
            .profiles
            .get(profile_id)
            .context("unknown profile")?;
        let settings =
            crate::connection_profile::normalize_profile_settings(prof.provider, &prof.settings);
        let model = settings
            .get("model")
            .cloned()
            .unwrap_or_else(|| alan_llm::gemini_catalog::GEMINI_DEFAULT_MODEL.to_string());
        let location = settings
            .get("location")
            .cloned()
            .unwrap_or_else(|| "global".to_string());
        let client = if let Some(api_key) = &self.express_api_key {
            alan_llm::GoogleGeminiGenerateContentClient::from_vertex_config(
                alan_llm::VertexClientConfig {
                    project_id: None,
                    location,
                    model,
                    auth: alan_llm::VertexAuth::ApiKey(api_key.clone()),
                    base_url: Some(self.base_url.clone()),
                },
            )?
        } else {
            alan_llm::GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
                alan_llm::VertexClientConfig {
                    project_id: settings.get("project_id").cloned(),
                    location,
                    model,
                    auth: alan_llm::VertexAuth::Adc,
                    base_url: Some(self.base_url.clone()),
                },
                Arc::new(StaticVertexTokenSource),
            )?
        };
        Ok(LlmClient::new(client))
    }
}

async fn spawn_vertex_http_mock(
    healthy: Arc<std::sync::atomic::AtomicBool>,
    requests: Arc<AtomicUsize>,
) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let healthy = healthy.clone();
            let requests = requests.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                requests.fetch_add(1, Ordering::SeqCst);
                let (status, body) = if healthy.load(Ordering::SeqCst) {
                    (
                        "200 OK",
                        r#"{"publisherModels":[{"name":"publishers/google/models/gemini-4.2-pro-preview","launchStage":"PUBLIC_PREVIEW"}]}"#,
                    )
                } else {
                    ("403 Forbidden", r#"{"error":"transient iam error"}"#)
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (format!("http://{addr}"), handle)
}

#[tokio::test]
async fn real_vertex_client_express_and_failed_adc_stay_bundled_until_explicit_retry_recovers() {
    let healthy = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let requests = Arc::new(AtomicUsize::new(0));
    let (base_url, server_handle) = spawn_vertex_http_mock(healthy.clone(), requests.clone()).await;

    // 1. Express Mode: 0 HTTP requests, llmfs remains `bundled-gemini`, baseline models usable.
    let express_llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let express_service = ConnectionService::ephemeral().unwrap();
    express_service
        .attach_callable_registry(
            express_llmfs.clone(),
            Arc::new(RealVertexClientFactory {
                base_url: base_url.clone(),
                express_api_key: Some("express-test-key".to_string()),
            }),
            Config::default(),
            None,
        )
        .await
        .unwrap();
    let mut express_conns = express_service.metadata();
    express_conns
        .profiles
        .insert("vertex-express".into(), vertex_profile("gemini-2.5-flash"));
    express_service
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: express_service.metadata().fingerprint().unwrap(),
            connections: express_conns,
        })
        .await
        .unwrap();
    let express_authority = ProcessConnection {
        service: express_service.clone(),
        profile: "vertex-express".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    let express_doc =
        read_llmfs_json(&express_llmfs, "/mnt/llm/connections/vertex-express/models").await;
    assert_eq!(express_doc["source"], "bundled-gemini");
    let express_ui = express_authority.catalog().await.unwrap();
    assert!(
        express_ui["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["model"] == "gemini-2.5-flash")
    );

    // 2. ADC Mode: transient 403 keeps llmfs at `bundled-gemini` (not falsely `live`) and
    //    caches `Some(None)` so idle observation does not retry, while explicit selection retries.
    let adc_llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let adc_service = ConnectionService::ephemeral().unwrap();
    adc_service
        .attach_callable_registry(
            adc_llmfs.clone(),
            Arc::new(RealVertexClientFactory {
                base_url,
                express_api_key: None,
            }),
            Config::default(),
            None,
        )
        .await
        .unwrap();
    let mut adc_conns = adc_service.metadata();
    adc_conns
        .profiles
        .insert("vertex-adc".into(), vertex_profile("gemini-2.5-flash"));
    adc_service
        .apply(ConnectionCommand::ReplaceMetadata {
            expected: adc_service.metadata().fingerprint().unwrap(),
            connections: adc_conns,
        })
        .await
        .unwrap();
    adc_service.wait_for_catalog_discovery().await;
    let adc_authority = ProcessConnection {
        service: adc_service.clone(),
        profile: "vertex-adc".into(),
        namespace: alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new()),
        evaluator: None,
        guardian_profile: None,
        allow_profile_switch: false,
        selected_profile: std::sync::Mutex::new(None),
    };
    assert_eq!(requests.load(Ordering::SeqCst), 1);

    let failed_doc = read_llmfs_json(&adc_llmfs, "/mnt/llm/connections/vertex-adc/models").await;
    assert_eq!(failed_doc["source"], "bundled-gemini");
    for _ in 0..5 {
        let ui = adc_authority.catalog().await.unwrap();
        assert!(
            ui["models"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["model"] == "gemini-2.5-flash")
        );
    }
    assert_eq!(
        requests.load(Ordering::SeqCst),
        1,
        "status observation must not retry failed Vertex discovery"
    );

    // Once upstream IAM/network recovers, explicit model selection retries discovery and
    // promotes `/mnt/llm/connections/vertex-adc/models` to `source: "live"`.
    healthy.store(true, Ordering::SeqCst);
    let recovered = adc_authority
        .capture(Some("gemini-4.2-pro-preview"))
        .await
        .unwrap();
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    assert_eq!(recovered.config.effective_model(), "gemini-4.2-pro-preview");

    let live_doc = read_llmfs_json(&adc_llmfs, "/mnt/llm/connections/vertex-adc/models").await;
    assert_eq!(live_doc["source"], "live");
    assert_eq!(live_doc["models"][0]["slug"], "gemini-4.2-pro-preview");
    let live_ui = adc_authority.catalog().await.unwrap();
    assert_eq!(live_ui["models"][0]["model"], "gemini-4.2-pro-preview");

    server_handle.abort();
}
