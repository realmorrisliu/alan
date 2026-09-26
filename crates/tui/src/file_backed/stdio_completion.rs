use super::{StdioTaskSnapshot, StdioTaskWaitContext, file_surface::TapeRecordV1};
use alan_agent_protocol::{UiEvent, UiInputStatus};
use anyhow::{Context, Result, anyhow};

pub(super) fn tape_outcome(
    submission_id: &str,
    tape_history: &[u8],
) -> Result<(bool, Option<String>)> {
    let mut started = false;
    let mut answer = None;
    for line in std::str::from_utf8(tape_history)
        .context("machine/tape is not utf8")?
        .lines()
    {
        let Ok(record) = serde_json::from_str::<TapeRecordV1>(line) else {
            continue;
        };
        if record.kind != "message" || !record.belongs_to(submission_id) {
            continue;
        }
        started = true;
        if record.role == "assistant" {
            answer = Some(record.content);
        }
    }
    Ok((started, answer))
}

pub(super) fn observe_event(id: &str, snapshot: &mut StdioTaskSnapshot, event: UiEvent) {
    if let UiEvent::Activity { snapshot: activity } = &event {
        snapshot.activity_state = Some(activity.state);
        snapshot.waiting_for_response = activity.state
            == alan_agent_protocol::UiActivityState::Paused
            && activity
                .waiting_submission_ids
                .iter()
                .any(|candidate| candidate == id);
    }
    if let UiEvent::InputCompleted {
        submission_ids,
        status,
        error,
    } = event
        && submission_ids.iter().any(|candidate| candidate == id)
    {
        snapshot.task_started = true;
        snapshot.completion = Some(status);
        snapshot.waiting_for_response = false;
        snapshot.task_error = match status {
            UiInputStatus::Completed => None,
            UiInputStatus::Failed => Some(error.unwrap_or_else(|| "input failed".into())),
            UiInputStatus::Cancelled => Some(error.unwrap_or_else(|| "input cancelled".into())),
        };
    }
}

pub(super) async fn refresh_answer_after_completion(
    shell: &alan_shell::Shell,
    agent_path: &str,
    task: &StdioTaskWaitContext,
    snapshot: &mut StdioTaskSnapshot,
) -> Result<()> {
    if task.record.intent == alan_agent_protocol::InputIntent::Command {
        if snapshot.completion.is_some() {
            snapshot.command_output =
                Some(read_command_output(shell, agent_path, &task.record.submission_id).await?);
            if snapshot.completion != Some(UiInputStatus::Completed)
                && let Some(output) = snapshot.command_output.as_mut()
                && output.exit_code == 0
            {
                output.exit_code = 1;
                if let Some(error) = &snapshot.task_error {
                    output.stderr.push_str(&format!("\n{error}\n"));
                }
            }
        }
        return Ok(());
    }
    if snapshot.completion != Some(UiInputStatus::Completed) {
        return Ok(());
    }
    let tape = shell
        .cat(&format!("{agent_path}/machine/tape"))
        .await
        .map_err(|err| anyhow!("read final Agent tape snapshot failed: {err:?}"))?;
    let (_, answer) = tape_outcome(&task.record.submission_id, &tape)?;
    snapshot.assistant_answer = Some(answer.ok_or_else(|| {
        anyhow!("Agent task completed without a correlated final answer; outcome is unknown")
    })?);
    Ok(())
}

async fn read_command_output(
    shell: &alan_shell::Shell,
    agent_path: &str,
    submission_id: &str,
) -> Result<super::CommandOutput> {
    let mut output = None;
    for action in super::file_surface::read_action_snapshots(shell, agent_path).await? {
        let Ok(result) = serde_json::from_str::<serde_json::Value>(&action.result) else {
            continue;
        };
        if result["call_id"].as_str() != Some(submission_id) {
            continue;
        }
        anyhow::ensure!(
            output.is_none(),
            "multiple command results match this input; outcome is unknown"
        );
        anyhow::ensure!(
            matches!(action.status.as_str(), "completed" | "failed" | "cancelled"),
            "command result is not terminal; outcome is unknown"
        );
        let exit_code = result["exit_code"]
            .as_i64()
            .and_then(|code| i32::try_from(code).ok())
            .context("command exit status is missing or invalid; outcome is unknown")?;
        let mut streams = match serde_json::from_str::<super::CommandOutput>(&action.output) {
            Ok(streams) => streams,
            Err(error) => {
                let failure: serde_json::Value = serde_json::from_str(&action.output)
                    .context("command output is invalid; outcome is unknown")?;
                let Some(message) = failure["error"].as_str().filter(|_| exit_code != 0) else {
                    return Err(error).context("command streams are missing; outcome is unknown");
                };
                super::CommandOutput {
                    stdout: String::new(),
                    stderr: format!("{message}\n"),
                    exit_code,
                }
            }
        };
        streams.exit_code = exit_code;
        output = Some(streams);
    }
    output.context("input completed without a correlated command result; outcome is unknown")
}

#[cfg(test)]
mod tests {
    use super::super::stdio_tests::{completion, correlated_records, task};
    use super::*;
    use alan_ap::{InProcessTransport, reference::MemFs};
    use alan_kernel::{Access, MountFs, Namespace};
    use std::sync::Arc;

    #[test]
    fn response_wait_only_belongs_to_the_identified_input() {
        let task = task("queued input");
        let mut snapshot = super::super::stdio_task_snapshot_from_history(&task, b"", b"").unwrap();
        let mut activity = alan_agent_protocol::UiActivitySnapshot::paused(None);
        activity.waiting_submission_ids = vec!["another-input".into()];
        observe_event(
            &task.record.submission_id,
            &mut snapshot,
            UiEvent::Activity {
                snapshot: activity.clone(),
            },
        );
        assert!(!snapshot.waiting_for_response);
        assert!(!snapshot.task_started);
        activity
            .waiting_submission_ids
            .push(task.record.submission_id.clone());
        let event = UiEvent::Activity { snapshot: activity };
        let recovered = super::super::stdio_task_snapshot_from_history(
            &task,
            b"",
            &serde_json::to_vec(&event).unwrap(),
        )
        .unwrap();
        assert!(recovered.waiting_for_response);
        observe_event(&task.record.submission_id, &mut snapshot, event);
        assert!(snapshot.waiting_for_response);
        observe_event(
            &task.record.submission_id,
            &mut snapshot,
            UiEvent::Activity {
                snapshot: alan_agent_protocol::UiActivitySnapshot::running(0),
            },
        );
        assert!(!snapshot.waiting_for_response);
        assert!(!snapshot.task_started);
    }

    #[tokio::test]
    async fn completion_reloads_final_tape_record_instead_of_streamed_preamble() {
        let tape = correlated_records(
            br#"{"version":1,"kind":"message","role":"user","content":"current task"}
{"version":1,"kind":"message","role":"assistant","content":"preamble"}
{"version":1,"kind":"message","role":"assistant","content":"final answer"}
"#,
        );
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1/machine",
            InProcessTransport::new(Arc::new(MemFs::with_read_only_file("tape", tape))),
            Access::ReadOnly,
        );
        let shell =
            alan_shell::Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))));
        let task = task("current task");
        let mut snapshot = super::super::stdio_task_snapshot_from_history(
            &task,
            b"",
            &completion(UiInputStatus::Completed, None),
        )
        .unwrap();
        snapshot.assistant_answer = Some("preamble".into());
        refresh_answer_after_completion(&shell, "/agent/1", &task, &mut snapshot)
            .await
            .unwrap();
        assert_eq!(
            super::super::finish_stdio_task_if_ready(&mut snapshot)
                .unwrap()
                .map(super::super::StdioTaskOutput::agent_answer)
                .as_deref(),
            Some("final answer")
        );
    }

    #[tokio::test]
    async fn stdio_submission_writes_the_same_id_used_for_completion() {
        let (shell, agent_root, _, pid) = super::super::stdio_tests::live_root_agent().await;
        let attachment = super::super::tail::open_stdio_tail_attachment(&shell, "/agent/root")
            .await
            .unwrap();
        let task = StdioTaskWaitContext::new("same text").unwrap();
        for expected in [None, Some(attachment.root_agent_pid + 1)] {
            assert!(
                super::super::write_agent_input(&shell, "/agent/root", expected, &task.record)
                    .await
                    .is_err()
            );
        }
        assert!(
            shell
                .cat(&format!("/agent/{pid}/io/input"))
                .await
                .unwrap()
                .is_empty()
        );
        // Move only the alias: a submission must still use the observed Process.
        agent_root.set_root_process("99999").await;
        super::super::submit_stdio_task(&shell, &task, &attachment)
            .await
            .unwrap();
        let frame = shell.cat(&format!("/agent/{pid}/io/input")).await.unwrap();
        let header = frame.iter().position(|byte| *byte == b'\n').unwrap();
        let record = alan_agent_protocol::UserInputRecord::decode_payload(&frame[header + 1..])
            .unwrap()
            .unwrap();
        assert_eq!(record, task.record);
        super::super::tail::close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
            .await
            .unwrap();
    }

    #[test]
    fn identical_text_and_other_clients_events_cannot_complete_an_input() {
        let mine = StdioTaskWaitContext::new("same text").unwrap();
        let other = StdioTaskWaitContext::new("same text").unwrap();
        assert_ne!(mine.record.submission_id, other.record.submission_id);
        let tape = format!(
            "{}\n",
            serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"other answer","submission_id":other.record.submission_id})
        );
        let ui = format!(
            "{}\n",
            serde_json::json!({"type":"input_completed","submission_ids":[other.record.submission_id],"status":"completed"})
        );
        let mut snapshot =
            super::super::stdio_task_snapshot_from_history(&mine, tape.as_bytes(), ui.as_bytes())
                .unwrap();
        assert!(!snapshot.task_started);
        assert!(snapshot.assistant_answer.is_none());
        assert!(
            super::super::finish_stdio_task_if_ready(&mut snapshot)
                .unwrap()
                .is_none()
        );
        let shared = format!(
            "{}\n",
            serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"shared answer","submission_id":other.record.submission_id,"related_submission_ids":[mine.record.submission_id]})
        );
        let (_, answer) = tape_outcome(&mine.record.submission_id, shared.as_bytes()).unwrap();
        assert_eq!(answer.as_deref(), Some("shared answer"));
        observe_event(
            &mine.record.submission_id,
            &mut snapshot,
            UiEvent::InputCompleted {
                submission_ids: vec![mine.record.submission_id.clone()],
                status: UiInputStatus::Cancelled,
                error: None,
            },
        );
        assert!(
            super::super::finish_stdio_task_if_ready(&mut snapshot)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }
}

#[cfg(test)]
mod command_tests {
    use super::super::{StdioTaskOutput, StdioTaskWaitContext, stdio_tests, tail};
    use super::*;

    #[test]
    fn redirected_prefixes_preserve_exact_bodies_and_reject_empty_overrides() {
        use alan_agent_protocol::InputIntent;
        for (text, intent, body) in [
            ("!printf x\n ", InputIntent::Command, "printf x\n "),
            (":!explain", InputIntent::ForceAgent, "!explain"),
            ("!!literal", InputIntent::Command, "!literal"),
        ] {
            let task = StdioTaskWaitContext::new(text).unwrap();
            assert_eq!(task.record.intent, intent);
            assert_eq!(task.record.body, body);
        }
        for text in ["", "!", ":", "! \n", " \n"] {
            assert!(StdioTaskWaitContext::new(text).is_err());
        }
    }

    #[tokio::test]
    async fn command_completion_returns_only_correlated_streams_and_exit_status() {
        for exit_code in [0, 7, 2] {
            let (shell, agent_root, _, pid) = stdio_tests::live_root_agent().await;
            let task = StdioTaskWaitContext::new("!printf partial").unwrap();
            let id = task.record.submission_id.clone();
            let path = format!("/agent/{pid}");
            let mut attachment = tail::open_stdio_tail_attachment_for_submit(&shell, "/agent/root")
                .await
                .unwrap();
            super::super::submit_stdio_task(&shell, &task, &attachment)
                .await
                .unwrap();
            assert!(read_command_output(&shell, &path, &id).await.is_err());
            for call_id in ["another-input", id.as_str()] {
                use alan_ap::FileServer;
                let fid = alan_ap::Fid(900_000);
                agent_root
                    .walk(
                        alan_ap::Fid(0),
                        fid,
                        &[pid.clone(), "actions".into(), "clone".into()],
                    )
                    .await
                    .unwrap();
                agent_root
                    .open(fid, alan_ap::OpenMode::ReadWrite)
                    .await
                    .unwrap();
                let action =
                    String::from_utf8(agent_root.read(fid, 0, 4096).await.unwrap()).unwrap();
                agent_root.clunk(fid).await.unwrap();
                let action_path = format!("{path}/actions/{}", action.trim());
                let output = if exit_code == 2 && call_id == id {
                    serde_json::json!({"success":false,"error":"launch denied"})
                } else {
                    serde_json::json!({"stdout":if call_id == id {"partial"} else {"other"}, "stderr":"diagnostic\n"})
                };
                let result = serde_json::json!({"call_id":call_id,"exit_code":exit_code});
                for (name, content) in [
                    ("name", "bash".into()),
                    ("output", output.to_string()),
                    ("result", result.to_string()),
                    (
                        "status",
                        if exit_code == 0 {
                            "completed"
                        } else {
                            "failed"
                        }
                        .into(),
                    ),
                ] {
                    shell
                        .write(&format!("{action_path}/{name}"), content.as_bytes())
                        .await
                        .unwrap();
                }
            }
            let completed = UiEvent::InputCompleted {
                submission_ids: vec![id],
                status: UiInputStatus::Completed,
                error: None,
            };
            shell
                .write(
                    &format!("{path}/machine/ui/events"),
                    format!("{}\n", serde_json::to_string(&completed).unwrap()).as_bytes(),
                )
                .await
                .unwrap();
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                super::super::wait_for_stdio_answer_after_submit(
                    &shell,
                    "/agent/root",
                    task,
                    &mut attachment,
                    std::future::pending(),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            let StdioTaskOutput::Command(output) = result else {
                panic!("command output")
            };
            assert_eq!(output.stdout, if exit_code == 2 { "" } else { "partial" });
            assert_eq!(
                output.stderr,
                if exit_code == 2 {
                    "launch denied\n"
                } else {
                    "diagnostic\n"
                }
            );
            assert_eq!(output.exit_code, exit_code);
            tail::close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
                .await
                .unwrap();
        }
    }
}
