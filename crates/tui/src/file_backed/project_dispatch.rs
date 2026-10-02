use super::*;

pub(super) fn dispatch_with_pending_submissions(
    app: &mut FileBackedApp,
    event: FileBackedEvent,
    pending_turns: &VecDeque<PendingRootAgentTurn>,
) -> Option<FileBackedAction> {
    let blocked = pending_turns.iter().any(|turn| {
        let owner = turn
            .submitted_process
            .map_or_else(|| app.agent_path.clone(), |pid| format!("/agent/{pid}"));
        owner != app.queue.owner
            || !app.queue.snapshot.as_ref().is_some_and(|q| {
                q.known && q.paused && q.pending_submission_ids.contains(&turn.submission_id)
            })
    });
    if blocked
        && let FileBackedEvent::Terminal(crossterm::event::Event::Key(key)) = &event
        && key.code == crossterm::event::KeyCode::Enter
        && (app.project_selection.is_some() || app.composer.text().trim().starts_with("/project"))
    {
        app.notice = Some("project selection blocked by admitted or queued input".into());
        return None;
    }
    if app.model_chooser.active {
        if let FileBackedEvent::Terminal(crossterm::event::Event::Key(key)) = &event {
            return app.model_key(*key, blocked);
        }
        if matches!(
            event,
            FileBackedEvent::Terminal(crossterm::event::Event::Paste(_))
        ) {
            return None;
        }
    }
    let action = app.dispatch_with_pending_submission(event, !pending_turns.is_empty());
    if matches!(action, Some(FileBackedAction::Project(_)))
        && !app.project_boundary_available(blocked)
    {
        app.notice =
            Some("project selection requires settled Idle or Paused with no admitted input".into());
        return None;
    }
    action
}
