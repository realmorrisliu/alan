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
