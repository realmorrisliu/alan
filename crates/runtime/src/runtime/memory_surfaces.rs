use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Utc};
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::session::Session;
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
    session_summary: String,
    daily_entry: String,
}

pub(crate) async fn refresh_turn_memory_surfaces(state: &RuntimeLoopState) -> Result<()> {
    if !state.core_config.memory.enabled {
        return Ok(());
    }

    let Some(memory_dir) = state.core_config.memory.workspace_dir.as_deref() else {
        return Ok(());
    };

    crate::prompts::ensure_workspace_memory_layout_at(memory_dir)
        .with_context(|| format!("failed to ensure memory layout at {}", memory_dir.display()))?;

    let now = Utc::now();
    let rendered = render_memory_surfaces(&state.session, &state.turn_state, now);

    write_text_file(
        &working_memory_path(memory_dir, &state.session.id),
        &rendered.working_memory,
    )
    .await?;
    write_text_file(&latest_handoff_path(memory_dir), &rendered.handoff).await?;
    write_text_file(
        &session_summary_path(memory_dir, &state.session.id, now),
        &rendered.session_summary,
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
    session: &Session,
    turn_state: &TurnState,
    now: DateTime<Utc>,
) -> RenderedMemorySurfaces {
    let current_goal = derive_current_goal(session, turn_state);
    let latest_assistant_state = derive_latest_assistant_state(session, turn_state);
    let active_plan_items = render_plan_items(turn_state, &["in_progress", "pending"]);
    let completed_plan_items = render_plan_items(turn_state, &["completed"]);
    let recent_messages = render_recent_messages(session);
    let compaction_summary = render_compaction_summary(session);
    let latest_memory_flush = render_latest_memory_flush(session);
    let session_id = &session.id;
    let updated_at = now.to_rfc3339();

    let working_memory = format!(
        "# Working Memory\n\nsession_id: {session_id}\nupdated_at: {updated_at}\n\n## Current Goal\n{current_goal}\n\n## Active Subgoals\n{active_plan_items}\n\n## Confirmed Constraints\n{compaction_summary}\n\n## Pending Verification\n{active_plan_items}\n\n## Open Loops\n{active_plan_items}\n\n## Recent Findings\n- Latest assistant state: {latest_assistant_state}\n{recent_messages}\n\n## Active Recall\n{latest_memory_flush}\n"
    );

    let handoff = format!(
        "# Latest Handoff\n\nupdated_at: {updated_at}\nsession_id: {session_id}\n\n## Current Goal\n{current_goal}\n\n## What Just Happened\n- {latest_assistant_state}\n\n## Next Steps\n{active_plan_items}\n\n## Recent Context\n{compaction_summary}\n{recent_messages}\n"
    );

    let session_summary = format!(
        "# Session Summary\n\nsession_id: {session_id}\nupdated_at: {updated_at}\n\n## Current Goal\n{current_goal}\n\n## Latest Assistant State\n- {latest_assistant_state}\n\n## Active Plan\n{active_plan_items}\n\n## Completed Plan Items\n{completed_plan_items}\n\n## Prior Compaction Summary\n{compaction_summary}\n\n## Recent Conversation Highlights\n{recent_messages}\n\n## Latest Memory Flush\n{latest_memory_flush}\n"
    );

    let daily_entry = format!(
        "## {updated_at}\n\nsession_id: {session_id}\n\n### Current Goal\n{current_goal}\n\n### Latest Assistant State\n- {latest_assistant_state}\n\n### Next Steps\n{active_plan_items}\n\n### Latest Memory Flush\n{latest_memory_flush}\n\n"
    );

    RenderedMemorySurfaces {
        working_memory,
        handoff,
        session_summary,
        daily_entry,
    }
}

fn derive_current_goal(session: &Session, turn_state: &TurnState) -> String {
    let source_ref = memory_source_ref(session);
    turn_state
        .plan_snapshot()
        .and_then(|snapshot| snapshot.explanation.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            session
                .tape
                .messages()
                .iter()
                .rev()
                .find(|message| message.is_user())
                .map(Message::text_content)
                .map(|text| truncate_memory_text(text.trim(), MAX_INLINE_TEXT_CHARS, &source_ref))
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "No current goal recorded.".to_string())
}

fn derive_latest_assistant_state(session: &Session, turn_state: &TurnState) -> String {
    let source_ref = memory_source_ref(session);
    let messages = turn_state
        .active_turn_message_start()
        .and_then(|start| session.tape.messages().get(start..))
        .unwrap_or_else(|| session.tape.messages());

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
                alan_protocol::PlanItemStatus::Pending => "pending",
                alan_protocol::PlanItemStatus::InProgress => "in_progress",
                alan_protocol::PlanItemStatus::Completed => "completed",
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

fn format_plan_status(status: &alan_protocol::PlanItemStatus) -> &'static str {
    match status {
        alan_protocol::PlanItemStatus::Pending => "pending",
        alan_protocol::PlanItemStatus::InProgress => "in_progress",
        alan_protocol::PlanItemStatus::Completed => "completed",
    }
}

fn render_recent_messages(session: &Session) -> String {
    let source_ref = memory_source_ref(session);
    let items: Vec<String> = session
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

fn render_compaction_summary(session: &Session) -> String {
    let source_ref = memory_source_ref(session);
    session
        .tape
        .summary()
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .map(|summary| truncate_memory_text(summary, MAX_COMPACTION_SUMMARY_CHARS, &source_ref))
        .unwrap_or_else(|| "No compaction summary recorded.".to_string())
}

fn render_latest_memory_flush(session: &Session) -> String {
    session
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

fn working_memory_path(memory_dir: &Path, session_id: &str) -> PathBuf {
    memory_dir.join("working").join(format!("{session_id}.md"))
}

fn latest_handoff_path(memory_dir: &Path) -> PathBuf {
    memory_dir.join("handoffs").join("LATEST.md")
}

fn session_summary_path(memory_dir: &Path, session_id: &str, now: DateTime<Utc>) -> PathBuf {
    memory_dir.join("sessions").join(format!(
        "{:04}/{:02}/{:02}/{}.md",
        now.year(),
        now.month(),
        now.day(),
        session_id
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

fn memory_source_ref(session: &Session) -> String {
    session
        .rollout_path()
        .map(|path| format!("rollout {}", path.display()))
        .unwrap_or_else(|| format!("session {}", session.id))
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
    use crate::runtime::turn_state::TurnState;
    use crate::session::Session;

    #[test]
    fn render_memory_surfaces_follow_pure_text_layout_and_content() {
        let mut session = Session::new();
        session.id = "sess-123".to_string();
        session.add_user_message("Finish the pure-text memory slice.");
        session.add_assistant_message("Added scaffolding and prompt bootstrap.", None);

        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot(
            Some("Finish the pure-text memory slice.".to_string()),
            vec![
                alan_protocol::PlanItem {
                    id: "p1".to_string(),
                    content: "Write the scaffolding".to_string(),
                    status: alan_protocol::PlanItemStatus::Completed,
                },
                alan_protocol::PlanItem {
                    id: "p2".to_string(),
                    content: "Refresh the handoff".to_string(),
                    status: alan_protocol::PlanItemStatus::InProgress,
                },
            ],
        );

        let now = DateTime::parse_from_rfc3339("2026-04-15T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rendered = render_memory_surfaces(&session, &turn_state, now);

        assert!(rendered.working_memory.contains("# Working Memory"));
        assert!(rendered.handoff.contains("# Latest Handoff"));
        assert!(rendered.session_summary.contains("# Session Summary"));
        assert!(
            rendered
                .daily_entry
                .contains("## 2026-04-15T15:30:00+00:00")
        );
        assert!(
            rendered
                .session_summary
                .contains("Finish the pure-text memory slice.")
        );
        assert!(
            rendered
                .session_summary
                .contains("[in_progress] Refresh the handoff")
        );
        assert!(
            rendered
                .session_summary
                .contains("[completed] Write the scaffolding")
        );
    }

    #[test]
    fn render_memory_surfaces_scopes_latest_assistant_state_to_active_turn() {
        let mut session = Session::new();
        session.id = "sess-123".to_string();
        session.add_user_message("Earlier task");
        session.add_assistant_message("Earlier assistant response.", None);

        let mut turn_state = TurnState::default();
        turn_state.begin_turn(session.tape.messages().len());
        session.add_user_message("Current tool-only turn");

        let now = DateTime::parse_from_rfc3339("2026-04-15T15:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rendered = render_memory_surfaces(&session, &turn_state, now);

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
    fn truncate_memory_text_keeps_markdown_lines_and_marks_source() {
        let text = "### Top-level directories\n- crates/runtime has the runtime code\n- crates/tui has the terminal UI\n- docs/spec has contracts\n";

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

        let truncated = truncate_memory_text(text, 120, "session sess-code");

        assert!(truncated.chars().count() <= 120);
        assert!(truncated.contains("```rust"));
        assert!(truncated.matches("```").count() >= 2);
        assert!(truncated.contains("truncated"));
        assert!(truncated.contains("session sess-code"));
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
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let mut session = Session::new();
        session.id = "sess-write".to_string();
        session.add_user_message("Keep the latest handoff fresh.");
        session.add_assistant_message("Wrote the memory surfaces.", None);

        let mut turn_state = TurnState::default();
        turn_state.set_plan_snapshot(
            Some("Keep the latest handoff fresh.".to_string()),
            vec![alan_protocol::PlanItem {
                id: "p1".to_string(),
                content: "Verify the memory files".to_string(),
                status: alan_protocol::PlanItemStatus::Pending,
            }],
        );

        let state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session,
            current_submission_id: None,
            llm_client: crate::llm::LlmClient::new(alan_llm::MockLlmProvider::new()),
            core_config: {
                let mut config = crate::Config::default();
                config.memory.workspace_dir = Some(memory_dir.clone());
                config
            },
            runtime_config: super::super::RuntimeConfig::default(),
            workspace_persona_dirs: Vec::new(),
            tools: crate::tools::ToolRegistry::new(),
            prompt_cache: super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state,
        };

        refresh_turn_memory_surfaces(&state).await.unwrap();

        assert!(working_memory_path(&memory_dir, "sess-write").exists());
        assert!(latest_handoff_path(&memory_dir).exists());
        assert!(
            std::fs::read_dir(memory_dir.join("daily"))
                .unwrap()
                .next()
                .is_some()
        );
        let session_summary_glob = memory_dir.join("sessions");
        assert!(session_summary_glob.exists());
        let handoff = tokio::fs::read_to_string(latest_handoff_path(&memory_dir))
            .await
            .unwrap();
        assert!(handoff.contains("Keep the latest handoff fresh."));
    }
}
