use super::*;
use crate::agent_machine::AgentMachine;
use crate::runtime::transition::RuntimeLoopState;
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

    machine.set_plan_snapshot(
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
    let rendered = render_memory_surfaces(&machine, "/proc/1", machine.memory_record_id(), now);

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

    machine.begin_turn(machine.messages().len());
    machine.add_user_message("Current tool-only turn");

    let now = DateTime::parse_from_rfc3339("2026-04-15T15:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let rendered = render_memory_surfaces(&machine, "/proc/1", machine.memory_record_id(), now);

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

    let goal = derive_current_goal(&machine);

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
        derive_current_goal(&machine),
        "Remove the obsolete compatibility endpoints."
    );
}

#[test]
fn new_substantive_turn_request_replaces_stale_plan_goal() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("Finish the old memory task.");
    machine.set_plan_snapshot(Some("Finish the old memory task.".to_string()), Vec::new());
    machine.begin_turn(machine.messages().len());
    machine.add_user_message("Rewrite the provider connection documentation.");

    assert_eq!(
        derive_current_goal(&machine),
        "Rewrite the provider connection documentation."
    );
}

#[test]
fn in_turn_plan_update_refines_the_initial_user_goal() {
    let mut machine = AgentMachine::new();
    machine.begin_turn(machine.messages().len());
    machine.add_user_message("Implement the memory contract changes.");
    machine.set_plan_snapshot(
        Some("Validate salience and compaction fallback behavior.".to_string()),
        Vec::new(),
    );

    assert_eq!(
        derive_current_goal(&machine),
        "Validate salience and compaction fallback behavior."
    );
}

#[test]
fn substantive_resume_input_overrides_earlier_active_plan() {
    let mut machine = AgentMachine::new();
    machine.begin_turn(machine.messages().len());
    machine.add_user_message("Implement the old memory contract.");
    machine.set_plan_snapshot(
        Some("Finish the old memory contract.".to_string()),
        Vec::new(),
    );
    machine.note_resumed_user_input();
    machine.add_user_message("Switch to the provider connection contract.");

    assert_eq!(
        derive_current_goal(&machine),
        "Switch to the provider connection contract."
    );
}

#[test]
fn terse_imperative_passes_salience_filter() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("Prepare the release.");
    machine.set_plan_snapshot(Some("Prepare the release.".to_string()), Vec::new());
    machine.begin_turn(machine.messages().len());
    machine.add_user_message("deploy it");

    assert_eq!(derive_current_goal(&machine), "deploy it");
}

#[test]
fn active_plan_goal_wins_when_latest_message_is_acknowledgement() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("Complete the broader migration.");
    machine.set_plan_snapshot_at_message_count(
        Some("Validate the namespace-native migration.".to_string()),
        Vec::new(),
        machine.messages().len(),
    );
    machine.begin_turn(machine.messages().len());
    machine.add_user_message("ok");

    assert_eq!(
        derive_current_goal(&machine),
        "[carried forward] Validate the namespace-native migration."
    );
}

#[test]
fn later_substantive_goal_wins_before_stale_plan_on_acknowledgement() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("Finish task A.");
    machine.set_plan_snapshot_at_message_count(
        Some("Complete task A plan.".to_string()),
        Vec::new(),
        machine.messages().len(),
    );
    machine.add_assistant_message("Task A paused.", None);
    machine.add_user_message("Switch to substantive task B.");
    machine.add_assistant_message("Task B underway.", None);
    machine.add_user_message("ok");

    assert_eq!(
        derive_current_goal(&machine),
        "[carried forward] Switch to substantive task B."
    );
}

#[test]
fn compaction_summary_wins_before_acknowledgement_fallback() {
    let mut machine = AgentMachine::new();
    machine.set_tape_summary(
        "Complete the namespace-native lifecycle migration and verify parent visibility."
            .to_string(),
    );
    machine.add_user_message("ok");

    assert_eq!(
        derive_current_goal(&machine),
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
            derive_current_goal(&machine),
            "[carried forward] Archive the completed Alan OS contract changes.",
            "acknowledgement {acknowledgement:?} must not become the goal"
        );
    }
}

#[test]
fn active_plan_goal_prefers_in_progress_before_pending_order() {
    let mut machine = AgentMachine::new();
    machine.set_plan_snapshot(
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
        derive_active_plan_goal(&machine).as_deref(),
        Some("Verify the current contract.")
    );
}

#[test]
fn acknowledgement_is_used_verbatim_when_no_better_context_exists() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("ok");

    assert_eq!(derive_current_goal(&machine), "ok");
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

    machine.set_plan_snapshot(
        Some("Keep the latest handoff fresh.".to_string()),
        vec![alan_agent_protocol::PlanItem {
            id: "p1".to_string(),
            content: "Verify the memory files".to_string(),
            status: alan_agent_protocol::PlanItemStatus::Pending,
        }],
    );

    let state = RuntimeLoopState {
        machine,
        environment: namespace_environment_for_test(),
        core_config: {
            let mut config = crate::Config::default();
            config.memory.store_dir = Some(memory_dir.clone());
            config
        },
        runtime_config: super::super::RuntimeConfig::default(),
        definition_persona_dirs: Vec::new(),
        prompt_cache: super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let process_path = state.process_path();
    refresh_turn_memory_surfaces(&state.machine, Some(&memory_dir), &process_path)
        .await
        .unwrap();

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
    let message_count = machine.messages().len();
    let state = RuntimeLoopState {
        machine,
        environment: namespace_environment_for_test(),
        core_config: {
            let mut config = crate::Config::default();
            config.memory.store_dir = Some(memory_dir.clone());
            config
        },
        runtime_config: super::super::RuntimeConfig::default(),
        definition_persona_dirs: Vec::new(),
        prompt_cache: super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let process_path = state.process_path();
    refresh_turn_memory_surfaces(&state.machine, Some(&memory_dir), &process_path)
        .await
        .unwrap();

    assert_eq!(state.machine.messages().len(), message_count);
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
    assert_eq!(first.memory_record_id(), first.rollout_id().unwrap());
    assert_eq!(second.memory_record_id(), second.rollout_id().unwrap());
}
