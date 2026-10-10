#[derive(Debug)]
struct PresentationAuthority {
    epoch: Arc<AtomicUsize>,
    oversized: bool,
}

impl crate::tools::ToolExecutionAuthority for PresentationAuthority {
    fn reconcile(
        &self,
        _pid: u64,
        binding: crate::tools::ToolExecutionBinding,
    ) -> anyhow::Result<crate::tools::ToolExecutionBinding> {
        Ok(binding)
    }

    fn presentation_scope(&self, _pid: u64) -> Option<String> {
        Some(if self.oversized {
            "x".repeat(4097)
        } else {
            self.epoch.load(Ordering::SeqCst).to_string()
        })
    }
}

struct PresentationProbe {
    readonly: bool,
    capability: ToolCapability,
    epoch: Arc<AtomicUsize>,
    count: Arc<AtomicUsize>,
}

impl Tool for PresentationProbe {
    fn name(&self) -> &str {
        "probe"
    }
    fn description(&self) -> &str {
        "Presentation fixture"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object"})
    }
    fn presentation_is_read_only(&self) -> bool {
        self.readonly
    }
    fn capability(&self, _arguments: &Value) -> ToolCapability {
        self.capability
    }
    fn execute(&self, args: Value, _context: &ToolContext) -> ToolResult {
        let epoch = self.epoch.clone();
        let count = self.count.clone();
        Box::pin(async move {
            count.fetch_add(1, Ordering::SeqCst);
            if args["change_scope"] == true {
                epoch.fetch_add(1, Ordering::SeqCst);
            }
            if args["fail"] == true {
                anyhow::bail!("probe failed");
            }
            Ok(json!({"success": true, "read_only_context": {"authority": "forged Tool claim"}}))
        })
    }
}

#[tokio::test]
async fn read_only_action_metadata_requires_native_execution_and_unchanged_scope() {
    for case in [
        "eligible",
        "different_runner",
        "no_authority",
        "approved",
        "no_submission",
        "write",
        "unclassified",
        "failure",
        "changed_scope",
        "no_adapter",
        "wrong_parent",
        "oversized_scope",
    ] {
        let epoch = Arc::new(AtomicUsize::new(1));
        let count = Arc::new(AtomicUsize::new(0));
        let mut tools = ToolRegistry::new();
        tools.register(PresentationProbe {
            readonly: case != "unclassified",
            capability: if case == "write" {
                ToolCapability::Write
            } else {
                ToolCapability::Read
            },
            epoch: epoch.clone(),
            count: count.clone(),
        });
        let runner = crate::tools::ToolProcessRunner::from_registry(&tools);
        let mut state = create_test_state_with_native_runner(
            AgentMachine::new(),
            tools,
            SimpleMockProvider,
            "/agent/1",
            (case != "different_runner").then_some(runner.clone()),
        )
        .await;
        // Same native metadata context with a different actual runner must not qualify.
        runner.register_process_binding(
            1,
            crate::tools::test_execution_binding(
                "/mnt/source",
                "/tmp".into(),
                "/tmp/alan-group-probe".into(),
            ),
        );
        if case == "no_adapter" {
            runner.register_process_binding(
                1,
                crate::tools::ToolExecutionBinding::awaiting_host_projection(
                    "/mnt/source".into(),
                    "/tmp".into(),
                ),
            );
        }
        if case != "no_authority" {
            runner.register_process_authority(
                1,
                Arc::new(PresentationAuthority {
                    epoch,
                    oversized: case == "oversized_scope",
                }),
            );
        }
        state.environment = state
            .environment
            .clone()
            .with_tool_process_context(if case == "wrong_parent" { 99 } else { 1 }, runner.clone());
        let shell = Shell::new(state.environment.root_transport());
        let args = json!({"fail": case == "failure", "change_scope": case == "changed_scope"});
        let result = state
            .tool_execution()
            .run_action_with_cancel_and_timeout(
                "probe",
                Some(NamespaceToolActionEvidence {
                    call_id: "call-q",
                    arguments: &args,
                    approval: if case == "approved" {
                        "approved"
                    } else {
                        "not_required"
                    },
                    submission_id: (case != "no_submission").then_some("submission-q"),
                }),
                "/bin/probe",
                [args.to_string()],
                &CancellationToken::new(),
                30,
            )
            .await
            .unwrap();
        let metadata: Value = serde_json::from_slice(
            &shell
                .cat(&format!("/agent/1/actions/{}/result", result.action_id))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1, "{case}");
        if case == "eligible" {
            let context: alan_agent_protocol::ActionReadOnlyContext =
                serde_json::from_value(metadata["read_only_context"].clone()).unwrap();
            assert!(context.is_valid());
            assert_eq!(context.owner, "/agent/1");
            assert_eq!(context.submission, "submission-q");
            assert_eq!(context.authority.len(), 64);
            assert!(!context.authority.contains("forged"));
            runner.register_process_binding(
                1,
                crate::tools::test_execution_binding(
                    "/mnt/other",
                    "/tmp".into(),
                    "/tmp/alan-group-probe".into(),
                ),
            );
            assert_ne!(
                runner
                    .read_only_presentation_scope(1, "probe", &args)
                    .unwrap(),
                context.authority
            );
            for pid in 1000..1130 {
                let outcome = runner
                    .run(crate::tools::ToolProcessInvocation {
                        pid,
                        parent: Some(1),
                        executable: "/bin/probe".into(),
                        args: vec!["{}".into()],
                    })
                    .await;
                assert_eq!(outcome.exit_code, 0);
            }
            assert!(runner.take_read_only_receipt(1000, 1, "probe").is_none());
            assert!(runner.take_read_only_receipt(1129, 2, "probe").is_none());
            assert!(runner.take_read_only_receipt(1129, 1, "probe").is_none());
            assert!(
                runner
                    .take_read_only_receipt(1128, 1, "different_tool")
                    .is_none()
            );
            assert!(runner.take_read_only_receipt(1127, 1, "probe").is_some());
            assert!(runner.take_read_only_receipt(1127, 1, "probe").is_none());
        } else {
            assert!(
                metadata.get("read_only_context").is_none(),
                "{case}: {metadata}"
            );
        }
        assert!(
            runner
                .take_read_only_receipt(result.pid.parse().unwrap(), 1, "probe")
                .is_none()
        );
    }
}
