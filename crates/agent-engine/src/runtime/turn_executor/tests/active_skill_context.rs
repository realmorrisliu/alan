use super::*;

#[tokio::test]
async fn test_run_turn_resume_turn_preserves_active_skill_context() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let skill_dir = definition_root.join("skills/release-check");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("skill.yaml"),
        r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "checkpoint_type": "test",
                "summary": "Confirm risky action"
            }),
        }],
        "",
        seen_system_prompts.clone(),
    ));
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
        "please use $release-check for this task",
    )]));
    state.machine.set_active_skills(prior_prompt.active_skills);
    state
        .machine
        .add_user_message("continue the prior approval flow");

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn,
        None,
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let system_prompts = seen_system_prompts.lock().unwrap();
    let resumed_prompt = system_prompts.last().expect("expected system prompt");
    assert!(resumed_prompt.contains("## Skill: Release Check"));
    assert!(resumed_prompt.contains("Use this skill when asked."));

    let confirmation = events.into_iter().find_map(|event| match event {
        Event::Yield {
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload,
            ..
        } => Some(payload),
        _ => None,
    });
    let confirmation = confirmation.expect("expected confirmation yield");
    let hints = confirmation["details"]["skill_permission_hints"]
        .as_array()
        .cloned()
        .unwrap();

    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0]["skill_id"], "release-check");
    assert_eq!(
        hints[0]["permission_hints"][0],
        "May require write approval."
    );
}

#[tokio::test]
async fn test_run_turn_resume_turn_with_steer_preserves_active_skill_context() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let skill_dir = definition_root.join("skills/release-check");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("skill.yaml"),
        r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "checkpoint_type": "test",
                "summary": "Confirm risky action"
            }),
        }],
        "",
        seen_system_prompts.clone(),
    ));
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
        "please use $release-check for this task",
    )]));
    state.machine.set_active_skills(prior_prompt.active_skills);

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn,
        Some(vec![ContentPart::text(
            "steer: tighten the approval explanation",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let system_prompts = seen_system_prompts.lock().unwrap();
    let resumed_prompt = system_prompts.last().expect("expected system prompt");
    assert!(resumed_prompt.contains("## Skill: Release Check"));
    assert!(resumed_prompt.contains("Use this skill when asked."));

    let confirmation = events.into_iter().find_map(|event| match event {
        Event::Yield {
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload,
            ..
        } => Some(payload),
        _ => None,
    });
    let confirmation = confirmation.expect("expected confirmation yield");
    let hints = confirmation["details"]["skill_permission_hints"]
        .as_array()
        .cloned()
        .unwrap();

    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0]["skill_id"], "release-check");
    assert_eq!(
        hints[0]["permission_hints"][0],
        "May require write approval."
    );
}

#[tokio::test]
async fn test_run_turn_resume_turn_without_prior_active_skills_can_activate_skill_from_steer() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let skill_dir = definition_root.join("skills/release-check");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("skill.yaml"),
        r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "checkpoint_type": "test",
                "summary": "Confirm risky action"
            }),
        }],
        "",
        seen_system_prompts.clone(),
    ));
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn,
        Some(vec![ContentPart::text(
            "steer: please use $release-check for this task",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let system_prompts = seen_system_prompts.lock().unwrap();
    let resumed_prompt = system_prompts.last().expect("expected system prompt");
    assert!(resumed_prompt.contains("## Skill: Release Check"));
    assert!(resumed_prompt.contains("Use this skill when asked."));

    let confirmation = events.into_iter().find_map(|event| match event {
        Event::Yield {
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload,
            ..
        } => Some(payload),
        _ => None,
    });
    let confirmation = confirmation.expect("expected confirmation yield");
    let hints = confirmation["details"]["skill_permission_hints"]
        .as_array()
        .cloned()
        .unwrap();

    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0]["skill_id"], "release-check");
    assert_eq!(
        hints[0]["permission_hints"][0],
        "May require write approval."
    );
}

#[tokio::test]
async fn test_run_turn_resume_turn_with_steer_can_add_new_skill_context() {
    let temp = tempfile::TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");

    let release_skill_dir = definition_root.join("skills/release-check");
    std::fs::create_dir_all(&release_skill_dir).unwrap();
    std::fs::write(
        release_skill_dir.join("SKILL.md"),
        r#"---
name: Release Check
description: Review risky release actions
---

# Instructions
Use this release skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        release_skill_dir.join("skill.yaml"),
        r#"
runtime:
  permission_hints:
    - "May require write approval."
"#,
    )
    .unwrap();

    let audit_skill_dir = definition_root.join("skills/safety-audit");
    std::fs::create_dir_all(&audit_skill_dir).unwrap();
    std::fs::write(
        audit_skill_dir.join("SKILL.md"),
        r#"---
name: Safety Audit
description: Review risky operations for safety concerns
---

# Instructions
Use this safety skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        audit_skill_dir.join("skill.yaml"),
        r#"
runtime:
  permission_hints:
    - "May require network approval."
"#,
    )
    .unwrap();

    let seen_system_prompts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut state = create_test_state_with_provider(RecordingToolCallProvider::new(
        vec![ToolCall {
            id: Some("call_1".to_string()),
            name: "request_confirmation".to_string(),
            arguments: json!({
                "checkpoint_type": "test",
                "summary": "Confirm risky action"
            }),
        }],
        "",
        seen_system_prompts.clone(),
    ));
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let prior_prompt = state.prompt_cache.build(Some(&[ContentPart::text(
        "please use $release-check for this task",
    )]));
    state.machine.set_active_skills(prior_prompt.active_skills);

    let cancel = CancellationToken::new();
    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = run_turn_with_cancel(
        &mut state,
        TurnRunKind::ResumeTurn,
        Some(vec![ContentPart::text(
            "steer: also use $safety-audit before approving this",
        )]),
        &mut emit,
        &cancel,
        None,
    )
    .await;

    assert!(result.is_ok());

    let system_prompts = seen_system_prompts.lock().unwrap();
    let resumed_prompt = system_prompts.last().expect("expected system prompt");
    assert!(resumed_prompt.contains("## Skill: Release Check"));
    assert!(resumed_prompt.contains("Use this release skill when asked."));
    assert!(resumed_prompt.contains("## Skill: Safety Audit"));
    assert!(resumed_prompt.contains("Use this safety skill when asked."));

    let confirmation = events.into_iter().find_map(|event| match event {
        Event::Yield {
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload,
            ..
        } => Some(payload),
        _ => None,
    });
    let confirmation = confirmation.expect("expected confirmation yield");
    let hints = confirmation["details"]["skill_permission_hints"]
        .as_array()
        .cloned()
        .unwrap();

    assert_eq!(hints.len(), 2);
    let skill_ids: std::collections::BTreeSet<String> = hints
        .iter()
        .filter_map(|hint| {
            hint.get("skill_id")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    assert_eq!(
        skill_ids,
        std::collections::BTreeSet::from(
            ["release-check".to_string(), "safety-audit".to_string(),]
        )
    );
}
