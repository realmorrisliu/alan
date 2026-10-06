use super::*;

#[tokio::test]
async fn delegated_resolution_publishes_without_outer_poll() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("descriptor");
    create_repo_skill(
        &root,
        "release-check",
        "Release Check",
        "test",
        "instructions",
    );
    let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
        path: root.join("skills"),
        scope: SkillScope::Descriptor,
    }]);
    let mut state = create_test_state_with_provider(ToolCallMockProvider::new(Vec::new(), "done"));
    state.prompt_cache =
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            view,
            Vec::new(),
            crate::skills::SkillHostCapabilities::default().with_delegated_skill_invocation(),
        );
    let arguments =
        serde_json::json!({"skill_id": "release-check", "target": "missing", "task": "test"});
    let call = crate::agent_machine::NormalizedToolCall {
        id: "resolution".into(),
        name: "invoke_delegated_skill".into(),
        arguments: arguments.clone(),
    };
    let mut emit = |_event: Event| async {};
    crate::runtime::transition::dispatch_virtual_tool_call(
        &mut state,
        &call,
        &arguments,
        &CancellationToken::new(),
        false,
        &mut emit,
    )
    .await
    .unwrap();
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    let snapshot: alan_agent_protocol::UiSkillSnapshot =
        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/skills").await.unwrap()).unwrap();
    assert!(
        snapshot.known,
        "delegated resolution must publish its ensured cache"
    );
    assert!(
        snapshot
            .mentionable_skill_ids
            .contains(&"release-check".into())
    );
}

#[tokio::test]
async fn skill_presentation_failure_preserves_tool_disposition_and_lease() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("descriptor");
    create_repo_skill(
        &root,
        "release-check",
        "Release Check",
        "test",
        "old instructions",
    );
    let mut state = create_test_state_with_provider(ToolCallMockProvider::new(Vec::new(), "done"));
    state.prompt_cache =
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
                path: root.join("skills"),
                scope: SkillScope::Descriptor,
            }]),
            Vec::new(),
            crate::skills::SkillHostCapabilities::default().with_delegated_skill_invocation(),
        );
    let lease = state
        .prompt_cache
        .build(Some(&[ContentPart::text("$release-check")]));
    state.machine.set_active_skills(lease.active_skills);
    state.environment = NamespaceRuntimeEnvironment::new(
        state.environment.root_transport(),
        "/agent/99",
        "default",
    );
    let arguments = json!({"skill_id":"release-check", "target":"missing", "task":"test"});
    let call = crate::agent_machine::NormalizedToolCall {
        id: "original-tool".into(),
        name: "invoke_delegated_skill".into(),
        arguments: arguments.clone(),
    };
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };
    let result = crate::runtime::transition::dispatch_virtual_tool_call(
        &mut state,
        &call,
        &arguments,
        &CancellationToken::new(),
        false,
        &mut emit,
    )
    .await;
    assert!(
        matches!(
            result,
            Ok(crate::runtime::virtual_tool::VirtualToolOutcome::Continue {
                refresh_context: true
            })
        ),
        "presentation failure must not replace Tool disposition: {result:?}"
    );
    assert_eq!(state.machine.active_skills().len(), 1);
    assert!(events.iter().any(|event| matches!(event, Event::ToolCallCompleted { id, success: Some(false), .. } if id == "original-tool")));
    assert!(state.machine.messages().iter().any(|message| matches!(message, Message::Tool { responses } if responses.iter().any(|response| response.id == "original-tool" && response.text_content().contains("skill_not_delegated")))));
}

#[tokio::test]
async fn runtime_skill_build_publishes_without_outer_poll() {
    for resumed in [false, true] {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("descriptor");
        let skill = root.join("skills/release-check");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: Release Check\ndescription: test\n---\nold instructions\n",
        )
        .unwrap();
        let mut state =
            create_test_state_with_provider(ToolCallMockProvider::new(Vec::new(), "done"));
        state.prompt_cache = prompt_cache_for_definition_root(&root, Vec::new());
        if resumed {
            let lease = state
                .prompt_cache
                .build(Some(&[ContentPart::text("$release-check")]));
            state.machine.set_active_skills(lease.active_skills);
        }
        let mut emit = |_event: Event| async {};
        run_turn_with_cancel(
            &mut state,
            if resumed {
                TurnRunKind::ResumeTurn
            } else {
                TurnRunKind::NewTurn
            },
            Some(vec![ContentPart::text("$release-check")]),
            &mut emit,
            &CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        let shell = alan_shell::Shell::new(state.environment.root_transport());
        let snapshot: alan_agent_protocol::UiSkillSnapshot =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/skills").await.unwrap())
                .unwrap();
        assert!(snapshot.known, "actual build must publish before returning");
        assert_eq!(snapshot.process_path, "/proc/1");
        assert!(
            snapshot
                .mentionable_skill_ids
                .contains(&"release-check".into())
        );
        let events = shell.cat("/agent/1/events").await.unwrap();
        assert!(String::from_utf8(events).unwrap().contains("skills"));
    }
}
