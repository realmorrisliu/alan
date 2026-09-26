use std::time::Instant;

use alan_agent_protocol::{
    CompactionAttemptSnapshot, MemoryFlushAttemptSnapshot, UiActivitySnapshot, UiActivityState,
    UiEvent, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
};
use anyhow::Result;

use super::{transition::NamespaceAgentFiles, turn_input::TurnInputBroker};

/// The Process pump is the only activity file writer. Transitions enqueue observations.
pub(crate) async fn flush_activity(
    namespace: &NamespaceAgentFiles,
    queue: &TurnInputBroker,
) -> Result<()> {
    let mut last = namespace.read_ui_activity_snapshot().await?;
    for activity in queue
        .drain_activity_events()
        .into_iter()
        .chain(std::iter::once(queue.activity_snapshot()))
    {
        if activity == last {
            continue;
        }
        namespace.write_ui_activity_snapshot(&activity).await?;
        namespace
            .append_ui_event(&UiEvent::Activity {
                snapshot: activity.clone(),
            })
            .await?;
        last = activity;
    }
    Ok(())
}

pub(crate) async fn initialize(namespace: &NamespaceAgentFiles) -> Result<()> {
    namespace
        .write_ui_activity_snapshot(&UiActivitySnapshot::idle())
        .await?;
    namespace
        .write_ui_plan_snapshot(&UiPlanSnapshot::empty())
        .await?;
    namespace
        .write_ui_thinking_snapshot(&UiThinkingSnapshot::idle())
        .await?;
    namespace
        .write_ui_notice_snapshot(&UiNoticeSnapshot::none())
        .await
}

pub(crate) async fn turn_started(
    namespace: &NamespaceAgentFiles,
    queue: &TurnInputBroker,
) -> Result<()> {
    queue.set_ui_activity(UiActivityState::Running);
    let thinking = UiThinkingSnapshot::idle();
    namespace.write_ui_thinking_snapshot(&thinking).await?;
    namespace
        .append_ui_event(&UiEvent::Thinking { snapshot: thinking })
        .await?;
    let notice = UiNoticeSnapshot::none();
    namespace.write_ui_notice_snapshot(&notice).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot: notice })
        .await
}

pub(crate) async fn turn_completed(
    namespace: &NamespaceAgentFiles,
    queue: &TurnInputBroker,
    cancelled: bool,
) -> Result<()> {
    if cancelled {
        plan_updated(namespace, None, Vec::new()).await?;
    }
    queue.set_ui_activity(UiActivityState::Idle);
    Ok(())
}

pub(crate) async fn turn_failed(
    namespace: &NamespaceAgentFiles,
    queue: &TurnInputBroker,
    message: &str,
    submission_id: Option<&str>,
) -> Result<()> {
    error_notice(namespace, message, submission_id).await?;
    turn_completed(namespace, queue, false).await
}

pub(crate) async fn error_notice(
    namespace: &NamespaceAgentFiles,
    message: &str,
    submission_id: Option<&str>,
) -> Result<()> {
    let notice = UiNoticeSnapshot::new(UiNoticeKind::Error, message);
    namespace.write_ui_notice_snapshot(&notice).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot: notice })
        .await?;
    namespace
        .append_ui_event(&UiEvent::Error {
            message: message.to_string(),
            recoverable: true,
            submission_id: submission_id.map(str::to_owned),
        })
        .await
}

pub(crate) async fn paused(
    _namespace: &NamespaceAgentFiles,
    queue: &TurnInputBroker,
) -> Result<()> {
    queue.set_ui_activity(UiActivityState::Paused);
    Ok(())
}

pub(crate) async fn resumed(
    _namespace: &NamespaceAgentFiles,
    queue: &TurnInputBroker,
) -> Result<()> {
    queue.set_ui_activity(UiActivityState::Running);
    Ok(())
}

pub(crate) async fn heartbeat(
    namespace: &NamespaceAgentFiles,
    queue: &TurnInputBroker,
) -> Result<()> {
    flush_activity(namespace, queue).await
}

pub(crate) async fn plan_updated(
    namespace: &NamespaceAgentFiles,
    explanation: Option<String>,
    items: Vec<alan_agent_protocol::PlanItem>,
) -> Result<()> {
    let snapshot = UiPlanSnapshot::new(explanation, items);
    namespace.write_ui_plan_snapshot(&snapshot).await?;
    namespace.append_ui_event(&UiEvent::Plan { snapshot }).await
}

pub(crate) async fn rollback(namespace: &NamespaceAgentFiles, turns: u32) -> Result<()> {
    let snapshot =
        UiNoticeSnapshot::new(UiNoticeKind::Rollback, format!("rolled back {turns} turns"));
    namespace.write_ui_notice_snapshot(&snapshot).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot })
        .await
}

pub(crate) async fn thinking(namespace: &NamespaceAgentFiles, text: &str) -> Result<()> {
    let started = Instant::now();
    let mut visible = String::new();
    for chunk in super::turn_support::split_text_for_typing(text) {
        visible.push_str(&chunk);
        let snapshot = UiThinkingSnapshot::streaming(visible.clone());
        namespace.write_ui_thinking_snapshot(&snapshot).await?;
        namespace
            .append_ui_event(&UiEvent::Thinking { snapshot })
            .await?;
    }
    let snapshot = UiThinkingSnapshot::complete(visible, started.elapsed().as_secs());
    namespace.write_ui_thinking_snapshot(&snapshot).await?;
    namespace
        .append_ui_event(&UiEvent::Thinking { snapshot })
        .await
}

pub(crate) async fn warning(
    namespace: &NamespaceAgentFiles,
    message: impl Into<String>,
) -> Result<()> {
    let snapshot = UiNoticeSnapshot::new(UiNoticeKind::Warning, message.into());
    namespace.write_ui_notice_snapshot(&snapshot).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot })
        .await
}

pub(crate) async fn compaction(
    namespace: &NamespaceAgentFiles,
    attempt: &CompactionAttemptSnapshot,
) -> Result<()> {
    let snapshot =
        UiNoticeSnapshot::new(UiNoticeKind::Compaction, compaction_notice_message(attempt));
    namespace.write_ui_notice_snapshot(&snapshot).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot })
        .await
}

pub(crate) async fn memory_flush(
    namespace: &NamespaceAgentFiles,
    attempt: &MemoryFlushAttemptSnapshot,
) -> Result<()> {
    let snapshot = UiNoticeSnapshot::new(
        UiNoticeKind::MemoryFlush,
        memory_flush_notice_message(attempt),
    );
    namespace.write_ui_notice_snapshot(&snapshot).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot })
        .await
}

fn compaction_notice_message(attempt: &CompactionAttemptSnapshot) -> String {
    if let Some(message) = attempt
        .warning_message
        .as_deref()
        .or(attempt.error_message.as_deref())
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return message.to_string();
    }

    match attempt.result {
        alan_agent_protocol::CompactionResult::Success => "context compacted".to_string(),
        alan_agent_protocol::CompactionResult::Retry => "context compaction retrying".to_string(),
        alan_agent_protocol::CompactionResult::Degraded => {
            "context compaction degraded".to_string()
        }
        alan_agent_protocol::CompactionResult::Failure => "context compaction failed".to_string(),
    }
}

fn memory_flush_notice_message(attempt: &MemoryFlushAttemptSnapshot) -> String {
    if let Some(message) = attempt
        .warning_message
        .as_deref()
        .or(attempt.error_message.as_deref())
        .map(str::trim)
        .filter(|message| !message.is_empty())
    {
        return message.to_string();
    }

    match attempt.result {
        alan_agent_protocol::MemoryFlushResult::Success => "memory flushed".to_string(),
        alan_agent_protocol::MemoryFlushResult::Skipped => "memory flush skipped".to_string(),
        alan_agent_protocol::MemoryFlushResult::Failure => "memory flush failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use alan_agent_protocol::{
        PlanItem, PlanItemStatus, UiActivityState, UiEvent, UiThinkingState,
    };
    use alan_agentfs::AgentFs;
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use serde_json::Value;

    use super::*;
    use crate::runtime::transition::NamespaceRuntimeEnvironment;

    fn agent_files() -> (NamespaceAgentFiles, alan_shell::Shell) {
        let agentfs = Arc::new(AgentFs::new());
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        (environment.agent_files(), shell)
    }

    #[tokio::test]
    async fn owners_write_snapshots_and_append_ui_events() {
        let (environment, shell) = agent_files();
        let queue = TurnInputBroker::default();
        initialize(&environment).await.unwrap();
        turn_started(&environment, &queue).await.unwrap();
        thinking(&environment, "reasoning").await.unwrap();
        plan_updated(
            &environment,
            Some("ship parity".to_string()),
            vec![PlanItem {
                id: "1".to_string(),
                content: "wire ui files".to_string(),
                status: PlanItemStatus::InProgress,
            }],
        )
        .await
        .unwrap();
        warning(&environment, "retrying").await.unwrap();
        turn_completed(&environment, &queue, false).await.unwrap();

        flush_activity(&environment, &queue).await.unwrap();
        let activity: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/activity").await.unwrap())
                .unwrap();
        assert_eq!(activity["state"], "idle");

        let thinking: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/thinking").await.unwrap())
                .unwrap();
        assert_eq!(thinking["state"], "complete");
        assert_eq!(thinking["text"], "reasoning");

        let plan: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/plan").await.unwrap()).unwrap();
        assert_eq!(plan["explanation"], "ship parity");
        assert_eq!(plan["items"][0]["content"], "wire ui files");

        let notice: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/notice").await.unwrap())
                .unwrap();
        assert_eq!(notice["message"], "retrying");

        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        let parsed = events
            .lines()
            .map(|line| serde_json::from_str::<UiEvent>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(parsed.iter().any(|event| matches!(
            event,
            UiEvent::Activity { snapshot } if snapshot.state == UiActivityState::Running
        )));
        assert!(parsed.iter().any(|event| matches!(
            event,
            UiEvent::Thinking { snapshot }
                if snapshot.state == UiThinkingState::Complete && snapshot.text == "reasoning"
        )));
        assert!(parsed.iter().any(|event| matches!(
            event,
            UiEvent::Notice { snapshot } if snapshot.message == "retrying"
        )));
    }

    #[tokio::test]
    async fn cancelled_turn_clears_plan_snapshot() {
        let (environment, shell) = agent_files();
        let queue = TurnInputBroker::default();
        initialize(&environment).await.unwrap();
        plan_updated(
            &environment,
            Some("ship parity".to_string()),
            vec![PlanItem {
                id: "1".to_string(),
                content: "wire ui files".to_string(),
                status: PlanItemStatus::InProgress,
            }],
        )
        .await
        .unwrap();
        turn_completed(&environment, &queue, true).await.unwrap();

        let plan: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/plan").await.unwrap()).unwrap();
        assert_eq!(plan["items"], Value::Array(Vec::new()));
    }

    #[tokio::test]
    async fn failed_turn_records_file_terminal_error() {
        let (environment, shell) = agent_files();
        let queue = TurnInputBroker::default();
        initialize(&environment).await.unwrap();
        turn_started(&environment, &queue).await.unwrap();
        turn_failed(
            &environment,
            &queue,
            "provider failed",
            Some("failed-input"),
        )
        .await
        .unwrap();

        let notice = environment.read_ui_notice_snapshot().await.unwrap();
        assert_eq!(notice.kind, UiNoticeKind::Error);
        assert_eq!(notice.message, "provider failed");
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert!(events.lines().any(|line| matches!(
            serde_json::from_str::<UiEvent>(line).unwrap(),
            UiEvent::Error { submission_id: Some(id), .. } if id == "failed-input"
        )));
    }

    #[tokio::test]
    async fn heartbeat_preserves_paused_activity() {
        let (environment, _) = agent_files();
        let queue = TurnInputBroker::default();
        initialize(&environment).await.unwrap();
        paused(&environment, &queue).await.unwrap();

        heartbeat(&environment, &queue).await.unwrap();

        assert_eq!(
            environment.read_ui_activity_snapshot().await.unwrap().state,
            UiActivityState::Paused
        );
    }
    #[tokio::test]
    async fn activity_publisher_preserves_queue_identity_and_start_time_across_heartbeat() {
        let (files, shell) = agent_files();
        let mut machine = crate::agent_machine::AgentMachine::new();
        let queue = machine.input_broker();
        initialize(&files).await.unwrap();
        machine.accept_submission_identity(alan_agent_protocol::UiSubmission {
            submission_id: "active".into(),
            intent: alan_agent_protocol::InputIntent::ForceAgent,
        });
        turn_started(&files, &queue).await.unwrap();
        flush_activity(&files, &queue).await.unwrap();
        let running = files.read_ui_activity_snapshot().await.unwrap();
        queue.push_outer_submission(alan_agent_protocol::Submission::with_id_and_intent(
            "next",
            alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("printf next")],
                mode: alan_agent_protocol::InputMode::FollowUp,
            },
            alan_agent_protocol::InputIntent::Command,
        ));
        heartbeat(&files, &queue).await.unwrap();
        let queued = files.read_ui_activity_snapshot().await.unwrap();
        assert_eq!(queued.started_at_ms, running.started_at_ms);
        assert_eq!(queued.active_submission, running.active_submission);
        assert_eq!(queued.pending_submissions[0].submission_id, "next");
        queue.interrupt("active").unwrap();
        machine.finish_submission();
        turn_completed(&files, &queue, true).await.unwrap();
        flush_activity(&files, &queue).await.unwrap();
        let paused = files.read_ui_activity_snapshot().await.unwrap();
        assert!(paused.queue_paused);
        assert!(paused.active_submission.is_none());
        assert_eq!(paused.pending_submissions, queued.pending_submissions);
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        let last: UiEvent = serde_json::from_str(events.lines().last().unwrap()).unwrap();
        assert_eq!(last, UiEvent::Activity { snapshot: paused });
    }
}
