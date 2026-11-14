//! Whether a paused Agent Process is still able to receive the answer it waits for.
//!
//! A Process that pauses has published a request; the answer arrives on its own
//! submission channel. Pausing therefore means waiting, not finishing. The wait ends
//! only when the Process is stopped (cancel/timeout) — never because a clock elapsed
//! while an answer could still arrive.

/// `paused_with_request`: the Process is paused and has a pending request to answer.
/// `stopped`: the run was cancelled, timed out, or its parent went away.
pub(super) fn pause_keeps_waiting(paused_with_request: bool, stopped: bool) -> bool {
    paused_with_request && !stopped
}

#[cfg(test)]
mod tests {
    use super::pause_keeps_waiting;

    #[test]
    fn a_process_paused_on_a_request_keeps_waiting_for_its_answer() {
        assert!(pause_keeps_waiting(true, false));
    }

    #[test]
    fn a_stopped_run_ends_even_while_a_request_is_pending() {
        assert!(!pause_keeps_waiting(true, true));
    }

    #[test]
    fn a_process_without_a_pending_request_is_not_waiting() {
        assert!(!pause_keeps_waiting(false, false));
    }
}

/// What the host should do with one observation of a child Process.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ChildTerminal {
    /// Keep reading: the answer can still arrive.
    Wait,
    /// A pause that can no longer be answered (`stopped`: cancelled or timed out).
    Paused,
    Completed,
    Failed,
}

/// The host's terminal decision, separated from the filesystem it reads.
pub(super) fn decide_child_terminal(
    activity: alan_agent_engine::UiActivityState,
    has_pending_request: bool,
    notice_is_error: bool,
    stopped: bool,
) -> ChildTerminal {
    use alan_agent_engine::UiActivityState;
    if notice_is_error {
        return ChildTerminal::Failed;
    }
    if activity == UiActivityState::Running {
        // Work in progress is the furthest thing from an outcome.
        return ChildTerminal::Wait;
    }
    if activity != UiActivityState::Paused {
        return ChildTerminal::Completed;
    }
    if !has_pending_request {
        return ChildTerminal::Wait;
    }
    if pause_keeps_waiting(true, stopped) {
        ChildTerminal::Wait
    } else {
        ChildTerminal::Paused
    }
}

#[cfg(test)]
mod terminal_tests {
    use super::{ChildTerminal, decide_child_terminal};

    #[test]
    fn a_pause_that_can_still_be_answered_keeps_waiting() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Paused,
                true,
                false,
                false
            ),
            ChildTerminal::Wait
        );
    }

    #[test]
    fn a_stopped_run_publishes_its_pause() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Paused,
                true,
                false,
                true
            ),
            ChildTerminal::Paused
        );
    }

    #[test]
    fn an_answered_run_completes() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Idle,
                false,
                false,
                false
            ),
            ChildTerminal::Completed
        );
    }

    #[test]
    fn a_running_process_is_not_finished() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Running,
                false,
                false,
                false
            ),
            ChildTerminal::Wait
        );
    }

    #[test]
    fn an_error_notice_fails_the_run() {
        assert_eq!(
            decide_child_terminal(alan_agent_engine::UiActivityState::Idle, false, true, false),
            ChildTerminal::Failed
        );
    }

    #[test]
    fn a_pause_without_a_request_keeps_reading() {
        assert_eq!(
            decide_child_terminal(
                alan_agent_engine::UiActivityState::Paused,
                false,
                false,
                false
            ),
            ChildTerminal::Wait
        );
    }
}

#[cfg(test)]
mod host_behaviour_tests {
    use super::super::wait_for_child_terminal;
    use alan_agent_engine::{
        AgentProcessConfig, ContentPart, Op, Submission, spawn_with_namespace_environment,
    };
    use alan_kernel::{Access, MountFs, Namespace};
    use alan_llm::{GenerationResponse, MockLlmProvider, ToolCall};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn asking_then_answering() -> MockLlmProvider {
        let ask = GenerationResponse {
            content: String::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![ToolCall {
                id: Some("call-ask".to_string()),
                name: "request_confirmation".to_string(),
                arguments: serde_json::json!({
                    "summary": "approve this step",
                    "options": ["approve", "reject"],
                }),
            }],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        };
        let answer = GenerationResponse {
            content: "finished after approval".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        };
        MockLlmProvider::new().with_responses(vec![ask, answer])
    }

    async fn paused_runtime(
        temp: &TempDir,
    ) -> (
        alan_kernel::Pid,
        alan_agent_engine::RuntimeController,
        super::super::InProcessTransport,
        alan_shell::Shell,
    ) {
        let registry = alan_llmfs::LlmFs::new();
        registry.register_connection("A", Box::new(asking_then_answering()));
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            super::super::InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadWrite,
        );
        namespace.mount(
            "/mnt/llm",
            super::super::InProcessTransport::new(Arc::new(registry)),
            Access::ReadWrite,
        );
        let root = super::super::InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root.clone());
        let mut core = alan_agent_engine::Config::default();
        core.memory.enabled = false;
        core.streaming_mode = alan_agent_engine::StreamingMode::Off;
        let environment = alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
            root.clone(),
            "/agent/1",
            "A",
        );
        let mut controller = spawn_with_namespace_environment(
            AgentProcessConfig {
                agent_config: alan_agent_engine::AgentConfig::from(core.clone()),
                store_bindings: Some(alan_agent_engine::AgentRuntimeStoreBindings {
                    rollouts: temp.path().join("rollouts"),
                    checkpoints: temp.path().join("checkpoints"),
                    cache: temp.path().join("cache"),
                    tmp: temp.path().join("tmp"),
                    metadata: temp.path().join("metadata"),
                }),
                ..Default::default()
            },
            environment,
            alan_agent_engine::skills::SkillHostCapabilities::default(),
            alan_agent_engine::provider_capabilities_for_config(&core),
        )
        .unwrap();
        controller.wait_until_ready().await.unwrap();
        controller
            .handle
            .submission_tx
            .send(Submission::new(Op::Turn {
                parts: vec![ContentPart::text("ask before you answer")],
                context: None,
            }))
            .await
            .unwrap();
        (alan_kernel::Pid(1), controller, root, shell)
    }

    async fn pending_request(shell: &alan_shell::Shell) -> Option<String> {
        let entries = shell.ls("/agent/1/requests").await.ok()?;
        entries
            .into_iter()
            .find(|entry| !matches!(entry.as_str(), "clone" | "events"))
    }

    #[tokio::test]
    async fn a_yielded_turn_keeps_the_process_waiting_and_completes_after_the_resume() {
        let temp = TempDir::new().unwrap();
        let (pid, controller, root, shell) = paused_runtime(&temp).await;

        // (a) the yield does not end the Process: nothing terminal is written yet.
        let waiting = tokio::time::timeout(
            std::time::Duration::from_millis(400),
            wait_for_child_terminal(&root, pid, &controller, None),
        )
        .await;
        assert!(
            waiting.is_err(),
            "a yielded turn must leave the Process waiting, not finished"
        );

        // (b) the resume is consumed and the turn finishes.
        let request = pending_request(&shell)
            .await
            .expect("the yield published a request");
        controller
            .handle
            .submission_tx
            .send(Submission::new(Op::Resume {
                request_id: request,
                content: vec![ContentPart::structured(
                    serde_json::json!({"choice": "approve"}),
                )],
            }))
            .await
            .unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            wait_for_child_terminal(&root, pid, &controller, None),
        )
        .await
        .expect("the resumed Process reaches its outcome")
        .unwrap();
        assert_eq!(
            result.status,
            alan_agent_engine::AgentExecutableStatus::Completed
        );
    }

    #[tokio::test]
    async fn a_yielded_turn_ends_paused_at_its_own_run_timeout() {
        let temp = TempDir::new().unwrap();
        let (pid, controller, root, _shell) = paused_runtime(&temp).await;

        // (c) an unanswered yield ends the Process at its own timeout, as paused.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            wait_for_child_terminal(
                &root,
                pid,
                &controller,
                Some(std::time::Duration::from_millis(400)),
            ),
        )
        .await
        .expect("an unanswered Process ends at its own timeout")
        .unwrap();
        assert_eq!(
            result.status,
            alan_agent_engine::AgentExecutableStatus::Paused
        );
        // A timed-out Process is not stuck: it stops when the host shuts it down.
        controller.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn a_cancelled_request_neither_completes_nor_immortalises_the_child() {
        let temp = TempDir::new().unwrap();
        let (pid, controller, root, shell) = paused_runtime(&temp).await;
        tokio::time::timeout(
            std::time::Duration::from_millis(300),
            wait_for_child_terminal(&root, pid, &controller, None),
        )
        .await
        .expect_err("the yield keeps the Process waiting first");

        // Cancelling the pending request settles that request, not the turn: the
        // turn is ended by the host stop below.
        let request = pending_request(&shell)
            .await
            .expect("the yield published a request");
        shell
            .write(&format!("/agent/1/requests/{request}/ctl"), b"cancel")
            .await
            .unwrap();
        let after_cancel = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            wait_for_child_terminal(&root, pid, &controller, None),
        )
        .await;
        assert!(
            after_cancel.is_err(),
            "a cancelled request must not be reported as a completed run"
        );

        let submissions = controller.handle.submission_tx.clone();
        controller.shutdown().await.unwrap();
        assert!(
            submissions
                .send(Submission::new(Op::Interrupt))
                .await
                .is_err(),
            "the host stop ends the cancelled child, so it cannot be immortal"
        );
    }

    #[tokio::test]
    async fn a_parent_exit_stops_a_yielded_child() {
        let temp = TempDir::new().unwrap();
        let (pid, controller, root, shell) = paused_runtime(&temp).await;
        tokio::time::timeout(
            std::time::Duration::from_millis(300),
            wait_for_child_terminal(&root, pid, &controller, None),
        )
        .await
        .expect_err("the yield keeps the Process waiting first");

        // The parent going away is the Process-exit signal the child inherits.
        let submissions = controller.handle.submission_tx.clone();
        controller.shutdown().await.unwrap();
        assert!(
            submissions
                .send(Submission::new(Op::Interrupt))
                .await
                .is_err(),
            "a yielded child cannot outlive its parent Process"
        );
        let _ = shell;
    }
}
