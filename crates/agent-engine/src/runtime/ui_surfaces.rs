use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alan_agent_protocol::{
    CompactionAttemptSnapshot, Event, MemoryFlushAttemptSnapshot, UiActivitySnapshot,
    UiActivityState, UiEvent, UiNoticeKind, UiNoticeSnapshot, UiPlanSnapshot, UiThinkingSnapshot,
    UiThinkingState,
};
use anyhow::Result;

use super::agent_loop::NamespaceRuntimeEnvironment;

pub(crate) struct RuntimeUiProjector {
    namespace: NamespaceRuntimeEnvironment,
    activity: UiActivitySnapshot,
    plan: UiPlanSnapshot,
    thinking: UiThinkingSnapshot,
    notice: UiNoticeSnapshot,
    thinking_started: Option<Instant>,
}

impl RuntimeUiProjector {
    pub(crate) async fn initialize(namespace: NamespaceRuntimeEnvironment) -> Result<Self> {
        let projector = Self {
            namespace,
            activity: UiActivitySnapshot::idle(),
            plan: UiPlanSnapshot::empty(),
            thinking: UiThinkingSnapshot::idle(),
            notice: UiNoticeSnapshot::none(),
            thinking_started: None,
        };
        projector.sync_snapshots().await?;
        Ok(projector)
    }

    async fn sync_snapshots(&self) -> Result<()> {
        self.namespace
            .write_ui_activity_snapshot(&self.activity)
            .await?;
        self.namespace.write_ui_plan_snapshot(&self.plan).await?;
        self.namespace
            .write_ui_thinking_snapshot(&self.thinking)
            .await?;
        self.namespace
            .write_ui_notice_snapshot(&self.notice)
            .await?;
        Ok(())
    }

    pub(crate) async fn apply_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::TurnStarted {} => {
                let started_at_ms = now_unix_ms();
                self.publish_activity(UiActivitySnapshot::running(started_at_ms))
                    .await?;
                self.thinking_started = None;
                self.publish_thinking(UiThinkingSnapshot::idle()).await?;
                self.publish_notice(UiNoticeSnapshot::none()).await?;
            }
            Event::TurnCompleted { summary } => {
                if matches!(self.thinking.state, UiThinkingState::Streaming)
                    && !self.thinking.text.trim().is_empty()
                {
                    self.finalize_thinking().await?;
                }
                if summary.as_deref() == Some("Task cancelled by user") {
                    self.publish_plan(UiPlanSnapshot::empty()).await?;
                }
                self.publish_activity(UiActivitySnapshot::idle()).await?;
            }
            Event::TextDelta { .. }
            | Event::ToolCallStarted { .. }
            | Event::ToolCallCompleted { .. } => {
                self.resume_activity_if_paused().await?;
            }
            Event::ThinkingDelta { chunk, is_final } => {
                self.resume_activity_if_paused().await?;
                if !matches!(self.thinking.state, UiThinkingState::Streaming) {
                    self.thinking_started = Some(Instant::now());
                    self.thinking = UiThinkingSnapshot::streaming(String::new());
                }
                self.thinking.text.push_str(chunk);
                if *is_final {
                    self.finalize_thinking().await?;
                } else {
                    self.publish_thinking(self.thinking.clone()).await?;
                }
            }
            Event::PlanUpdated { explanation, items } => {
                self.resume_activity_if_paused().await?;
                self.publish_plan(UiPlanSnapshot::new(explanation.clone(), items.clone()))
                    .await?;
            }
            Event::MachineRolledBack { turns, .. } => {
                self.publish_plan(UiPlanSnapshot::empty()).await?;
                self.publish_notice(UiNoticeSnapshot::new(
                    UiNoticeKind::Rollback,
                    format!("rolled back {turns} turns"),
                ))
                .await?;
            }
            Event::Yield { .. } => {
                if !matches!(self.activity.state, UiActivityState::Idle) {
                    self.publish_activity(UiActivitySnapshot::paused(self.activity.started_at_ms))
                        .await?;
                }
            }
            Event::CompactionObserved { attempt } => {
                self.publish_notice(UiNoticeSnapshot::new(
                    UiNoticeKind::Compaction,
                    compaction_notice_message(attempt),
                ))
                .await?;
            }
            Event::MemoryFlushObserved { attempt } => {
                self.publish_notice(UiNoticeSnapshot::new(
                    UiNoticeKind::MemoryFlush,
                    memory_flush_notice_message(attempt),
                ))
                .await?;
            }
            Event::Warning { message } => {
                self.publish_notice(UiNoticeSnapshot::new(
                    UiNoticeKind::Warning,
                    message.clone(),
                ))
                .await?;
            }
            Event::Error {
                message,
                recoverable,
            } => {
                if *recoverable {
                    self.publish_notice(UiNoticeSnapshot::new(
                        UiNoticeKind::Warning,
                        message.clone(),
                    ))
                    .await?;
                }
                self.namespace
                    .append_ui_event(&UiEvent::Error {
                        message: message.clone(),
                        recoverable: *recoverable,
                    })
                    .await?;
            }
        }
        Ok(())
    }

    async fn finalize_thinking(&mut self) -> Result<()> {
        let duration_secs = self
            .thinking_started
            .take()
            .map(|started| started.elapsed().as_secs())
            .unwrap_or(0);
        let snapshot = UiThinkingSnapshot::complete(self.thinking.text.clone(), duration_secs);
        self.publish_thinking(snapshot).await
    }

    async fn resume_activity_if_paused(&mut self) -> Result<()> {
        if matches!(self.activity.state, UiActivityState::Paused) {
            let started_at_ms = self.activity.started_at_ms.unwrap_or_else(now_unix_ms);
            self.publish_activity(UiActivitySnapshot::running(started_at_ms))
                .await?;
        }
        Ok(())
    }

    async fn publish_activity(&mut self, snapshot: UiActivitySnapshot) -> Result<()> {
        if self.activity != snapshot {
            self.namespace.write_ui_activity_snapshot(&snapshot).await?;
            self.activity = snapshot.clone();
        }
        self.namespace
            .append_ui_event(&UiEvent::Activity { snapshot })
            .await
    }

    async fn publish_plan(&mut self, snapshot: UiPlanSnapshot) -> Result<()> {
        if self.plan != snapshot {
            self.namespace.write_ui_plan_snapshot(&snapshot).await?;
            self.plan = snapshot.clone();
        }
        self.namespace
            .append_ui_event(&UiEvent::Plan { snapshot })
            .await
    }

    async fn publish_thinking(&mut self, snapshot: UiThinkingSnapshot) -> Result<()> {
        if self.thinking != snapshot {
            self.namespace.write_ui_thinking_snapshot(&snapshot).await?;
            self.thinking = snapshot.clone();
        }
        self.namespace
            .append_ui_event(&UiEvent::Thinking { snapshot })
            .await
    }

    async fn publish_notice(&mut self, snapshot: UiNoticeSnapshot) -> Result<()> {
        if self.notice != snapshot {
            self.namespace.write_ui_notice_snapshot(&snapshot).await?;
            self.notice = snapshot.clone();
        }
        self.namespace
            .append_ui_event(&UiEvent::Notice { snapshot })
            .await
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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

    use alan_agent_protocol::{Event, PlanItem, PlanItemStatus, UiEvent, UiThinkingState};
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
    async fn projector_initializes_snapshots_and_appends_ui_events() {
        let (environment, shell) = namespace_environment();
        let mut projector = RuntimeUiProjector::initialize(environment).await.unwrap();

        projector.apply_event(&Event::TurnStarted {}).await.unwrap();
        projector
            .apply_event(&Event::ThinkingDelta {
                chunk: "reason".to_string(),
                is_final: false,
            })
            .await
            .unwrap();
        projector
            .apply_event(&Event::ThinkingDelta {
                chunk: "ing".to_string(),
                is_final: true,
            })
            .await
            .unwrap();
        projector
            .apply_event(&Event::PlanUpdated {
                explanation: Some("ship parity".to_string()),
                items: vec![PlanItem {
                    id: "1".to_string(),
                    content: "wire ui files".to_string(),
                    status: PlanItemStatus::InProgress,
                }],
            })
            .await
            .unwrap();
        projector
            .apply_event(&Event::Warning {
                message: "retrying".to_string(),
            })
            .await
            .unwrap();
        projector
            .apply_event(&Event::TurnCompleted { summary: None })
            .await
            .unwrap();

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
        let mut projector = RuntimeUiProjector::initialize(environment).await.unwrap();

        projector
            .apply_event(&Event::PlanUpdated {
                explanation: Some("ship parity".to_string()),
                items: vec![PlanItem {
                    id: "1".to_string(),
                    content: "wire ui files".to_string(),
                    status: PlanItemStatus::InProgress,
                }],
            })
            .await
            .unwrap();
        projector
            .apply_event(&Event::TurnCompleted {
                summary: Some("Task cancelled by user".to_string()),
            })
            .await
            .unwrap();

        let plan: Value =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/plan").await.unwrap()).unwrap();
        assert_eq!(plan["items"], Value::Array(Vec::new()));
    }
}
