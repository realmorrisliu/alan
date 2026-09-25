use super::*;

pub(super) async fn handle_explicit_command<E, F>(
    state: &mut RuntimeLoopState,
    submission_id: String,
    op: Op,
    emit: &mut E,
    cancel: &CancellationToken,
    steering_broker: Option<&TurnInputBroker>,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let Op::Input { parts, mode } = op else {
        anyhow::bail!("command intent requires an input operation");
    };
    if mode != InputMode::FollowUp {
        emit(Event::Error {
            message: "command intent requires follow_up scheduling".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(());
    }
    let command = alan_agent_protocol::parts_to_text(&parts);
    if command.trim().is_empty() {
        emit(Event::Error {
            message: "missing command after ! prefix".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(());
    }

    state.machine.add_user_message_parts(parts);
    let standalone_cd = crate::tools::parse_standalone_cd(&command);
    if !matches!(&standalone_cd, Ok(None)) {
        emit(Event::ToolCallStarted {
            title: Some("cd".to_string()),
            id: submission_id.clone(),
            name: "cd".to_string(),
            audit: None,
        })
        .await;
        state.machine.set_turn_activity(TurnActivityState::Running);
        let outcome = match standalone_cd {
            Ok(Some(_directory)) if cancel.is_cancelled() => Err(anyhow::anyhow!(
                "standalone cd was cancelled before execution"
            )),
            Ok(Some(directory)) => state.environment.change_process_directory(&directory),
            Err(error) => Err(error),
            Ok(None) => unreachable!("standalone cd match excludes no-op inputs"),
        };
        return finish_standalone_cd(state, &submission_id, outcome, emit).await;
    }

    let tool_call = NormalizedToolCall {
        id: submission_id,
        name: "bash".to_string(),
        arguments: serde_json::json!({"command": command}),
    };
    let mut loop_guard = ToolLoopGuard::new(None, state.runtime_config.tool_repeat_limit);
    state.machine.set_turn_activity(TurnActivityState::Running);
    let outcome = super::orchestrate_tool_batch(
        &mut loop_guard,
        state,
        std::slice::from_ref(&tool_call),
        ToolOrchestratorInputs {
            cancel,
            steering_broker,
        },
        emit,
    )
    .await;
    match outcome {
        Ok(ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. })
        | Ok(ToolBatchOrchestratorOutcome::EndTurn { .. }) => {
            state.machine.set_turn_activity(TurnActivityState::Idle);
            Ok(())
        }
        Ok(ToolBatchOrchestratorOutcome::PauseTurn) => {
            state.machine.set_turn_activity(TurnActivityState::Paused);
            Ok(())
        }
        Err(error) => {
            state.machine.set_turn_activity(TurnActivityState::Idle);
            Err(error)
        }
    }
}

async fn finish_standalone_cd<E, F>(
    state: &mut RuntimeLoopState,
    submission_id: &str,
    outcome: Result<std::path::PathBuf>,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let (success, payload, action_status) = match outcome {
        Ok(namespace_cwd) => (
            true,
            serde_json::json!({"success": true, "cwd": namespace_cwd}),
            "completed",
        ),
        Err(error) => (
            false,
            serde_json::json!({"success": false, "error": error.to_string()}),
            "failed",
        ),
    };
    let arguments = serde_json::json!({"operation": "cd"});
    state
        .machine
        .record_tool_call("cd", arguments.clone(), payload.clone(), success);
    state
        .machine
        .add_tool_message(submission_id, "cd", payload.clone());
    let process_path = state.environment.process_files().process_path()?;
    state
        .agent_files()
        .write_action(
            NamespaceActionRecord::new("cd", action_status)
                .with_result(
                    serde_json::json!({
                        "call_id": submission_id,
                        "outcome": payload,
                    })
                    .to_string(),
                )
                .with_approval("not_required")
                .with_process(process_path),
        )
        .await?;

    emit(Event::ToolCallCompleted {
        presentation: None,
        id: submission_id.to_string(),
        name: Some("cd".to_string()),
        success: Some(success),
        result_preview: crate::runtime::turn_support::tool_result_preview(&payload),
        audit: None,
    })
    .await;
    if !success {
        emit(Event::Error {
            message: payload["error"]
                .as_str()
                .unwrap_or("standalone cd failed")
                .to_string(),
            recoverable: true,
        })
        .await;
    }
    state.machine.set_turn_activity(TurnActivityState::Idle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agentfs::AgentFs;
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use alan_shell::Shell;
    use std::{path::PathBuf, sync::Arc};

    #[tokio::test]
    async fn standalone_cd_records_correlated_action_against_agent_process() {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let shell = Shell::new(root.clone());
        let mut state = RuntimeLoopState {
            machine: AgentMachine::new(),
            environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            core_config: Config::default(),
            runtime_config: RuntimeConfig::default(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        };
        let submission_id = "submission-cd-1";
        let mut emit = |_event: Event| async {};

        finish_standalone_cd(
            &mut state,
            submission_id,
            Ok(PathBuf::from("/mnt/project/src")),
            &mut emit,
        )
        .await
        .unwrap();

        let action_path = "/agent/1/actions/a0";
        assert_eq!(
            String::from_utf8(shell.cat(&format!("{action_path}/name")).await.unwrap()).unwrap(),
            "cd"
        );
        assert_eq!(
            String::from_utf8(shell.cat(&format!("{action_path}/process")).await.unwrap()).unwrap(),
            "/proc/1"
        );
        let result: serde_json::Value =
            serde_json::from_slice(&shell.cat(&format!("{action_path}/result")).await.unwrap())
                .unwrap();
        assert_eq!(result["call_id"], submission_id);
        assert_eq!(result["outcome"]["cwd"], "/mnt/project/src");
        assert_eq!(
            state
                .machine
                .tool_payload_by_call_id(submission_id)
                .unwrap()["cwd"],
            "/mnt/project/src"
        );
    }
}
