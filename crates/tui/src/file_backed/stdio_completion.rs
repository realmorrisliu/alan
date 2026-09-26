#[cfg(test)]
use super::StdioTaskCompletion;
use super::{
    StdioTaskSnapshot, StdioTaskWaitContext,
    file_surface::{ActionSnapshot, TapeRecordV1},
};
use alan_agent_protocol::UiActivityState;
use anyhow::{Context, Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandResult {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) exit_code: i32,
}

pub(super) fn command_result_for_submission(
    submission_id: &str,
    actions: &[ActionSnapshot],
) -> Result<Option<CommandResult>> {
    for action in actions.iter().rev() {
        if !matches!(action.status.as_str(), "completed" | "failed") {
            continue;
        }
        let Ok(result) = serde_json::from_str::<serde_json::Value>(&action.result) else {
            continue;
        };
        if result.get("call_id").and_then(serde_json::Value::as_str) != Some(submission_id) {
            continue;
        }

        let exit_code = result
            .get("exit_code")
            .and_then(serde_json::Value::as_i64)
            .and_then(|code| i32::try_from(code).ok())
            .ok_or_else(|| anyhow!("correlated command Action has no valid exit code"))?;
        let output: serde_json::Value = serde_json::from_str(&action.output)
            .context("parse correlated command Action output")?;
        let stdout = output
            .get("stdout")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("correlated command Action has no stdout result"))?;
        let stderr = output
            .get("stderr")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("correlated command Action has no stderr result"))?;

        return Ok(Some(CommandResult {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            exit_code,
        }));
    }
    Ok(None)
}

pub(super) fn tape_outcome(
    submission_id: &str,
    tape_history: &[u8],
) -> Result<(bool, Option<String>)> {
    let tape_history = std::str::from_utf8(tape_history).context("machine/tape is not utf8")?;
    let records = tape_history
        .lines()
        .filter_map(|line| serde_json::from_str::<TapeRecordV1>(line).ok())
        .filter(|record| record.kind == "message")
        .collect::<Vec<_>>();
    let started = records.iter().any(|record| {
        record.role == "user" && record.submission_id.as_deref() == Some(submission_id)
    });
    let answer = records
        .iter()
        .rev()
        .find(|record| {
            record.role == "assistant" && record.submission_id.as_deref() == Some(submission_id)
        })
        .map(|record| record.content.clone());

    Ok((started, answer))
}

/// Error events can precede Tape admission; only their explicit identity is evidence.
pub(super) fn submission_error(submission_id: &str, ui_history: &[u8]) -> Result<Option<String>> {
    let history = std::str::from_utf8(ui_history).context("ui events are not utf8")?;
    let mut error = None;
    for line in history.lines().filter(|line| !line.trim().is_empty()) {
        if let alan_agent_protocol::UiEvent::Error {
            submission_id: Some(id),
            message,
            ..
        } = serde_json::from_str(line).context("parse Agent UI event")?
            && id == submission_id
        {
            error = Some(message);
        }
    }
    Ok(error)
}

/// Activity is run state; only matching Tape/Action/error evidence can complete a task.
pub(super) fn observe_activity(
    task: &StdioTaskWaitContext,
    snapshot: &mut StdioTaskSnapshot,
    activity: &alan_agent_protocol::UiActivitySnapshot,
) {
    if snapshot.task_error.is_some() {
        return;
    }
    if activity.version >= 2 {
        let id = task.record.submission_id.as_str();
        if activity
            .pending_submissions
            .iter()
            .any(|input| input.submission_id == id)
        {
            snapshot.task_started = true;
            snapshot.activity_state = None;
        } else if activity
            .active_submission
            .as_ref()
            .is_some_and(|input| input.submission_id == id)
        {
            snapshot.task_started = true;
            // An idle active ID is admission, unless this client already observed execution.
            if activity.state != UiActivityState::Idle
                || matches!(
                    snapshot.activity_state,
                    Some(UiActivityState::Running | UiActivityState::Paused)
                )
            {
                snapshot.activity_state = Some(activity.state);
            }
        } else if activity.active_submission.is_none()
            && activity.state == UiActivityState::Idle
            && snapshot.task_started
        {
            snapshot.activity_state = Some(UiActivityState::Idle);
        }
        return;
    }
    // Retained v1 history has no identity; the existing client lease still guards live v1 use.
    if activity.state == UiActivityState::Running
        && activity
            .started_at_ms
            .is_none_or(|time| time >= task.submitted_at_ms)
    {
        snapshot.task_started = true;
        snapshot.activity_state = Some(UiActivityState::Running);
    } else if matches!(
        snapshot.activity_state,
        Some(UiActivityState::Running | UiActivityState::Paused)
    ) {
        snapshot.activity_state = Some(activity.state);
    }
}

pub(super) async fn refresh_answer_after_idle(
    shell: &alan_shell::Shell,
    agent_path: &str,
    task: &StdioTaskWaitContext,
    snapshot: &mut StdioTaskSnapshot,
) -> Result<()> {
    if !snapshot.task_started || snapshot.activity_state != Some(UiActivityState::Idle) {
        return Ok(());
    }

    if task.record.intent == alan_agent_protocol::InputIntent::Command {
        if snapshot.command_result.is_none() {
            let actions = super::file_surface::read_action_snapshots(shell, agent_path).await?;
            snapshot.command_result =
                command_result_for_submission(&task.record.submission_id, &actions)?;
        }
        if snapshot.command_result.is_none() && snapshot.task_error.is_none() {
            return Err(anyhow!(
                "Agent task reached idle without a correlated command result; outcome is unknown"
            ));
        }
        return Ok(());
    }
    if snapshot.task_error.is_some() {
        return Ok(());
    }

    let tape = shell
        .cat(&format!("{agent_path}/machine/tape"))
        .await
        .map_err(|err| anyhow!("read final Agent tape snapshot failed: {err:?}"))?;
    let (_, answer) = tape_outcome(&task.record.submission_id, &tape)?;
    snapshot.assistant_answer = Some(answer.ok_or_else(|| {
        anyhow!("Agent task reached idle without a correlated final answer; outcome is unknown")
    })?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::{InProcessTransport, reference::MemFs};
    use alan_kernel::{Access, MountFs, Namespace};
    use std::sync::Arc;

    #[test]
    fn command_completion_uses_only_the_action_correlated_to_its_submission() {
        let action = |id: &str, call_id: &str, stdout: &str, exit_code: i32| {
            super::super::file_surface::ActionSnapshot {
                id: id.to_string(),
                name: "bash".to_string(),
                status: if exit_code == 0 {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                },
                output: serde_json::json!({"stdout": stdout, "stderr": "", "exit_code": exit_code})
                    .to_string(),
                result: serde_json::json!({"call_id": call_id, "exit_code": exit_code}).to_string(),
            }
        };

        let result = command_result_for_submission(
            "submission-2",
            &[
                action("a0", "submission-1", "old output", 0),
                action("a1", "submission-2", "new output", 7),
            ],
        )
        .unwrap()
        .unwrap();

        assert_eq!(result.stdout, "new output");
        assert_eq!(result.stderr, "");
        assert_eq!(result.exit_code, 7);
    }

    #[test]
    fn command_completion_does_not_accept_an_unrelated_or_unfinished_action() {
        let action = super::super::file_surface::ActionSnapshot {
            id: "a0".to_string(),
            name: "bash".to_string(),
            status: "running".to_string(),
            output: r#"{"stdout":"not finished","stderr":"","exit_code":0}"#.to_string(),
            result: r#"{"call_id":"other-submission","exit_code":0}"#.to_string(),
        };

        assert!(
            command_result_for_submission("submission-2", &[action])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn command_completion_rejects_a_correlated_action_with_malformed_output() {
        let action = super::super::file_surface::ActionSnapshot {
            id: "a0".to_string(),
            name: "bash".to_string(),
            status: "completed".to_string(),
            output: "not-json".to_string(),
            result: r#"{"call_id":"submission-2","exit_code":0}"#.to_string(),
        };

        assert!(command_result_for_submission("submission-2", &[action]).is_err());
    }

    #[test]
    fn tape_completion_uses_the_submission_id_not_the_prompt_text() {
        let tape = br#"{"version":1,"kind":"message","role":"user","content":"same prompt","submission_id":"old"}
{"version":1,"kind":"message","role":"assistant","content":"old answer","submission_id":"old"}
{"version":1,"kind":"message","role":"user","content":"same prompt","submission_id":"current"}
{"version":1,"kind":"message","role":"assistant","content":"current answer","submission_id":"current"}
"#;

        assert_eq!(
            tape_outcome("current", tape).unwrap(),
            (true, Some("current answer".to_string()))
        );
    }

    #[tokio::test]
    async fn idle_reloads_final_tape_record_instead_of_streamed_preamble() {
        let baseline = b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"prior task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"prior answer\"}\n";
        let tape = [
            baseline.as_slice(),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"current task\",\"submission_id\":\"submission-current\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"preamble\",\"submission_id\":\"submission-current\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"final answer\",\"submission_id\":\"submission-current\"}\n",
        ]
        .concat();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1/machine",
            InProcessTransport::new(Arc::new(MemFs::with_read_only_file("tape", tape))),
            Access::ReadOnly,
        );
        let shell =
            alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))));
        let mut record = alan_agent_protocol::UserInputRecord::new(
            alan_agent_protocol::InputIntent::Agent,
            alan_agent_protocol::InputMode::FollowUp,
            "current task",
        );
        record.submission_id = "submission-current".to_string();
        let task = StdioTaskWaitContext {
            record,
            submitted_at_ms: 20,
        };
        let mut snapshot = StdioTaskSnapshot {
            task_started: true,
            assistant_answer: Some("preamble".to_string()),
            command_result: None,
            activity_state: Some(UiActivityState::Idle),
            task_error: None,
        };

        refresh_answer_after_idle(&shell, "/agent/1", &task, &mut snapshot)
            .await
            .unwrap();

        assert_eq!(
            super::super::finish_stdio_task_if_ready(
                &mut snapshot,
                alan_agent_protocol::InputIntent::Agent,
            )
            .unwrap(),
            Some(StdioTaskCompletion::AgentAnswer("final answer".to_string()))
        );
    }

    #[tokio::test]
    async fn idle_completes_explicit_command_from_its_action_without_assistant_tape() {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1/actions/a0",
            InProcessTransport::new(Arc::new(MemFs::with_read_only_files([
                ("name".to_string(), b"bash".to_vec()),
                ("status".to_string(), b"failed".to_vec()),
                (
                    "output".to_string(),
                    br#"{"stdout":"command output\n","stderr":"command warning\n","exit_code":6}"#
                        .to_vec(),
                ),
                (
                    "result".to_string(),
                    br#"{"call_id":"submission-1","exit_code":6}"#.to_vec(),
                ),
            ]))),
            Access::ReadOnly,
        );
        let shell =
            alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))));
        let mut record = alan_agent_protocol::UserInputRecord::new(
            alan_agent_protocol::InputIntent::Command,
            alan_agent_protocol::InputMode::FollowUp,
            "printf command output",
        );
        record.submission_id = "submission-1".to_string();
        let task = StdioTaskWaitContext {
            record,
            submitted_at_ms: 20,
        };
        let mut snapshot = StdioTaskSnapshot {
            task_started: true,
            assistant_answer: Some("unrelated old answer".to_string()),
            command_result: None,
            activity_state: Some(UiActivityState::Idle),
            task_error: None,
        };

        refresh_answer_after_idle(&shell, "/agent/1", &task, &mut snapshot)
            .await
            .unwrap();

        assert_eq!(
            super::super::finish_stdio_task_if_ready(
                &mut snapshot,
                alan_agent_protocol::InputIntent::Command,
            )
            .unwrap(),
            Some(StdioTaskCompletion::Command(CommandResult {
                stdout: "command output\n".to_string(),
                stderr: "command warning\n".to_string(),
                exit_code: 6,
            }))
        );
    }
    #[test]
    fn one_shot_ignores_other_client_and_uncorrelated_failures() {
        let task = StdioTaskWaitContext::new("my input");
        let history = br#"{"type":"activity","snapshot":{"version":1,"state":"running","started_at_ms":18446744073709551615}}
    {"type":"error","message":"other failure","recoverable":true,"submission_id":"other-client"}
    {"type":"error","message":"legacy failure","recoverable":true}
    {"type":"activity","snapshot":{"version":1,"state":"idle"}}
    "#;
        let mut snapshot =
            super::super::stdio_task_snapshot_from_history(&task, b"", history).unwrap();
        assert!(snapshot.task_error.is_none());
        assert!(
            super::super::finish_stdio_task_if_ready(
                &mut snapshot,
                alan_agent_protocol::InputIntent::Agent
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn v2_activity_tracks_only_the_submitted_id_and_never_substitutes_for_a_result() {
        use alan_agent_protocol::{InputIntent, UiActivitySnapshot, UiSubmission};
        let task = StdioTaskWaitContext::new("my task");
        let mine = UiSubmission {
            submission_id: task.record.submission_id.clone(),
            intent: InputIntent::Agent,
        };
        let other = UiSubmission {
            submission_id: "another-client".into(),
            intent: InputIntent::Command,
        };
        let mut snapshot = super::super::stdio_task_snapshot_from_history(&task, b"", b"").unwrap();
        let mut activity = UiActivitySnapshot::running(u64::MAX);
        activity.active_submission = Some(other.clone());
        observe_activity(&task, &mut snapshot, &activity);
        assert!(!snapshot.task_started);
        assert!(snapshot.activity_state.is_none());
        activity.pending_submissions.push(mine.clone());
        observe_activity(&task, &mut snapshot, &activity);
        assert!(snapshot.task_started);
        assert!(
            snapshot.activity_state.is_none(),
            "accepted is not running or completed"
        );
        activity.pending_submissions.clear();
        activity.active_submission = Some(mine);
        activity.state = UiActivityState::Idle;
        observe_activity(&task, &mut snapshot, &activity);
        assert!(
            snapshot.activity_state.is_none(),
            "idle admission has no terminal result"
        );
        activity.state = UiActivityState::Running;
        observe_activity(&task, &mut snapshot, &activity);
        let own_running = activity.clone();
        activity.active_submission = Some(other.clone());
        activity.state = UiActivityState::Paused;
        observe_activity(&task, &mut snapshot, &activity);
        assert_eq!(snapshot.activity_state, Some(UiActivityState::Running));
        activity = own_running;
        activity.state = UiActivityState::Idle;
        observe_activity(&task, &mut snapshot, &activity);
        activity.active_submission = Some(other);
        activity.state = UiActivityState::Running;
        observe_activity(&task, &mut snapshot, &activity);
        assert_eq!(snapshot.activity_state, Some(UiActivityState::Idle));
        assert!(
            super::super::finish_stdio_task_if_ready(&mut snapshot, InputIntent::Agent)
                .unwrap()
                .is_none()
        );
    }
}
