//! Generation as a clone-via-open connection directory (add-llm-file-server §4,
//! the minimal callable slice brought into the Plan 9 core). A caller opens
//! `connections/<conn>/clone` (allocating a Generation directory), writes one
//! request document to `data` (committed on clunk), and reads the streamed token
//! records from `events`. Backed by the mock provider — no real API key.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_llm::mock::MockLlmProvider;
use alan_llmfs::{ConnectionLimits, ConnectionProfile, LlmFs};

fn llmfs() -> LlmFs {
    let fs = LlmFs::new();
    fs.register_connection("default", Box::new(MockLlmProvider::new()));
    fs
}

async fn read_all(fs: &LlmFs, path: &[&str], fid: Fid) -> Vec<u8> {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.expect("walk");
    fs.open(fid, OpenMode::Read).await.expect("open");
    fs.read(fid, 0, 65536).await.expect("read")
}

/// Tail `events` until a terminal `done` record appears, accumulating the text.
async fn drain_events(fs: &LlmFs, gen_id: &str, fid: Fid) -> String {
    let path = [
        "connections".to_string(),
        "default".to_string(),
        gen_id.to_string(),
        "events".to_string(),
    ];
    fs.walk(Fid::ROOT, fid, &path).await.unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    let mut acc = String::new();
    let mut offset = 0u64;
    loop {
        let chunk = tokio::time::timeout(Duration::from_millis(500), fs.read(fid, offset, 65536))
            .await
            .expect("events stream stalled")
            .unwrap();
        offset += chunk.len() as u64;
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("\"done\"") {
            break;
        }
    }
    acc
}

#[tokio::test]
async fn clone_open_allocates_a_generation_directory() {
    let fs = llmfs();
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let gen_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    assert!(!gen_id.is_empty());

    // The allocated generation is a real, walkable directory with its files.
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "default", &gen_id], Fid(2)).await)
            .unwrap();
    for f in ["data", "events", "ctl", "status"] {
        assert!(
            listing.lines().any(|l| l == f),
            "generation dir lists {f}: {listing:?}"
        );
    }
}

async fn open_clone(fs: &LlmFs, fid: Fid) -> String {
    fs.walk(
        Fid::ROOT,
        fid,
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(fid, OpenMode::ReadWrite).await.unwrap();
    String::from_utf8(fs.read(fid, 0, 64).await.unwrap()).unwrap()
}

#[tokio::test]
async fn two_clone_opens_allocate_independent_generations() {
    let fs = llmfs();
    let a = open_clone(&fs, Fid(1)).await;
    let b = open_clone(&fs, Fid(2)).await;
    assert_ne!(a, b, "each clone-open allocates a distinct Generation");
}

#[tokio::test]
async fn providers_expose_introspection_files() {
    let fs = llmfs();

    let providers = String::from_utf8(read_all(&fs, &["providers"], Fid(1)).await).unwrap();
    assert!(
        providers.lines().any(|line| line == "openai_responses"),
        "providers dir should list OpenAI Responses: {providers:?}"
    );

    let provider_listing =
        String::from_utf8(read_all(&fs, &["providers", "openai_responses"], Fid(2)).await).unwrap();
    for name in ["models", "capabilities", "status"] {
        assert!(
            provider_listing.lines().any(|line| line == name),
            "provider dir should list {name}: {provider_listing:?}"
        );
    }

    let capabilities: serde_json::Value = serde_json::from_slice(
        &read_all(
            &fs,
            &["providers", "openai_responses", "capabilities"],
            Fid(3),
        )
        .await,
    )
    .unwrap();
    assert_eq!(capabilities["version"], 1);
    assert_eq!(capabilities["provider"], "openai_responses");
    assert_eq!(
        capabilities["capabilities"]["instruction_role"],
        "ResponsesInstructions"
    );
    assert_eq!(
        capabilities["capabilities"]["supports_provider_compaction"],
        true
    );

    let models: serde_json::Value = serde_json::from_slice(
        &read_all(&fs, &["providers", "openai_responses", "models"], Fid(4)).await,
    )
    .unwrap();
    assert_eq!(models["version"], 1);
    assert_eq!(models["provider"], "openai_responses");
    assert_eq!(models["default_model"], "gpt-5.4");
    let model_slugs = models["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|model| model["slug"].as_str())
        .collect::<Vec<_>>();
    assert!(
        model_slugs.contains(&"gpt-5.4"),
        "provider model catalog should expose known models: {models}"
    );

    let status: serde_json::Value = serde_json::from_slice(
        &read_all(&fs, &["providers", "openai_responses", "status"], Fid(5)).await,
    )
    .unwrap();
    assert_eq!(status["status"], "available");
    assert_eq!(status["callable"], false);
    assert_eq!(status["has_model_catalog"], true);

    assert_eq!(
        fs.walk(
            Fid::ROOT,
            Fid(6),
            &[
                "providers".into(),
                "openai_responses".into(),
                "clone".into()
            ],
        )
        .await,
        Err(ErrorCode::NotFound),
        "provider directories are introspect-only; generations start on connections"
    );
}

#[tokio::test]
async fn connections_expose_provider_and_capabilities_without_credentials() {
    let fs = llmfs();

    let connection_listing =
        String::from_utf8(read_all(&fs, &["connections", "default"], Fid(1)).await).unwrap();
    for name in ["clone", "provider", "profile", "meter", "capabilities"] {
        assert!(
            connection_listing.lines().any(|line| line == name),
            "connection dir should list {name}: {connection_listing:?}"
        );
    }

    let provider =
        String::from_utf8(read_all(&fs, &["connections", "default", "provider"], Fid(2)).await)
            .unwrap();
    assert_eq!(provider.trim(), "mock");

    let profile: serde_json::Value = serde_json::from_slice(
        &read_all(&fs, &["connections", "default", "profile"], Fid(3)).await,
    )
    .unwrap();
    assert_eq!(profile["version"], 1);
    assert_eq!(profile["connection"], "default");
    assert_eq!(profile["provider"], "mock");
    assert_eq!(profile["model"], serde_json::Value::Null);
    assert_eq!(profile["credential_ref"], serde_json::Value::Null);
    assert!(
        profile
            .as_object()
            .unwrap()
            .keys()
            .all(|key| key != "credential" && key != "api_key"),
        "connection profile must not expose credential plaintext: {profile}"
    );

    let capabilities: serde_json::Value = serde_json::from_slice(
        &read_all(&fs, &["connections", "default", "capabilities"], Fid(4)).await,
    )
    .unwrap();
    assert_eq!(capabilities["version"], 1);
    assert_eq!(capabilities["connection"], "default");
    assert_eq!(capabilities["provider"], "mock");
    assert_eq!(
        capabilities["capabilities"]["compatibility_tier"],
        "TierAFullFidelityStateful"
    );
    assert_eq!(
        capabilities["capabilities"]["supports_reasoning_effort_control"],
        true
    );
    assert!(
        capabilities
            .as_object()
            .unwrap()
            .keys()
            .all(|key| key != "credential" && key != "api_key"),
        "connection introspection must not expose credential material: {capabilities}"
    );
}

#[tokio::test]
async fn connection_profiles_appear_and_disappear_as_endpoints() {
    let fs = LlmFs::new();
    assert_eq!(
        String::from_utf8(read_all(&fs, &["connections"], Fid(1)).await).unwrap(),
        ""
    );

    fs.register_connection_profile(
        "work",
        ConnectionProfile::new("openai_responses", "gpt-5.4", "credential:openai-main"),
        Box::new(MockLlmProvider::new()),
    );
    let connections = String::from_utf8(read_all(&fs, &["connections"], Fid(2)).await).unwrap();
    assert_eq!(connections, "work");

    let profile: serde_json::Value =
        serde_json::from_slice(&read_all(&fs, &["connections", "work", "profile"], Fid(3)).await)
            .unwrap();
    assert_eq!(profile["connection"], "work");
    assert_eq!(profile["provider"], "openai_responses");
    assert_eq!(profile["model"], "gpt-5.4");
    assert_eq!(profile["credential_ref"], "credential:openai-main");

    let provider =
        String::from_utf8(read_all(&fs, &["connections", "work", "provider"], Fid(4)).await)
            .unwrap();
    assert_eq!(provider.trim(), "openai_responses");

    fs.unregister_connection("work");
    let connections = String::from_utf8(read_all(&fs, &["connections"], Fid(5)).await).unwrap();
    assert_eq!(connections, "");
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(6), &["connections".into(), "work".into()],)
            .await,
        Err(ErrorCode::NotFound),
        "removed connection endpoint is no longer walkable"
    );
}

#[tokio::test]
async fn connection_generation_limit_is_enforced_at_clone_open() {
    let fs = LlmFs::new();
    fs.register_connection_profile_with_limits(
        "limited",
        ConnectionProfile::new("openai_responses", "gpt-5.4", "credential:openai-main"),
        ConnectionLimits::max_generations(1),
        Box::new(MockLlmProvider::new()),
    );

    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "limited".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(2),
        &["connections".into(), "limited".into(), "clone".into()],
    )
    .await
    .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::ReadWrite).await,
        Err(ErrorCode::NoAccess),
        "rate-limit exhaustion is a dial-time open error"
    );

    let meter: serde_json::Value =
        serde_json::from_slice(&read_all(&fs, &["connections", "limited", "meter"], Fid(3)).await)
            .unwrap();
    assert_eq!(meter["limits"]["max_generations"], 1);
    assert_eq!(meter["meter"]["generation_starts"], 1);
    assert_eq!(meter["meter"]["total_tokens"], 0);
    assert_eq!(meter["meter"]["total_cost_microusd"], 0);
}

#[tokio::test]
async fn writing_the_request_streams_tokens_to_events() {
    let fs = llmfs();

    // clone-via-open → generation id
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let gen_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    // write the request document to data; commit on clunk starts the generation
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            gen_id.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, br#"{"user":"hi"}"#).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();

    // events carries the streamed tokens, ending with a done record
    let events = drain_events(&fs, &gen_id, Fid(3)).await;
    assert!(
        events.contains("Mock response"),
        "events should carry the model tokens: {events:?}"
    );
    assert!(
        events.contains("\"done\""),
        "events should end with a terminal record"
    );
}

#[tokio::test]
async fn a_malformed_request_is_rejected_at_clunk() {
    let fs = llmfs();
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let gen_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            gen_id,
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"{ truncated").await.unwrap();
    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::BadRequest));
}

// ---------------------------------------------------------------------------
// Hardening regressions (add-llm-file-server review): lifecycle, access intent,
// discovery, qids, stat, and terminal-event guarantees.
// ---------------------------------------------------------------------------

use alan_llm::{
    GenerationRequest, GenerationResponse, LlmProvider, MessageRole, ReasoningEffort, StreamChunk,
    TokenUsage,
};
use tokio::sync::mpsc;

fn text_chunk(text: &str, is_finished: bool) -> StreamChunk {
    StreamChunk {
        text: Some(text.to_string()),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        usage: None,
        provider_response_id: None,
        provider_response_status: None,
        sequence_number: None,
        tool_call_delta: None,
        is_finished,
        finish_reason: None,
    }
}

/// Yields one text chunk then drops the sender WITHOUT a finished chunk.
struct EarlyCloseProvider;
/// Returns an error before handing back a receiver.
struct StartupFailProvider;
/// Returns a receiver that never yields (its sender is parked), so the Generation
/// stays running until aborted.
struct HangingProvider;
/// Emits a finished chunk with provider usage metadata.
struct UsageProvider;

struct RecordingProvider {
    requests: Arc<Mutex<Vec<GenerationRequest>>>,
}

#[async_trait::async_trait]
impl LlmProvider for RecordingProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        self.requests.lock().unwrap().push(request);
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx.send(text_chunk("recorded", true)).await;
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "recording"
    }
}

#[async_trait::async_trait]
impl LlmProvider for EarlyCloseProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx.send(text_chunk("partial", false)).await;
            // tx dropped here: the stream closes before a finished chunk.
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "early-close"
    }
}

#[async_trait::async_trait]
impl LlmProvider for StartupFailProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        Err(anyhow::anyhow!("startup failed"))
    }
    fn provider_name(&self) -> &'static str {
        "startup-fail"
    }
}

#[async_trait::async_trait]
impl LlmProvider for HangingProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(4);
        // Park the sender so the receiver stays open (the Generation is "running").
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            drop(tx);
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "hanging"
    }
}

#[async_trait::async_trait]
impl LlmProvider for UsageProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let mut chunk = text_chunk("usage", true);
            chunk.usage = Some(TokenUsage {
                prompt_tokens: 11,
                cached_prompt_tokens: Some(3),
                completion_tokens: 7,
                total_tokens: 18,
                reasoning_tokens: Some(5),
            });
            let _ = tx.send(chunk).await;
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "usage"
    }
}

fn llmfs_with(provider: impl LlmProvider + 'static) -> LlmFs {
    let fs = LlmFs::new();
    fs.register_connection("default", Box::new(provider));
    fs
}

async fn clone_gen(fs: &LlmFs, fid: Fid) -> String {
    fs.walk(
        Fid::ROOT,
        fid,
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(fid, OpenMode::ReadWrite).await.unwrap();
    String::from_utf8(fs.read(fid, 0, 64).await.unwrap()).unwrap()
}

async fn commit_request(fs: &LlmFs, gen_id: &str, fid: Fid, body: &[u8]) -> Result<(), ErrorCode> {
    fs.walk(
        Fid::ROOT,
        fid,
        &[
            "connections".into(),
            "default".into(),
            gen_id.into(),
            "data".into(),
        ],
    )
    .await?;
    fs.open(fid, OpenMode::Write).await?;
    fs.write(fid, 0, body).await?;
    fs.clunk(fid).await
}

async fn status_of(fs: &LlmFs, gen_id: &str, fid: Fid) -> String {
    status_doc_of(fs, gen_id, fid).await["status"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn status_doc_of(fs: &LlmFs, gen_id: &str, fid: Fid) -> serde_json::Value {
    serde_json::from_slice(&read_all(fs, &["connections", "default", gen_id, "status"], fid).await)
        .unwrap()
}

#[tokio::test]
async fn versioned_request_dto_maps_to_generation_request() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fs = llmfs_with(RecordingProvider {
        requests: Arc::clone(&requests),
    });
    let gen_id = clone_gen(&fs, Fid(1)).await;

    commit_request(
        &fs,
        &gen_id,
        Fid(2),
        br#"{
            "version": 1,
            "system": "system prompt",
            "messages": [
                {"role": "context", "content": "prior summary"},
                {"role": "user", "content": "hello"},
                {
                    "role": "assistant",
                    "content": "need a tool",
                    "tool_calls": [
                        {"id": "call-1", "name": "lookup", "arguments": {"q": "alan"}}
                    ]
                },
                {"role": "tool", "content": "result", "tool_call_id": "call-1"}
            ],
            "tools": [
                {
                    "name": "lookup",
                    "description": "look up data",
                    "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}
                }
            ],
            "temperature": 0.2,
            "max_tokens": 123,
            "reasoning": {"effort": "low"},
            "extra_params": {"store": true}
        }"#,
    )
    .await
    .unwrap();
    let _events = drain_events(&fs, &gen_id, Fid(3)).await;

    let recorded = requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    let request = &recorded[0];
    assert_eq!(request.system_prompt.as_deref(), Some("system prompt"));
    assert_eq!(request.messages.len(), 4);
    assert_eq!(request.messages[0].role, MessageRole::Context);
    assert_eq!(request.messages[1].role, MessageRole::User);
    assert_eq!(request.messages[2].role, MessageRole::Assistant);
    assert_eq!(request.messages[3].role, MessageRole::Tool);
    assert_eq!(
        request.messages[2].tool_calls.as_ref().unwrap()[0].name,
        "lookup"
    );
    assert_eq!(request.messages[3].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(request.tools.len(), 1);
    assert_eq!(request.tools[0].name, "lookup");
    assert_eq!(request.temperature, Some(0.2));
    assert_eq!(request.max_tokens, Some(123));
    assert_eq!(request.reasoning.effort, Some(ReasoningEffort::Low));
    assert_eq!(
        request.extra_params.get("store"),
        Some(&serde_json::Value::Bool(true))
    );
}

#[tokio::test]
async fn clone_open_requires_read_write() {
    let fs = llmfs();
    // Clone-open allocates a Generation and must read its id back, so it requires
    // ReadWrite: both read-only and write-only opens are refused.
    for mode in [OpenMode::Read, OpenMode::Write] {
        fs.walk(
            Fid::ROOT,
            Fid(1),
            &["connections".into(), "default".into(), "clone".into()],
        )
        .await
        .unwrap();
        assert_eq!(fs.open(Fid(1), mode).await, Err(ErrorCode::NoAccess));
        fs.clunk(Fid(1)).await.ok();
    }
}

#[tokio::test]
async fn a_non_write_data_fid_clunk_does_not_reject_the_generation() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    // An observer walks `data` and clunks it without a write-open: this must not
    // commit an empty request or mark the Generation rejected.
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "open");
    // The real writer can still start it.
    commit_request(&fs, &g, Fid(4), br#"{"user":"hi"}"#)
        .await
        .unwrap();
}

#[tokio::test]
async fn only_one_of_two_concurrent_data_commits_starts_the_generation() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    // Two write-opened data fids for the same Generation, both buffered before
    // either commits.
    for fid in [Fid(2), Fid(3)] {
        fs.walk(
            Fid::ROOT,
            fid,
            &[
                "connections".into(),
                "default".into(),
                g.clone(),
                "data".into(),
            ],
        )
        .await
        .unwrap();
        fs.open(fid, OpenMode::Write).await.unwrap();
        fs.write(fid, 0, br#"{"user":"hi"}"#).await.unwrap();
    }
    // The first commit reserves the Generation; the second is refused, so only one
    // request reaches the provider.
    fs.clunk(Fid(2)).await.unwrap();
    assert_eq!(fs.clunk(Fid(3)).await, Err(ErrorCode::BadRequest));
}

#[tokio::test]
async fn reading_requires_a_read_open() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "status".into(),
        ],
    )
    .await
    .unwrap();
    // No open: read is denied.
    assert_eq!(fs.read(Fid(2), 0, 64).await, Err(ErrorCode::NoAccess));
}

#[tokio::test]
async fn writing_requires_a_write_open() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &["connections".into(), "default".into(), g, "data".into()],
    )
    .await
    .unwrap();
    // Walked but not opened for write: a write is refused.
    assert_eq!(fs.write(Fid(2), 0, b"{}").await, Err(ErrorCode::NoAccess));
}

#[tokio::test]
async fn read_open_of_a_write_only_file_is_refused() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    // `data` and `ctl` are write-only sinks: a read-intent open fails at dial time.
    for leaf in ["data", "ctl"] {
        fs.walk(
            Fid::ROOT,
            Fid(2),
            &[
                "connections".into(),
                "default".into(),
                g.clone(),
                leaf.into(),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            fs.open(Fid(2), OpenMode::Read).await,
            Err(ErrorCode::NoAccess)
        );
        fs.clunk(Fid(2)).await.ok();
    }
}

#[tokio::test]
async fn an_empty_request_is_rejected() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    // Zero bytes written: an empty request is malformed.
    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::BadRequest));
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "rejected");
}

#[tokio::test]
async fn invalid_v1_request_is_rejected_before_running() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fs = llmfs_with(RecordingProvider {
        requests: Arc::clone(&requests),
    });
    let g = clone_gen(&fs, Fid(1)).await;

    assert_eq!(
        commit_request(&fs, &g, Fid(2), br#"{"version":1,"messages":[]}"#).await,
        Err(ErrorCode::BadRequest)
    );
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "rejected");
    assert!(
        requests.lock().unwrap().is_empty(),
        "invalid v1 documents must not reach the provider"
    );

    let events =
        String::from_utf8(read_all(&fs, &["connections", "default", &g, "events"], Fid(4)).await)
            .unwrap();
    assert!(
        events.contains("\"rejected\""),
        "invalid v1 documents append a rejected event: {events:?}"
    );
}

#[tokio::test]
async fn unknown_request_fields_are_rejected() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    assert_eq!(
        commit_request(&fs, &g, Fid(2), br#"{"user":"hi","temperature":1}"#).await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_started_generation_cannot_be_restarted() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), br#"{"user":"hi"}"#)
        .await
        .unwrap();
    // A second request document into the same Generation is refused.
    assert_eq!(
        commit_request(&fs, &g, Fid(3), br#"{"user":"again"}"#).await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_connection_lists_its_generations() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "default"], Fid(2)).await).unwrap();
    assert!(listing.lines().any(|l| l == "clone"));
    assert!(
        listing.lines().any(|l| l == g),
        "connection lists gen {g}: {listing:?}"
    );
}

#[tokio::test]
async fn distinct_generations_get_distinct_status_qids() {
    let fs = llmfs();
    let a = clone_gen(&fs, Fid(1)).await;
    let b = clone_gen(&fs, Fid(2)).await;
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &["connections".into(), "default".into(), a, "status".into()],
    )
    .await
    .unwrap();
    let pa = fs.stat(Fid(3)).await.unwrap().qid.path;
    fs.walk(
        Fid::ROOT,
        Fid(4),
        &["connections".into(), "default".into(), b, "status".into()],
    )
    .await
    .unwrap();
    let pb = fs.stat(Fid(4)).await.unwrap().qid.path;
    assert_ne!(
        pa, pb,
        "distinct generations must have distinct status qids"
    );
}

#[tokio::test]
async fn status_qid_version_bumps_as_the_generation_advances() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "status".into(),
        ],
    )
    .await
    .unwrap();
    let v0 = fs.stat(Fid(2)).await.unwrap().qid.version;
    commit_request(&fs, &g, Fid(3), br#"{"user":"hi"}"#)
        .await
        .unwrap();
    drain_events(&fs, &g, Fid(4)).await;
    fs.walk(
        Fid::ROOT,
        Fid(5),
        &["connections".into(), "default".into(), g, "status".into()],
    )
    .await
    .unwrap();
    let v1 = fs.stat(Fid(5)).await.unwrap().qid.version;
    assert_ne!(v0, v1, "status qid version must change as status advances");
}

#[tokio::test]
async fn stat_reports_real_status_length() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "status".into(),
        ],
    )
    .await
    .unwrap();
    let stat_len = fs.stat(Fid(2)).await.unwrap().length;
    let status =
        String::from_utf8(read_all(&fs, &["connections", "default", &g, "status"], Fid(3)).await)
            .unwrap();
    assert_eq!(
        stat_len,
        status.len() as u64,
        "status stat length matches the readable status document"
    );
}

#[tokio::test]
async fn status_exposes_progress_tokens_and_cost() {
    let fs = llmfs_with(UsageProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), br#"{"user":"hi"}"#)
        .await
        .unwrap();
    drain_events(&fs, &g, Fid(3)).await;

    let status = status_doc_of(&fs, &g, Fid(4)).await;
    assert_eq!(status["version"], 1);
    assert_eq!(status["generation"], g);
    assert_eq!(status["connection"], "default");
    assert_eq!(status["status"], "done");
    assert_eq!(status["progress"]["terminal"], true);
    assert_eq!(status["tokens"]["available"], true);
    assert_eq!(status["tokens"]["prompt_tokens"], 11);
    assert_eq!(status["tokens"]["cached_prompt_tokens"], 3);
    assert_eq!(status["tokens"]["completion_tokens"], 7);
    assert_eq!(status["tokens"]["total_tokens"], 18);
    assert_eq!(status["tokens"]["reasoning_tokens"], 5);
    assert_eq!(status["cost"]["currency"], "USD");
    assert_eq!(status["cost"]["amount_microusd"], 0);
    assert_eq!(status["cost"]["metered"], false);

    let meter: serde_json::Value =
        serde_json::from_slice(&read_all(&fs, &["connections", "default", "meter"], Fid(5)).await)
            .unwrap();
    assert_eq!(meter["meter"]["generation_starts"], 1);
    assert_eq!(meter["meter"]["total_tokens"], 18);
    assert_eq!(meter["meter"]["total_cost_microusd"], 0);
}

#[tokio::test]
async fn terminal_generations_are_reaped_by_retention_policy() {
    let fs = llmfs();
    let mut ids = Vec::new();
    for i in 0..17 {
        let gen_id = clone_gen(&fs, Fid(10 + i * 3)).await;
        commit_request(&fs, &gen_id, Fid(11 + i * 3), br#"{"user":"hi"}"#)
            .await
            .unwrap();
        drain_events(&fs, &gen_id, Fid(12 + i * 3)).await;
        ids.push(gen_id);
    }

    let open_gen = clone_gen(&fs, Fid(1000)).await;
    let listing =
        String::from_utf8(read_all(&fs, &["connections", "default"], Fid(1001)).await).unwrap();
    assert!(
        !listing.lines().any(|line| line == ids[0]),
        "oldest terminal Generation should be reaped: {listing:?}"
    );
    assert!(
        listing.lines().any(|line| line == ids[1]),
        "recent terminal Generation should stay retained: {listing:?}"
    );
    assert!(
        listing.lines().any(|line| line == open_gen),
        "new open Generation should not be reaped: {listing:?}"
    );
    assert_eq!(
        fs.walk(
            Fid::ROOT,
            Fid(1002),
            &[
                "connections".into(),
                "default".into(),
                ids[0].clone(),
                "status".into(),
            ],
        )
        .await,
        Err(ErrorCode::NotFound),
        "reaped Generation is no longer walkable"
    );
}

#[tokio::test]
async fn a_reused_fid_is_rejected() {
    let fs = llmfs();
    fs.walk(Fid::ROOT, Fid(1), &["connections".into()])
        .await
        .unwrap();
    // Fid(1) is live; rebinding it must be refused.
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(1), &["connections".into()]).await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_repeated_clone_open_is_rejected() {
    let fs = llmfs();
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["connections".into(), "default".into(), "clone".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    // A second open of the same clone fid must not allocate a second Generation.
    assert_eq!(
        fs.open(Fid(1), OpenMode::ReadWrite).await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn ctl_accepts_a_newline_terminated_abort() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    // `echo abort > ctl` delivers "abort\n".
    fs.write(Fid(2), 0, b"abort\n").await.unwrap();
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "aborted");
}

#[tokio::test]
async fn abort_before_commit_makes_the_later_commit_fail() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    // Abort while still open.
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"abort").await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    // The aborted Generation cannot then be started by committing a request.
    assert_eq!(
        commit_request(&fs, &g, Fid(3), br#"{"user":"hi"}"#).await,
        Err(ErrorCode::BadRequest)
    );
    // And a watcher sees a terminal record rather than blocking forever.
    let events =
        String::from_utf8(read_all(&fs, &["connections", "default", &g, "events"], Fid(4)).await)
            .unwrap();
    assert!(
        events.contains("aborted"),
        "events has a terminal abort record: {events:?}"
    );
}

#[tokio::test]
async fn abort_after_done_is_rejected() {
    let fs = llmfs();
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), br#"{"user":"hi"}"#)
        .await
        .unwrap();
    drain_events(&fs, &g, Fid(3)).await; // runs to done
    fs.walk(
        Fid::ROOT,
        Fid(4),
        &["connections".into(), "default".into(), g, "ctl".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(4), OpenMode::Write).await.unwrap();
    assert_eq!(
        fs.write(Fid(4), 0, b"abort").await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn a_running_generation_can_be_aborted() {
    let fs = llmfs_with(HangingProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), br#"{"user":"hi"}"#)
        .await
        .unwrap();
    // The provider never finishes; abort settles the Generation and records it.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Write).await.unwrap();
    fs.write(Fid(3), 0, b"abort").await.unwrap();
    assert_eq!(status_of(&fs, &g, Fid(4)).await, "aborted");
}

#[tokio::test]
async fn an_early_closed_stream_ends_with_a_terminal_error() {
    let fs = llmfs_with(EarlyCloseProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), br#"{"user":"hi"}"#)
        .await
        .unwrap();
    // Tail events: a stream that closes without a finished chunk must still reach a
    // terminal record (error), not block the reader forever.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "events".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();
    let mut acc = String::new();
    let mut offset = 0u64;
    loop {
        let chunk =
            tokio::time::timeout(Duration::from_millis(500), fs.read(Fid(3), offset, 65536))
                .await
                .expect("events stalled without a terminal record")
                .unwrap();
        offset += chunk.len() as u64;
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("error") {
            break;
        }
    }
    assert!(
        acc.contains("partial"),
        "the partial text was recorded: {acc:?}"
    );
    assert_eq!(status_of(&fs, &g, Fid(5)).await, "error");
}

#[tokio::test]
async fn a_startup_failure_is_terminal() {
    let fs = llmfs_with(StartupFailProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    // generate_stream errors before a receiver: the commit fails and the
    // Generation is left in a terminal error state (not stuck open).
    assert_eq!(
        commit_request(&fs, &g, Fid(2), br#"{"user":"hi"}"#).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(status_of(&fs, &g, Fid(3)).await, "error");
}

/// A provider whose `generate_stream` startup takes a while before returning a
/// receiver, so a test can abort *during* startup.
struct SlowStartupProvider;

#[async_trait::async_trait]
impl LlmProvider for SlowStartupProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx.send(text_chunk("late", true)).await;
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "slow-startup"
    }
}

#[tokio::test]
async fn abort_during_provider_startup_cancels_it() {
    let fs = Arc::new(llmfs_with(SlowStartupProvider));
    let g = clone_gen(&fs, Fid(1)).await;
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "data".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, br#"{"user":"hi"}"#).await.unwrap();

    // Commit in a task: it parks awaiting the slow provider startup.
    let committer = fs.clone();
    let commit = tokio::spawn(async move { committer.clunk(Fid(2)).await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Abort while startup is still in flight.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "ctl".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Write).await.unwrap();
    fs.write(Fid(3), 0, b"abort").await.unwrap();

    // The commit returns promptly (the provider future is dropped), well before
    // the 5s startup would finish, and the Generation is aborted.
    let r = tokio::time::timeout(Duration::from_millis(500), commit)
        .await
        .expect("commit did not cancel on abort")
        .unwrap();
    assert_eq!(r, Ok(()));
    assert_eq!(status_of(&fs, &g, Fid(4)).await, "aborted");
}

/// Emits some text then a *finished* chunk whose finish_reason is `stream_error`
/// (an upstream failure after partial output), like the real adapters.
struct StreamErrorProvider;

#[async_trait::async_trait]
impl LlmProvider for StreamErrorProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx.send(text_chunk("partial", false)).await;
            let mut err = text_chunk("", true);
            err.text = None;
            err.finish_reason = Some("stream_error".to_string());
            let _ = tx.send(err).await;
        });
        Ok(rx)
    }
    fn provider_name(&self) -> &'static str {
        "stream-error"
    }
}

#[tokio::test]
async fn a_stream_error_finish_reason_is_terminal_error_not_done() {
    let fs = llmfs_with(StreamErrorProvider);
    let g = clone_gen(&fs, Fid(1)).await;
    commit_request(&fs, &g, Fid(2), br#"{"user":"hi"}"#)
        .await
        .unwrap();
    // Tail events to a terminal record.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &[
            "connections".into(),
            "default".into(),
            g.clone(),
            "events".into(),
        ],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();
    let mut acc = String::new();
    let mut offset = 0u64;
    loop {
        let chunk =
            tokio::time::timeout(Duration::from_millis(500), fs.read(Fid(3), offset, 65536))
                .await
                .expect("events stalled")
                .unwrap();
        offset += chunk.len() as u64;
        acc.push_str(&String::from_utf8_lossy(&chunk));
        if acc.contains("error") {
            break;
        }
    }
    assert!(
        !acc.contains("\"done\""),
        "a stream error must not record done: {acc:?}"
    );
    assert_eq!(status_of(&fs, &g, Fid(4)).await, "error");
}
