// One adapter/authority fixture exercises the real handler -> cwd binding seam,
// without native shell execution or changes to production cwd control.
#[derive(Debug)]
struct MetadataCwdAdapter;
impl crate::tools::ToolExecutionAdapter for MetadataCwdAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        PathBuf::from("/mnt/source")
    }
    fn cwd(&self) -> anyhow::Result<PathBuf> {
        Ok(PathBuf::from("/tmp"))
    }
    fn resolve_path(
        &self,
        cwd: &std::path::Path,
        path: &std::path::Path,
    ) -> anyhow::Result<PathBuf> {
        Ok(cwd.join(path))
    }
    fn resolve_directory(
        &self,
        cwd: &std::path::Path,
        path: &std::path::Path,
    ) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(
            path == std::path::Path::new("src"),
            "directory does not exist: {}",
            path.display()
        );
        Ok(cwd.join(path))
    }
    fn visible_path(&self, path: &std::path::Path) -> PathBuf {
        path.to_path_buf()
    }
    fn project_text(&self, text: &str) -> String {
        text.to_string()
    }
    fn sandbox(&self) -> anyhow::Result<crate::tools::Sandbox> {
        Ok(crate::tools::Sandbox::new(PathBuf::from("/tmp")))
    }
}
#[derive(Debug)]
struct MetadataCwdAuthority;
impl crate::tools::ToolExecutionAuthority for MetadataCwdAuthority {
    fn reconcile(
        &self,
        _pid: u64,
        binding: crate::tools::ToolExecutionBinding,
    ) -> anyhow::Result<crate::tools::ToolExecutionBinding> {
        Ok(binding)
    }
}

#[tokio::test]
async fn standalone_cd_handler_projects_actual_cwd_and_failure_without_command_presentation() {
    let (mut state, shell) = create_namespace_test_state_and_shell().await;
    let tools = ToolRegistry::new();
    let runner = crate::tools::ToolProcessRunner::from_registry(&tools);
    let binding = crate::tools::ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/mnt/source"),
        PathBuf::from("/tmp"),
    )
    .with_adapter(Arc::new(MetadataCwdAdapter));
    runner.register_process_binding(1, binding);
    runner.register_process_authority(1, Arc::new(MetadataCwdAuthority));
    state.environment = state
        .environment
        .clone()
        .with_tool_process_context(1, runner);
    for (id, command, success) in [
        ("cd-ok", "cd src", true),
        ("cd-failed", "cd missing", false),
    ] {
        state.machine.accept_submission(id.to_string());
        let mut emit = |_event: Event| async {};
        handle_submission_with_cancel(
            &mut state,
            Submission {
                id: id.to_string(),
                intent: alan_agent_protocol::InputIntent::Command,
                op: Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text(command)],
                    mode: InputMode::FollowUp,
                },
            },
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let ids = state.agent_files().action_ids().await.unwrap();
        let base = format!("/agent/1/actions/{}", ids.last().unwrap());
        let result: Value =
            serde_json::from_slice(&shell.cat(&format!("{base}/result")).await.unwrap()).unwrap();
        assert_eq!(result["call_id"], id);
        assert_eq!(result["title"], command);
        assert_eq!(result["exit_code"], if success { 0 } else { 1 });
        assert_eq!(result["outcome"]["success"], success);
        assert!(result.get("presentation").is_none());
        assert_eq!(
            shell.cat(&format!("{base}/process")).await.unwrap(),
            b"/proc/1"
        );
        assert_eq!(
            shell.cat(&format!("{base}/approval")).await.unwrap(),
            b"not_required"
        );
        assert_eq!(
            shell.cat(&format!("{base}/status")).await.unwrap(),
            if success {
                b"completed".as_slice()
            } else {
                b"failed".as_slice()
            }
        );
        if success {
            assert_eq!(result["outcome"]["cwd"], "/mnt/source/src");
            assert_eq!(result["result_preview"], "cwd: /mnt/source/src");
        } else {
            assert!(
                result["result_preview"]
                    .as_str()
                    .unwrap()
                    .contains("directory does not exist")
            );
        }
        assert_eq!(
            state.tool_execution().default_cwd().unwrap(),
            PathBuf::from("/mnt/source/src")
        );
    }
}
