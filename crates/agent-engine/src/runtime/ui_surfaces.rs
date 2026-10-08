use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alan_agent_protocol::{
    CompactionAttemptSnapshot, MemoryFlushAttemptSnapshot, UiActivitySnapshot, UiActivityState,
    UiEvent, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
};
use anyhow::Result;

use super::transition::NamespaceAgentFiles;

const HOST_MOUNT_WAIT_NOTICE: &str =
    "Waiting for Host Mount authorization; Ctrl+C cancels current input";

pub(super) async fn retire_host_mount_wait_notice(namespace: &NamespaceAgentFiles) {
    let result: Result<()> = async {
        let current = namespace.read_ui_notice_snapshot().await?;
        if current.kind == UiNoticeKind::Warning && current.message == HOST_MOUNT_WAIT_NOTICE {
            let snapshot = UiNoticeSnapshot::none();
            namespace.write_ui_notice_snapshot(&snapshot).await?;
            namespace
                .append_ui_event(&UiEvent::Notice { snapshot })
                .await?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, "Failed to retire Host Mount wait notice");
    }
}

impl super::transition::RuntimeLoopState {
    pub(crate) async fn publish_ensured_skills(&mut self) -> Result<()> {
        let mut last = self.prompt_cache.skill_publication.clone();
        publish_skills(
            &self.agent_files(),
            &self.prompt_cache,
            self.process_path(),
            &mut last,
        )
        .await?;
        self.prompt_cache.skill_publication = last;
        Ok(())
    }
}

pub(crate) async fn publish_skills(
    files: &NamespaceAgentFiles,
    cache: &super::prompt_cache::PromptAssemblyCache,
    process_path: String,
    last: &mut alan_agent_protocol::UiSkillSnapshot,
) -> Result<()> {
    let (known, mentionable_skill_ids) = cache.skill_observation();
    let mut next = alan_agent_protocol::UiSkillSnapshot {
        version: alan_agent_protocol::UI_SURFACE_VERSION,
        publication_version: last.publication_version,
        process_path,
        known,
        mentionable_skill_ids,
    };
    if serde_json::to_vec(&next)?.len() > (1 << 20) {
        next = next.unknown();
    }
    if next == *last {
        return Ok(());
    }
    next.publication_version = last
        .publication_version
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Skill publication version exhausted"))?;
    if serde_json::to_vec(&next)?.len() > (1 << 20) {
        next = next.unknown();
    }
    files.write_ui_skill_snapshot(&next).await?;
    *last = next;
    Ok(())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) async fn initialize(
    namespace: &NamespaceAgentFiles,
    queue_paused: bool,
    recovered_unknown_effects: bool,
) -> Result<()> {
    let activity = if queue_paused {
        UiActivitySnapshot::paused(None)
    } else {
        UiActivitySnapshot::idle()
    };
    namespace.write_ui_activity_snapshot(&activity).await?;
    if queue_paused {
        namespace
            .append_ui_event(&UiEvent::Activity { snapshot: activity })
            .await?;
    }
    namespace
        .write_ui_plan_snapshot(&UiPlanSnapshot::empty())
        .await?;
    namespace
        .write_ui_thinking_snapshot(&UiThinkingSnapshot::idle())
        .await?;
    let notice = if recovered_unknown_effects {
        UiNoticeSnapshot::new(
            UiNoticeKind::Warning,
            "Recovered work has unknown outcomes. It was not replayed; check its effects before retrying.",
        )
    } else {
        UiNoticeSnapshot::none()
    };
    namespace.write_ui_notice_snapshot(&notice).await?;
    if recovered_unknown_effects {
        namespace
            .append_ui_event(&UiEvent::Notice { snapshot: notice })
            .await?;
    }
    Ok(())
}

pub(crate) async fn turn_started(namespace: &NamespaceAgentFiles) -> Result<()> {
    let activity = UiActivitySnapshot::running(now_unix_ms());
    namespace.write_ui_activity_snapshot(&activity).await?;
    namespace
        .append_ui_event(&UiEvent::Activity { snapshot: activity })
        .await?;
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

pub(crate) async fn turn_completed(namespace: &NamespaceAgentFiles, cancelled: bool) -> Result<()> {
    retire_host_mount_wait_notice(namespace).await;
    if cancelled {
        plan_updated(namespace, None, Vec::new()).await?;
    }
    let activity = UiActivitySnapshot::idle();
    namespace.write_ui_activity_snapshot(&activity).await?;
    namespace
        .append_ui_event(&UiEvent::Activity { snapshot: activity })
        .await
}

pub(crate) async fn turn_failed(
    namespace: &NamespaceAgentFiles,
    message: &str,
    machine: Option<&crate::agent_machine::AgentMachine>,
) -> Result<()> {
    if machine.is_some_and(|machine| machine.has_pending_interaction()) {
        paused(namespace, machine).await?;
        return error_notice(namespace, message).await;
    }
    error_notice(namespace, message).await?;
    turn_completed(namespace, false).await
}

pub(crate) async fn error_notice(namespace: &NamespaceAgentFiles, message: &str) -> Result<()> {
    let notice = UiNoticeSnapshot::new(UiNoticeKind::Error, message);
    namespace.write_ui_notice_snapshot(&notice).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot: notice })
        .await?;
    namespace
        .append_ui_event(&UiEvent::Error {
            message: message.to_string(),
            recoverable: true,
        })
        .await
}

pub(crate) async fn paused(
    namespace: &NamespaceAgentFiles,
    machine: Option<&crate::agent_machine::AgentMachine>,
) -> Result<()> {
    let mut activity = UiActivitySnapshot::paused(None);
    if let Some(machine) = machine {
        activity.waiting_submission_ids = machine.related_submission_ids().to_vec();
        if let Some(id) = machine.current_submission_id() {
            activity.waiting_submission_ids.push(id.to_owned());
        }
    }
    namespace.write_ui_activity_snapshot(&activity).await?;
    namespace
        .append_ui_event(&UiEvent::Activity { snapshot: activity })
        .await?;
    if machine.is_some_and(|machine| {
        machine
            .pending_request_ids()
            .iter()
            .any(|id| machine.pending_host_mount(id).is_some())
    }) {
        if let Err(error) = warning(namespace, HOST_MOUNT_WAIT_NOTICE).await {
            tracing::warn!(%error, "Failed to publish Host Mount wait notice");
        }
    } else {
        retire_host_mount_wait_notice(namespace).await
    }
    Ok(())
}

pub(crate) async fn resumed(namespace: &NamespaceAgentFiles) -> Result<()> {
    retire_host_mount_wait_notice(namespace).await;
    let activity = UiActivitySnapshot::running(now_unix_ms());
    namespace.write_ui_activity_snapshot(&activity).await?;
    namespace
        .append_ui_event(&UiEvent::Activity { snapshot: activity })
        .await
}

pub(crate) async fn heartbeat(namespace: &NamespaceAgentFiles) -> Result<()> {
    if namespace.read_ui_activity_snapshot().await?.state != UiActivityState::Running {
        return Ok(());
    }
    namespace
        .write_ui_activity_snapshot(&UiActivitySnapshot::running(now_unix_ms()))
        .await
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
        initialize(&environment, false, false).await.unwrap();
        turn_started(&environment).await.unwrap();
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
        turn_completed(&environment, false).await.unwrap();

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
        initialize(&environment, false, false).await.unwrap();
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
        turn_completed(&environment, true).await.unwrap();

        let plan: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/plan").await.unwrap()).unwrap();
        assert_eq!(plan["items"], Value::Array(Vec::new()));
    }

    #[tokio::test]
    async fn failed_turn_records_file_terminal_error() {
        let (environment, _) = agent_files();
        initialize(&environment, false, false).await.unwrap();
        turn_started(&environment).await.unwrap();
        turn_failed(&environment, "provider failed", None)
            .await
            .unwrap();

        let notice = environment.read_ui_notice_snapshot().await.unwrap();
        assert_eq!(notice.kind, UiNoticeKind::Error);
        assert_eq!(notice.message, "provider failed");
        assert_eq!(
            environment.read_ui_activity_snapshot().await.unwrap().state,
            UiActivityState::Idle
        );
    }

    #[tokio::test]
    async fn host_mount_unreadable_notice_does_not_block_activity_lifecycle() {
        let (environment, shell) = agent_files();
        shell
            .write("/agent/1/machine/ui/notice", b"{")
            .await
            .unwrap();
        resumed(&environment).await.unwrap();
        assert_eq!(
            environment.read_ui_activity_snapshot().await.unwrap().state,
            UiActivityState::Running
        );
        turn_completed(&environment, true).await.unwrap();
        assert_eq!(
            environment.read_ui_activity_snapshot().await.unwrap().state,
            UiActivityState::Idle
        );
        paused(&environment, None).await.unwrap();
        assert_eq!(
            environment.read_ui_activity_snapshot().await.unwrap().state,
            UiActivityState::Paused
        );
    }

    #[tokio::test]
    async fn host_mount_notice_retirement_preserves_other_notice_owners() {
        let (environment, shell) = agent_files();
        for boundary in 0..4 {
            for notice in [
                UiNoticeSnapshot::new(UiNoticeKind::Warning, HOST_MOUNT_WAIT_NOTICE),
                UiNoticeSnapshot::new(UiNoticeKind::Warning, "unrelated warning"),
                UiNoticeSnapshot::new(UiNoticeKind::Error, "unrelated failure"),
            ] {
                environment.write_ui_notice_snapshot(&notice).await.unwrap();
                match boundary {
                    0 => resumed(&environment).await.unwrap(),
                    1 => turn_completed(&environment, false).await.unwrap(),
                    2 => turn_completed(&environment, true).await.unwrap(),
                    _ => paused(&environment, None).await.unwrap(),
                }
                let actual = environment.read_ui_notice_snapshot().await.unwrap();
                if notice.message == HOST_MOUNT_WAIT_NOTICE {
                    assert_eq!(actual.kind, UiNoticeKind::None);
                } else {
                    assert_eq!(actual, notice);
                }
            }
        }
        for recovery in [true, false] {
            warning(&environment, HOST_MOUNT_WAIT_NOTICE).await.unwrap();
            if recovery {
                initialize(&environment, true, false).await.unwrap();
            } else {
                turn_started(&environment).await.unwrap();
            }
            assert_eq!(
                environment.read_ui_notice_snapshot().await.unwrap().kind,
                UiNoticeKind::None
            );
        }
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert!(events.lines().any(
            |line| matches!(serde_json::from_str::<UiEvent>(line).unwrap(),
            UiEvent::Notice { snapshot } if snapshot.kind == UiNoticeKind::None)
        ));
    }

    #[tokio::test]
    async fn paused_activity_correlates_waiting_inputs_and_clears_on_resume() {
        let (environment, shell) = agent_files();
        let mut machine = crate::agent_machine::AgentMachine::new();
        machine.accept_submission("original");
        machine.accept_steering_submission("steering".into());
        paused(&environment, Some(&machine)).await.unwrap();
        heartbeat(&environment).await.unwrap();
        let activity = environment.read_ui_activity_snapshot().await.unwrap();
        assert_eq!(activity.waiting_submission_ids, ["original", "steering"]);
        let events = shell.cat("/agent/1/machine/ui/events").await.unwrap();
        let event: UiEvent = serde_json::from_slice(&events).unwrap();
        assert_eq!(event, UiEvent::Activity { snapshot: activity });
        resumed(&environment).await.unwrap();
        assert!(
            environment
                .read_ui_activity_snapshot()
                .await
                .unwrap()
                .waiting_submission_ids
                .is_empty()
        );
        paused(&environment, None).await.unwrap();
        assert!(
            environment
                .read_ui_activity_snapshot()
                .await
                .unwrap()
                .waiting_submission_ids
                .is_empty()
        );
        let legacy: UiActivitySnapshot =
            serde_json::from_str(r#"{"version":1,"state":"paused"}"#).unwrap();
        assert!(legacy.waiting_submission_ids.is_empty());
    }

    #[tokio::test]
    async fn heartbeat_preserves_paused_activity() {
        let (environment, _) = agent_files();
        initialize(&environment, false, false).await.unwrap();
        paused(&environment, None).await.unwrap();

        heartbeat(&environment).await.unwrap();

        assert_eq!(
            environment.read_ui_activity_snapshot().await.unwrap().state,
            UiActivityState::Paused
        );
    }
}
