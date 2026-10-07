//! Public finite-choice operation contracts; mock results do not qualify a real adapter.
use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_llm::{
    ChoiceEvaluationRequest, ChoiceEvaluationResponse, EvaluationSelection, GenerationRequest,
    GenerationResponse, LlmProvider,
};
use alan_llmfs::{ConnectionLimits, ConnectionProfile, LlmFs};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

struct Evaluator {
    calls: Arc<AtomicUsize>,
    selection: EvaluationSelection,
    delay: Duration,
}
#[async_trait::async_trait]
impl LlmProvider for Evaluator {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        panic!("evaluation must never call generation")
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        panic!("evaluation must never call chat")
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        panic!("evaluation must never stream generation")
    }
    fn provider_name(&self) -> &'static str {
        "test"
    }
    fn supports_choice_evaluation(&self) -> bool {
        true
    }
    async fn evaluate_choice(
        &mut self,
        request: ChoiceEvaluationRequest,
    ) -> anyhow::Result<ChoiceEvaluationResponse> {
        request.validate()?;
        assert_eq!(request.input, "  original input\n");
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        Ok(ChoiceEvaluationResponse {
            selection: self.selection.clone(),
            usage: None,
        })
    }
}
fn setup(
    selection: EvaluationSelection,
    delay: Duration,
    limit: Option<u64>,
) -> (LlmFs, Arc<AtomicUsize>) {
    let fs = LlmFs::new();
    let calls = Arc::new(AtomicUsize::new(0));
    fs.register_connection_profile_with_limits(
        "test",
        ConnectionProfile::new("test", "captured-model", "ref"),
        ConnectionLimits {
            max_generations: Some(0),
            max_evaluations: limit,
        },
        Box::new(Evaluator {
            calls: calls.clone(),
            selection,
            delay,
        }),
    );
    (fs, calls)
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
async fn allocate(fs: &LlmFs, fid: Fid) -> String {
    open(
        fs,
        fid,
        &["connections", "test", "evaluate"],
        OpenMode::ReadWrite,
    )
    .await
    .unwrap();
    String::from_utf8(fs.read(fid, 0, 100).await.unwrap()).unwrap()
}
fn request(deadline: u64) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"version":1,"schema":"choice.v1",
        "input":"  original input\n","candidates":[{"id":"agent","description":"advice"}],
        "deadline_ms":deadline}))
    .unwrap()
}
async fn submit(fs: &LlmFs, id: &str, fid: Fid, body: &[u8]) -> Result<(), ErrorCode> {
    open(
        fs,
        fid,
        &["connections", "test", id, "data"],
        OpenMode::Write,
    )
    .await?;
    fs.write(fid, 0, body).await?;
    fs.clunk(fid).await
}
async fn terminal(fs: &LlmFs, id: &str, fid: Fid) -> serde_json::Value {
    open(
        fs,
        fid,
        &["connections", "test", id, "events"],
        OpenMode::Read,
    )
    .await
    .unwrap();
    let data = tokio::time::timeout(Duration::from_secs(1), fs.read(fid, 0, 65536))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_slice(&data).unwrap()
}

#[tokio::test]
async fn selection_is_typed_bound_once_and_separately_budgeted() {
    let (fs, calls) = setup(
        EvaluationSelection::Selected("agent".into()),
        Duration::ZERO,
        Some(1),
    );
    let id = allocate(&fs, Fid(1)).await;
    assert_eq!(
        open(
            &fs,
            Fid(2),
            &["connections", "test", "evaluate"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        open(
            &fs,
            Fid(3),
            &["connections", "test", "clone"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::NoAccess)
    );
    // Replacement cannot reroute an allocated operation to the new callable.
    fs.register_connection("test", Box::new(alan_llm::mock::MockLlmProvider::new()));
    submit(&fs, &id, Fid(4), &request(1000)).await.unwrap();
    assert_eq!(
        submit(&fs, &id, Fid(5), &request(1000)).await,
        Err(ErrorCode::BadRequest)
    );
    let event = terminal(&fs, &id, Fid(6)).await;
    assert_eq!(event["done"], true);
    assert_eq!(event["evaluation"]["selection"]["id"], "agent");
    assert_eq!(event["evaluation"]["model"], "captured-model");
    assert!(event["evaluation"]["cost_microusd"].is_null());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        open(
            &fs,
            Fid(7),
            &["connections", "test", "evaluate"],
            OpenMode::ReadWrite
        )
        .await,
        Err(ErrorCode::Unsupported)
    );
}

#[tokio::test]
async fn malformed_unknown_and_timeout_never_fall_back() {
    for (selection, delay, body, expected) in [
        (
            EvaluationSelection::NoMatch,
            Duration::ZERO,
            b"{}".to_vec(),
            "rejected",
        ),
        (
            EvaluationSelection::Selected("command".into()),
            Duration::ZERO,
            request(1000),
            "invalid_evaluation_selection",
        ),
        (
            EvaluationSelection::NoMatch,
            Duration::from_secs(5),
            request(5),
            "evaluation_timeout",
        ),
    ] {
        let (fs, calls) = setup(selection, delay, None);
        let id = allocate(&fs, Fid(1)).await;
        let result = submit(&fs, &id, Fid(2), &body).await;
        let event = terminal(&fs, &id, Fid(3)).await;
        if expected == "rejected" {
            assert_eq!(result, Err(ErrorCode::BadRequest));
            assert_eq!(event["rejected"], true);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        } else {
            result.unwrap();
            assert_eq!(event["error"], expected);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }
}

#[tokio::test]
async fn cancellation_and_deadline_cover_provider_lock_wait_without_extra_calls() {
    let (fs, calls) = setup(EvaluationSelection::NoMatch, Duration::from_secs(5), None);
    let first = allocate(&fs, Fid(10)).await;
    submit(&fs, &first, Fid(11), &request(10_000))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while calls.load(Ordering::SeqCst) != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let cancelled = allocate(&fs, Fid(12)).await;
    submit(&fs, &cancelled, Fid(13), &request(10_000))
        .await
        .unwrap();
    let expired = allocate(&fs, Fid(14)).await;
    submit(&fs, &expired, Fid(15), &request(5)).await.unwrap();
    open(
        &fs,
        Fid(16),
        &["connections", "test", &cancelled, "ctl"],
        OpenMode::Write,
    )
    .await
    .unwrap();
    fs.write(Fid(16), 0, b"abort").await.unwrap();
    assert_eq!(terminal(&fs, &cancelled, Fid(17)).await["aborted"], true);
    assert_eq!(
        terminal(&fs, &expired, Fid(18)).await["error"],
        "evaluation_timeout"
    );
    open(
        &fs,
        Fid(19),
        &["connections", "test", &first, "ctl"],
        OpenMode::Write,
    )
    .await
    .unwrap();
    fs.write(Fid(19), 0, b"abort").await.unwrap();
    assert_eq!(terminal(&fs, &first, Fid(20)).await["aborted"], true);
    // The lock is now free. A fresh request must be the next provider invocation.
    let last = allocate(&fs, Fid(21)).await;
    submit(&fs, &last, Fid(22), &request(5)).await.unwrap();
    assert_eq!(
        terminal(&fs, &last, Fid(23)).await["error"],
        "evaluation_timeout"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let records = fs.read(Fid(17), 0, 65536).await.unwrap();
    assert_eq!(String::from_utf8(records).unwrap().lines().count(), 1);
}

#[tokio::test]
async fn no_match_is_a_typed_success_and_request_extensions_are_rejected() {
    let (fs, calls) = setup(EvaluationSelection::NoMatch, Duration::ZERO, None);
    let id = allocate(&fs, Fid(1)).await;
    submit(&fs, &id, Fid(2), &request(1000)).await.unwrap();
    let event = terminal(&fs, &id, Fid(3)).await;
    assert_eq!(event["done"], true);
    assert_eq!(event["evaluation"]["selection"]["kind"], "no_match");
    for (index, (field, value)) in [
        ("tools", serde_json::json!([])),
        ("schema", serde_json::json!("generation")),
        ("deadline_ms", serde_json::json!(30_001)),
        (
            "candidates",
            serde_json::json!([{"id":"agent","description":"a"},{"id":"agent","description":"b"}]),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let base = 100 + index as u64 * 3;
        let id = allocate(&fs, Fid(base)).await;
        let mut body: serde_json::Value = serde_json::from_slice(&request(1000)).unwrap();
        body[field] = value;
        assert_eq!(
            submit(&fs, &id, Fid(base + 1), &serde_json::to_vec(&body).unwrap()).await,
            Err(ErrorCode::BadRequest)
        );
        assert_eq!(terminal(&fs, &id, Fid(base + 2)).await["rejected"], true);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
