use super::*;

#[tokio::test]
async fn redaction_expansion_preserves_delegated_output_reference_and_child_paths() {
    let (state, shell) = create_namespace_transition_state_and_shell();
    let request = DelegatedSkillInvocationRequest {
        skill_id: "repo-review".to_string(),
        target: "reviewer".to_string(),
        task: "Review local files".to_string(),
        cwd: None,
        timeout_secs: None,
    };
    let result = ChildRuntimeResult {
        status: ChildRuntimeStatus::Completed,
        process_path: "/proc/42".to_string(),
        child_run_id: Some("child-run-1".to_string()),
        rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
        output_text: "Bearer x ".repeat(400),
        turn_summary: Some("Turn summary".to_string()),
        structured_output: None,
        warnings: Vec::new(),
        error_message: None,
        pause: None,
        child_run: Some(test_child_run_record("child-run-1", &state.process_path())),
    };

    assert!(result.output_text.chars().count() <= MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS);
    let agent_files = state.agent_files();
    let output_ref = persist_delegated_child_evidence(&agent_files, &request, &result)
        .await
        .expect("redaction-expanded child output should retain a reference");
    let delegated = result.delegated_result(Some(output_ref.clone()));
    assert!(delegated.output_text.is_none());
    assert_eq!(delegated.output_ref.as_ref(), Some(&output_ref));
    let action_result_path = format!(
        "{}/result",
        output_ref
            .path
            .strip_suffix("/output")
            .expect("action output path")
    );
    let action_result: serde_json::Value = serde_json::from_slice(
        &shell
            .cat(&action_result_path)
            .await
            .expect("delegated action result"),
    )
    .expect("delegated action result JSON");
    assert_eq!(action_result["child_process_path"], json!("/proc/42"));
    assert_eq!(action_result["child_agent_path"], json!("/agent/42"));
    let retained = String::from_utf8(shell.cat(&output_ref.path).await.unwrap()).unwrap();
    assert!(retained.chars().count() > MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS);
    assert!(retained.contains("[REDACTED reason=credential_token]"));
}

#[tokio::test]
async fn long_delegated_output_uses_parent_resolvable_namespace_reference() {
    let (mut state, shell) = create_namespace_transition_state_and_shell();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    let tool_call = NormalizedToolCall {
        id: "call_long_child".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review local files"
        }),
    };
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let output_len = (1 << 20) + 1_024;
    handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        move |_state, _spec, _cancel| {
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "child-machine".to_string(),
                    child_run_id: Some("child-run".to_string()),
                    rollout_path: Some(PathBuf::from("/tmp/debug-child.jsonl")),
                    output_text: "x".repeat(output_len),
                    turn_summary: Some("Long child completed".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await
    .unwrap();

    let tool_result = tool_result_text_for_call(&state, "call_long_child");
    let record: DelegatedSkillInvocationRecord = serde_json::from_str(&tool_result).unwrap();
    let output_ref = record.result.output_ref.unwrap();
    assert_eq!(output_ref.path, "/agent/1/actions/a0/output");
    assert_eq!(
        output_ref
            .debug
            .as_ref()
            .and_then(|debug| debug.rollout_path.as_deref()),
        Some("/tmp/debug-child.jsonl")
    );
    let full = String::from_utf8(shell.cat(&output_ref.path).await.unwrap()).unwrap();
    assert_eq!(full.len(), output_len);
    let namespace_ref = crate::evidence::NamespaceEvidenceReference {
        path: output_ref.path,
        offset: output_ref.offset,
        length: output_ref.length,
    };
    let resolved = state
        .agent_files()
        .resolve_evidence_reference(&namespace_ref, None, None)
        .await
        .unwrap();
    assert_eq!(resolved.len(), full.len());
}

#[tokio::test]
async fn failed_delegated_evidence_uses_namespace_refs_with_debug_rollout_paths() {
    for (index, status) in [ChildRuntimeStatus::TimedOut, ChildRuntimeStatus::Terminated]
        .into_iter()
        .enumerate()
    {
        let (state, shell) = create_namespace_transition_state_and_shell();
        let request = DelegatedSkillInvocationRequest {
            skill_id: "repo-review".to_string(),
            target: "reviewer".to_string(),
            task: "Review local files".to_string(),
            cwd: None,
            timeout_secs: None,
        };
        let output_text = format!("partial child evidence {index}");
        let child_run_id = format!("failed-child-{index}");
        let result = ChildRuntimeResult {
            status,
            process_path: format!("failed-machine-{index}"),
            child_run_id: Some(child_run_id.clone()),
            rollout_path: Some(PathBuf::from(format!("/tmp/private-child-{index}.jsonl"))),
            output_text: output_text.clone(),
            turn_summary: None,
            structured_output: None,
            warnings: Vec::new(),
            error_message: Some("delegated child did not complete".to_string()),
            pause: None,
            child_run: Some(test_child_run_record(&child_run_id, "parent-machine")),
        };

        let agent_files = state.agent_files();
        let output_ref = persist_delegated_child_evidence(&agent_files, &request, &result)
            .await
            .expect("failed child output reference");
        assert!(output_ref.path.starts_with("/agent/1/actions/"));
        assert_eq!(
            output_ref
                .debug
                .as_ref()
                .and_then(|debug| debug.rollout_path.as_deref()),
            Some(format!("/tmp/private-child-{index}.jsonl").as_str())
        );
        assert_eq!(
            String::from_utf8(shell.cat(&output_ref.path).await.unwrap()).unwrap(),
            output_text
        );
        assert!(
            serde_json::to_string(&output_ref)
                .unwrap()
                .contains("/tmp/private-child-")
        );
    }
}

#[tokio::test]
async fn evidence_resolution_distinguishes_missing_and_retention_expired() {
    let (state, _shell) = create_namespace_transition_state_and_shell();
    let preview = Some("bounded preview".to_string());
    let child_run = Some(json!({"child_run_id": "child-1"}));
    let missing_ref = crate::evidence::NamespaceEvidenceReference {
        path: "/agent/1/actions/missing/output".to_string(),
        offset: Some(0),
        length: None,
    };

    let missing = state
        .agent_files()
        .resolve_evidence_reference(&missing_ref, preview.clone(), child_run.clone())
        .await
        .unwrap_err();
    assert_eq!(
        missing.code,
        crate::evidence::EvidenceResolutionErrorCode::Missing
    );
    assert_eq!(missing.preview, preview);
    assert_eq!(missing.child_run, child_run);

    let expired_record = json!({
        "type": crate::evidence::RETENTION_EXPIRED_RECORD_TYPE,
        "reference": "/agent/1/actions/a0/output",
        "cause": "simulated_gc"
    });
    let action_id = state
        .agent_files()
        .write_action(
            NamespaceActionRecord::new("expired-test", "completed")
                .with_output(expired_record.to_string()),
        )
        .await
        .unwrap();
    let mut expired_ref = state
        .agent_files()
        .evidence_reference(format!("/agent/1/actions/{action_id}/output"))
        .await
        .unwrap();
    expired_ref.length = Some(10_000);
    let expired = state
        .agent_files()
        .resolve_evidence_reference(&expired_ref, None, None)
        .await
        .unwrap_err();
    assert_eq!(
        expired.code,
        crate::evidence::EvidenceResolutionErrorCode::RetentionExpired
    );
}

#[tokio::test]
async fn evidence_resolution_honors_open_ended_ranges() {
    let (state, _shell) = create_namespace_transition_state_and_shell();
    let action_id = state
        .agent_files()
        .write_action(NamespaceActionRecord::new("range-test", "completed").with_output("abcdef"))
        .await
        .unwrap();
    let path = format!("/agent/1/actions/{action_id}/output");

    for (offset, length, expected) in [
        (Some(2), None, b"cdef".as_slice()),
        (None, Some(3), b"abc".as_slice()),
        (None, None, b"abcdef".as_slice()),
    ] {
        let reference = crate::evidence::NamespaceEvidenceReference {
            path: path.clone(),
            offset,
            length,
        };
        let resolved = state
            .agent_files()
            .resolve_evidence_reference(&reference, None, None)
            .await
            .unwrap();
        assert_eq!(resolved, expected);
    }
}
