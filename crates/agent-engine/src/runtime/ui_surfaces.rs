use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alan_agent_protocol::{
    CompactionAttemptSnapshot, MemoryFlushAttemptSnapshot, UiActivitySnapshot, UiEvent,
    UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
};
use anyhow::Result;

use super::agent_loop::NamespaceRuntimeEnvironment;

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) async fn initialize(namespace: &NamespaceRuntimeEnvironment) -> Result<()> {
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

pub(crate) async fn turn_started(namespace: &NamespaceRuntimeEnvironment) -> Result<()> {
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

pub(crate) async fn turn_completed(
    namespace: &NamespaceRuntimeEnvironment,
    cancelled: bool,
) -> Result<()> {
    if cancelled {
        plan_updated(namespace, None, Vec::new()).await?;
    }
    let activity = UiActivitySnapshot::idle();
    namespace.write_ui_activity_snapshot(&activity).await?;
    namespace
        .append_ui_event(&UiEvent::Activity { snapshot: activity })
        .await
}

pub(crate) async fn paused(namespace: &NamespaceRuntimeEnvironment) -> Result<()> {
    let activity = UiActivitySnapshot::paused(None);
    namespace.write_ui_activity_snapshot(&activity).await?;
    namespace
        .append_ui_event(&UiEvent::Activity { snapshot: activity })
        .await
}

pub(crate) async fn resumed(namespace: &NamespaceRuntimeEnvironment) -> Result<()> {
    let activity = UiActivitySnapshot::running(now_unix_ms());
    namespace.write_ui_activity_snapshot(&activity).await?;
    namespace
        .append_ui_event(&UiEvent::Activity { snapshot: activity })
        .await
}

pub(crate) async fn plan_updated(
    namespace: &NamespaceRuntimeEnvironment,
    explanation: Option<String>,
    items: Vec<alan_agent_protocol::PlanItem>,
) -> Result<()> {
    let snapshot = UiPlanSnapshot::new(explanation, items);
    namespace.write_ui_plan_snapshot(&snapshot).await?;
    namespace.append_ui_event(&UiEvent::Plan { snapshot }).await
}

pub(crate) async fn rollback(namespace: &NamespaceRuntimeEnvironment, turns: u32) -> Result<()> {
    let snapshot =
        UiNoticeSnapshot::new(UiNoticeKind::Rollback, format!("rolled back {turns} turns"));
    namespace.write_ui_notice_snapshot(&snapshot).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot })
        .await
}

pub(crate) async fn thinking(namespace: &NamespaceRuntimeEnvironment, text: &str) -> Result<()> {
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
    namespace: &NamespaceRuntimeEnvironment,
    message: impl Into<String>,
) -> Result<()> {
    let snapshot = UiNoticeSnapshot::new(UiNoticeKind::Warning, message.into());
    namespace.write_ui_notice_snapshot(&snapshot).await?;
    namespace
        .append_ui_event(&UiEvent::Notice { snapshot })
        .await
}

pub(crate) async fn compaction(
    namespace: &NamespaceRuntimeEnvironment,
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
    namespace: &NamespaceRuntimeEnvironment,
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

    fn namespace_environment() -> (NamespaceRuntimeEnvironment, alan_shell::Shell) {
        let agentfs = Arc::new(AgentFs::new());
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root.clone());
        (
            NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            shell,
        )
    }

    #[tokio::test]
    async fn owners_write_snapshots_and_append_ui_events() {
        let (environment, shell) = namespace_environment();
        initialize(&environment).await.unwrap();
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
        let (environment, shell) = namespace_environment();
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
        turn_completed(&environment, true).await.unwrap();

        let plan: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/plan").await.unwrap()).unwrap();
        assert_eq!(plan["items"], Value::Array(Vec::new()));
    }
}
