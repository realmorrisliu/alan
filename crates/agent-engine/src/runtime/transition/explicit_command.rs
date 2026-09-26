use super::*;
use anyhow::Context;

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
    let command = alan_agent_protocol::parts_to_text(&parts);
    if command.trim().is_empty() {
        emit(Event::Error {
            message: "missing command after ! prefix".to_string(),
            recoverable: true,
        })
        .await;
        return Ok(());
    }

    anyhow::ensure!(
        mode == alan_agent_protocol::InputMode::FollowUp,
        "command steering and next-turn scheduling require ordered queue admission"
    );
    anyhow::ensure!(
        !state.machine.is_turn_active() && !state.machine.has_pending_interaction(),
        "command follow-up requires an idle Machine until ordered admission is integrated"
    );
    crate::runtime::turn_support::reset_turn_after_cancelling_host_mounts(
        &mut state.machine,
        &state.environment.host_mount_requests(),
    )
    .await?;
    state.machine.add_user_message_parts(parts);
    let agent_files = state.agent_files();
    agent_files
        .write_user_state(&command)
        .await
        .context("write explicit command submission to Agent tape")?;
    crate::runtime::ui_surfaces::turn_started(&agent_files)
        .await
        .context("write explicit command start UI state")?;

    let tool_call = NormalizedToolCall {
        id: submission_id.clone(),
        name: "bash".to_string(),
        arguments: serde_json::json!({"command": command.clone()}),
    };
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
        return finish_standalone_cd(state, &submission_id, &command, outcome, emit).await;
    }

    state
        .machine
        .add_assistant_message_with_tool_calls_and_reasoning(
            "",
            vec![crate::tape::ToolRequest {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
            }],
            None,
            None,
            &[],
        );
    let mut loop_guard = ToolLoopGuard::new(None, state.runtime_config.tool_repeat_limit);
    state.machine.set_turn_activity(TurnActivityState::Running);
    let mut command_error = None;
    let outcome = {
        let mut command_emit = |event: Event| {
            if let Event::Error { message, .. } = &event {
                command_error.get_or_insert_with(|| message.clone());
            }
            emit(event)
        };
        super::orchestrate_tool_call(
            &mut loop_guard,
            state,
            &tool_call,
            ToolOrchestratorInputs {
                cancel,
                steering_broker,
            },
            false,
            false,
            &mut command_emit,
        )
        .await
    };
    match outcome {
        Ok(ToolOrchestratorOutcome::ContinueToolBatch { .. })
        | Ok(ToolOrchestratorOutcome::EndTurn) => {
            let error = command_error.as_deref().or_else(|| {
                cancel
                    .is_cancelled()
                    .then_some("Command interrupted; completed changes are preserved")
            });
            record_missing_command_action(state, &tool_call, error, None).await?;
            state.machine.set_turn_activity(TurnActivityState::Idle);
            Ok(())
        }
        Ok(ToolOrchestratorOutcome::PauseTurn) => {
            retain_pending_tool_batch(state, std::slice::from_ref(&tool_call));
            state.machine.set_turn_activity(TurnActivityState::Paused);
            Ok(())
        }
        Err(error) => {
            state.machine.set_turn_activity(TurnActivityState::Idle);
            record_missing_command_action(state, &tool_call, Some(&error.to_string()), None)
                .await?;
            Err(error)
        }
    }
}

pub(super) async fn replay_command<E, F>(
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    approved_unknown_effect_call_id: Option<&str>,
    approved_tool_escalation_call_id: Option<&str>,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    crate::runtime::ui_surfaces::resumed(&state.agent_files())
        .await
        .context("write resumed command UI state")?;
    state.machine.set_turn_activity(TurnActivityState::Running);
    let result = replay_approved_tool_batch_with_cancel(
        state,
        tool_calls,
        approved_unknown_effect_call_id,
        approved_tool_escalation_call_id,
        inputs,
        emit,
    )
    .await;
    match result {
        Ok(ToolBatchOrchestratorOutcome::PauseTurn) => {
            retain_pending_tool_batch(state, tool_calls);
            state.machine.set_turn_activity(TurnActivityState::Paused);
            Ok(())
        }
        outcome => {
            state.machine.set_turn_activity(TurnActivityState::Idle);
            let error = outcome.as_ref().err().map(ToString::to_string);
            for call in tool_calls {
                let approval = (approved_unknown_effect_call_id == Some(call.id.as_str())
                    || approved_tool_escalation_call_id == Some(call.id.as_str()))
                .then_some("approved");
                record_missing_command_action(state, call, error.as_deref(), approval).await?;
            }
            outcome.map(|_| ())
        }
    }
}

async fn record_missing_command_action(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    emitted_error: Option<&str>,
    approval: Option<&str>,
) -> Result<()> {
    let payload = state.machine.tool_payload_by_call_id(&tool_call.id);
    if payload
        .as_ref()
        .and_then(|payload| payload.get("action_id"))
        .and_then(serde_json::Value::as_str)
        .is_some()
    {
        return Ok(());
    }

    let message = payload
        .as_ref()
        .and_then(|payload| payload.get("error"))
        .and_then(serde_json::Value::as_str)
        .or(emitted_error)
        .unwrap_or("command did not produce a correlated Action result")
        .to_string();
    let missing_tool_response = payload.is_none();
    let outcome = payload.unwrap_or_else(|| {
        serde_json::json!({
            "success": false,
            "error": message.clone(),
        })
    });
    if missing_tool_response {
        state
            .machine
            .add_tool_message(&tool_call.id, &tool_call.name, outcome.clone());
    }
    let mut action = NamespaceActionRecord::new(&tool_call.name, "failed")
        .with_output(serde_json::json!({"stdout": "", "stderr": message}).to_string())
        .with_result(
            serde_json::json!({
                "call_id": tool_call.id,
                "exit_code": 1,
                "outcome": outcome,
            })
            .to_string(),
        );
    if let Some(process) = outcome.get("process").and_then(serde_json::Value::as_str) {
        action = action.with_process(process);
    }
    if let Some(approval) = approval {
        action = action.with_approval(approval);
    }
    state.agent_files().write_action(action).await?;
    Ok(())
}

pub(super) async fn finish_failed_explicit_command<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    error: &str,
    approval: Option<&str>,
    emit: &mut E,
) -> Result<()>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    record_missing_command_action(state, tool_call, Some(error), approval).await?;
    let result = serde_json::json!({"success": false, "error": error});
    emit(Event::ToolCallCompleted {
        presentation: None,
        id: tool_call.id.clone(),
        name: Some(tool_call.name.clone()),
        success: Some(false),
        result_preview: crate::runtime::turn_support::tool_result_preview(&result),
        audit: None,
    })
    .await;
    state.machine.set_turn_activity(TurnActivityState::Idle);
    Ok(())
}

async fn finish_standalone_cd<E, F>(
    state: &mut RuntimeLoopState,
    submission_id: &str,
    command: &str,
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
    let exit_code = if success { 0 } else { 1 };
    let stderr = if success {
        ""
    } else {
        payload["error"].as_str().unwrap_or("standalone cd failed")
    };
    let output = serde_json::json!({"stdout": "", "stderr": stderr});
    let arguments = serde_json::json!({"operation": "cd"});
    state
        .machine
        .add_assistant_message_with_tool_calls_and_reasoning(
            "",
            vec![crate::tape::ToolRequest {
                id: submission_id.to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"command": command}),
            }],
            None,
            None,
            &[],
        );
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
                .with_output(output.to_string())
                .with_result(
                    serde_json::json!({
                        "call_id": submission_id,
                        "exit_code": exit_code,
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
    state.machine.set_turn_activity(TurnActivityState::Idle);
    Ok(())
}

pub(super) fn retain_pending_tool_batch(
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
) {
    if let Some(pending) = state.machine.pending_confirmation()
        && replays_tool_calls(&pending.checkpoint_type)
    {
        state
            .machine
            .set_tool_replay_batch(pending.checkpoint_id, tool_calls.to_vec(), false);
    }
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
            "cd src",
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
        let output: serde_json::Value =
            serde_json::from_slice(&shell.cat(&format!("{action_path}/output")).await.unwrap())
                .unwrap();
        assert_eq!(result["call_id"], submission_id);
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["outcome"]["cwd"], "/mnt/project/src");
        assert_eq!(output["stdout"], "");
        assert_eq!(output["stderr"], "");
        assert_eq!(
            state
                .machine
                .tool_payload_by_call_id(submission_id)
                .unwrap()["cwd"],
            "/mnt/project/src"
        );
        assert!(matches!(
            state.machine.messages().first(),
            Some(crate::tape::Message::Assistant { tool_requests, .. })
                if tool_requests.len() == 1
                    && tool_requests[0].id == submission_id
                    && tool_requests[0].name == "bash"
                    && tool_requests[0].arguments["command"] == "cd src"
        ));
        assert!(matches!(
            state.machine.messages().get(1),
            Some(crate::tape::Message::Tool { responses })
                if responses.len() == 1 && responses[0].id == submission_id
        ));
    }
}
