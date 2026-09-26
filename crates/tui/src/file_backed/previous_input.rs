use super::file_surface::TapeRecordV1;
use super::{FileBackedApp, FileBackedEvent};
use alan_agent_protocol::UiEvent;
use std::collections::VecDeque;

pub(super) async fn snapshot(
    ui_tail: &alan_shell::Tail,
    tape_tail: &alan_shell::Tail,
    submitted: Option<(&str, u64, &str)>,
) -> (Option<UiEvent>, Option<String>) {
    let Some((_, _, id)) = submitted else {
        return (None, None);
    };
    // Read completion first: a matching terminal record implies its final Tape
    // write already happened. Descriptor read failures fall back to queued data.
    let ui = ui_tail.snapshot().await.unwrap_or_default();
    let completion = ui
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<UiEvent>(line).ok())
        .rfind(|event| {
            matches!(event, UiEvent::InputCompleted { submission_ids, .. }
            if submission_ids.iter().any(|candidate| candidate == id))
        });
    let tape = tape_tail.snapshot().await.unwrap_or_default();
    let answer = tape
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<TapeRecordV1>(line).ok())
        .filter(|record| {
            record.kind == "message" && record.role == "assistant" && record.belongs_to(id)
        })
        .map(|record| record.content)
        .next_back();
    (completion, answer)
}

pub(super) async fn restore_tape_history(
    app: &mut FileBackedApp,
    tape: &alan_shell::Tail,
    consumed_bytes: usize,
) -> bool {
    let Ok(bytes) = tape.snapshot().await else {
        return false;
    };
    let Ok(raw) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let Some(unseen) = raw.get(consumed_bytes..) else {
        return false;
    };
    let history = super::file_surface::parse_tape_history(unseen);
    // These indices belong to the retained transcript, not freshly hydrated actions.
    let actions = std::mem::take(&mut app.action_cells);
    app.merge_reconnected_idle_history(history);
    app.action_cells = actions;
    true
}

pub(super) fn restore_answer(app: &mut FileBackedApp, input: &str, answer: String) {
    use crate::history::HistoryCell;
    // This synthetic turn has no action indices; retain the existing ones.
    let actions = std::mem::take(&mut app.action_cells);
    app.merge_reconnected_history(
        vec![
            HistoryCell::User(input.into()),
            HistoryCell::Assistant(answer),
        ],
        input,
        0,
    );
    app.action_cells = actions;
}

pub(super) fn discard_superseded_attachment_events(
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
    pending_terminal_events: &mut VecDeque<FileBackedEvent>,
    submission_id: Option<&str>,
) -> (Option<UiEvent>, Option<String>) {
    let mut completion = None;
    let mut answer = None;
    for _ in 0..rx.len() {
        let Ok(event) = rx.try_recv() else {
            break;
        };
        if matches!(
            event,
            FileBackedEvent::Terminal(_) | FileBackedEvent::TerminalError(_)
        ) {
            pending_terminal_events.push_back(event);
        } else if let FileBackedEvent::Ui(
            ref ui @ UiEvent::InputCompleted {
                ref submission_ids, ..
            },
        ) = event
            && submission_id
                .is_some_and(|id| submission_ids.iter().any(|candidate| candidate == id))
        {
            completion = Some(ui.clone());
        } else if let FileBackedEvent::Tape(record) = event
            && record.role == "assistant"
            && submission_id.is_some_and(|id| record.belongs_to(id))
        {
            answer = Some(record.content);
        }
    }
    (completion, answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::HistoryCell;

    #[tokio::test]
    async fn completed_input_recovers_tape_after_local_pending_state_is_released() {
        let (shell, root, _, pid) = super::super::stdio_tests::live_root_agent().await;
        let tape = [
            serde_json::json!({"version":1,"kind":"message","role":"user","content":"task","submission_id":"mine"}),
            serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"final answer","submission_id":"mine"}),
        ].map(|record| format!("{record}\n")).concat();
        shell
            .write(&format!("/agent/{pid}/machine/tape"), tape.as_bytes())
            .await
            .unwrap();
        let tail = shell
            .tail(&format!("/agent/{pid}/machine/tape"))
            .await
            .unwrap();
        assert!(root.unbind_process(&pid).await);
        for preview in [None, Some("final"), Some("final answer")] {
            let mut app = FileBackedApp::new("/agent/root".into());
            app.transcript = vec![HistoryCell::User("task".into())];
            if let Some(preview) = preview {
                app.transcript.push(HistoryCell::Assistant(preview.into()));
            }
            let mut pending = Some(super::super::interrupt::PendingRootAgentTurn {
                input: "task".into(),
                submission_id: "mine".into(),
                submitted_process: Some(pid.parse().unwrap()),
                observed_active: true,
                interrupt_requested: false,
                submitted_at_ms: 0,
            });
            super::super::interrupt::observe_root_agent_completion(
                &mut pending,
                &UiEvent::InputCompleted {
                    submission_ids: vec!["mine".into()],
                    status: alan_agent_protocol::UiInputStatus::Completed,
                    error: None,
                },
                &mut app,
            );
            assert!(pending.is_none());
            restore_tape_history(&mut app, &tail, 0).await;
            restore_tape_history(&mut app, &tail, 0).await;
            assert_eq!(app.transcript.len(), 2);
            assert!(
                matches!(&app.transcript[1], HistoryCell::Assistant(text) if text == "final answer")
            );
        }
        // Clearing after consuming only the user must not resurrect that boundary.
        let mut app = FileBackedApp::new("/agent/root".into());
        let consumed = tape.find('\n').unwrap() + 1;
        assert!(restore_tape_history(&mut app, &tail, consumed).await);
        assert_eq!(
            app.transcript,
            vec![HistoryCell::Assistant("final answer".into())]
        );

        // Consuming the final record advances recovery even after /clear.
        let mut app = FileBackedApp::new("/agent/root".into());
        let mut final_record: TapeRecordV1 =
            serde_json::from_str(tape.lines().last().unwrap()).unwrap();
        final_record.end_offset = tape.len();
        app.apply_tape_record(final_record);
        app.transcript.clear();
        let consumed = app.tape_consumed_offset;
        restore_tape_history(&mut app, &tail, consumed).await;
        assert!(app.transcript.is_empty());
        tail.close().await.unwrap();
    }

    #[tokio::test]
    async fn durable_old_process_outcome_survives_without_watcher_delivery() {
        let (shell, root, _, pid) = super::super::stdio_tests::live_root_agent().await;
        let completion = UiEvent::InputCompleted {
            submission_ids: vec!["mine".into()],
            status: alan_agent_protocol::UiInputStatus::Completed,
            error: None,
        };
        let ui = format!("{}\n", serde_json::to_string(&completion).unwrap());
        shell
            .write(&format!("/agent/{pid}/machine/ui/events"), ui.as_bytes())
            .await
            .unwrap();
        let tape = [
            serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"final answer","submission_id":"mine"}),
            serde_json::json!({"version":1,"kind":"message","role":"assistant","content":"another answer","submission_id":"other"}),
        ].map(|record| format!("{record}\n")).concat();
        shell
            .write(&format!("/agent/{pid}/machine/tape"), tape.as_bytes())
            .await
            .unwrap();
        let ui_tail = shell
            .tail(&format!("/agent/{pid}/machine/ui/events"))
            .await
            .unwrap();
        let tape_tail = shell
            .tail(&format!("/agent/{pid}/machine/tape"))
            .await
            .unwrap();
        assert!(root.unbind_process(&pid).await);
        assert!(
            shell
                .cat(&format!("/agent/{pid}/machine/ui/events"))
                .await
                .is_err()
        );
        let (observed, answer) = snapshot(&ui_tail, &tape_tail, Some(("task", 0, "mine"))).await;
        assert_eq!(ui_tail.offset(), 0);
        ui_tail.close().await.unwrap();
        tape_tail.close().await.unwrap();
        assert_eq!(observed, Some(completion));
        assert_eq!(answer.as_deref(), Some("final answer"));
        let mut app = FileBackedApp::new("/agent/root".into());
        app.transcript = vec![
            HistoryCell::User("task".into()),
            HistoryCell::Assistant("final".into()),
        ];
        restore_answer(&mut app, "task", answer.clone().unwrap());
        restore_answer(&mut app, "task", answer.unwrap());
        assert_eq!(app.transcript.len(), 2);
        assert!(
            matches!(&app.transcript[1], HistoryCell::Assistant(text) if text == "final answer")
        );
    }
}
