//! Public first-class typed Decision (`decision.v1`) file-surface contracts.

use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_llm::{
    CalibrationKind, DecisionAnswer, DecisionRequest, DecisionResponse, FallbackDecisionEvaluator,
    GenerationRequest, GenerationResponse, LlmProvider, MalformedDecisionResponse, TokenUsage,
    mock::MockLlmProvider,
};
use alan_llmfs::{ConnectionLimits, ConnectionProfile, LlmFs};
use serde_json::{Value, json};

enum DecisionOutcome {
    Ok(DecisionResponse),
    Malformed,
    TransportError(String),
}

struct TestDecider {
    calls: Arc<AtomicUsize>,
    recorded: Arc<Mutex<Vec<DecisionRequest>>>,
    calibration: CalibrationKind,
    outcome: Arc<Mutex<Option<DecisionOutcome>>>,
    delay: Duration,
    supports_generation: bool,
}

impl TestDecider {
    fn new(
        calibration: CalibrationKind,
        outcome: DecisionOutcome,
    ) -> (Self, Arc<AtomicUsize>, Arc<Mutex<Vec<DecisionRequest>>>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                calls: Arc::clone(&calls),
                recorded: Arc::clone(&recorded),
                calibration,
                outcome: Arc::new(Mutex::new(Some(outcome))),
                delay: Duration::ZERO,
                supports_generation: true,
            },
            calls,
            recorded,
        )
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    fn without_generation(mut self) -> Self {
        self.supports_generation = false;
        self
    }
}

#[async_trait::async_trait]
impl LlmProvider for TestDecider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        panic!("decision operation must never call generate")
    }

    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        panic!("decision operation must never call chat")
    }

    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        panic!("decision operation must never call generate_stream")
    }

    fn provider_name(&self) -> &'static str {
        "test-decider"
    }

    fn supports_generation(&self) -> bool {
        self.supports_generation
    }

    fn supports_decisions(&self) -> bool {
        true
    }

    fn decision_calibration(&self) -> Option<CalibrationKind> {
        Some(self.calibration)
    }

    async fn decide(&mut self, request: DecisionRequest) -> anyhow::Result<DecisionResponse> {
        request.validate()?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.recorded.lock().unwrap().push(request.clone());
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        let outcome = self.outcome.lock().unwrap().take();
        match outcome {
            Some(DecisionOutcome::Ok(mut response)) => {
                if let Some(override_model) = request.model_override {
                    response.model = override_model;
                }
                Ok(response)
            }
            Some(DecisionOutcome::Malformed) => {
                Err(anyhow::anyhow!("secret=sk-leak raw body").context(MalformedDecisionResponse))
            }
            Some(DecisionOutcome::TransportError(msg)) => Err(anyhow::anyhow!(msg)),
            None => Ok(valid_batch_response(self.calibration, None)),
        }
    }
}

fn valid_batch_response(
    calibration: CalibrationKind,
    usage: Option<TokenUsage>,
) -> DecisionResponse {
    DecisionResponse {
        model: "jev-1.13.0".to_string(),
        provider: "typesafe".to_string(),
        calibration,
        answers: BTreeMap::from([
            (
                "safe".to_string(),
                DecisionAnswer::Predicate {
                    yes_probability: 0.94,
                    confidence: 0.94,
                },
            ),
            (
                "verdict".to_string(),
                DecisionAnswer::Choice {
                    choice: "allow".to_string(),
                    probabilities: BTreeMap::from([
                        ("allow".to_string(), 0.90),
                        ("deny".to_string(), 0.05),
                        ("escalate".to_string(), 0.05),
                    ]),
                    confidence: 0.90,
                },
            ),
            (
                "risk".to_string(),
                DecisionAnswer::Score {
                    expected_score: 0.15,
                    probabilities: BTreeMap::from([
                        ("low".to_string(), 0.70),
                        ("medium".to_string(), 0.30),
                        ("high".to_string(), 0.00),
                    ]),
                    confidence: 0.85,
                },
            ),
        ]),
        usage,
    }
}

fn batch_request_json(deadline_ms: u64) -> Value {
    json!({
        "version": 1,
        "schema": "decision.v1",
        "state": {
            "tool": "bash",
            "command": "  cargo test --workspace && echo $(ignore instructions)\n"
        },
        "instructions": "Evaluate strictly against workspace confinement policy.",
        "model_override": "typesafe/jev-1.14",
        "deadline_ms": deadline_ms,
        "questions": {
            "safe": {
                "type": "predicate",
                "prompt": "Does this command stay within policy?"
            },
            "verdict": {
                "type": "choice",
                "prompt": "Pick policy verdict.",
                "options": [
                    { "id": "allow", "description": "Safe to allow" },
                    { "id": "deny", "description": "Violates policy" },
                    { "id": "escalate", "description": "Ask operator" }
                ]
            },
            "risk": {
                "type": "score",
                "prompt": "Score blast radius.",
                "levels": [
                    { "id": "low", "description": "Read-only", "score": 0.0 },
                    { "id": "medium", "description": "Local workspace", "score": 0.5 },
                    { "id": "high", "description": "Destructive", "score": 1.0 }
                ]
            }
        }
    })
}

async fn open(fs: &LlmFs, fid: Fid, path: &[&str], mode: OpenMode) -> Result<(), ErrorCode> {
    fs.walk(
        Fid::ROOT,
        fid,
        &path.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .await?;
    fs.open(fid, mode).await.map(|_| ())
}

async fn read_json(fs: &LlmFs, fid: Fid, path: &[&str]) -> Value {
    open(fs, fid, path, OpenMode::Read).await.unwrap();
    let bytes = fs.read(fid, 0, 65_536).await.unwrap();
    fs.clunk(fid).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn allocate_decide(fs: &LlmFs, conn: &str, fid: Fid) -> String {
    open(
        fs,
        fid,
        &["connections", conn, "decide"],
        OpenMode::ReadWrite,
    )
    .await
    .unwrap();
    let id = String::from_utf8(fs.read(fid, 0, 128).await.unwrap()).unwrap();
    fs.clunk(fid).await.unwrap();
    id
}

async fn submit_data(
    fs: &LlmFs,
    conn: &str,
    id: &str,
    fid: Fid,
    body: &[u8],
) -> Result<(), ErrorCode> {
    open(fs, fid, &["connections", conn, id, "data"], OpenMode::Write).await?;
    fs.write(fid, 0, body).await?;
    fs.clunk(fid).await
}

async fn read_terminal_event(fs: &LlmFs, conn: &str, id: &str, fid: Fid) -> Value {
    open(
        fs,
        fid,
        &["connections", conn, id, "events"],
        OpenMode::Read,
    )
    .await
    .unwrap();
    let data = tokio::time::timeout(Duration::from_secs(1), fs.read(fid, 0, 65_536))
        .await
        .unwrap()
        .unwrap();
    fs.clunk(fid).await.unwrap();
    serde_json::from_slice(&data).unwrap()
}

#[tokio::test]
async fn decide_succeeds_for_predicate_choice_score_preserves_state_and_model_override() {
    let usage = TokenUsage {
        prompt_tokens: 120,
        completion_tokens: 0,
        total_tokens: 120,
        cost_usd: Some(0.00024),
        ..Default::default()
    };
    let (decider, calls, recorded) = TestDecider::new(
        CalibrationKind::Calibrated,
        DecisionOutcome::Ok(valid_batch_response(
            CalibrationKind::Calibrated,
            Some(usage),
        )),
    );
    let fs = LlmFs::new();
    fs.register_connection_profile(
        "decider",
        ConnectionProfile::new("openrouter", "typesafe/jev-1.13", "cred-ref"),
        Box::new(decider),
    );

    // Listing exposes `decide` alongside `clone` and `evaluate`.
    open(&fs, Fid(1), &["connections", "decider"], OpenMode::Read)
        .await
        .unwrap();
    let listing = String::from_utf8(fs.read(Fid(1), 0, 4096).await.unwrap()).unwrap();
    fs.clunk(Fid(1)).await.unwrap();
    assert!(listing.lines().any(|line| line == "decide"));

    // Read-only and write-only opens of `decide` are refused.
    assert_eq!(
        open(
            &fs,
            Fid(2),
            &["connections", "decider", "decide"],
            OpenMode::Read
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(2)).await.unwrap();
    assert_eq!(
        open(
            &fs,
            Fid(3),
            &["connections", "decider", "decide"],
            OpenMode::Write
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(3)).await.unwrap();

    let id = allocate_decide(&fs, "decider", Fid(4)).await;

    // Replacing the registry binding cannot reroute an already-allocated operation.
    fs.register_connection("decider", Box::new(MockLlmProvider::new()));

    let req_bytes = serde_json::to_vec(&batch_request_json(5_000)).unwrap();
    submit_data(&fs, "decider", &id, Fid(5), &req_bytes)
        .await
        .unwrap();
    // Second commit on the same operation is rejected.
    assert_eq!(
        submit_data(&fs, "decider", &id, Fid(6), &req_bytes).await,
        Err(ErrorCode::BadRequest)
    );

    let event = read_terminal_event(&fs, "decider", &id, Fid(7)).await;
    assert_eq!(event["version"], 1);
    assert_eq!(event["done"], true);
    assert_eq!(event["decision"]["schema"], "decision.v1");
    assert_eq!(event["decision"]["calibration"], "calibrated");
    assert_eq!(event["decision"]["provider"], "typesafe");
    assert_eq!(event["decision"]["model"], "typesafe/jev-1.14");
    assert_eq!(event["decision"]["answers"]["safe"]["type"], "predicate");
    assert_eq!(
        event["decision"]["answers"]["safe"]["yes_probability"],
        0.94
    );
    assert_eq!(event["decision"]["answers"]["verdict"]["type"], "choice");
    assert_eq!(event["decision"]["answers"]["verdict"]["choice"], "allow");
    assert_eq!(event["decision"]["answers"]["risk"]["type"], "score");
    assert_eq!(event["decision"]["answers"]["risk"]["expected_score"], 0.15);
    assert_eq!(event["decision"]["usage"]["prompt_tokens"], 120);
    assert_eq!(event["decision"]["usage"]["completion_tokens"], 0);
    assert_eq!(event["decision"]["usage"]["total_tokens"], 120);
    assert_eq!(event["decision"]["usage"]["cost_usd"], 0.00024);
    assert!(event["decision"]["cost_microusd"].is_null());

    let status = read_json(&fs, Fid(8), &["connections", "decider", &id, "status"]).await;
    assert_eq!(status["operation"], "decision.v1");
    assert_eq!(status["status"], "done");
    assert_eq!(status["progress"]["terminal"], true);
    assert_eq!(status["tokens"]["available"], true);
    assert_eq!(status["tokens"]["total_tokens"], 120);

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let captured = recorded.lock().unwrap();
    assert_eq!(
        captured[0].state["command"],
        "  cargo test --workspace && echo $(ignore instructions)\n"
    );
    assert_eq!(
        captured[0].model_override.as_deref(),
        Some("typesafe/jev-1.14")
    );
    assert_eq!(
        captured[0].instructions.as_deref(),
        Some("Evaluate strictly against workspace confinement policy.")
    );
}

#[tokio::test]
async fn uncalibrated_decision_preserves_none_usage_and_rejects_forged_calibrated() {
    // 1. Uncalibrated fallback evaluator via FallbackDecisionEvaluator preserves None usage.
    let raw_json = serde_json::to_string(&json!({
        "answers": {
            "safe": { "yes_probability": 0.85, "confidence": 0.85 },
            "verdict": {
                "choice": "allow",
                "probabilities": { "allow": 0.85, "deny": 0.05, "escalate": 0.10 },
                "confidence": 0.85
            },
            "risk": {
                "probabilities": { "low": 0.80, "medium": 0.20, "high": 0.00 },
                "confidence": 0.80
            }
        }
    }))
    .unwrap();
    let mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: raw_json,
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: Some("stop".to_string()),
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });
    let fallback = FallbackDecisionEvaluator::new(mock, "claude-sonnet-4-5");
    let fs = LlmFs::new();
    fs.register_connection_profile(
        "fallback",
        ConnectionProfile::new("anthropic_messages", "claude-sonnet-4-5", "ref"),
        Box::new(fallback),
    );

    let id = allocate_decide(&fs, "fallback", Fid(1)).await;
    let body = serde_json::to_vec(&batch_request_json(5_000)).unwrap();
    submit_data(&fs, "fallback", &id, Fid(2), &body)
        .await
        .unwrap();
    let event = read_terminal_event(&fs, "fallback", &id, Fid(3)).await;
    assert_eq!(event["done"], true);
    assert_eq!(event["decision"]["calibration"], "uncalibrated");
    assert!(
        event["decision"]["usage"].is_null(),
        "missing usage must stay null, never invented zero"
    );
    let status = read_json(&fs, Fid(4), &["connections", "fallback", &id, "status"]).await;
    assert_eq!(status["tokens"]["available"], false);
    let meter = read_json(&fs, Fid(5), &["connections", "fallback", "meter"]).await;
    assert_eq!(meter["meter"]["total_tokens"], 0);
    assert_eq!(meter["meter"]["decision_starts"], 1);

    // 2. An Uncalibrated connection cannot forge CalibrationKind::Calibrated.
    let (forging, _, _) = TestDecider::new(
        CalibrationKind::Uncalibrated,
        DecisionOutcome::Ok(valid_batch_response(CalibrationKind::Calibrated, None)),
    );
    fs.register_connection("forger", Box::new(forging));
    let id = allocate_decide(&fs, "forger", Fid(10)).await;
    submit_data(&fs, "forger", &id, Fid(11), &body)
        .await
        .unwrap();
    let event = read_terminal_event(&fs, "forger", &id, Fid(12)).await;
    assert_eq!(event["error"], "invalid_decision_answer");
}

#[tokio::test]
async fn unsupported_connection_rejects_decide_at_dial_time_without_fallback() {
    let fs = LlmFs::new();
    fs.register_connection("gen-only", Box::new(MockLlmProvider::new()));

    assert!(!fs.supports_decisions("gen-only"));
    assert_eq!(fs.decision_calibration("gen-only"), None);
    assert_eq!(
        open(
            &fs,
            Fid(1),
            &["connections", "gen-only", "decide"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::Unsupported)
    );
    let meter = read_json(&fs, Fid(2), &["connections", "gen-only", "meter"]).await;
    assert_eq!(meter["meter"]["decision_starts"], 0);
    assert_eq!(meter["meter"]["generation_starts"], 0);
}

#[tokio::test]
async fn malformed_requests_provider_errors_timeouts_and_aborts_behave_deterministically() {
    // 1. Malformed request documents are rejected at commit time without invoking provider.
    let (decider, calls, _) = TestDecider::new(
        CalibrationKind::Calibrated,
        DecisionOutcome::Ok(valid_batch_response(CalibrationKind::Calibrated, None)),
    );
    let fs = LlmFs::new();
    fs.register_connection("test", Box::new(decider));

    let bad_bodies: Vec<Vec<u8>> = vec![
        b"{}".to_vec(),
        {
            let mut v = batch_request_json(1_000);
            v["version"] = json!(2);
            serde_json::to_vec(&v).unwrap()
        },
        {
            let mut v = batch_request_json(1_000);
            v["schema"] = json!("choice.v1");
            serde_json::to_vec(&v).unwrap()
        },
        {
            let mut v = batch_request_json(1_000);
            v["tools"] = json!([]);
            serde_json::to_vec(&v).unwrap()
        },
        serde_json::to_vec(&batch_request_json(0)).unwrap(),
        serde_json::to_vec(&batch_request_json(30_001)).unwrap(),
        {
            let mut v = batch_request_json(1_000);
            v["questions"]["risk"]["levels"] = json!([]);
            serde_json::to_vec(&v).unwrap()
        },
        {
            let mut v = batch_request_json(1_000);
            v["questions"]["risk"]["levels"] = json!([
                { "id": "dup", "description": "a", "score": 0.0 },
                { "id": "dup", "description": "b", "score": 1.0 }
            ]);
            serde_json::to_vec(&v).unwrap()
        },
        br#"{"version":1,"schema":"decision.v1","state":{},"deadline_ms":1000,"questions":{"q":{"type":"predicate","prompt":"a"},"q":{"type":"predicate","prompt":"b"}}}"#.to_vec(),
    ];

    for (idx, body) in bad_bodies.iter().enumerate() {
        let base = 100 + idx as u64 * 4;
        let id = allocate_decide(&fs, "test", Fid(base)).await;
        assert_eq!(
            submit_data(&fs, "test", &id, Fid(base + 1), body).await,
            Err(ErrorCode::BadRequest),
            "bad body index {idx} must be rejected"
        );
        let event = read_terminal_event(&fs, "test", &id, Fid(base + 2)).await;
        assert_eq!(event["rejected"], true);
        let status = read_json(&fs, Fid(base + 3), &["connections", "test", &id, "status"]).await;
        assert_eq!(status["status"], "rejected");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    // 2. Oversize (> 1 MiB) request fails on write before commit.
    let oversize_id = allocate_decide(&fs, "test", Fid(200)).await;
    open(
        &fs,
        Fid(201),
        &["connections", "test", &oversize_id, "data"],
        OpenMode::Write,
    )
    .await
    .unwrap();
    let big = vec![b'a'; (1 << 20) + 1];
    assert_eq!(
        fs.write(Fid(201), 0, &big).await,
        Err(ErrorCode::BadRequest)
    );
    fs.clunk(Fid(201)).await.unwrap_err();

    // 3. MalformedDecisionResponse maps to `malformed_decision_response` without secret leakage.
    let (malformed, _, _) =
        TestDecider::new(CalibrationKind::Calibrated, DecisionOutcome::Malformed);
    fs.register_connection("malformed", Box::new(malformed));
    let id = allocate_decide(&fs, "malformed", Fid(300)).await;
    let valid_body = serde_json::to_vec(&batch_request_json(1_000)).unwrap();
    submit_data(&fs, "malformed", &id, Fid(301), &valid_body)
        .await
        .unwrap();
    let event = read_terminal_event(&fs, "malformed", &id, Fid(302)).await;
    assert_eq!(event["error"], "malformed_decision_response");
    assert!(!event.to_string().contains("sk-leak"));

    // 4. Out-of-set answer from provider maps to `invalid_decision_answer`.
    let mut bad_resp = valid_batch_response(CalibrationKind::Calibrated, None);
    bad_resp.answers.insert(
        "verdict".to_string(),
        DecisionAnswer::Choice {
            choice: "sudo".to_string(),
            probabilities: BTreeMap::from([("sudo".to_string(), 1.0)]),
            confidence: 1.0,
        },
    );
    let (invalid, _, _) =
        TestDecider::new(CalibrationKind::Calibrated, DecisionOutcome::Ok(bad_resp));
    fs.register_connection("invalid", Box::new(invalid));
    let id = allocate_decide(&fs, "invalid", Fid(310)).await;
    submit_data(&fs, "invalid", &id, Fid(311), &valid_body)
        .await
        .unwrap();
    let event = read_terminal_event(&fs, "invalid", &id, Fid(312)).await;
    assert_eq!(event["error"], "invalid_decision_answer");

    // 5. Generic provider transport failure uses safe failure reason.
    let (err_provider, _, _) = TestDecider::new(
        CalibrationKind::Calibrated,
        DecisionOutcome::TransportError("connection refused".to_string()),
    );
    fs.register_connection("err", Box::new(err_provider));
    let id = allocate_decide(&fs, "err", Fid(320)).await;
    submit_data(&fs, "err", &id, Fid(321), &valid_body)
        .await
        .unwrap();
    let event = read_terminal_event(&fs, "err", &id, Fid(322)).await;
    assert!(event["error"].is_string());
    assert_ne!(event["error"], "malformed_decision_response");

    // 6. Timeout and ctl abort cover provider lock wait without extra calls.
    let (slow, slow_calls, _) = TestDecider::new(
        CalibrationKind::Calibrated,
        DecisionOutcome::Ok(valid_batch_response(CalibrationKind::Calibrated, None)),
    );
    fs.register_connection("slow", Box::new(slow.with_delay(Duration::from_secs(5))));
    let first = allocate_decide(&fs, "slow", Fid(400)).await;
    submit_data(
        &fs,
        "slow",
        &first,
        Fid(401),
        &serde_json::to_vec(&batch_request_json(10_000)).unwrap(),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while slow_calls.load(Ordering::SeqCst) != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let aborted = allocate_decide(&fs, "slow", Fid(402)).await;
    submit_data(
        &fs,
        "slow",
        &aborted,
        Fid(403),
        &serde_json::to_vec(&batch_request_json(10_000)).unwrap(),
    )
    .await
    .unwrap();
    let timed_out = allocate_decide(&fs, "slow", Fid(404)).await;
    submit_data(
        &fs,
        "slow",
        &timed_out,
        Fid(405),
        &serde_json::to_vec(&batch_request_json(5)).unwrap(),
    )
    .await
    .unwrap();

    open(
        &fs,
        Fid(406),
        &["connections", "slow", &aborted, "ctl"],
        OpenMode::Write,
    )
    .await
    .unwrap();
    fs.write(Fid(406), 0, b"abort\n").await.unwrap();
    fs.clunk(Fid(406)).await.unwrap();

    assert_eq!(
        read_terminal_event(&fs, "slow", &aborted, Fid(407)).await["aborted"],
        true
    );
    assert_eq!(
        read_terminal_event(&fs, "slow", &timed_out, Fid(408)).await["error"],
        "decision_timeout"
    );

    open(
        &fs,
        Fid(409),
        &["connections", "slow", &first, "ctl"],
        OpenMode::Write,
    )
    .await
    .unwrap();
    fs.write(Fid(409), 0, b"abort").await.unwrap();
    fs.clunk(Fid(409)).await.unwrap();
    assert_eq!(
        read_terminal_event(&fs, "slow", &first, Fid(410)).await["aborted"],
        true
    );
    assert_eq!(slow_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn capabilities_advertise_decisions_and_calibrated_decisions_accurately() {
    let fs = LlmFs::new();

    // Provider-level introspection across native calibrated vs generation-only providers.
    for provider in [
        "magpie",
        "google_gemini_generate_content",
        "openai_responses",
        "openrouter",
        "typesafe",
    ] {
        let doc = read_json(&fs, Fid(1), &["providers", provider, "capabilities"]).await;
        assert_eq!(
            doc["decisions"]["v1"], true,
            "{provider} must support decisions.v1"
        );
        assert_eq!(
            doc["decisions"]["calibration"], "calibrated",
            "{provider} must report calibrated"
        );
        assert_eq!(
            doc["calibrated_decisions"], true,
            "{provider} must advertise calibrated_decisions"
        );
        assert_eq!(doc["evaluation"]["choice_v1"], true);
        assert_eq!(doc["generation"], provider != "typesafe");
    }

    for provider in [
        "anthropic_messages",
        "chatgpt",
        "openai_chat_completions",
        "openai_chat_completions_compatible",
    ] {
        let doc = read_json(&fs, Fid(2), &["providers", provider, "capabilities"]).await;
        assert_eq!(
            doc["decisions"]["v1"], false,
            "{provider} default must not claim decisions.v1"
        );
        assert!(doc["decisions"]["calibration"].is_null());
        assert_eq!(doc["calibrated_decisions"], false);
        assert_eq!(doc["generation"], true);
    }

    // Connection-level introspection across calibrated, uncalibrated, and unsupported callables.
    let (calibrated_only, _, _) = TestDecider::new(
        CalibrationKind::Calibrated,
        DecisionOutcome::Ok(valid_batch_response(CalibrationKind::Calibrated, None)),
    );
    fs.register_connection_profile(
        "calibrated-eval-only",
        ConnectionProfile::new("typesafe", "jev-1.13.0", "ref"),
        Box::new(calibrated_only.without_generation()),
    );
    let uncalibrated = FallbackDecisionEvaluator::new(MockLlmProvider::new(), "claude-sonnet-4-5");
    fs.register_connection_profile(
        "uncalibrated-fallback",
        ConnectionProfile::new("anthropic_messages", "claude-sonnet-4-5", "ref"),
        Box::new(uncalibrated),
    );
    fs.register_connection("unsupported", Box::new(MockLlmProvider::new()));

    let cal_doc = read_json(
        &fs,
        Fid(10),
        &["connections", "calibrated-eval-only", "capabilities"],
    )
    .await;
    assert_eq!(cal_doc["decisions"]["v1"], true);
    assert_eq!(cal_doc["decisions"]["calibration"], "calibrated");
    assert_eq!(cal_doc["calibrated_decisions"], true);
    assert_eq!(cal_doc["evaluation"]["choice_v1"], true);
    assert_eq!(cal_doc["generation"], false);
    assert!(cal_doc["capabilities"].is_null());
    assert!(fs.supports_decisions("calibrated-eval-only"));
    assert_eq!(
        fs.decision_calibration("calibrated-eval-only"),
        Some(CalibrationKind::Calibrated)
    );

    let uncal_doc = read_json(
        &fs,
        Fid(11),
        &["connections", "uncalibrated-fallback", "capabilities"],
    )
    .await;
    assert_eq!(uncal_doc["decisions"]["v1"], true);
    assert_eq!(uncal_doc["decisions"]["calibration"], "uncalibrated");
    assert_eq!(uncal_doc["calibrated_decisions"], false);
    assert_eq!(uncal_doc["evaluation"]["choice_v1"], true);
    assert_eq!(uncal_doc["generation"], true);
    assert!(fs.supports_decisions("uncalibrated-fallback"));
    assert_eq!(
        fs.decision_calibration("uncalibrated-fallback"),
        Some(CalibrationKind::Uncalibrated)
    );

    let unsup_doc = read_json(
        &fs,
        Fid(12),
        &["connections", "unsupported", "capabilities"],
    )
    .await;
    assert_eq!(unsup_doc["decisions"]["v1"], false);
    assert!(unsup_doc["decisions"]["calibration"].is_null());
    assert_eq!(unsup_doc["calibrated_decisions"], false);
    assert_eq!(unsup_doc["evaluation"]["choice_v1"], false);
    assert_eq!(unsup_doc["generation"], true);
    assert!(!fs.supports_decisions("unsupported"));
    assert_eq!(fs.decision_calibration("unsupported"), None);
}

#[tokio::test]
async fn decision_limits_and_meter_are_independent_of_generations_and_evaluations() {
    let (decider, _, _) = TestDecider::new(
        CalibrationKind::Calibrated,
        DecisionOutcome::Ok(valid_batch_response(CalibrationKind::Calibrated, None)),
    );
    let fs = LlmFs::new();
    fs.register_connection_profile_with_limits(
        "bounded",
        ConnectionProfile::new("openrouter", "typesafe/jev-1.13", "ref"),
        ConnectionLimits {
            max_generations: Some(1),
            max_evaluations: Some(1),
            max_decisions: Some(2),
        },
        Box::new(decider),
    );

    // Exhaust the 2-decision budget.
    let _d1 = allocate_decide(&fs, "bounded", Fid(1)).await;
    let _d2 = allocate_decide(&fs, "bounded", Fid(2)).await;
    assert_eq!(
        open(
            &fs,
            Fid(3),
            &["connections", "bounded", "decide"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(3)).await.unwrap();

    let meter = read_json(&fs, Fid(4), &["connections", "bounded", "meter"]).await;
    assert_eq!(meter["limits"]["max_generations"], 1);
    assert_eq!(meter["limits"]["max_evaluations"], 1);
    assert_eq!(meter["limits"]["max_decisions"], 2);
    assert_eq!(meter["meter"]["generation_starts"], 0);
    assert_eq!(meter["meter"]["evaluation_starts"], 0);
    assert_eq!(meter["meter"]["decision_starts"], 2);

    // Generation and evaluation budgets remain completely unaffected.
    open(
        &fs,
        Fid(5),
        &["connections", "bounded", "clone"],
        OpenMode::ReadWrite,
    )
    .await
    .unwrap();
    fs.clunk(Fid(5)).await.unwrap();
    assert_eq!(
        open(
            &fs,
            Fid(6),
            &["connections", "bounded", "clone"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(6)).await.unwrap();

    open(
        &fs,
        Fid(7),
        &["connections", "bounded", "evaluate"],
        OpenMode::ReadWrite,
    )
    .await
    .unwrap();
    fs.clunk(Fid(7)).await.unwrap();
    assert_eq!(
        open(
            &fs,
            Fid(8),
            &["connections", "bounded", "evaluate"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(8)).await.unwrap();

    let meter = read_json(&fs, Fid(9), &["connections", "bounded", "meter"]).await;
    assert_eq!(meter["meter"]["generation_starts"], 1);
    assert_eq!(meter["meter"]["evaluation_starts"], 1);
    assert_eq!(meter["meter"]["decision_starts"], 2);

    // Convenience constructor ConnectionLimits::max_decisions(1) also works.
    let (decider2, _, _) = TestDecider::new(
        CalibrationKind::Calibrated,
        DecisionOutcome::Ok(valid_batch_response(CalibrationKind::Calibrated, None)),
    );
    fs.register_connection_profile_with_limits(
        "decide-only-limit",
        ConnectionProfile::new("openrouter", "typesafe/jev-1.13", "ref"),
        ConnectionLimits::max_decisions(1),
        Box::new(decider2),
    );
    let _ = allocate_decide(&fs, "decide-only-limit", Fid(10)).await;
    assert_eq!(
        open(
            &fs,
            Fid(11),
            &["connections", "decide-only-limit", "decide"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    fs.clunk(Fid(11)).await.unwrap();
}
