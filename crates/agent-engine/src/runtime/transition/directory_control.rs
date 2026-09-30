//! Directory controls never enter ordinary admission or change Machine activity.
use super::*;

pub(crate) async fn select_project_directory(
    state: &mut RuntimeLoopState,
    submission: &Submission,
) -> Result<bool> {
    let alan_agent_protocol::Op::SelectProjectDirectory { path } = &submission.op else {
        return Ok(false);
    };
    let outcome = (|| {
        anyhow::ensure!(
            submission.intent == alan_agent_protocol::InputIntent::Agent,
            "directory selection requires control intent"
        );
        anyhow::ensure!(
            uuid::Uuid::parse_str(&submission.id).is_ok(),
            "invalid directory selection UUID"
        );
        anyhow::ensure!(
            namespace_environment::valid_project_directory(path),
            "invalid absolute namespace directory"
        );
        anyhow::ensure!(
            !state.machine.is_turn_active() && !state.machine.has_pending_interaction(),
            "directory selection requires settled work"
        );
        state
            .environment
            .change_process_directory(std::path::Path::new(path))
    })();
    write_cd_action(
        &state.agent_files(),
        &submission.id,
        "Select Process directory",
        outcome,
        state.environment.process_files().process_path()?,
    )
    .await?;
    Ok(true)
}

pub(crate) async fn write_cd_action(
    files: &NamespaceAgentFiles,
    id: &str,
    command: &str,
    outcome: Result<std::path::PathBuf>,
    process: String,
) -> Result<()> {
    let (success, payload) = match outcome {
        Ok(cwd) => (true, serde_json::json!({"success":true,"cwd":cwd})),
        Err(error) => (
            false,
            serde_json::json!({"success":false,"error":error.to_string()}),
        ),
    };
    let mut result =
        serde_json::json!({"call_id":id,"exit_code":if success {0} else {1},"outcome":payload});
    crate::runtime::tool_presentation::write_action_metadata(
        &mut result,
        "cd",
        &serde_json::json!({"command":command}),
        &payload,
    )?;
    files.write_action(NamespaceActionRecord::new("cd", if success {"completed"} else {"failed"})
        .with_output(serde_json::json!({"stdout":"","stderr":payload["error"].as_str().unwrap_or("")}).to_string())
        .with_result(result.to_string()).with_approval("not_required").with_process(process)).await?;
    Ok(())
}
