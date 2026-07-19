use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Utc};
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::agent_machine::AgentMachine;
use crate::tape::Message;

use super::transition::RuntimeLoopState;

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
    let rendered = render_memory_surfaces(&state.machine, &process_path, memory_record_id, now);

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
    if state.machine.active_turn_message_start().is_none() {
        return;
    }

    refresh_turn_memory_surfaces_best_effort(state, context).await;
}

fn render_memory_surfaces(
    machine: &AgentMachine,
    process_path: &str,
    memory_record_id: &str,
    now: DateTime<Utc>,
) -> RenderedMemorySurfaces {
    let current_goal = derive_current_goal(machine);
    let latest_assistant_state = derive_latest_assistant_state(machine);
    let active_plan_items = render_plan_items(machine, &["in_progress", "pending"]);
    let completed_plan_items = render_plan_items(machine, &["completed"]);
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

fn derive_current_goal(machine: &AgentMachine) -> String {
    let source_ref = memory_source_ref(machine);
    let user_messages = machine
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
        .zip(machine.active_turn_message_start())
        .is_some_and(|((index, _), start)| *index >= start);

    if machine.plan_snapshot_is_from_active_turn()
        && let Some(plan_goal) = derive_active_plan_goal(machine)
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
        if machine.plan_snapshot_postdates_message(*index)
            && let Some(plan_goal) = derive_active_plan_goal(machine)
        {
            return mark_carried_goal_if_needed(plan_goal, latest_user, latest_is_substantive);
        }
        let goal = truncate_memory_text(substantive, MAX_INLINE_TEXT_CHARS, &source_ref);
        return mark_carried_goal_if_needed(goal, latest_user, latest_is_substantive);
    }

    if let Some(plan_goal) = derive_active_plan_goal(machine) {
        return mark_carried_goal_if_needed(plan_goal, latest_user, latest_is_substantive);
    }

    if let Some(summary) = machine
        .tape_summary()
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

fn derive_active_plan_goal(machine: &AgentMachine) -> Option<String> {
    let snapshot = machine.plan_snapshot()?;
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

fn derive_latest_assistant_state(machine: &AgentMachine) -> String {
    let source_ref = memory_source_ref(machine);
    let messages = machine
        .active_turn_message_start()
        .and_then(|start| machine.messages().get(start..))
        .unwrap_or_else(|| machine.messages());

    messages
        .iter()
        .rev()
        .find(|message| message.is_assistant())
        .map(Message::non_thinking_text_content)
        .map(|text| truncate_memory_text(text.trim(), MAX_INLINE_TEXT_CHARS, &source_ref))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if machine.active_turn_message_start().is_some() {
                "This turn completed without a new assistant response.".to_string()
            } else {
                "No assistant response recorded yet.".to_string()
            }
        })
}

fn render_plan_items(machine: &AgentMachine, statuses: &[&str]) -> String {
    let Some(snapshot) = machine.plan_snapshot() else {
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
        .tape_summary()
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
mod tests;
