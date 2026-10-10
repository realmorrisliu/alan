use super::*;

pub(super) fn project_selector(path: &str) -> (String, String) {
    let id = project_operation_id();
    let command = format!(
        "project-cwd-v1 {}",
        serde_json::json!({"id": id, "path": path})
    );
    (id, command)
}

pub(super) fn unsettled_submissions(
    app: &FileBackedApp,
    pending_turns: &VecDeque<PendingRootAgentTurn>,
) -> bool {
    pending_turns.iter().any(|turn| {
        let owner = turn
            .submitted_process
            .map_or_else(|| app.agent_path.clone(), |pid| format!("/agent/{pid}"));
        owner != app.queue.owner
            || !app.queue.snapshot.as_ref().is_some_and(|q| {
                q.known && q.paused && q.pending_submission_ids.contains(&turn.submission_id)
            })
    })
}

pub(super) fn dispatch_with_pending_submissions(
    app: &mut FileBackedApp,
    event: FileBackedEvent,
    pending_turns: &VecDeque<PendingRootAgentTurn>,
) -> Option<FileBackedAction> {
    let blocked = unsettled_submissions(app, pending_turns);
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
    if (app.project_host_pending
        || app.uncertain_project_mount.is_some()
        || app.pending_project_control.is_some())
        && matches!(action, Some(FileBackedAction::Submit(_)))
    {
        app.notice = Some(
            "project operation pending; draft retained; /help or /quit remain available".into(),
        );
        return None;
    }
    let retrying_mount = match &action {
        Some(FileBackedAction::Project(command @ ProjectControl::Mount { .. })) => {
            app.uncertain_project_mount
                .as_ref()
                .is_some_and(|(_, retained)| retained == command)
                && app.project_recovery_boundary_available(blocked)
        }
        _ => false,
    };
    if matches!(
        action,
        Some(FileBackedAction::Project(ProjectControl::Mount { .. }))
    ) && !retrying_mount
        && !app.project_boundary_available(blocked)
        || matches!(
            action,
            Some(
                FileBackedAction::Project(ProjectControl::Revoke { .. })
                    | FileBackedAction::RevokeCurrentProject
            )
        ) && !app.project_recovery_boundary_available(blocked)
    {
        app.notice =
            Some("project selection requires settled Idle or Paused with no admitted input".into());
        return None;
    }
    action
}

pub(super) fn project_operation_id() -> String {
    alan_agent_protocol::UserInputRecord::new(
        alan_agent_protocol::InputIntent::Command,
        alan_agent_protocol::InputMode::FollowUp,
        "",
    )
    .submission_id
}
