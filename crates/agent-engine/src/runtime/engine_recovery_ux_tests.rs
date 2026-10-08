//! Runtime UX at the empty recovered queue boundary.
use super::*;

#[tokio::test]
async fn completed_or_legacy_recovery_accepts_fresh_agent_question_without_project_authority() {
    for (legacy, uncertain) in [(false, true), (true, true), (false, false)] {
        let temp = TempDir::new().unwrap();
        let stores = crate::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
            metadata: temp.path().join("metadata"),
        };
        let mut source =
            AgentMachine::new_with_recorder_in_dir("/agent/old", "test-model", &stores.rollouts)
                .await
                .unwrap();
        let path = source.rollout_path().unwrap().clone();
        source.add_user_message("old completed task");
        source.add_assistant_message("old completed answer", None);
        if !legacy {
            for id in ["completed-command", "unknown-command"] {
                let input = Submission {
                    id: id.into(),
                    intent: alan_agent_protocol::InputIntent::Command,
                    op: Op::Input {
                        parts: vec![ContentPart::text("echo must-not-replay")],
                        mode: InputMode::FollowUp,
                    },
                };
                source.dispatch_input(&input).await.unwrap();
            }
        }
        for (id, status) in [
            ("completed-effect", crate::rollout::EffectStatus::Applied),
            (
                "unknown-effect",
                if uncertain {
                    crate::rollout::EffectStatus::Unknown
                } else {
                    crate::rollout::EffectStatus::Applied
                },
            ),
        ] {
            let record = crate::rollout::EffectRecord {
                effect_id: id.into(),
                process_path: "/agent/old".into(),
                tool_call_id: id.into(),
                idempotency_key: format!("machine:turn:1:{id}"),
                effect_type: "process".into(),
                request_fingerprint: id.into(),
                result_digest: None,
                result_payload: None,
                status,
                applied_at: None,
                reason: None,
                dedupe_hit: false,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            if !uncertain && id == "unknown-effect" {
                // A later acknowledged result supersedes the old unknown evidence.
                let mut started = record.clone();
                started.status = crate::rollout::EffectStatus::Unknown;
                source.record_effect(started);
            }
            source.record_effect(record);
        }
        source.input_recorder().unwrap().close().await.unwrap();
        let recovered = AgentMachine::load_from_rollout_in_dir(
            &path,
            "/agent/probe",
            "test-model",
            &stores.rollouts,
        )
        .await
        .unwrap();
        {
            let queue = recovered.input_queue();
            let queue = queue.lock().unwrap();
            assert!(queue.pending.is_empty());
            assert!(!queue.paused);
            assert!(!queue.recovered);
        }
        let mock = MockLlmProvider::new();
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(mock.clone()));
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/agent/2",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/2", "default");
        assert!(
            environment.tool_execution().execution_binding().is_none(),
            "no inherited project authority"
        );
        let mut core = crate::Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("test-model"),
        );
        core.memory.enabled = false;
        core.streaming_mode = crate::config::StreamingMode::Off;
        let capabilities = crate::provider_capabilities_for_config(&core);
        let mut runtime = spawn_with_namespace_environment(
            AgentProcessConfig {
                agent_config: crate::AgentConfig::from(core),
                store_bindings: Some(stores.clone()),
                recovery_rollout_path: Some(path),
                ..Default::default()
            },
            environment,
            crate::skills::SkillHostCapabilities::default(),
            capabilities,
        )
        .unwrap();
        let metadata = runtime.wait_until_ready().await.unwrap();
        let new_path = metadata.rollout_path.unwrap();
        let activity: alan_agent_protocol::UiActivitySnapshot =
            serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await.unwrap())
                .unwrap();
        assert_eq!(activity.state, alan_agent_protocol::UiActivityState::Idle);
        let notice = shell.cat("/agent/2/machine/ui/notice").await.unwrap();
        let notice: alan_agent_protocol::UiNoticeSnapshot =
            serde_json::from_slice(&notice).unwrap();
        assert_eq!(
            notice.kind,
            if uncertain {
                alan_agent_protocol::UiNoticeKind::Warning
            } else {
                alan_agent_protocol::UiNoticeKind::None
            }
        );
        if uncertain {
            assert!(notice.message.contains("unknown outcomes"));
            let events =
                String::from_utf8(shell.cat("/agent/2/machine/ui/events").await.unwrap()).unwrap();
            assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(), alan_agent_protocol::UiEvent::Notice { snapshot } if snapshot == notice)));
        }
        assert!(
            mock.recorded_requests().is_empty(),
            "history must not start generation or effects"
        );
        // Real file-native plain Agent intake: no /continue or project mount.
        shell
            .write("/agent/2/io/input", b"What is two plus two?")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let events =
                    String::from_utf8(shell.cat("/agent/2/machine/ui/events").await.unwrap())
                        .unwrap();
                if events.lines().any(|line| {
                    matches!(
                        serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
                        alan_agent_protocol::UiEvent::InputCompleted {
                            status: alan_agent_protocol::UiInputStatus::Completed,
                            ..
                        }
                    )
                }) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("fresh question should complete without continue or authority");
        runtime.shutdown().await.unwrap();
        let requests = mock.recorded_requests();
        assert_eq!(requests.len(), 1, "only the fresh question generates");
        assert!(format!("{:?}", requests[0]).contains("What is two plus two?"));
        let action_ids = shell.ls("/agent/2/actions").await.unwrap();
        assert!(
            !action_ids.iter().any(|id| id.starts_with('a')),
            "completed and unknown effects must not spawn actions: {action_ids:?}"
        );
        let history = crate::rollout::RolloutRecorder::load_history(&new_path)
            .await
            .unwrap();
        assert_eq!(history.iter().filter(|item| matches!(item, crate::rollout::RolloutItem::Event(event) if event.event_type == "machine_input_dispatched_v1")).count(), if legacy { 1 } else { 3 });
        let effects: Vec<_> = history
            .iter()
            .filter_map(|item| match item {
                crate::rollout::RolloutItem::Effect(effect) => Some(effect),
                _ => None,
            })
            .collect();
        assert_eq!(
            effects.len(),
            if uncertain { 2 } else { 3 },
            "no replayed effect records"
        );
        assert!(
            effects
                .iter()
                .any(|effect| effect.effect_id == "completed-effect"
                    && matches!(effect.status, crate::rollout::EffectStatus::Applied))
        );
        assert!(
            effects
                .iter()
                .any(|effect| effect.effect_id == "unknown-effect"
                    && (matches!(effect.status, crate::rollout::EffectStatus::Unknown)
                        == uncertain))
        );
    }
}
