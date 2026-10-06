//! Directory controls never enter ordinary admission or change Machine activity.
use super::*;

pub(crate) fn select_project_directory(
    state: &mut RuntimeLoopState,
    submission: &Submission,
) -> Result<NamespaceActionRecord> {
    let alan_agent_protocol::Op::SelectProjectDirectory { path } = &submission.op else {
        anyhow::bail!("not a directory control");
    };
    // Resolve the owning Process before any cwd effect; this is pure path validation.
    let process = state.environment.process_files().process_path()?;
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
    cd_action_record(&submission.id, "Select Process directory", outcome, process)
}

pub(crate) async fn write_cd_action(
    files: &NamespaceAgentFiles,
    id: &str,
    command: &str,
    outcome: Result<std::path::PathBuf>,
    process: String,
) -> Result<()> {
    files
        .write_action(cd_action_record(id, command, outcome, process)?)
        .await?;
    Ok(())
}

pub(in crate::runtime) fn cd_action_record(
    id: &str,
    command: &str,
    outcome: Result<std::path::PathBuf>,
    process: String,
) -> Result<NamespaceActionRecord> {
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
    Ok(
        NamespaceActionRecord::new("cd", if success { "completed" } else { "failed" })
            .with_output(
                serde_json::json!({"stdout":"","stderr":payload["error"].as_str().unwrap_or("")})
                    .to_string(),
            )
            .with_result(result.to_string())
            .with_approval("not_required")
            .with_process(process),
    )
}
