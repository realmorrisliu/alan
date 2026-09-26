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

pub(super) fn observe_completion(id: &str, snapshot: &mut StdioTaskSnapshot, event: UiEvent) {
    if let UiEvent::InputCompleted {
        submission_ids,
        status,
        error,
    } = event
        && submission_ids.iter().any(|candidate| candidate == id)
    {
        snapshot.task_started = true;
        snapshot.completion = Some(status);
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

#[cfg(test)]
mod tests {
    use super::super::stdio_tests::{completion, correlated_records, task};
    use super::*;
    use alan_ap::{InProcessTransport, reference::MemFs};
    use alan_kernel::{Access, MountFs, Namespace};
    use std::sync::Arc;

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
                .as_deref(),
            Some("final answer")
        );
    }

    #[tokio::test]
    async fn stdio_submission_writes_the_same_id_used_for_completion() {
        let (shell, _, _, _) = super::super::stdio_tests::live_root_agent().await;
        let attachment = super::super::tail::open_stdio_tail_attachment(&shell, "/agent/root")
            .await
            .unwrap();
        let task = StdioTaskWaitContext::new("same text");
        super::super::submit_stdio_task(&shell, &task, &attachment)
            .await
            .unwrap();
        let frame = shell.cat("/agent/root/io/input").await.unwrap();
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
        let mine = StdioTaskWaitContext::new("same text");
        let other = StdioTaskWaitContext::new("same text");
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
        observe_completion(
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
