use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Utc};
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::agent_machine::AgentMachine;
use crate::tape::Message;

use super::agent_loop::RuntimeLoopState;
use super::turn_state::TurnState;

const MAX_INLINE_TEXT_CHARS: usize = 280;
const MAX_RECENT_MESSAGE_ITEMS: usize = 6;
const MAX_PLAN_ITEMS_PER_SECTION: usize = 6;
const MAX_COMPACTION_SUMMARY_CHARS: usize = 1_000;
const MIN_TRUNCATED_MEMORY_BODY_CHARS: usize = 24;
const MEMORY_TRUNCATION_MARKER_PREFIX: &str = "\n[... truncated; inspect ";
const MEMORY_TRUNCATION_MARKER_SUFFIX: &str = " for full text]";
const CODE_FENCE_CLOSE: &str = "\n```";

#[derive(Debug, Clone)]
struct RenderedMemorySurfaces {
    working_memory: String,
    handoff: String,
    episodic_record: String,
    daily_entry: String,
}

pub(crate) async fn refresh_turn_memory_surfaces(state: &RuntimeLoopState) -> Result<()> {
    if !state.core_config.memory.enabled {
        return Ok(());
    }

    let Some(memory_dir) = state.core_config.memory.store_dir.as_deref() else {
        return Ok(());
    };

    crate::prompts::ensure_memory_store_layout_at(memory_dir)
        .with_context(|| format!("failed to ensure memory layout at {}", memory_dir.display()))?;

    let now = Utc::now();
    let process_path = state.process_path();
    let memory_record_id = state.machine.memory_record_id();
    let rendered = render_memory_surfaces(
        &state.machine,
        &state.turn_state,
        &process_path,
        memory_record_id,
        now,
    );

    write_text_file(
        &working_memory_path(memory_dir, memory_record_id),
        &rendered.working_memory,
    )
    .await?;
    write_text_file(&latest_handoff_path(memory_dir), &rendered.handoff).await?;
    write_text_file(
        &episodic_record_path(memory_dir, memory_record_id, now),
        &rendered.episodic_record,
    )
    .await?;
    append_text_file(&daily_note_path(memory_dir, now), &rendered.daily_entry).await?;

    Ok(())
}

pub(crate) async fn refresh_turn_memory_surfaces_best_effort(
    state: &RuntimeLoopState,
    context: &'static str,
) {
    if let Err(err) = refresh_turn_memory_surfaces(state).await {
        warn!(error = %err, context, "Failed to refresh memory surfaces");
    }
}

pub(crate) async fn refresh_active_turn_memory_surfaces_best_effort(
    state: &RuntimeLoopState,
    context: &'static str,
) {
    if state.turn_state.active_turn_message_start().is_none() {
        return;
    }

    refresh_turn_memory_surfaces_best_effort(state, context).await;
}

fn render_memory_surfaces(
    machine: &AgentMachine,
    turn_state: &TurnState,
    process_path: &str,
    memory_record_id: &str,
    now: DateTime<Utc>,
) -> RenderedMemorySurfaces {
    let current_goal = derive_current_goal(machine, turn_state);
    let latest_assistant_state = derive_latest_assistant_state(machine, turn_state);
    let active_plan_items = render_plan_items(turn_state, &["in_progress", "pending"]);
    let completed_plan_items = render_plan_items(turn_state, &["completed"]);
    let recent_messages = render_recent_messages(machine);
    let compaction_summary = render_compaction_summary(machine);
    let latest_memory_flush = render_latest_memory_flush(machine);
    let updated_at = now.to_rfc3339();

    let working_memory = format!(
        "# Working Memory\n\nprocess_path: {process_path}\nmemory_record_id: {memory_record_id}\nupdated_at: {updated_at}\n\n## Current Goal\n{current_goal}\n\n## Active Subgoals\n{active_plan_items}\n\n## Confirmed Constraints\n{compaction_summary}\n\n## Pending Verification\n{active_plan_items}\n\n## Open Loops\n{active_plan_items}\n\n## Recent Findings\n- Latest assistant state: {latest_assistant_state}\n{recent_messages}\n\n## Active Recall\n{latest_memory_flush}\n"
    );

    let handoff = format!(
        "# Latest Handoff\n\nupdated_at: {updated_at}\nprocess_path: {process_path}\nmemory_record_id: {memory_record_id}\n\n## Current Goal\n{current_goal}\n\n## What Just Happened\n- {latest_assistant_state}\n\n## Next Steps\n{active_plan_items}\n\n## Recent Context\n{compaction_summary}\n{recent_messages}\n"
    );

    let episodic_record = format!(
        "# Agent Process Activity\n\nprocess_path: {process_path}\nmemory_record_id: {memory_record_id}\nupdated_at: {updated_at}\n\n## Current Goal\n{current_goal}\n\n## Latest Assistant State\n- {latest_assistant_state}\n\n## Active Plan\n{active_plan_items}\n\n## Completed Plan Items\n{completed_plan_items}\n\n## Prior Compaction Summary\n{compaction_summary}\n\n## Recent Activity\n{recent_messages}\n\n## Latest Memory Flush\n{latest_memory_flush}\n"
    );

    let daily_entry = format!(
        "## {updated_at}\n\nprocess_path: {process_path}\nmemory_record_id: {memory_record_id}\n\n### Current Goal\n{current_goal}\n\n### Latest Assistant State\n- {latest_assistant_state}\n\n### Next Steps\n{active_plan_items}\n\n### Latest Memory Flush\n{latest_memory_flush}\n\n"
    );

    RenderedMemorySurfaces {
        working_memory,
        handoff,
        episodic_record,
        daily_entry,
    }
}

fn derive_current_goal(machine: &AgentMachine, turn_state: &TurnState) -> String {
    let source_ref = memory_source_ref(machine);
    let user_messages = machine
        .tape
        .messages()
        .iter()
        .enumerate()
        .filter(|(_, message)| message.is_user() && !message.is_internal_control())
        .filter_map(|(index, message)| {
            let text = message.text_content();
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| (index, trimmed.to_string()))
        })
        .collect::<Vec<_>>();
    let latest_user = user_messages.last();
    let latest_is_substantive = latest_user
        .map(|(_, text)| is_substantive_goal_message(text))
        .unwrap_or(false);
    let latest_is_current_turn = latest_user
        .zip(turn_state.active_turn_message_start())
        .is_some_and(|((index, _), start)| *index >= start);

    if turn_state.plan_snapshot_is_from_active_turn()
        && let Some(plan_goal) = derive_active_plan_goal(turn_state)
    {
        return plan_goal;
    }

    if latest_is_substantive && latest_is_current_turn {
        return truncate_memory_text(
            latest_user.unwrap().1.as_str(),
            MAX_INLINE_TEXT_CHARS,
            &source_ref,
        );
    }

    if let Some((index, substantive)) = user_messages
        .iter()
        .rev()
        .find(|(_, text)| is_substantive_goal_message(text))
    {
        if turn_state.plan_snapshot_postdates_message(*index)
            && let Some(plan_goal) = derive_active_plan_goal(turn_state)
        {
            return mark_carried_goal_if_needed(plan_goal, latest_user, latest_is_substantive);
        }
        let goal = truncate_memory_text(substantive, MAX_INLINE_TEXT_CHARS, &source_ref);
        return mark_carried_goal_if_needed(goal, latest_user, latest_is_substantive);
    }

    if let Some(plan_goal) = derive_active_plan_goal(turn_state) {
        return mark_carried_goal_if_needed(plan_goal, latest_user, latest_is_substantive);
    }

    if let Some(summary) = machine
        .tape
        .summary()
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
    {
        let goal = truncate_memory_text(summary, MAX_INLINE_TEXT_CHARS, &source_ref);
        return mark_carried_goal_if_needed(goal, latest_user, latest_is_substantive);
    }

    latest_user
        .map(|(_, text)| truncate_memory_text(text, MAX_INLINE_TEXT_CHARS, &source_ref))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "No current goal recorded.".to_string())
}

fn derive_active_plan_goal(turn_state: &TurnState) -> Option<String> {
    let snapshot = turn_state.plan_snapshot()?;
    snapshot
        .explanation
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            snapshot
                .items
                .iter()
                .find(|item| matches!(item.status, alan_agent_protocol::PlanItemStatus::InProgress))
                .or_else(|| {
                    snapshot.items.iter().find(|item| {
                        matches!(item.status, alan_agent_protocol::PlanItemStatus::Pending)
                    })
                })
                .map(|item| item.content.trim())
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
}

fn mark_carried_goal_if_needed(
    goal: String,
    latest_user: Option<&(usize, String)>,
    latest_is_substantive: bool,
) -> String {
    if latest_user.is_some() && !latest_is_substantive {
        format!("[carried forward] {goal}")
    } else {
        goal
    }
}

fn is_substantive_goal_message(text: &str) -> bool {
    !is_acknowledgement_class_fragment(text)
}

fn is_acknowledgement_class_fragment(text: &str) -> bool {
    let without_emoji_modifiers = text
        .chars()
        .filter(|character| !matches!(*character, '\u{fe0f}' | '\u{1f3fb}'..='\u{1f3ff}'))
        .collect::<String>();
    let normalized = without_emoji_modifiers
        .trim()
        .trim_matches(|character: char| character.is_ascii_punctuation())
        .trim()
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }
    if normalized.chars().count() == 1
        && normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return true;
    }
    matches!(
        normalized.as_str(),
        "ok" | "okay"
            | "yes"
            | "yep"
            | "yeah"
            | "sure"
            | "thanks"
            | "thank you"
            | "got it"
            | "sounds good"
            | "👍"
            | "👌"
    ) || normalized.split_whitespace().count() > 1
        && normalized.split_whitespace().all(|token| {
            let token = token.trim_matches(|character: char| character.is_ascii_punctuation());
            matches!(
                token,
                "ok" | "okay" | "yes" | "yep" | "yeah" | "sure" | "thanks" | "👍" | "👌"
            )
        })
}

fn derive_latest_assistant_state(machine: &AgentMachine, turn_state: &TurnState) -> String {
    let source_ref = memory_source_ref(machine);
    let messages = turn_state
        .active_turn_message_start()
        .and_then(|start| machine.tape.messages().get(start..))
        .unwrap_or_else(|| machine.tape.messages());

    messages
        .iter()
        .rev()
        .find(|message| message.is_assistant())
        .map(Message::non_thinking_text_content)
        .map(|text| truncate_memory_text(text.trim(), MAX_INLINE_TEXT_CHARS, &source_ref))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if turn_state.active_turn_message_start().is_some() {
                "This turn completed without a new assistant response.".to_string()
            } else {
                "No assistant response recorded yet.".to_string()
            }
        })
}

fn render_plan_items(turn_state: &TurnState, statuses: &[&str]) -> String {
    let Some(snapshot) = turn_state.plan_snapshot() else {
        return "- None recorded.\n".to_string();
    };

    let items: Vec<String> = snapshot
        .items
        .iter()
        .filter(|item| {
            let status = match &item.status {
                alan_agent_protocol::PlanItemStatus::Pending => "pending",
                alan_agent_protocol::PlanItemStatus::InProgress => "in_progress",
                alan_agent_protocol::PlanItemStatus::Completed => "completed",
            };
            statuses.contains(&status)
        })
        .take(MAX_PLAN_ITEMS_PER_SECTION)
        .map(|item| {
            format!(
                "- [{}] {}",
                format_plan_status(&item.status),
                item.content.trim()
            )
        })
        .collect();

    if items.is_empty() {
        "- None recorded.\n".to_string()
    } else {
        format!("{}\n", items.join("\n"))
    }
}

fn format_plan_status(status: &alan_agent_protocol::PlanItemStatus) -> &'static str {
    match status {
        alan_agent_protocol::PlanItemStatus::Pending => "pending",
        alan_agent_protocol::PlanItemStatus::InProgress => "in_progress",
        alan_agent_protocol::PlanItemStatus::Completed => "completed",
    }
}

fn render_recent_messages(machine: &AgentMachine) -> String {
    let source_ref = memory_source_ref(machine);
    let items: Vec<String> = machine
        .tape
        .messages()
        .iter()
        .filter_map(|message| match message {
            Message::User { .. } => Some(("user", message.text_content())),
            Message::Assistant { .. } => Some(("assistant", message.non_thinking_text_content())),
            Message::Tool { .. } => Some(("tool", message.text_content())),
            Message::System { .. } | Message::Context { .. } => None,
        })
        .filter_map(|(role, text)| {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| {
                format!(
                    "- {}: {}",
                    role,
                    truncate_memory_text(trimmed, MAX_INLINE_TEXT_CHARS, &source_ref)
                )
            })
        })
        .rev()
        .take(MAX_RECENT_MESSAGE_ITEMS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if items.is_empty() {
        "- No recent conversation highlights recorded.\n".to_string()
    } else {
        format!("{}\n", items.join("\n"))
    }
}

fn render_compaction_summary(machine: &AgentMachine) -> String {
    let source_ref = memory_source_ref(machine);
    machine
        .tape
        .summary()
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .map(|summary| truncate_memory_text(summary, MAX_COMPACTION_SUMMARY_CHARS, &source_ref))
        .unwrap_or_else(|| "No compaction summary recorded.".to_string())
}

fn render_latest_memory_flush(machine: &AgentMachine) -> String {
    machine
        .latest_memory_flush_attempt()
        .map(|attempt| {
            let output_path = attempt
                .output_path
                .as_deref()
                .unwrap_or("<no-output-path-recorded>");
            format!(
                "- {} flush at {} -> {}",
                format!("{:?}", attempt.result).to_lowercase(),
                attempt.timestamp,
                output_path
            )
        })
        .unwrap_or_else(|| "- No memory flush attempt recorded.\n".to_string())
}

fn working_memory_path(memory_dir: &Path, memory_record_id: &str) -> PathBuf {
    let key = crate::process_storage_key(memory_record_id);
    memory_dir.join("working").join(format!("process-{key}.md"))
}

fn latest_handoff_path(memory_dir: &Path) -> PathBuf {
    memory_dir.join("handoffs").join("LATEST.md")
}

fn episodic_record_path(memory_dir: &Path, memory_record_id: &str, now: DateTime<Utc>) -> PathBuf {
    let key = crate::process_storage_key(memory_record_id);
    memory_dir.join("episodic").join(format!(
        "{:04}/{:02}/{:02}/process-{}.md",
        now.year(),
        now.month(),
        now.day(),
        key
    ))
}

fn daily_note_path(memory_dir: &Path, now: DateTime<Utc>) -> PathBuf {
    memory_dir.join("daily").join(format!(
        "{:04}-{:02}-{:02}.md",
        now.year(),
        now.month(),
        now.day()
    ))
}

async fn write_text_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }
    tokio::fs::write(path, content)
        .await
        .with_context(|| format!("failed to write memory surface {}", path.display()))?;
    Ok(())
}

async fn append_text_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("failed to open memory surface {}", path.display()))?;
    file.write_all(content.as_bytes())
        .await
        .with_context(|| format!("failed to append memory surface {}", path.display()))?;
    Ok(())
}

fn memory_source_ref(machine: &AgentMachine) -> String {
    machine
        .rollout_path()
        .map(|path| format!("rollout {}", path.display()))
        .unwrap_or_else(|| "current Agent Machine".to_string())
}

fn truncate_memory_text(text: &str, max_chars: usize, source_ref: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }

    let marker_budget = max_chars.saturating_sub(MIN_TRUNCATED_MEMORY_BODY_CHARS);
    let marker = bounded_memory_truncation_marker(source_ref, marker_budget);
    let budget = max_chars.saturating_sub(marker.chars().count());
    let mut rendered = String::new();
    let mut used = 0usize;
    let mut in_code_fence = false;
    let code_fence_close_chars = CODE_FENCE_CLOSE.chars().count();

    for line in text.lines() {
        let line_chars = line.chars().count();
        let separator_chars = usize::from(!rendered.is_empty());
        let toggles_code_fence = line.trim_start().starts_with("```");
        let would_be_in_code_fence = if toggles_code_fence {
            !in_code_fence
        } else {
            in_code_fence
        };
        let close_budget = if would_be_in_code_fence {
            code_fence_close_chars
        } else {
            0
        };
        if used + separator_chars + line_chars + close_budget > budget {
            break;
        }
        if !rendered.is_empty() {
            rendered.push('\n');
            used += 1;
        }
        rendered.push_str(line);
        used += line_chars;
        if toggles_code_fence {
            in_code_fence = !in_code_fence;
        }
    }

    if rendered.trim().is_empty() {
        rendered = text.chars().take(budget).collect::<String>();
        used = rendered.chars().count();
    }
    if in_code_fence && used + code_fence_close_chars <= budget {
        rendered.push_str(CODE_FENCE_CLOSE);
    }
    rendered.push_str(&marker);
    rendered
}

fn bounded_memory_truncation_marker(source_ref: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let marker =
        format!("{MEMORY_TRUNCATION_MARKER_PREFIX}{source_ref}{MEMORY_TRUNCATION_MARKER_SUFFIX}");
    if marker.chars().count() <= max_chars {
        return marker;
    }

    let fixed_marker_chars = MEMORY_TRUNCATION_MARKER_PREFIX.chars().count()
        + MEMORY_TRUNCATION_MARKER_SUFFIX.chars().count();
    if max_chars > fixed_marker_chars {
        let source_budget = max_chars - fixed_marker_chars;
        let source_ref = truncate_text_with_suffix(source_ref, source_budget, "...");
        let marker = format!(
            "{MEMORY_TRUNCATION_MARKER_PREFIX}{source_ref}{MEMORY_TRUNCATION_MARKER_SUFFIX}"
        );
        if marker.chars().count() <= max_chars {
            return marker;
        }
    }

    truncate_text_with_suffix(&marker, max_chars, "...")
}

fn truncate_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }

    let suffix_chars = suffix.chars().count();
    if suffix_chars >= max_chars {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_chars))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_machine::AgentMachine;
    use crate::runtime::turn_state::TurnState;
    use std::sync::Arc;

    fn namespace_environment_for_test() -> crate::runtime::NamespaceRuntimeEnvironment {
        let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
            alan_kernel::Namespace::new(),
        )));
        crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
    }

    #[test]
    fn render_memory_surfaces_follow_pure_text_layout_and_content() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Finish the pure-text memory slice.");
        machine.add_assistant_message("Added scaffolding and prompt bootstrap.", None);

        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot(
            Some("Finish the pure-text memory slice.".to_string()),
            vec![
                alan_agent_protocol::PlanItem {
                    id: "p1".to_string(),
                    content: "Write the scaffolding".to_string(),
                    status: alan_agent_protocol::PlanItemStatus::Completed,
                },
                alan_agent_protocol::PlanItem {
                    id: "p2".to_string(),
                    content: "Refresh the handoff".to_string(),
                    status: alan_agent_protocol::PlanItemStatus::InProgress,
                },
            ],
        );

        let now = DateTime::parse_from_rfc3339("2026-04-15T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rendered = render_memory_surfaces(
            &machine,
            &turn_state,
            "/proc/1",
            machine.memory_record_id(),
            now,
        );

        assert!(rendered.working_memory.contains("# Working Memory"));
        assert!(rendered.handoff.contains("# Latest Handoff"));
        assert!(
            rendered
                .episodic_record
                .contains("# Agent Process Activity")
        );
        assert!(rendered.working_memory.contains("process_path: /proc/1"));
        assert!(
            rendered
                .working_memory
                .contains(&format!("memory_record_id: {}", machine.memory_record_id()))
        );
        assert!(
            rendered
                .daily_entry
                .contains("## 2026-04-15T15:30:00+00:00")
        );
        assert!(
            rendered
                .episodic_record
                .contains("Finish the pure-text memory slice.")
        );
        assert!(
            rendered
                .episodic_record
                .contains("[in_progress] Refresh the handoff")
        );
        assert!(
            rendered
                .episodic_record
                .contains("[completed] Write the scaffolding")
        );
    }

    #[test]
    fn render_memory_surfaces_scopes_latest_assistant_state_to_active_turn() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Earlier task");
        machine.add_assistant_message("Earlier assistant response.", None);

        let mut turn_state = TurnState::default();
        turn_state.begin_turn(machine.tape.messages().len());
        machine.add_user_message("Current tool-only turn");

        let now = DateTime::parse_from_rfc3339("2026-04-15T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rendered = render_memory_surfaces(
            &machine,
            &turn_state,
            "/proc/1",
            machine.memory_record_id(),
            now,
        );

        assert!(
            rendered
                .handoff
                .contains("This turn completed without a new assistant response.")
        );
        assert!(rendered.handoff.contains(
            "## What Just Happened\n- This turn completed without a new assistant response."
        ));
    }

    #[test]
    fn one_letter_follow_up_carries_prior_substantive_goal() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Implement namespace-backed child lifecycle reconciliation.");
        machine.add_assistant_message("Ready for confirmation.", None);
        machine.add_user_message("y");

        let goal = derive_current_goal(&machine, &TurnState::default());

        assert_eq!(
            goal,
            "[carried forward] Implement namespace-backed child lifecycle reconciliation."
        );
    }

    #[test]
    fn request_response_control_message_does_not_replace_goal() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Remove the obsolete compatibility endpoints.");
        machine.add_user_control_message_parts(vec![crate::tape::ContentPart::structured(
            serde_json::json!({
                "checkpoint_id": "tool_escalation_call-1",
                "checkpoint_type": "tool_escalation",
                "choice": "approve",
                "__alan_internal_control": {
                    "kind": "tool_escalation_confirmation",
                    "version": 1,
                    "source": "runtime/submission_handlers"
                }
            }),
        )]);

        assert_eq!(
            derive_current_goal(&machine, &TurnState::default()),
            "Remove the obsolete compatibility endpoints."
        );
    }

    #[test]
    fn new_substantive_turn_request_replaces_stale_plan_goal() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Finish the old memory task.");
        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot(Some("Finish the old memory task.".to_string()), Vec::new());
        turn_state.begin_turn(machine.tape.messages().len());
        machine.add_user_message("Rewrite the provider connection documentation.");

        assert_eq!(
            derive_current_goal(&machine, &turn_state),
            "Rewrite the provider connection documentation."
        );
    }

    #[test]
    fn in_turn_plan_update_refines_the_initial_user_goal() {
        let mut machine = AgentMachine::new();
        let mut turn_state = TurnState::default();
        turn_state.begin_turn(machine.tape.messages().len());
        machine.add_user_message("Implement the memory contract changes.");
        turn_state.set_plan_snapshot(
            Some("Validate salience and compaction fallback behavior.".to_string()),
            Vec::new(),
        );

        assert_eq!(
            derive_current_goal(&machine, &turn_state),
            "Validate salience and compaction fallback behavior."
        );
    }

    #[test]
    fn substantive_resume_input_overrides_earlier_active_plan() {
        let mut machine = AgentMachine::new();
        let mut turn_state = TurnState::default();
        turn_state.begin_turn(machine.tape.messages().len());
        machine.add_user_message("Implement the old memory contract.");
        turn_state.set_plan_snapshot(
            Some("Finish the old memory contract.".to_string()),
            Vec::new(),
        );
        turn_state.note_resumed_user_input();
        machine.add_user_message("Switch to the provider connection contract.");

        assert_eq!(
            derive_current_goal(&machine, &turn_state),
            "Switch to the provider connection contract."
        );
    }

    #[test]
    fn terse_imperative_passes_salience_filter() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Prepare the release.");
        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot(Some("Prepare the release.".to_string()), Vec::new());
        turn_state.begin_turn(machine.tape.messages().len());
        machine.add_user_message("deploy it");

        assert_eq!(derive_current_goal(&machine, &turn_state), "deploy it");
    }

    #[test]
    fn active_plan_goal_wins_when_latest_message_is_acknowledgement() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Complete the broader migration.");
        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot_at_message_count(
            Some("Validate the namespace-native migration.".to_string()),
            Vec::new(),
            machine.tape.messages().len(),
        );
        turn_state.begin_turn(machine.tape.messages().len());
        machine.add_user_message("ok");

        assert_eq!(
            derive_current_goal(&machine, &turn_state),
            "[carried forward] Validate the namespace-native migration."
        );
    }

    #[test]
    fn later_substantive_goal_wins_before_stale_plan_on_acknowledgement() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("Finish task A.");
        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot_at_message_count(
            Some("Complete task A plan.".to_string()),
            Vec::new(),
            machine.tape.messages().len(),
        );
        machine.add_assistant_message("Task A paused.", None);
        machine.add_user_message("Switch to substantive task B.");
        machine.add_assistant_message("Task B underway.", None);
        machine.add_user_message("ok");

        assert_eq!(
            derive_current_goal(&machine, &turn_state),
            "[carried forward] Switch to substantive task B."
        );
    }

    #[test]
    fn compaction_summary_wins_before_acknowledgement_fallback() {
        let mut machine = AgentMachine::new();
        machine.tape.set_summary(
            "Complete the namespace-native lifecycle migration and verify parent visibility."
                .to_string(),
        );
        machine.add_user_message("ok");

        assert_eq!(
            derive_current_goal(&machine, &TurnState::default()),
            "[carried forward] Complete the namespace-native lifecycle migration and verify parent visibility."
        );
    }

    #[test]
    fn acknowledgement_token_sequences_and_emoji_modifiers_do_not_replace_goal() {
        for acknowledgement in ["ok thanks", "ok 👍", "👍🏻", "okay, thanks!"] {
            let mut machine = AgentMachine::new();
            machine.add_user_message("Archive the completed Alan OS contract changes.");
            machine.add_user_message(acknowledgement);

            assert_eq!(
                derive_current_goal(&machine, &TurnState::default()),
                "[carried forward] Archive the completed Alan OS contract changes.",
                "acknowledgement {acknowledgement:?} must not become the goal"
            );
        }
    }

    #[test]
    fn active_plan_goal_prefers_in_progress_before_pending_order() {
        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot(
            None,
            vec![
                alan_agent_protocol::PlanItem {
                    id: "future".to_string(),
                    content: "Archive the next contract.".to_string(),
                    status: alan_agent_protocol::PlanItemStatus::Pending,
                },
                alan_agent_protocol::PlanItem {
                    id: "current".to_string(),
                    content: "Verify the current contract.".to_string(),
                    status: alan_agent_protocol::PlanItemStatus::InProgress,
                },
            ],
        );

        assert_eq!(
            derive_active_plan_goal(&turn_state).as_deref(),
            Some("Verify the current contract.")
        );
    }

    #[test]
    fn acknowledgement_is_used_verbatim_when_no_better_context_exists() {
        let mut machine = AgentMachine::new();
        machine.add_user_message("ok");

        assert_eq!(derive_current_goal(&machine, &TurnState::default()), "ok");
    }

    #[test]
    fn truncate_memory_text_keeps_markdown_lines_and_marks_source() {
        let text = "### Top-level directories\n- crates/agent-engine has the runtime code\n- crates/tui has the terminal UI\n- docs/spec has contracts\n";

        let truncated = truncate_memory_text(text, 96, "rollout /tmp/rollout.jsonl");

        assert!(truncated.chars().count() <= 96);
        assert!(truncated.contains("### Top-level directories"));
        assert!(!truncated.contains("- c..."));
        assert!(truncated.contains("truncated"));
        assert!(truncated.contains("rollout /tmp/rollout.jsonl"));
    }

    #[test]
    fn truncate_memory_text_closes_code_fence_when_omitting_detail() {
        let text = "```rust\nfn main() {\n    println!(\"important detail\");\n    println!(\"more detail that exceeds the memory surface budget\");\n}\n```\n\n## Follow-up\n- keep going\n";

        let truncated = truncate_memory_text(text, 120, "machine sess-code");

        assert!(truncated.chars().count() <= 120);
        assert!(truncated.contains("```rust"));
        assert!(truncated.matches("```").count() >= 2);
        assert!(truncated.contains("truncated"));
        assert!(truncated.contains("machine sess-code"));
    }

    #[test]
    fn truncate_memory_text_bounds_long_source_ref_marker() {
        let text = "Important memory detail. ".repeat(80);
        let source_ref = format!("rollout /{}", "deep/path/segment/".repeat(80));

        let truncated = truncate_memory_text(&text, 120, &source_ref);

        assert!(truncated.chars().count() <= 120);
        assert!(truncated.contains("truncated"));
        assert!(!truncated.contains(&source_ref));
    }

    #[test]
    fn truncate_memory_text_respects_tiny_budget() {
        let text = "Important memory detail. ".repeat(10);
        let source_ref = format!("rollout /{}", "deep/path/segment/".repeat(20));

        let truncated = truncate_memory_text(&text, 12, &source_ref);

        assert!(truncated.chars().count() <= 12);
    }

    #[tokio::test]
    async fn refresh_turn_memory_surfaces_writes_expected_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();

        let mut machine = AgentMachine::new();
        machine.add_user_message("Keep the latest handoff fresh.");
        machine.add_assistant_message("Wrote the memory surfaces.", None);

        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot(
            Some("Keep the latest handoff fresh.".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "p1".to_string(),
                content: "Verify the memory files".to_string(),
                status: alan_agent_protocol::PlanItemStatus::Pending,
            }],
        );

        let state = RuntimeLoopState {
            machine,
            current_submission_id: None,
            environment: namespace_environment_for_test(),
            core_config: {
                let mut config = crate::Config::default();
                config.memory.store_dir = Some(memory_dir.clone());
                config
            },
            runtime_config: super::super::RuntimeConfig::default(),
            definition_persona_dirs: Vec::new(),
            prompt_cache: super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state,
        };

        refresh_turn_memory_surfaces(&state).await.unwrap();

        assert!(working_memory_path(&memory_dir, state.machine.memory_record_id()).exists());
        assert!(latest_handoff_path(&memory_dir).exists());
        assert!(
            std::fs::read_dir(memory_dir.join("daily"))
                .unwrap()
                .next()
                .is_some()
        );
        let episodic_record_glob = memory_dir.join("episodic");
        assert!(episodic_record_glob.exists());
        let handoff = tokio::fs::read_to_string(latest_handoff_path(&memory_dir))
            .await
            .unwrap();
        assert!(handoff.contains("Keep the latest handoff fresh."));
    }

    #[tokio::test]
    async fn refresh_memory_surfaces_needs_no_model_request_or_llm_mount() {
        let temp = tempfile::TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        let mut machine = AgentMachine::new();
        machine.add_user_message("Refresh local memory surfaces mechanically.");
        let message_count = machine.tape.messages().len();
        let state = RuntimeLoopState {
            machine,
            current_submission_id: None,
            environment: namespace_environment_for_test(),
            core_config: {
                let mut config = crate::Config::default();
                config.memory.store_dir = Some(memory_dir.clone());
                config
            },
            runtime_config: super::super::RuntimeConfig::default(),
            definition_persona_dirs: Vec::new(),
            prompt_cache: super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };

        refresh_turn_memory_surfaces(&state).await.unwrap();

        assert_eq!(state.machine.tape.messages().len(), message_count);
        let working = tokio::fs::read_to_string(working_memory_path(
            &memory_dir,
            state.machine.memory_record_id(),
        ))
        .await
        .unwrap();
        assert!(working.contains("Refresh local memory surfaces mechanically."));
    }

    #[tokio::test]
    async fn reused_process_path_gets_distinct_durable_memory_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        let rollouts_dir = temp.path().join("rollouts");
        let memory_dir = temp.path().join("memory");
        let first = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", &rollouts_dir)
            .await
            .unwrap();
        let second = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", &rollouts_dir)
            .await
            .unwrap();

        assert_ne!(first.memory_record_id(), second.memory_record_id());
        assert_ne!(
            working_memory_path(&memory_dir, first.memory_record_id()),
            working_memory_path(&memory_dir, second.memory_record_id())
        );
        assert_eq!(
            first.memory_record_id(),
            first.recorder.as_ref().unwrap().rollout_id()
        );
        assert_eq!(
            second.memory_record_id(),
            second.recorder.as_ref().unwrap().rollout_id()
        );
    }
}
