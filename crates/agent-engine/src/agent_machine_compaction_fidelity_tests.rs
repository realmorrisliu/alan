use super::*;

#[tokio::test]
async fn compaction_snapshot_preserves_durable_tool_sanitization_and_retention() {
    let dir = TempDir::new().unwrap();
    let mut live = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    append_turn(&mut live, "removed-secret-turn");
    live.add_user_message("retained user");
    live.add_assistant_message_with_tool_calls_and_reasoning(
        "rich assistant",
        vec![crate::tape::ToolRequest {
            id: "sensitive-tool".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({}),
        }],
        Some("reasoning"),
        Some("signature"),
        &[],
    );
    let payload = serde_json::json!({
        "authorization":"Bearer private-compaction-credential",
        "nested":{"password":"private-nested-password"},
        "body":format!("{}private-large-tail", "large-data-".repeat(2000)),
        "success":true,
    });
    live.add_tool_message("sensitive-tool", "read_file", payload.clone());
    live.add_assistant_message("retained answer", None);
    live.flush_recorder().await.unwrap();
    let path = live.rollout_path().unwrap().clone();
    let before = RolloutRecorder::load_history(&path).await.unwrap();
    let durable_tool = before
        .iter()
        .find_map(|item| match item {
            RolloutItem::Message(record) => record.message.as_ref().filter(|message| {
                message
                    .tool_responses()
                    .iter()
                    .any(|response| response.id == "sensitive-tool")
            }),
            _ => None,
        })
        .unwrap()
        .clone();
    assert_eq!(
        live.tool_payload_by_call_id("sensitive-tool"),
        Some(payload)
    );
    live.compact_tape("sanitized summary".into(), 4);
    let mut expected = live.messages().to_vec();
    expected[2] = durable_tool;
    live.persist_compaction_observation(attempt(), Some(CompactedItem::new("sanitized summary")))
        .await
        .unwrap();
    live.flush_recorder().await.unwrap();
    let json = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(!json.contains("private-compaction-credential"));
    assert!(!json.contains("private-nested-password"));
    assert!(!json.contains("private-large-tail"));
    assert!(json.contains("[REDACTED"));
    assert!(json.contains("...[truncated]"));
    let snapshot = json
        .lines()
        .map(|line| serde_json::from_str::<RolloutItem>(line).unwrap())
        .find_map(|item| match item {
            RolloutItem::Compacted(item) => item.retained_messages,
            _ => None,
        })
        .unwrap();
    assert_eq!(
        snapshot, expected,
        "snapshot matches existing durable Tool message boundary"
    );
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
        .await
        .unwrap();
    assert_eq!(recovered.messages(), expected);
    assert_eq!(
        projected_pairs(&recovered),
        (vec!["sensitive-tool".into()], vec!["sensitive-tool".into()])
    );
    recovered.flush_recorder().await.unwrap();
    let again = AgentMachine::load_from_rollout_in_dir(
        recovered.rollout_path().unwrap(),
        "/agent/3",
        "test",
        dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        again.messages(),
        expected,
        "removed IDs never resurrect across replacement recovery"
    );
}

fn attempt() -> CompactionAttemptSnapshot {
    serde_json::from_value(serde_json::json!({
        "attempt_id":"fidelity", "request":{"mode":"auto_pre_turn", "trigger":"auto", "reason":"window_pressure"},
        "result":"success", "retry_count":0, "tape_mutated":true,
        "timestamp":"2026-09-30T00:00:00Z"
    })).unwrap()
}

fn append_turn(machine: &mut AgentMachine, id: &str) {
    machine.add_user_message(&format!("user-{id}"));
    machine.add_assistant_message_with_tool_calls_and_reasoning(
        "",
        vec![crate::tape::ToolRequest {
            id: id.into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path":id}),
        }],
        Some("retained reasoning"),
        Some("signature"),
        &[],
    );
    machine.add_tool_message(id, "read_file", serde_json::json!({"result":id}));
    machine.add_assistant_message(&format!("answer-{id}"), None);
}

fn projected_pairs(machine: &AgentMachine) -> (Vec<String>, Vec<String>) {
    let projected = crate::llm::project_messages(&machine.messages_for_prompt(), true);
    let calls = projected
        .iter()
        .flat_map(|message| message.tool_calls.iter().flatten())
        .filter_map(|call| call.id.clone())
        .collect();
    let responses = projected
        .iter()
        .filter_map(|message| message.tool_call_id.clone())
        .collect();
    (calls, responses)
}

#[tokio::test]
async fn legacy_compaction_count_and_summary_only_have_explicit_recovery_behavior() {
    for count in [Some(4), None] {
        let dir = TempDir::new().unwrap();
        let mut source = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
            .await
            .unwrap();
        append_turn(&mut source, "old");
        append_turn(&mut source, "retained");
        source.flush_recorder().await.unwrap();
        let mut history = RolloutRecorder::load_history(source.rollout_path().unwrap())
            .await
            .unwrap();
        let mut compacted = CompactedItem::new("legacy summary");
        compacted.output_messages = count;
        history.push(RolloutItem::Compacted(compacted));
        let fixture = dir.path().join("legacy.jsonl");
        let content = history
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        tokio::fs::write(&fixture, content).await.unwrap();
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&fixture, "/agent/2", "test", dir.path())
                .await
                .unwrap();
        assert_eq!(recovered.messages().len(), count.unwrap_or(8));
        assert_eq!(recovered.tape.summary(), Some("legacy summary"));
        let (calls, responses) = projected_pairs(&recovered);
        assert_eq!(calls, responses);
        assert_eq!(
            calls,
            if count.is_some() {
                vec!["retained"]
            } else {
                vec!["old", "retained"]
            }
        );
    }
}

#[tokio::test]
async fn compacted_rich_tape_matches_rollout_recovery_and_next_provider_projection() {
    let dir = TempDir::new().unwrap();
    let mut live = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    append_turn(&mut live, "removed");
    append_turn(&mut live, "retained");
    live.compact_tape("summary".into(), 4);
    let mut compacted = CompactedItem::new("summary");
    compacted.output_messages = Some(live.messages().len());
    live.persist_compaction_observation(attempt(), Some(compacted))
        .await
        .unwrap();
    append_turn(&mut live, "post-compaction");
    live.flush_recorder().await.unwrap();
    let path = live.rollout_path().unwrap().clone();
    let mut recovered =
        AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
            .await
            .unwrap();
    assert_eq!(recovered.messages().len(), live.messages().len());
    assert_eq!(
        recovered.messages(),
        live.messages(),
        "retained rich IDs, reasoning and contents match"
    );
    append_turn(&mut live, "subsequent");
    append_turn(&mut recovered, "subsequent");
    assert_eq!(projected_pairs(&recovered), projected_pairs(&live));
    let (calls, responses) = projected_pairs(&recovered);
    assert_eq!(calls, ["retained", "post-compaction", "subsequent"]);
    assert_eq!(calls, responses);
    assert_eq!(
        serde_json::to_value(crate::llm::project_messages(
            &recovered.messages_for_prompt(),
            true
        ))
        .unwrap(),
        serde_json::to_value(crate::llm::project_messages(
            &live.messages_for_prompt(),
            true
        ))
        .unwrap()
    );
    recovered.flush_recorder().await.unwrap();
    let again = AgentMachine::load_from_rollout_in_dir(
        recovered.rollout_path().unwrap(),
        "/agent/3",
        "test",
        dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        again.messages(),
        recovered.messages(),
        "replacement rollout preserves compaction boundary"
    );
    let history = RolloutRecorder::load_history(&path).await.unwrap();
    assert!(history.iter().any(|item| matches!(item, RolloutItem::Message(record)
        if record.message.as_ref().is_some_and(|message| message.text_content() == "user-removed"))),
        "source durable historical evidence is not deleted");
}
