use super::*;
use crate::file_backed::attachment_io::{MAX_IMAGE_BYTES, format_byte_size};
use crate::file_backed::layout;
use alan_agent_protocol::{
    InputIntent, InputMode, UiActivitySnapshot, UiNoticeKind, UiQueueSnapshot, YieldKind,
};
use crossterm::event::{Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const MINIMAL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn slash_command_disambiguation_routes_commands_and_keeps_paths_as_agent_inputs() {
    let mut app = FileBackedApp::new("/agent/root".into());

    for cmd in ["/help", "/settings channel beta", "/unknowncmd"] {
        app.composer.set_text(cmd);
        app.refresh_completion();
        assert!(!app.enter_submits_agent_task(), "{cmd}");
        let action = app.submit_with_mode(InputMode::FollowUp);
        assert!(action.is_none(), "{cmd} should handle locally");
        assert!(app.notice.is_some(), "{cmd} should set notice");
        app.notice = None;
    }

    for path_input in [
        "/var/folders/12/abcd/T/rex-clipboard-123.png",
        "/Users/morris/Desktop/shot.png inspect this",
        "/mnt/project/src/main.rs",
        "/",
        "/ help",
    ] {
        app.composer.set_text(path_input);
        app.completion = None;
        assert!(app.enter_submits_agent_task(), "{path_input}");
        let Some(FileBackedAction::Submit(record)) = app.submit_with_mode(InputMode::FollowUp)
        else {
            panic!("expected agent submit for `{path_input}`");
        };
        assert_eq!(record.intent, InputIntent::Agent);
        assert_eq!(record.body, path_input);
        assert!(
            !app.notice
                .as_deref()
                .is_some_and(|text| text.contains("unknown command")),
            "unexpected unknown command notice for `{path_input}`"
        );
    }

    // Command completion is suppressed as soon as '/' query contains another '/'.
    app.composer
        .set_text("/var/folders/12/abcd/T/rex-clipboard-123.png");
    app.refresh_completion();
    assert!(app.completion.is_none());
}

#[test]
fn pasting_pure_image_attaches_dedups_clears_warnings_and_works_while_working() {
    let dir = tempfile::tempdir().unwrap();
    let img_a = dir.path().join("rex-clipboard-a.png");
    let img_b = dir.path().join("rex-clipboard-b.png");
    let bad = dir.path().join("bad.png");
    let huge = dir.path().join("huge.png");
    std::fs::write(&img_a, MINIMAL_PNG).unwrap();
    std::fs::write(&img_b, b"\x89PNG\r\n\x1a\nSECOND").unwrap();
    std::fs::write(&bad, b"not a png").unwrap();
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(MAX_IMAGE_BYTES + 1)
        .unwrap();

    let mut app = FileBackedApp::new("/agent/root".into());

    // Invalid magic warns without inserting path garbage.
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        bad.display().to_string(),
    )));
    assert!(app.pending_attachments.is_empty());
    assert!(app.composer.text().is_empty());
    assert_eq!(
        app.notice.as_ref().map(|n| n.kind),
        Some(UiNoticeKind::Warning)
    );

    // >20 MiB warns without inserting path garbage.
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        huge.display().to_string(),
    )));
    assert!(app.pending_attachments.is_empty());
    assert!(app.composer.text().is_empty());
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|text| text.contains("20 MiB"))
    );

    // Valid paste attaches, clears stale warning, and keeps composer clean.
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img_a.display().to_string(),
    )));
    assert_eq!(app.pending_attachments.len(), 1);
    assert!(app.composer.text().is_empty());
    assert!(app.completion.is_none());
    assert!(app.notice.is_none());

    // Duplicate paste shows calm brief notice without duplicating attachment.
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img_a.display().to_string(),
    )));
    assert_eq!(app.pending_attachments.len(), 1);
    assert_eq!(app.notice.as_deref(), Some("image already attached"));

    // While Working, pasting another image also attaches cleanly and queues/steers.
    app.activity = UiActivitySnapshot::running(100);
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img_b.display().to_string(),
    )));
    assert_eq!(app.pending_attachments.len(), 2);
    assert!(app.notice.is_none());
}

#[test]
fn backspace_pops_last_attachment_only_when_empty_agent_composer() {
    let dir = tempfile::tempdir().unwrap();
    let img_a = dir.path().join("a.png");
    let img_b = dir.path().join("b.png");
    std::fs::write(&img_a, MINIMAL_PNG).unwrap();
    std::fs::write(&img_b, b"\x89PNG\r\n\x1a\nSECOND").unwrap();

    let mut app = FileBackedApp::new("/agent/root".into());
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(format!(
        "{}\n{}",
        img_a.display(),
        img_b.display()
    ))));
    assert_eq!(app.pending_attachments.len(), 2);

    // Non-empty text deletes text first, preserving attachments.
    app.insert_input_text("x");
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(app.composer.text().is_empty());
    assert_eq!(app.pending_attachments.len(), 2);

    // Command intent resets to Agent instead of popping attachments.
    app.input_intent = InputIntent::Command;
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.input_intent, InputIntent::Agent);
    assert_eq!(app.pending_attachments.len(), 2);

    // Pending yield does not pop attachments.
    app.pending_yield = Some(crate::history::PendingYieldCell {
        request_id: "r1".into(),
        kind: YieldKind::Custom("text".into()),
        title: "Q".into(),
        prompt: None,
        options: vec![],
        default_option: None,
        questions: vec![],
        capability: None,
        reason: None,
        presentation: None,
    });
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.pending_attachments.len(), 2);
    app.pending_yield = None;

    // Empty Agent composer pops last attachment one by one.
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.pending_attachments.len(), 1);
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(app.pending_attachments.is_empty());
}

#[test]
fn image_only_submit_works_for_idle_followup_and_working_ctrl_s_steer_with_dedup() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("clip.png");
    std::fs::write(&img, MINIMAL_PNG).unwrap();

    let mut app = FileBackedApp::new("/agent/root".into());
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img.display().to_string(),
    )));
    assert!(app.enter_submits_agent_task());

    let Some(FileBackedAction::Submit(record)) =
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
    else {
        panic!("expected empty-body image FollowUp submit");
    };
    assert_eq!(record.mode, InputMode::FollowUp);
    assert_eq!(record.intent, InputIntent::Agent);
    assert!(record.body.is_empty());
    assert_eq!(record.parts.len(), 1);
    record.validate().unwrap();
    app.accept_input();
    assert!(app.pending_attachments.is_empty());

    // Ctrl+S Steer while Working with attached image + same path in prompt deduplicates by hash.
    app.activity = UiActivitySnapshot::running(50);
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img.display().to_string(),
    )));
    app.insert_input_text(&format!("describe {}", img.display()));
    let Some(FileBackedAction::Submit(steer)) =
        app.handle_key(key(KeyCode::Char('s'), KeyModifiers::CONTROL))
    else {
        panic!("expected Steer submit with deduplicated attachment");
    };
    assert_eq!(steer.mode, InputMode::Steer);
    assert_eq!(steer.parts.len(), 1);
    steer.validate().unwrap();
}

#[test]
fn attachment_preview_renders_above_composer_without_breaking_cursor_math() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("rex-clipboard-preview.png");
    std::fs::write(&img, MINIMAL_PNG).unwrap();

    let mut app = FileBackedApp::new("/agent/root".into());
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img.display().to_string(),
    )));

    let height = layout::inline_viewport_height(&app, 80, 20);
    let mut terminal = Terminal::new(TestBackend::new(80, height)).unwrap();
    terminal.draw(|frame| layout::draw(frame, &app)).unwrap();

    let rows: Vec<String> = (0..height)
        .map(|y| {
            (0..80)
                .map(|x| terminal.backend().buffer().cell((x, y)).unwrap().symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    let preview_row = rows
        .iter()
        .position(|r| {
            r.contains("+ 1 image:")
                && r.contains("rex-clipboard-preview.png")
                && r.contains(&format_byte_size(MINIMAL_PNG.len() as u64))
                && r.contains("Backspace remove")
        })
        .expect("attachment preview strip missing");
    let prompt_row = rows
        .iter()
        .position(|r| r == ":")
        .expect("prompt row missing");
    assert_eq!(prompt_row, preview_row + 1);
    assert_eq!(terminal.backend().cursor_position().y as usize, prompt_row);
    assert_eq!(terminal.backend().cursor_position().x, 2);

    // Multiline composer text still places cursor on second composer line accurately.
    app.insert_input_text("line1\nline2");
    let height = layout::inline_viewport_height(&app, 80, 20);
    let mut terminal = Terminal::new(TestBackend::new(80, height)).unwrap();
    terminal.draw(|frame| layout::draw(frame, &app)).unwrap();
    let cursor = terminal.backend().cursor_position();
    let line2_row = (0..height)
        .find(|&y| {
            let text: String = (0..80)
                .map(|x| terminal.backend().buffer().cell((x, y)).unwrap().symbol())
                .collect();
            text.contains("line2")
        })
        .expect("line2 row missing");
    assert_eq!(cursor.y, line2_row);
    assert_eq!(cursor.x, 7);
}

#[test]
fn attaching_second_image_before_queue_ack_does_not_get_wiped_and_reconciles_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let img_a = dir.path().join("first.png");
    let img_b = dir.path().join("second.png");
    std::fs::write(&img_a, MINIMAL_PNG).unwrap();
    std::fs::write(&img_b, b"\x89PNG\r\n\x1a\nSECOND").unwrap();

    let mut app = FileBackedApp::new("/agent/1".into());
    app.queue.owner = "/agent/1".into();
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img_a.display().to_string(),
    )));
    let Some(FileBackedAction::Submit(first_record)) = app.handle_submit() else {
        panic!("submit first");
    };
    app.track_local_input(
        &first_record.submission_id,
        "/agent/1".into(),
        first_record.body.clone(),
        first_record.intent,
    );
    app.set_local_input_attachment_summary(
        &first_record.submission_id,
        crate::file_backed::attachment_io::summarize_attachments(&first_record.parts),
    );

    // User attaches a second image before QueueChanged acknowledges the first image-only submit.
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img_b.display().to_string(),
    )));
    assert_eq!(app.pending_attachments.len(), 2);
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            active_submission_ids: vec![first_record.submission_id.clone()],
            ..Default::default()
        }),
    );
    app.acknowledge_local_input(&first_record.submission_id, "/agent/1");
    assert_eq!(
        app.pending_attachments.len(),
        2,
        "newly pasted second image must survive first submit acknowledgement"
    );

    // Image-only turn response (tape-first: assistant tape arrives before io/output) settles cleanly.
    app.apply_tape_record(super::super::tests::tape_message("user", "turn 1"));
    app.push_output("ok 1".into());
    app.apply_tape_record(super::super::tests::tape_message("assistant", "ok 1"));
    app.apply_tape_record(super::super::file_surface::TapeRecordV1 {
        end_offset: 100,
        version: 1,
        kind: "message".into(),
        role: "assistant".into(),
        content: "image answer".into(),
        input_intent: None,
        submission_id: Some(first_record.submission_id.clone()),
        related_submission_ids: Vec::new(),
    });
    app.push_output("image ".into());
    app.push_output("answer".into());
    assert!(
        app.local_inputs[&first_record.submission_id].tape_seen,
        "correlated assistant record settles local attachment input"
    );
    app.apply_tape_record(super::super::tests::tape_message("user", "turn 3"));
    app.push_output("ok 3".into());
    assert!(
        !app.transcript
            .iter()
            .any(|c| matches!(c, HistoryCell::Assistant(t) if t.contains("image answerok 3"))),
        "tape-first image answer must not leak into turn 3: {:?}",
        app.transcript
    );
    assert_eq!(
        app.transcript
            .iter()
            .filter(|c| matches!(c, HistoryCell::Assistant(t) if t.contains("image answer")))
            .count(),
        1,
        "tape-first image answer must appear exactly once: {:?}",
        app.transcript
    );

    // Stream-first case also settles cleanly without leaking.
    app.apply_tape_record(super::super::tests::tape_message("assistant", "ok 3"));
    app.push_output("stream-first answer".into());
    app.apply_tape_record(super::super::file_surface::TapeRecordV1 {
        end_offset: 200,
        version: 1,
        kind: "message".into(),
        role: "assistant".into(),
        content: "stream-first answer".into(),
        input_intent: None,
        submission_id: Some(first_record.submission_id.clone()),
        related_submission_ids: Vec::new(),
    });
    app.apply_tape_record(super::super::tests::tape_message("user", "turn 4"));
    app.push_output("ok 4".into());
    assert!(
        !app.transcript.iter().any(
            |c| matches!(c, HistoryCell::Assistant(t) if t.contains("stream-first answerok 4"))
        ),
        "stream-first image answer must not leak into turn 4: {:?}",
        app.transcript
    );
    assert_eq!(
        app.transcript
            .iter()
            .filter(|c| matches!(c, HistoryCell::Assistant(t) if t.contains("stream-first answer")))
            .count(),
        1,
        "stream-first image answer must appear exactly once: {:?}",
        app.transcript
    );
}

#[test]
fn force_agent_prefix_supports_image_only_submit_steer_preview_and_backspace() {
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("force-agent.png");
    std::fs::write(&img, MINIMAL_PNG).unwrap();

    let mut app = FileBackedApp::new("/agent/root".into());
    app.handle_key(key(KeyCode::Char(':'), KeyModifiers::NONE));
    assert_eq!(app.input_intent, InputIntent::ForceAgent);
    assert!(app.composer.text().is_empty());

    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        img.display().to_string(),
    )));
    assert_eq!(app.pending_attachments.len(), 1);
    assert!(app.enter_submits_agent_task());

    let preview = app.attachment_preview_lines(80)[0].to_string();
    assert!(preview.contains("Backspace remove"), "{preview}");

    let Some(FileBackedAction::Submit(follow)) =
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
    else {
        panic!("expected ForceAgent FollowUp submit");
    };
    assert_eq!(follow.intent, InputIntent::ForceAgent);
    assert_eq!(follow.mode, InputMode::FollowUp);
    assert!(follow.body.is_empty());
    assert_eq!(follow.parts.len(), 1);
    follow.validate().unwrap();

    // Ctrl+S Steer also succeeds with ForceAgent image-only attachment.
    app.activity = UiActivitySnapshot::running(10);
    let Some(FileBackedAction::Submit(steer)) =
        app.handle_key(key(KeyCode::Char('s'), KeyModifiers::CONTROL))
    else {
        panic!("expected ForceAgent Steer submit");
    };
    assert_eq!(steer.intent, InputIntent::ForceAgent);
    assert_eq!(steer.mode, InputMode::Steer);
    assert_eq!(steer.parts.len(), 1);
    steer.validate().unwrap();

    // Backspace pops attachment while keeping ForceAgent intent untouched.
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(app.pending_attachments.is_empty());
    assert_eq!(app.input_intent, InputIntent::ForceAgent);
}
