use super::*;
use alan_agent_protocol::{UiEvent, UiNoticeKind};

#[tokio::test]
async fn delegated_prelaunch_rejection_reaches_retained_ui_without_changing_continuation() {
    for case in ["launch", "invalid", "unavailable", "target", "publication"] {
        let (mut state, mut shell) = create_namespace_transition_state_and_shell();
        if case == "publication" {
            let mut namespace = Namespace::new();
            namespace.mount(
                "/agent/1",
                InProcessTransport::new(Arc::new(AgentFs::new())),
                Access::ReadOnly,
            );
            let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
            shell = Shell::new(root.clone());
            state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        }
        activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
        if case == "unavailable" {
            state
                .prompt_cache
                .set_host_capabilities(SkillHostCapabilities::default());
        }
        let mut arguments = json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff.",
            "cwd": "/mnt/project-request-1"
        });
        if case == "invalid" {
            arguments.as_object_mut().unwrap().remove("task");
        } else if case == "target" {
            arguments["target"] = json!("other");
        }
        let call = NormalizedToolCall {
            id: format!("call_{case}"),
            name: "invoke_delegated_skill".into(),
            arguments,
        };
        let spawned = Arc::new(Mutex::new(0));
        let spawn_count = Arc::clone(&spawned);
        let mut events = Vec::new();
        let result = handle_invoke_delegated_skill_with_spawn(
            &mut state,
            &call,
            &call.arguments,
            &CancellationToken::new(),
            &mut |event| {
                events.push(event);
                async {}
            },
            move |_runtime, _spec, _cancel| {
                *spawn_count.lock().unwrap() += 1;
                Box::pin(async {
                    anyhow::bail!(
                        "Child Agent Process cwd '/mnt/project-request-1' is outside its delegated namespace"
                    )
                })
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            VirtualToolOutcome::Continue {
                refresh_context: true
            }
        ));
        assert_eq!(
            *spawned.lock().unwrap(),
            usize::from(matches!(case, "launch" | "publication"))
        );
        let tool_result: serde_json::Value =
            serde_json::from_str(&tool_result_text_for_call(&state, &call.id)).unwrap();
        assert!(events.iter().any(|event| matches!(event,
            Event::ToolCallCompleted { id, success: Some(false), .. } if id == &call.id
        )));
        let reason = if matches!(case, "invalid" | "unavailable") {
            assert!(tool_result["result"].is_null());
            tool_result["error"].as_str().unwrap()
        } else {
            assert_eq!(tool_result["result"]["status"], "failed");
            tool_result["result"]["summary"].as_str().unwrap()
        };
        let files = state.agent_files();
        assert!(files.action_ids().await.unwrap().is_empty());
        let notice = files.read_ui_notice_snapshot().await.unwrap();
        if case == "publication" {
            assert_eq!(notice.kind, UiNoticeKind::None);
            continue;
        }
        assert_eq!(notice.kind, UiNoticeKind::Error, "{case}");
        assert_eq!(notice.message, reason);
        crate::runtime::ui_surfaces::turn_completed(&files, false)
            .await
            .unwrap();
        crate::runtime::ui_surfaces::turn_started(&files)
            .await
            .unwrap();
        assert_eq!(
            files.read_ui_notice_snapshot().await.unwrap().kind,
            UiNoticeKind::None
        );
        let history = read_shell_utf8(&shell, "/agent/1/machine/ui/events").await;
        assert_eq!(
            history
                .lines()
                .map(|line| serde_json::from_str::<UiEvent>(line).unwrap())
                .filter(|event| matches!(event,
                    UiEvent::Error { message, recoverable: true } if message == reason
                ))
                .count(),
            1,
            "exact original rejection survives the next input: {case}"
        );
        assert!(files.action_ids().await.unwrap().is_empty());
    }
}
