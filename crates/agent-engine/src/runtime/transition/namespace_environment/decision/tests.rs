use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use alan_ap::{ErrorCode, FileKind, FileServer, InProcessTransport, Qid, Stat};
use alan_kernel::{Access, MountFs, Namespace};
use alan_llm::{
    CalibrationKind, DecisionAnswer, DecisionOption, DecisionQuestion, DecisionRequest,
    DecisionResponse, LlmProvider, MalformedDecisionResponse,
};
use alan_llmfs::{ConnectionProfile, LlmFs};
use tokio::sync::Notify;

use super::*;

struct DecisionEvaluator {
    calls: Arc<AtomicUsize>,
    started: Arc<Notify>,
    hold: bool,
    calibration: CalibrationKind,
    mode: EvalMode,
}

#[derive(Clone, Copy)]
enum EvalMode {
    Ok,
    ProviderError,
    ProviderMalformed,
    InvalidAnswer,
}

#[async_trait::async_trait]
impl LlmProvider for DecisionEvaluator {
    fn provider_name(&self) -> &'static str {
        "typesafe"
    }
    fn supports_generation(&self) -> bool {
        false
    }
    fn supports_decisions(&self) -> bool {
        true
    }
    fn decision_calibration(&self) -> Option<CalibrationKind> {
        Some(self.calibration)
    }
    async fn decide(&mut self, request: DecisionRequest) -> Result<DecisionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        match self.mode {
            EvalMode::ProviderError => anyhow::bail!("synthetic provider outage"),
            EvalMode::ProviderMalformed => return Err(MalformedDecisionResponse.into()),
            EvalMode::InvalidAnswer => {
                return Ok(DecisionResponse {
                    model: "jev-1.13.0".into(),
                    provider: "typesafe".into(),
                    calibration: self.calibration,
                    answers: BTreeMap::from([(
                        "wrong_key".into(),
                        DecisionAnswer::Predicate {
                            yes_probability: 0.9,
                            confidence: 0.9,
                        },
                    )]),
                    usage: None,
                });
            }
            EvalMode::Ok => {}
        }
        if self.hold {
            std::future::pending::<()>().await;
        }
        let mut answers = BTreeMap::new();
        for (id, question) in &request.questions {
            match question {
                DecisionQuestion::Predicate { .. } => {
                    answers.insert(
                        id.clone(),
                        DecisionAnswer::Predicate {
                            yes_probability: 0.96,
                            confidence: 0.96,
                        },
                    );
                }
                DecisionQuestion::Choice { options, .. } => {
                    let selected = options[0].id.clone();
                    let mut probabilities = BTreeMap::new();
                    for (idx, opt) in options.iter().enumerate() {
                        probabilities.insert(opt.id.clone(), if idx == 0 { 0.94 } else { 0.03 });
                    }
                    // Normalize remainder onto first option so simplex sums to 1.0.
                    let rest: f64 = probabilities
                        .iter()
                        .filter(|(k, _)| *k != &selected)
                        .map(|(_, v)| *v)
                        .sum();
                    probabilities.insert(selected.clone(), (1.0 - rest).max(0.0));
                    answers.insert(
                        id.clone(),
                        DecisionAnswer::Choice {
                            choice: selected,
                            probabilities,
                            confidence: 0.94,
                        },
                    );
                }
                DecisionQuestion::Score { levels, .. } => {
                    let mut probabilities = BTreeMap::new();
                    for (idx, lvl) in levels.iter().enumerate() {
                        probabilities.insert(lvl.id.clone(), if idx == 0 { 1.0 } else { 0.0 });
                    }
                    answers.insert(
                        id.clone(),
                        DecisionAnswer::Score {
                            expected_score: levels[0].score,
                            probabilities,
                            confidence: 0.95,
                        },
                    );
                }
            }
        }
        Ok(DecisionResponse {
            model: "jev-1.13.0".into(),
            provider: "typesafe".into(),
            calibration: self.calibration,
            answers,
            usage: None,
        })
    }
}

fn sample_request() -> DecisionRequest {
    DecisionRequest::new(
        serde_json::json!({"tool_name": "bash", "command": "cargo test"}),
        BTreeMap::from([
            (
                "safe".into(),
                DecisionQuestion::Predicate {
                    prompt: "Is the action safe?".into(),
                },
            ),
            (
                "verdict".into(),
                DecisionQuestion::Choice {
                    prompt: "Pick a verdict".into(),
                    options: vec![
                        DecisionOption {
                            id: "allow".into(),
                            description: "allow".into(),
                        },
                        DecisionOption {
                            id: "escalate".into(),
                            description: "escalate".into(),
                        },
                        DecisionOption {
                            id: "deny".into(),
                            description: "deny".into(),
                        },
                    ],
                },
            ),
        ]),
    )
    .with_instructions("Evaluate untrusted state only.")
}

fn setup(
    hold: bool,
    mode: EvalMode,
    calibration: CalibrationKind,
) -> (NamespaceRuntimeEnvironment, Arc<AtomicUsize>, Arc<Notify>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(Notify::new());
    let fs = LlmFs::new();
    fs.register_connection_profile(
        "eval-conn",
        ConnectionProfile::new("typesafe", "jev-1.13.0", "cred-eval"),
        Box::new(DecisionEvaluator {
            calls: calls.clone(),
            started: started.clone(),
            hold,
            calibration,
            mode,
        }),
    );
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(fs)),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    (
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "eval-conn"),
        calls,
        started,
    )
}

#[tokio::test]
async fn decision_round_trip_validates_and_preserves_calibration() {
    let (env, calls, _) = setup(false, EvalMode::Ok, CalibrationKind::Calibrated);
    let cancel = CancellationToken::new();
    let response = env.decide(sample_request(), 5_000, &cancel).await.unwrap();
    assert_eq!(response.calibration, CalibrationKind::Calibrated);
    assert_eq!(response.provider, "typesafe");
    assert_eq!(response.model, "jev-1.13.0");
    assert_eq!(response.answers.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn decision_rejects_out_of_range_deadlines_before_allocation() {
    let (env, calls, _) = setup(false, EvalMode::Ok, CalibrationKind::Calibrated);
    let cancel = CancellationToken::new();
    for deadline in [0, 30_001] {
        let err = env
            .decide(sample_request(), deadline, &cancel)
            .await
            .unwrap_err();
        assert_eq!(
            err.downcast_ref::<NamespaceDecisionFailure>(),
            Some(&Malformed)
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn decision_distinguishes_malformed_unavailable_cancelled_and_timed_out() {
    let cancel = CancellationToken::new();

    // Provider error -> Unavailable
    let (env, _, _) = setup(false, EvalMode::ProviderError, CalibrationKind::Calibrated);
    let err = env
        .decide(sample_request(), 5_000, &cancel)
        .await
        .unwrap_err();
    assert_eq!(
        err.downcast_ref::<NamespaceDecisionFailure>(),
        Some(&Unavailable)
    );
    assert!(err.downcast_ref::<NamespaceDecisionUncertainty>().is_none());

    // Provider malformed -> Malformed
    let (env, _, _) = setup(
        false,
        EvalMode::ProviderMalformed,
        CalibrationKind::Calibrated,
    );
    let err = env
        .decide(sample_request(), 5_000, &cancel)
        .await
        .unwrap_err();
    assert_eq!(
        err.downcast_ref::<NamespaceDecisionFailure>(),
        Some(&Malformed)
    );
    assert!(err.downcast_ref::<NamespaceDecisionUncertainty>().is_none());

    // Invalid answer shape -> Malformed
    let (env, _, _) = setup(false, EvalMode::InvalidAnswer, CalibrationKind::Calibrated);
    let err = env
        .decide(sample_request(), 5_000, &cancel)
        .await
        .unwrap_err();
    assert_eq!(
        err.downcast_ref::<NamespaceDecisionFailure>(),
        Some(&Malformed)
    );

    // Timeout while in-flight -> TimedOut + ctl abort
    let (env, calls, _) = setup(true, EvalMode::Ok, CalibrationKind::Calibrated);
    let err = env.decide(sample_request(), 40, &cancel).await.unwrap_err();
    assert_eq!(
        err.downcast_ref::<NamespaceDecisionFailure>(),
        Some(&TimedOut)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Cancel while in-flight -> Cancelled + ctl abort
    let (env, calls, started) = setup(true, EvalMode::Ok, CalibrationKind::Calibrated);
    let inflight_cancel = CancellationToken::new();
    let task_cancel = inflight_cancel.clone();
    let op = env
        .allocate_decision(sample_request(), 5_000, &inflight_cancel)
        .await
        .unwrap();
    let op_id = op.operation_id().to_string();
    let task = tokio::spawn(async move { op.commit(&task_cancel).await });
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    inflight_cancel.cancel();
    let err = task.await.unwrap().unwrap_err();
    assert_eq!(
        err.downcast_ref::<NamespaceDecisionFailure>(),
        Some(&Cancelled)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let status = NamespaceClient::new(env.root.clone())
        .read_file(&format!("/mnt/llm/connections/eval-conn/{op_id}/status"))
        .await
        .unwrap();
    assert!(String::from_utf8(status).unwrap().contains("aborted"));
}

#[tokio::test]
async fn cancelling_after_data_write_aborts_before_cleanup_clunk() {
    struct DataWriteGate {
        fs: LlmFs,
        data_fids: std::sync::Mutex<std::collections::HashSet<Fid>>,
        buffered: Arc<Notify>,
    }

    #[async_trait::async_trait]
    impl FileServer for DataWriteGate {
        async fn walk(
            &self,
            fid: Fid,
            newfid: Fid,
            names: &[String],
        ) -> std::result::Result<Qid, ErrorCode> {
            let qid = self.fs.walk(fid, newfid, names).await?;
            if names.last().is_some_and(|n| n == "data") {
                self.data_fids.lock().unwrap().insert(newfid);
            }
            Ok(qid)
        }
        async fn open(&self, fid: Fid, mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
            self.fs.open(fid, mode).await
        }
        async fn read(
            &self,
            fid: Fid,
            offset: u64,
            count: u32,
        ) -> std::result::Result<Vec<u8>, ErrorCode> {
            self.fs.read(fid, offset, count).await
        }
        async fn write(
            &self,
            fid: Fid,
            offset: u64,
            data: &[u8],
        ) -> std::result::Result<u32, ErrorCode> {
            let count = self.fs.write(fid, offset, data).await?;
            if self.data_fids.lock().unwrap().contains(&fid) {
                self.buffered.notify_one();
                std::future::pending::<()>().await;
            }
            Ok(count)
        }
        async fn stat(&self, fid: Fid) -> std::result::Result<Stat, ErrorCode> {
            self.fs.stat(fid).await
        }
        async fn create(
            &self,
            fid: Fid,
            newfid: Fid,
            name: &str,
            kind: FileKind,
        ) -> std::result::Result<Qid, ErrorCode> {
            self.fs.create(fid, newfid, name, kind).await
        }
        async fn remove(&self, fid: Fid) -> std::result::Result<(), ErrorCode> {
            self.fs.remove(fid).await
        }
        async fn clunk(&self, fid: Fid) -> std::result::Result<(), ErrorCode> {
            self.data_fids.lock().unwrap().remove(&fid);
            self.fs.clunk(fid).await
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let fs = LlmFs::new();
    fs.register_connection_profile(
        "eval-conn",
        ConnectionProfile::new("typesafe", "jev-1.13.0", "cred-eval"),
        Box::new(DecisionEvaluator {
            calls: calls.clone(),
            started: Arc::new(Notify::new()),
            hold: false,
            calibration: CalibrationKind::Calibrated,
            mode: EvalMode::Ok,
        }),
    );
    let buffered = Arc::new(Notify::new());
    let gate = Arc::new(DataWriteGate {
        fs,
        data_fids: Default::default(),
        buffered: buffered.clone(),
    });
    let mut ns = Namespace::new();
    ns.mount("/mnt/llm", InProcessTransport::new(gate), Access::ReadWrite);
    let env = NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(MountFs::new(ns))),
        "/agent/1",
        "eval-conn",
    );
    let cancel = CancellationToken::new();
    let op = env
        .allocate_decision(sample_request(), 5_000, &cancel)
        .await
        .unwrap();
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move { op.commit(&task_cancel).await });
    tokio::time::timeout(Duration::from_secs(2), buffered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let err = task.await.unwrap().unwrap_err();
    assert_eq!(
        err.downcast_ref::<NamespaceDecisionFailure>(),
        Some(&Cancelled)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
