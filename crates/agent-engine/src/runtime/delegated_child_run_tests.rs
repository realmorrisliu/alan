use super::*;
use crate::runtime::ChildRunRecord;
use crate::skills::DelegatedSkillOutputDebugMetadata;
use serde_json::json;

fn child_result(status: ChildRuntimeStatus) -> ChildRuntimeResult {
    ChildRuntimeResult {
        status,
        process_path: "/proc/42".to_string(),
        child_run_id: Some("child-run-1".to_string()),
        rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
        output_text: String::new(),
        turn_summary: None,
        structured_output: None,
        warnings: Vec::new(),
        error_message: None,
        pause: None,
        child_run: None,
    }
}

fn child_run_record() -> ChildRunRecord {
    ChildRunRecord::new(
        "child-run-1".to_string(),
        "/proc/1".to_string(),
        "/proc/42".to_string(),
        Some("/agent/42".to_string()),
        Some("definition:reviewer".to_string()),
    )
}

fn output_reference(path: &str, length: usize) -> DelegatedSkillOutputRef {
    DelegatedSkillOutputRef {
        path: path.to_string(),
        offset: Some(0),
        length: Some(length as u64),
        debug: Some(DelegatedSkillOutputDebugMetadata {
            process_path: "/proc/42".to_string(),
            rollout_path: None,
            field: "output_text".to_string(),
        }),
    }
}

#[test]
fn completed_handoff_prefers_structured_summary_and_redacts_payload() {
    let mut result = child_result(ChildRuntimeStatus::Completed);
    result.output_text = "api_key=child-secret".to_string();
    result.structured_output = Some(json!({
        "status": "completed",
        "summary": "api_key=structured-secret"
    }));

    let delegated = result.delegated_result(None);

    assert_eq!(
        delegated.structured_output.as_ref().unwrap()["summary"],
        json!("api_key= [REDACTED reason=secret_key]")
    );
    assert!(!delegated.summary.contains("structured-secret"));
    assert!(!delegated.output_text.unwrap().contains("child-secret"));
}

#[test]
fn completed_handoff_prefers_short_output_over_turn_summary_and_keeps_run_metadata() {
    let mut result = child_result(ChildRuntimeStatus::Completed);
    result.output_text =
        "Verification was environment_blocked: pytest was not installed.".to_string();
    result.turn_summary = Some("Task completed".to_string());
    result.child_run = Some(child_run_record());

    let delegated = result.delegated_result(None);

    assert_eq!(
        delegated.summary,
        "Verification was environment_blocked: pytest was not installed."
    );
    assert_eq!(
        delegated.output_text.as_deref(),
        Some(delegated.summary.as_str())
    );
    assert_eq!(
        delegated
            .child_run
            .as_ref()
            .and_then(|value| value.get("id")),
        Some(&json!("child-run-1"))
    );
    assert!(delegated.truncation.is_none());
}

#[test]
fn completed_handoff_uses_namespace_reference_for_long_output() {
    let mut result = child_result(ChildRuntimeStatus::Completed);
    result.output_text = "x".repeat(MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS + 1);
    let reference = output_reference(
        "/agent/1/actions/a0/output",
        MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS + 1,
    );

    let delegated = result.delegated_result(Some(reference.clone()));

    assert!(delegated.output_text.is_none());
    assert_eq!(delegated.output_ref, Some(reference));
    assert_eq!(
        delegated
            .truncation
            .as_ref()
            .and_then(|truncation| truncation.original_output_chars),
        Some(MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS + 1)
    );
}

#[test]
fn timed_out_handoff_keeps_failure_and_child_run_metadata() {
    let mut result = child_result(ChildRuntimeStatus::TimedOut);
    result.output_text = "partial output".to_string();
    result.error_message = Some("idle timeout exceeded".to_string());
    result.warnings = vec!["child was idle".to_string()];
    result.child_run = Some(child_run_record());
    let reference = output_reference("/agent/1/actions/a1/output", result.output_text.len());

    let delegated = result.delegated_result(Some(reference.clone()));

    assert_eq!(delegated.error_kind.as_deref(), Some("child_timed_out"));
    assert_eq!(
        delegated.error_message.as_deref(),
        Some("idle timeout exceeded")
    );
    assert_eq!(delegated.warnings, vec!["child was idle"]);
    assert_eq!(delegated.output_ref, Some(reference));
    assert_eq!(
        result.reference().rollout_debug_path.as_deref(),
        Some("/tmp/child-rollout.jsonl")
    );
    assert_eq!(
        delegated
            .child_run
            .as_ref()
            .and_then(|value| value.get("status")),
        Some(&json!("starting"))
    );
}

#[test]
fn paused_handoff_preserves_pause_kind_and_partial_output_reference() {
    let mut result = child_result(ChildRuntimeStatus::Paused);
    result.output_text = "partial output before pause".to_string();
    result.pause = Some(ChildRuntimePause {
        request_id: "confirmation-1".to_string(),
        kind: YieldKind::Confirmation,
    });
    let reference = output_reference("/agent/1/actions/paused/output", result.output_text.len());

    let delegated = result.delegated_result(Some(reference.clone()));

    assert_eq!(delegated.error_kind.as_deref(), Some("child_paused"));
    assert!(delegated.summary.contains("confirmation"));
    assert_eq!(delegated.output_ref, Some(reference));
}

#[test]
fn terminated_handoff_uses_namespace_state_reference_not_rollout_path() {
    let state_ref = output_reference("/agent/1/actions/a2/output", 18);
    let mut result = child_result(ChildRuntimeStatus::Terminated);
    result.output_text = "partial termination".to_string();
    result.error_message = Some("operator requested stop".to_string());
    result.child_run = Some(child_run_record().with_state_ref(state_ref.clone()));

    let delegated = result.delegated_result(Some(state_ref));
    let encoded = serde_json::to_string(&delegated).unwrap();

    assert_eq!(delegated.error_kind.as_deref(), Some("child_terminated"));
    assert_eq!(
        delegated
            .child_run
            .as_ref()
            .and_then(|value| value.pointer("/state_ref/path")),
        Some(&json!("/agent/1/actions/a2/output"))
    );
    assert!(encoded.contains("/proc/42"));
    assert!(!encoded.contains("/tmp/child-rollout.jsonl"));
}

#[test]
fn cancelled_before_launch_has_no_synthetic_process_or_run_identity() {
    let result = ChildRuntimeResult::cancelled_before_launch();

    assert!(result.is_cancelled());
    assert_eq!(result.terminal_status_label(), "cancelled");
    assert!(result.process_path.is_empty());
    assert!(result.child_run_id.is_none());
    assert_eq!(
        result
            .child_run_value()
            .as_ref()
            .and_then(|value| value.get("terminal_status")),
        Some(&json!("cancelled"))
    );
}
