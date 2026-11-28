use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use alan_agent_protocol::{Event, UiBudgetSnapshot, UiSystem1Calibration, UiSystem1Snapshot};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_llm::{
    CalibrationKind, DecisionAnswer, DecisionRequest, DecisionResponse, GenerationRequest,
    GenerationResponse, LlmProvider, StreamChunk,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::agent_machine::AgentMachine;
use crate::agent_machine::task_budget::{Admission, Dimension};
use crate::policy::TrustPosture;
use crate::runtime::task_budget::auto_extend::{
    BUDGET_Q_CONVERGING_HEALTHY, BUDGET_Q_VERDICT, BUDGET_VERDICT_ESCALATE, BUDGET_VERDICT_EXTEND,
    ProgressAssessment, ProgressDecision, evaluate_progress_assessment,
};
use crate::runtime::transition::NamespaceRuntimeEnvironment;

type CapturedSchemas = Arc<std::sync::Mutex<Vec<Option<serde_json::Value>>>>;

struct MockBudgetDecisionProvider {
    supports_decisions_flag: bool,
    responses: Mutex<Vec<anyhow::Result<DecisionResponse>>>,
    generate_responses: Mutex<Vec<anyhow::Result<String>>>,
    calls: Arc<AtomicUsize>,
    generate_calls: Arc<AtomicUsize>,
    captured_response_schemas: CapturedSchemas,
}

impl MockBudgetDecisionProvider {
    fn new(responses: Vec<anyhow::Result<DecisionResponse>>) -> (Self, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            Self {
                supports_decisions_flag: true,
                responses: Mutex::new(responses),
                generate_responses: Mutex::new(Vec::new()),
                calls: Arc::clone(&calls),
                generate_calls: Arc::new(AtomicUsize::new(0)),
                captured_response_schemas: Arc::new(std::sync::Mutex::new(Vec::new())),
            },
            calls,
        )
    }

    fn with_tier2(
        mut self,
        supports_decisions: bool,
        generate_responses: Vec<anyhow::Result<String>>,
    ) -> (Self, Arc<AtomicUsize>, CapturedSchemas) {
        self.supports_decisions_flag = supports_decisions;
        self.generate_responses = Mutex::new(generate_responses);
        let gen_calls = Arc::clone(&self.generate_calls);
        let schemas = Arc::clone(&self.captured_response_schemas);
        (self, gen_calls, schemas)
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockBudgetDecisionProvider {
    fn supports_decisions(&self) -> bool {
        self.supports_decisions_flag
    }

    fn decision_calibration(&self) -> Option<CalibrationKind> {
        self.supports_decisions_flag
            .then_some(CalibrationKind::Calibrated)
    }

    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.generate_calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            request.tools.is_empty(),
            "Tier 2 budget structured rubric must carry zero tools"
        );
        self.captured_response_schemas
            .lock()
            .unwrap()
            .push(request.response_schema.clone());
        let content = {
            let mut guard = self.generate_responses.lock().await;
            if guard.is_empty() {
                anyhow::bail!("unexpected generate() call during System 1 budget auto-extend test")
            }
            guard.remove(0)?
        };
        Ok(GenerationResponse {
            content,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        anyhow::bail!("unused")
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let resp = self.generate(request).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        let _ = tx
            .send(StreamChunk {
                text: Some(resp.content),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: None,
                provider_response_id: None,
                provider_response_status: None,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some("stop".into()),
            })
            .await;
        Ok(rx)
    }

    async fn decide(&mut self, request: DecisionRequest) -> anyhow::Result<DecisionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(request.questions.contains_key(BUDGET_Q_CONVERGING_HEALTHY));
        assert!(request.questions.contains_key(BUDGET_Q_VERDICT));
        let mut guard = self.responses.lock().await;
        if guard.is_empty() {
            anyhow::bail!("no mock decision response queued")
        }
        guard.remove(0)
    }

    fn provider_name(&self) -> &'static str {
        "magpie"
    }
}

fn budget_decision_response(
    calibration: CalibrationKind,
    converging: (f64, f64),
    choice: &str,
    extend_prob: f64,
    escalate_prob: f64,
    verdict_conf: f64,
) -> DecisionResponse {
    DecisionResponse {
        model: "magpie-small".into(),
        provider: "magpie".into(),
        calibration,
        answers: BTreeMap::from([
            (
                BUDGET_Q_CONVERGING_HEALTHY.into(),
                DecisionAnswer::Predicate {
                    yes_probability: converging.0,
                    confidence: converging.1,
                },
            ),
            (
                BUDGET_Q_VERDICT.into(),
                DecisionAnswer::Choice {
                    choice: choice.into(),
                    probabilities: BTreeMap::from([
                        (BUDGET_VERDICT_EXTEND.into(), extend_prob),
                        (BUDGET_VERDICT_ESCALATE.into(), escalate_prob),
                    ]),
                    confidence: verdict_conf,
                },
            ),
        ]),
        usage: None,
    }
}

async fn setup_smart_budget_state(
    posture: TrustPosture,
    provider: MockBudgetDecisionProvider,
) -> (RuntimeLoopState, alan_shell::Shell, String) {
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection_profile(
        "default",
        alan_llmfs::ConnectionProfile::new("magpie", "magpie-small", "cred-test"),
        Box::new(provider),
    );
    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let mount_fs = Arc::new(MountFs::new(Namespace::new()));
    let live = mount_fs.live_namespace();
    live.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    live.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(mount_fs);
    let shell = alan_shell::Shell::new(root.clone());
    let pid = shell
        .spawn(
            r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#,
        )
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    let owner = format!("/agent/{pid}");

    let mut config = crate::Config::default();
    config.task_budget.model_calls = NonZeroU64::new(1).unwrap();
    let runtime_config = crate::runtime::RuntimeConfig {
        trust_posture: posture,
        ..Default::default()
    };
    let environment = NamespaceRuntimeEnvironment::new(root, owner.clone(), "default")
        .with_trust_posture(posture);
    let mut state = RuntimeLoopState {
        machine: AgentMachine::new(),
        environment,
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };
    state.machine.accept_submission("smart-budget-task");
    state
        .machine
        .add_user_message("Refactor module and run tests");
    begin(&mut state).await.unwrap();
    state
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .reserve(Admission::Model)
        .await
        .unwrap();
    (state, shell, owner)
}

#[tokio::test]
async fn confined_smart_auto_extends_once_updates_ui_and_escalates_beyond_cap() {
    let (provider, calls) = MockBudgetDecisionProvider::new(vec![
        Ok(budget_decision_response(
            CalibrationKind::Calibrated,
            (0.78, 0.80),
            BUDGET_VERDICT_EXTEND,
            0.85,
            0.15,
            0.80,
        )),
        Ok(budget_decision_response(
            CalibrationKind::Calibrated,
            (0.85, 0.85),
            BUDGET_VERDICT_EXTEND,
            0.88,
            0.12,
            0.82,
        )),
    ]);
    let (mut state, shell, owner) =
        setup_smart_budget_state(TrustPosture::ConfinedSmart, provider).await;
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let exhausted = Exhausted {
        dimension: Dimension::ModelCalls,
    };

    let first = try_auto_extend(&mut state, &exhausted, &cancel, &mut |ev| {
        events.push(ev);
        async {}
    })
    .await
    .unwrap();
    assert!(
        first,
        "first exhaustion should auto-extend under ConfinedSmart"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let budget_snap = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(budget_snap.limits.model_calls.get(), 2);
    assert_eq!(budget_snap.auto_extensions_used, 1);

    let ui_budget: UiBudgetSnapshot = serde_json::from_slice(
        &shell
            .cat(&format!("{owner}/machine/ui/budget"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ui_budget.max_model_calls, 2);
    assert_eq!(ui_budget.auto_extensions_used, Some(1));
    assert_eq!(
        ui_budget.max_auto_extensions,
        Some(MAX_SMART_AUTO_EXTENSIONS)
    );

    let ui_sys1: UiSystem1Snapshot = serde_json::from_slice(
        &shell
            .cat(&format!("{owner}/machine/ui/system1"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ui_sys1.budget_extensions, 1);
    assert_eq!(ui_sys1.escalations, 0);
    assert_eq!(ui_sys1.evaluator_profile.as_deref(), Some("default"));
    assert_eq!(ui_sys1.evaluator_model.as_deref(), Some("magpie-small"));
    assert_eq!(ui_sys1.calibration, UiSystem1Calibration::Calibrated);
    assert!(events.iter().any(|ev| matches!(
        ev,
        Event::Warning { message } if message.contains("auto-extended task budget (1/1)")
    )));

    // Exhaust the newly added model call, then verify cap blocks a second automatic extension.
    state
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .reserve(Admission::Model)
        .await
        .unwrap();
    events.clear();
    let second = try_auto_extend(&mut state, &exhausted, &cancel, &mut |ev| {
        events.push(ev);
        async {}
    })
    .await
    .unwrap();
    assert!(
        !second,
        "second exhaustion beyond cap must escalate to human"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "cap check must short-circuit without calling System 1 again"
    );
    assert!(events.iter().any(|ev| matches!(
        ev,
        Event::Warning { message } if message.contains("auto-extension cap reached")
    )));
}

#[tokio::test]
async fn looping_or_stuck_and_error_fail_closed_and_record_escalation() {
    // 1. Looping / stuck verdict escalates to human.
    let (provider, _) = MockBudgetDecisionProvider::new(vec![Ok(budget_decision_response(
        CalibrationKind::Calibrated,
        (0.25, 0.88),
        BUDGET_VERDICT_ESCALATE,
        0.15,
        0.85,
        0.86,
    ))]);
    let (mut state, shell, owner) =
        setup_smart_budget_state(TrustPosture::ConfinedSmart, provider).await;
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let extended = try_auto_extend(
        &mut state,
        &Exhausted {
            dimension: Dimension::ModelCalls,
        },
        &cancel,
        &mut |ev| {
            events.push(ev);
            async {}
        },
    )
    .await
    .unwrap();
    assert!(!extended);
    let snap = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(snap.limits.model_calls.get(), 1);
    assert_eq!(snap.auto_extensions_used, 0);
    let ui_sys1: UiSystem1Snapshot = serde_json::from_slice(
        &shell
            .cat(&format!("{owner}/machine/ui/system1"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ui_sys1.escalations, 1);
    assert_eq!(ui_sys1.budget_extensions, 0);
    assert!(events.iter().any(|ev| matches!(
        ev,
        Event::Warning { message } if message.contains("looping_or_stuck")
    )));

    // 2. Evaluator error fails closed and publishes Unavailable calibration.
    let (err_provider, _) =
        MockBudgetDecisionProvider::new(vec![Err(anyhow::anyhow!("provider offline"))]);
    let (mut err_state, err_shell, err_owner) =
        setup_smart_budget_state(TrustPosture::ConfinedSmart, err_provider).await;
    let extended_on_err = try_auto_extend(
        &mut err_state,
        &Exhausted {
            dimension: Dimension::ModelCalls,
        },
        &cancel,
        &mut |_| async {},
    )
    .await
    .unwrap();
    assert!(!extended_on_err);
    let err_sys1: UiSystem1Snapshot = serde_json::from_slice(
        &err_shell
            .cat(&format!("{err_owner}/machine/ui/system1"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(err_sys1.escalations, 1);
    assert_eq!(err_sys1.calibration, UiSystem1Calibration::Unavailable);
}

#[tokio::test]
async fn non_smart_postures_never_invoke_auto_extend() {
    for posture in [TrustPosture::Governed, TrustPosture::ConfinedTrusted] {
        let (provider, calls) = MockBudgetDecisionProvider::new(vec![]);
        let (mut state, _, _) = setup_smart_budget_state(posture, provider).await;
        let extended = try_auto_extend(
            &mut state,
            &Exhausted {
                dimension: Dimension::ModelCalls,
            },
            &CancellationToken::new(),
            &mut |_| async {},
        )
        .await
        .unwrap();
        assert!(!extended);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(state.machine.system1_snapshot().budget_extensions, 0);
        assert_eq!(state.machine.system1_snapshot().escalations, 0);
    }
}

#[test]
fn progress_rubric_enforces_low_confidence_and_uncalibrated_thresholds() {
    let low_conf = ProgressAssessment::from_response(&budget_decision_response(
        CalibrationKind::Calibrated,
        (0.90, 0.50),
        BUDGET_VERDICT_EXTEND,
        0.88,
        0.12,
        0.55,
    ))
    .unwrap();
    assert!(matches!(
        evaluate_progress_assessment(&low_conf),
        ProgressDecision::Escalate { reason } if reason.contains("low_confidence")
    ));

    // 0.72 converging / 0.78 extend clears the relaxed Calibrated thresholds
    // (0.70 / 0.75 / 0.70) but must still escalate under Uncalibrated (0.75 / 0.80 / 0.75).
    let calibrated_moderate = ProgressAssessment::from_response(&budget_decision_response(
        CalibrationKind::Calibrated,
        (0.72, 0.76),
        BUDGET_VERDICT_EXTEND,
        0.78,
        0.22,
        0.76,
    ))
    .unwrap();
    assert_eq!(
        evaluate_progress_assessment(&calibrated_moderate),
        ProgressDecision::Extend
    );

    let uncalibrated_moderate = ProgressAssessment::from_response(&budget_decision_response(
        CalibrationKind::Uncalibrated,
        (0.72, 0.76),
        BUDGET_VERDICT_EXTEND,
        0.78,
        0.22,
        0.76,
    ))
    .unwrap();
    assert!(matches!(
        evaluate_progress_assessment(&uncalibrated_moderate),
        ProgressDecision::Escalate { reason } if reason.contains("converging_healthy")
    ));

    let uncalibrated_strong = ProgressAssessment::from_response(&budget_decision_response(
        CalibrationKind::Uncalibrated,
        (0.78, 0.80),
        BUDGET_VERDICT_EXTEND,
        0.85,
        0.15,
        0.80,
    ))
    .unwrap();
    assert_eq!(
        evaluate_progress_assessment(&uncalibrated_strong),
        ProgressDecision::Extend
    );

    let stuck_verdict = ProgressAssessment::from_response(&budget_decision_response(
        CalibrationKind::Calibrated,
        (0.95, 0.95),
        BUDGET_VERDICT_ESCALATE,
        0.10,
        0.90,
        0.95,
    ))
    .unwrap();
    assert!(matches!(
        evaluate_progress_assessment(&stuck_verdict),
        ProgressDecision::Escalate { reason } if reason.contains("looping_or_stuck")
    ));
}

#[tokio::test]
async fn budget_observer_updates_auto_extension_cap_when_posture_toggles() {
    let (provider, _) = MockBudgetDecisionProvider::new(vec![]);
    let (state, shell, owner) = setup_smart_budget_state(TrustPosture::Governed, provider).await;
    let mut observer = BudgetObserver::new(&state);

    observer.observe().await;
    let governed: UiBudgetSnapshot = serde_json::from_slice(
        &shell
            .cat(&format!("{owner}/machine/ui/budget"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(governed.auto_extensions_used, None);
    assert_eq!(governed.max_auto_extensions, None);

    state
        .environment
        .set_trust_posture(TrustPosture::ConfinedSmart);
    observer.observe().await;
    let smart: UiBudgetSnapshot = serde_json::from_slice(
        &shell
            .cat(&format!("{owner}/machine/ui/budget"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(smart.auto_extensions_used, Some(0));
    assert_eq!(smart.max_auto_extensions, Some(MAX_SMART_AUTO_EXTENSIONS));
}

fn tier2_budget_json(
    converging: (f64, f64),
    choice: &str,
    extend_prob: f64,
    escalate_prob: f64,
    verdict_conf: f64,
) -> String {
    serde_json::json!({
        "answers": {
            BUDGET_Q_CONVERGING_HEALTHY: {
                "yes_probability": converging.0,
                "confidence": converging.1,
            },
            BUDGET_Q_VERDICT: {
                "choice": choice,
                "probabilities": {
                    BUDGET_VERDICT_EXTEND: extend_prob,
                    BUDGET_VERDICT_ESCALATE: escalate_prob,
                },
                "confidence": verdict_conf,
            },
        }
    })
    .to_string()
}

#[tokio::test]
async fn tier2_uncalibrated_fallback_auto_extends_on_unavailable_and_records_uncalibrated() {
    let (provider, decide_calls) = MockBudgetDecisionProvider::new(vec![Err(anyhow::anyhow!(
        "native decide endpoint unavailable"
    ))]);
    let (provider, gen_calls, schemas) = provider.with_tier2(
        true,
        vec![Ok(tier2_budget_json(
            (0.95, 0.92),
            BUDGET_VERDICT_EXTEND,
            0.95,
            0.05,
            0.92,
        ))],
    );
    let (mut state, shell, owner) =
        setup_smart_budget_state(TrustPosture::ConfinedSmart, provider).await;
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let exhausted = Exhausted {
        dimension: Dimension::ModelCalls,
    };

    let extended = try_auto_extend(&mut state, &exhausted, &cancel, &mut |ev| {
        events.push(ev);
        async {}
    })
    .await
    .unwrap();

    assert!(
        extended,
        "high-confidence Tier 2 Uncalibrated fallback should auto-extend once"
    );
    assert_eq!(decide_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gen_calls.load(Ordering::SeqCst), 1);
    {
        let recorded = schemas.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert!(
            recorded[0].is_some(),
            "Tier 2 budget rubric must attach a strict JSON Schema"
        );
    }

    let budget_snap = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(budget_snap.limits.model_calls.get(), 2);
    assert_eq!(budget_snap.auto_extensions_used, 1);

    let ui_sys1: UiSystem1Snapshot = serde_json::from_slice(
        &shell
            .cat(&format!("{owner}/machine/ui/system1"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ui_sys1.budget_extensions, 1);
    assert_eq!(ui_sys1.escalations, 0);
    assert_eq!(ui_sys1.calibration, UiSystem1Calibration::Uncalibrated);
    assert_eq!(ui_sys1.evaluator_profile.as_deref(), Some("default"));
    assert_eq!(ui_sys1.evaluator_model.as_deref(), Some("magpie-small"));
    assert!(events.iter().any(|ev| matches!(
        ev,
        Event::Warning { message } if message.contains("auto-extended task budget (1/1)")
    )));
}

#[tokio::test]
async fn tier2_uncalibrated_fallback_enforces_stricter_threshold_and_fails_closed_on_malformed() {
    let cancel = CancellationToken::new();
    let exhausted = Exhausted {
        dimension: Dimension::ModelCalls,
    };

    // 1. Generation-only provider with moderate scores (0.72 converging / 0.78 extend
    //    clears Calibrated 0.70/0.75/0.70, but must escalate under stricter Uncalibrated
    //    0.75/0.80/0.75 thresholds).
    let (provider, decide_calls) = MockBudgetDecisionProvider::new(vec![]);
    let (provider, gen_calls, _) = provider.with_tier2(
        false,
        vec![Ok(tier2_budget_json(
            (0.72, 0.76),
            BUDGET_VERDICT_EXTEND,
            0.78,
            0.22,
            0.76,
        ))],
    );
    let (mut state, shell, owner) =
        setup_smart_budget_state(TrustPosture::ConfinedSmart, provider).await;
    let extended = try_auto_extend(&mut state, &exhausted, &cancel, &mut |_| async {})
        .await
        .unwrap();
    assert!(!extended);
    assert_eq!(decide_calls.load(Ordering::SeqCst), 0);
    assert_eq!(gen_calls.load(Ordering::SeqCst), 1);
    let snap = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(snap.limits.model_calls.get(), 1);
    assert_eq!(snap.auto_extensions_used, 0);
    let ui_sys1: UiSystem1Snapshot = serde_json::from_slice(
        &shell
            .cat(&format!("{owner}/machine/ui/system1"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ui_sys1.budget_extensions, 0);
    assert_eq!(ui_sys1.escalations, 1);
    assert_eq!(ui_sys1.calibration, UiSystem1Calibration::Uncalibrated);

    // 2. Double failure: native Unavailable + malformed Tier 2 JSON fails closed to human.
    let (bad_provider, bad_decide_calls) =
        MockBudgetDecisionProvider::new(vec![Err(anyhow::anyhow!("native decide unavailable"))]);
    let (bad_provider, bad_gen_calls, _) =
        bad_provider.with_tier2(true, vec![Ok("extend please".to_string())]);
    let (mut bad_state, bad_shell, bad_owner) =
        setup_smart_budget_state(TrustPosture::ConfinedSmart, bad_provider).await;
    let mut bad_events = Vec::new();
    let bad_extended = try_auto_extend(&mut bad_state, &exhausted, &cancel, &mut |ev| {
        bad_events.push(ev);
        async {}
    })
    .await
    .unwrap();
    assert!(!bad_extended);
    assert_eq!(bad_decide_calls.load(Ordering::SeqCst), 1);
    assert_eq!(bad_gen_calls.load(Ordering::SeqCst), 1);
    let bad_snap = bad_state
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .snapshot()
        .await;
    assert_eq!(bad_snap.limits.model_calls.get(), 1);
    assert_eq!(bad_snap.auto_extensions_used, 0);
    let bad_sys1: UiSystem1Snapshot = serde_json::from_slice(
        &bad_shell
            .cat(&format!("{bad_owner}/machine/ui/system1"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(bad_sys1.budget_extensions, 0);
    assert_eq!(bad_sys1.escalations, 1);
    assert_eq!(bad_sys1.calibration, UiSystem1Calibration::Unavailable);
    assert!(bad_events.iter().any(|ev| matches!(
        ev,
        Event::Warning { message } if message.contains("failed closed") && message.contains("malformed")
    )));
}
