use crate::runtime::child_agents::ChildTaskContext;
use crate::tape::Message;
use alan_agent_protocol::{PlanItem, SpawnHandle, SpawnSpec};

const MAX_CHILD_CONVERSATION_MESSAGES: usize = 8;
const MAX_CHILD_CONVERSATION_CHARS: usize = 4_000;
const MAX_CHILD_PLAN_ITEMS: usize = 16;
const MAX_CHILD_PLAN_ITEM_CHARS: usize = 240;
const MAX_CHILD_TOOL_RESULTS: usize = 6;
const MAX_CHILD_TOOL_RESULT_CHARS: usize = 1_200;

pub(crate) fn project_child_task_context(
    tape_summary: Option<&str>,
    messages: &[Message],
    plan_explanation: Option<&str>,
    plan_items: &[PlanItem],
    spec: &SpawnSpec,
) -> ChildTaskContext {
    ChildTaskContext::new(
        spec.has_handle(SpawnHandle::ConversationSnapshot)
            .then(|| render_conversation_snapshot(tape_summary, messages))
            .flatten(),
        spec.has_handle(SpawnHandle::Plan)
            .then(|| render_plan_snapshot(plan_explanation, plan_items))
            .flatten(),
        spec.has_handle(SpawnHandle::ToolResults)
            .then(|| render_tool_results_snapshot(messages))
            .flatten(),
    )
}

pub(super) fn build_child_task_text(parent: &ChildTaskContext, spec: &SpawnSpec) -> String {
    let mut sections = vec![spec.launch.task.trim().to_string()];

    if let Some(metadata) = render_launch_metadata(spec) {
        sections.push(metadata);
    }
    if let Some(snapshot) = parent.conversation_snapshot.as_ref() {
        sections.push(snapshot.clone());
    }
    if let Some(snapshot) = parent.plan_snapshot.as_ref() {
        sections.push(snapshot.clone());
    }
    if let Some(snapshot) = parent.tool_results_snapshot.as_ref() {
        sections.push(snapshot.clone());
    }

    sections
        .into_iter()
        .filter(|section| !section.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_launch_metadata(spec: &SpawnSpec) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(cwd) = spec.launch.cwd.as_ref() {
        lines.push(format!("cwd: {}", cwd.display()));
    }
    if let Some(output_dir) = spec.launch.output_dir.as_ref() {
        lines.push(format!("output_dir: {}", output_dir.display()));
    }

    (!lines.is_empty()).then(|| format!("Execution Context\n{}", lines.join("\n")))
}

fn render_conversation_snapshot(
    tape_summary: Option<&str>,
    messages: &[Message],
) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(summary) = tape_summary {
        lines.push("Summary:".to_string());
        lines.push(truncate_chars(summary.trim(), MAX_CHILD_CONVERSATION_CHARS));
    }

    let recent_messages = messages
        .iter()
        .rev()
        .filter(|message| matches!(message, Message::User { .. } | Message::Assistant { .. }))
        .take(MAX_CHILD_CONVERSATION_MESSAGES)
        .collect::<Vec<_>>();
    if !recent_messages.is_empty() {
        lines.push("Recent Messages:".to_string());
        for message in recent_messages.into_iter().rev() {
            let role = match message {
                Message::User { .. } => "user",
                Message::Assistant { .. } => "assistant",
                Message::Tool { .. } => unreachable!("tool messages are filtered out above"),
                Message::System { .. } => "system",
                Message::Context { .. } => "context",
            };
            let text = match message {
                Message::Assistant { .. } => message.non_thinking_text_content(),
                _ => message.text_content(),
            };
            if !text.trim().is_empty() {
                lines.push(format!(
                    "- {role}: {}",
                    truncate_chars(text.trim(), MAX_CHILD_CONVERSATION_CHARS / 2)
                ));
            }
        }
    }

    (!lines.is_empty()).then(|| format!("Parent Conversation Snapshot\n{}", lines.join("\n")))
}

fn render_plan_snapshot(plan_explanation: Option<&str>, plan_items: &[PlanItem]) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(explanation) = plan_explanation
        && !explanation.trim().is_empty()
    {
        lines.push(format!(
            "Explanation: {}",
            truncate_chars(explanation.trim(), MAX_CHILD_PLAN_ITEM_CHARS)
        ));
    }
    for item in plan_items.iter().take(MAX_CHILD_PLAN_ITEMS) {
        lines.push(format!(
            "- [{}] {}",
            match item.status {
                alan_agent_protocol::PlanItemStatus::Pending => "pending",
                alan_agent_protocol::PlanItemStatus::InProgress => "in_progress",
                alan_agent_protocol::PlanItemStatus::Completed => "completed",
            },
            truncate_chars(item.content.trim(), MAX_CHILD_PLAN_ITEM_CHARS)
        ));
    }

    (!lines.is_empty()).then(|| format!("Parent Plan Snapshot\n{}", lines.join("\n")))
}

fn render_tool_results_snapshot(messages: &[Message]) -> Option<String> {
    let mut lines = Vec::new();
    for message in messages
        .iter()
        .rev()
        .filter(|message| matches!(message, Message::Tool { .. }))
        .take(MAX_CHILD_TOOL_RESULTS)
    {
        for response in message.tool_responses() {
            let content =
                truncate_chars(response.text_content().trim(), MAX_CHILD_TOOL_RESULT_CHARS);
            if !content.is_empty() {
                lines.push(format!("- {}: {}", response.id, content));
            }
        }
    }
    lines.reverse();
    (!lines.is_empty()).then(|| format!("Parent Tool Results\n{}", lines.join("\n")))
}

fn truncate_chars(text: &str, limit: usize) -> String {
    let truncated: String = text.chars().take(limit).collect();
    if truncated.chars().count() == text.chars().count() {
        truncated
    } else {
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_protocol::{SpawnLaunchInputs, SpawnRuntimeOverrides, SpawnTarget};

    fn spawn_spec(handles: Vec<SpawnHandle>) -> SpawnSpec {
        SpawnSpec {
            target: SpawnTarget::DefinitionDescriptor {
                descriptor: "test-agent".to_string(),
            },
            launch: SpawnLaunchInputs {
                task: "test child task".to_string(),
                cwd: None,
                timeout_secs: None,
                output_dir: None,
            },
            handles,
            host_mounts: Vec::new(),
            runtime_overrides: SpawnRuntimeOverrides::default(),
            delegated: None,
        }
    }

    #[test]
    fn tool_result_projection_is_handle_gated_and_bounded_before_storage() {
        let messages = vec![Message::tool_text(
            "large-tool-call",
            "x".repeat(MAX_CHILD_TOOL_RESULT_CHARS * 4),
        )];
        let without_tool_results =
            project_child_task_context(None, &messages, None, &[], &spawn_spec(Vec::new()));
        assert!(without_tool_results.tool_results_snapshot.is_none());

        let with_tool_results = project_child_task_context(
            None,
            &messages,
            None,
            &[],
            &spawn_spec(vec![SpawnHandle::ToolResults]),
        );
        let snapshot = with_tool_results
            .tool_results_snapshot
            .expect("ToolResults handle should project bounded text");
        assert!(snapshot.starts_with("Parent Tool Results\n- large-tool-call: "));
        assert!(snapshot.ends_with("..."));
        assert!(
            snapshot.chars().count()
                <= "Parent Tool Results\n- large-tool-call: ".chars().count()
                    + MAX_CHILD_TOOL_RESULT_CHARS
                    + 3
        );
    }
}
