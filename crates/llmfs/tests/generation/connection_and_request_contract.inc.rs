use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_llm::mock::MockLlmProvider;
use alan_llmfs::{ConnectionLimits, ConnectionProfile, LlmFs};

const SIMPLE_REQUEST: &[u8] = br#"{"version":2,"messages":[{"role":"user","content":"hi"}]}"#;
const AGAIN_REQUEST: &[u8] = br#"{"version":2,"messages":[{"role":"user","content":"again"}]}"#;

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

#[tokio::test]
async fn connection_view_exposes_only_the_selected_live_connection() {
    let fs = LlmFs::new();
    fs.register_connection("selected", Box::new(MockLlmProvider::new()));
    fs.register_connection("withheld", Box::new(MockLlmProvider::new()));
    let view = fs.connection_view("selected");

    assert_eq!(
        String::from_utf8(read_all(&view, &["connections"], Fid(900)).await).unwrap(),
        "selected"
    );
    assert_eq!(
        view.walk(
            Fid::ROOT,
            Fid(901),
            &["connections".into(), "withheld".into()]
        )
        .await,
        Err(ErrorCode::NotFound)
    );
    read_all(&fs, &["connections", "withheld", "provider"], Fid(903)).await;
    assert_eq!(
        view.read(Fid(903), 0, 65536).await,
        Err(ErrorCode::NotFound),
        "a narrowed view must not inherit a fid opened through another view"
    );
    view.walk(
        Fid::ROOT,
        Fid(903),
        &["connections".into(), "selected".into(), "provider".into()],
    )
    .await
    .expect("views must have independent fid namespaces");
    let narrowed_again = view.connection_view("withheld");
    assert_eq!(
        read_all(&narrowed_again, &["connections"], Fid(904)).await,
        b"",
        "deriving a view must not widen its parent's capability"
    );

    fs.unregister_connection("selected").await;
    assert_eq!(read_all(&view, &["connections"], Fid(902)).await, b"");
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

    fs.unregister_connection("work").await;
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
    fs.write(Fid(2), 0, SIMPLE_REQUEST).await.unwrap();
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
            "version": 2,
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
    commit_request(&fs, &g, Fid(4), SIMPLE_REQUEST)
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
        fs.write(fid, 0, SIMPLE_REQUEST).await.unwrap();
    }
    // The first commit reserves the Generation; the second is refused, so only one
    // request reaches the provider.
    fs.clunk(Fid(2)).await.unwrap();
    assert_eq!(fs.clunk(Fid(3)).await, Err(ErrorCode::BadRequest));
}
