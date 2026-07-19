use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct RecordingProcessLifecycle {
    finish_count: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl AgentProcessLifecycle for RecordingProcessLifecycle {
    async fn finish(&self, _exit_code: i32) {
        self.finish_count.fetch_add(1, Ordering::Relaxed);
    }
}

fn test_supervisor(finish_count: Arc<AtomicUsize>) -> DelegatedChildRunSupervisor {
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new()));
    let process_environment = NamespaceRuntimeEnvironment::new(root, "/agent/42", "default");
    DelegatedChildRunSupervisor::new(DelegatedChildRunSupervision {
        runtime: None,
        startup_metadata: RuntimeStartupMetadata {
            process_path: "/proc/42".to_string(),
            agent_path: "/agent/42".to_string(),
            rollout_id: None,
            rollout_path: None,
            durability: crate::runtime::AgentMachineDurabilityState {
                durable: false,
                required: false,
            },
            execution_backend: "test".to_string(),
            request_controls: crate::ResolvedRequestControls::default(),
            warnings: Vec::new(),
        },
        child_run_id: "child-run-1".to_string(),
        child_run_registry: ChildRunRegistry::default(),
        timeout: None,
        process_lifecycle: Arc::new(RecordingProcessLifecycle { finish_count }),
        agent_files: process_environment.agent_files(),
        process_environment,
        process_pid: "42".to_string(),
    })
}

#[test]
fn bounded_warnings_keep_recent_truncated_entries() {
    let mut warnings = Vec::new();

    for index in 0..(MAX_OBSERVED_CHILD_WARNINGS + 2) {
        push_bounded_child_warning(
            &mut warnings,
            format!(
                "warning-{index:03}-{}",
                "x".repeat(MAX_OBSERVED_CHILD_WARNING_CHARS)
            ),
        );
    }

    assert_eq!(warnings.len(), MAX_OBSERVED_CHILD_WARNINGS);
    assert!(warnings[0].starts_with("warning-002-"));
    assert!(
        warnings
            .iter()
            .all(|warning| warning.chars().count() <= MAX_OBSERVED_CHILD_WARNING_CHARS)
    );
    assert!(warnings.last().unwrap().ends_with("..."));
}

#[test]
fn structured_output_reads_last_json_fence() {
    let text = "Notes before\n```json\n{\"status\":\"completed\",\"summary\":\"first\"}\n```\nMore notes\n```json\n{\"status\":\"completed\",\"summary\":\"second\"}\n```";

    let parsed = parse_child_structured_output(text).unwrap();
    assert_eq!(parsed["summary"], serde_json::json!("second"));
}

#[test]
fn rollout_fallback_reads_latest_nested_assistant_text() {
    let contents = concat!(
        "{\"type\":\"message\",\"role\":\"assistant\",\"content\":null,\"message\":{\"parts\":[{\"type\":\"text\",\"text\":\"first\"}]}}\n",
        "{\"type\":\"message\",\"role\":\"assistant\",\"content\":null,\"message\":{\"parts\":[{\"type\":\"text\",\"text\":\"second\"},{\"type\":\"tool_request\",\"id\":\"ignored\"}]}}\n"
    );

    assert_eq!(
        extract_latest_assistant_text_from_rollout(contents).as_deref(),
        Some("second")
    );
}

#[test]
fn timeout_retains_latest_file_output_and_warnings() {
    let observed = timed_out_observation(
        "idle timeout",
        r#"{"status":"partial"}"#,
        &["latest warning".to_string()],
    );

    assert_eq!(observed.status, ChildRuntimeStatus::TimedOut);
    assert_eq!(observed.output_text, r#"{"status":"partial"}"#);
    assert_eq!(
        observed.structured_output,
        Some(serde_json::json!({"status": "partial"}))
    );
    assert_eq!(observed.warnings, vec!["latest warning"]);
}

#[test]
fn governed_termination_retains_latest_file_evidence() {
    let observed = terminated_observation(
        ChildRunTerminationRequest {
            actor: "parent".to_string(),
            reason: "result no longer needed".to_string(),
            mode: ChildRunTerminationMode::Graceful,
            requested_at_ms: 42,
        },
        "partial child output",
        &["latest warning".to_string()],
    );

    assert_eq!(observed.status, ChildRuntimeStatus::Terminated);
    assert_eq!(observed.output_text, "partial child output");
    assert_eq!(observed.warnings, vec!["latest warning"]);
    assert!(
        observed
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("parent"))
    );
}

#[tokio::test]
async fn aborting_runtime_task_defers_the_single_process_finish_to_terminal_commit() {
    let finish_count = Arc::new(AtomicUsize::new(0));
    let mut supervisor = test_supervisor(finish_count.clone());

    supervisor.abort_runtime_task().await;
    assert_eq!(finish_count.load(Ordering::Relaxed), 0);

    supervisor
        .finish_runtime_and_process(&ChildRuntimeStatus::TimedOut)
        .await;
    assert_eq!(finish_count.load(Ordering::Relaxed), 1);
}
