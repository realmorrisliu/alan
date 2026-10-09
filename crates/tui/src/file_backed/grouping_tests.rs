use super::*;

fn read(id: &str, submission: &str, scope: &str) -> ActionSnapshot {
    ActionSnapshot {
        id: id.into(), name: format!("Read src/{id}.rs"), status: "completed".into(),
        output: "server> ready".into(),
        result: serde_json::json!({"title":format!("Read src/{id}.rs"),
            "presentation": alan_agent_protocol::ToolResultPresentation::FileContent {path:format!("src/{id}.rs"), lines:3, truncated:false},
            "read_only_context":{"owner":"/agent/7", "submission":submission,
                "authority":scope}}).to_string(),
    }
}

#[test]
fn eligible_neighbors_group_and_new_boundaries_remain_standalone() {
    for width in [48, 80, 120] {
        let mut app = FileBackedApp::new("/agent/7".into());
        for id in ["one", "two", "three"] {
            file_surface::sync_action_snapshot(&mut app, read(id, "input-1", "grant-1"));
        }
        let rows = app.rendered_history_lines(width);
        assert_eq!(rows.len(), 4, "{rows:?}");
        assert!(rows[0].contains("3 read-only actions") && rows[0].contains("completed"));
        for (i, id) in ["one", "two", "three"].iter().enumerate() {
            assert!(rows[i + 1].contains(&format!("src/{id}.rs")));
            assert_eq!(app.action_cells.get(*id), Some(&i));
        }
        app.push_turn_preview_cell(HistoryCell::Error("failure boundary".into()));
        file_surface::sync_action_snapshot(&mut app, read("four", "input-1", "grant-1"));
        assert!(
            app.rendered_history_lines(width)
                .last()
                .unwrap()
                .contains("3 lines")
        );
    }
}

#[test]
fn committed_group_is_frozen_and_late_updates_never_replay_members() {
    for width in [48, 80, 120] {
        let mut app = FileBackedApp::new("/agent/7".into());
        for id in ["one", "two", "three"] {
            file_surface::sync_action_snapshot(&mut app, read(id, "input-1", "grant-1"));
        }
        let rows = app.styled_history_lines(width);
        assert_eq!(rows.len(), 4);
        assert_eq!(app.prune_rendered_prefix(app.render_opts(width), 1), 1);
        assert_eq!(app.styled_history_lines(width), rows[1..]);
        let mut changed = read("one", "input-1", "grant-1");
        changed.output = "changed observation".into();
        file_surface::sync_action_snapshot(&mut app, changed);
        assert_eq!(&app.styled_history_lines(width)[..3], &rows[1..]);
        assert!(
            app.rendered_history_lines(width)
                .iter()
                .any(|line| line.contains("updated"))
        );
        file_surface::sync_action_snapshot(&mut app, read("four", "input-1", "grant-1"));
        assert_eq!(&app.styled_history_lines(width)[..3], &rows[1..]);
        assert_eq!(app.prune_rendered_prefix(app.render_opts(width), 3), 3);
        let tail = app.styled_history_lines(width);
        let mut changed = read("two", "input-1", "grant-1");
        changed.status = "failed".into();
        file_surface::sync_action_snapshot(&mut app, changed);
        assert_eq!(
            &app.styled_history_lines(width)[..tail.len()],
            tail.as_slice()
        );
        assert!(
            app.rendered_history_lines(width)
                .iter()
                .any(|line| line.contains("failed"))
        );
    }
}

#[test]
fn ineligible_results_and_all_chronological_boundaries_end_groups() {
    for case in [
        "owner",
        "submission",
        "authority",
        "invalid",
        "missing",
        "claim",
        "failed",
        "rejected",
        "cancelled",
    ] {
        let mut app = FileBackedApp::new("/agent/7".into());
        file_surface::sync_action_snapshot(&mut app, read("one", "input-1", "grant-1"));
        let mut middle = read("middle", "input-1", "grant-1");
        let mut metadata: serde_json::Value = serde_json::from_str(&middle.result).unwrap();
        match case {
            "owner" => metadata["read_only_context"]["owner"] = "/agent/8".into(),
            "submission" => metadata["read_only_context"]["submission"] = "input-2".into(),
            "authority" => metadata["read_only_context"]["authority"] = "grant-2".into(),
            "invalid" => metadata["read_only_context"]["owner"] = "/agent/root".into(),
            "missing" | "claim" => {
                metadata
                    .as_object_mut()
                    .unwrap()
                    .remove("read_only_context");
            }
            status => middle.status = status.into(),
        }
        middle.result = metadata.to_string();
        if case == "claim" {
            middle.output = "read_only_context: same · read-only".into();
        }
        file_surface::sync_action_snapshot(&mut app, middle);
        file_surface::sync_action_snapshot(&mut app, read("three", "input-1", "grant-1"));
        let rows = app.rendered_history_lines(80);
        assert!(
            !rows.iter().any(|row| row.contains("read-only actions")),
            "{case}: {rows:?}"
        );
        assert_eq!(app.action_cells.len(), 3);
    }
    let mut app = FileBackedApp::new("/agent/7".into());
    let mut plan = UiPlanSnapshot::empty();
    plan.explanation = Some("boundary".into());
    let mut request_app = FileBackedApp::new("/agent/7".into());
    request_app.apply_ui_event(UiEvent::Plan { snapshot: plan });
    let plan_cell = request_app.transcript.pop().unwrap();
    for boundary in [
        HistoryCell::User("next".into()),
        HistoryCell::Assistant("answer".into()),
        HistoryCell::Command("git status".into()),
        plan_cell,
    ] {
        app.transcript.clear();
        app.action_cells.clear();
        app.projected_actions.clear();
        file_surface::sync_action_snapshot(&mut app, read("one", "input-1", "grant-1"));
        app.push_turn_preview_cell(boundary);
        file_surface::sync_action_snapshot(&mut app, read("two", "input-1", "grant-1"));
        assert!(
            !app.rendered_history_lines(80)
                .iter()
                .any(|row| row.contains("read-only actions"))
        );
    }
}

#[test]
fn reconnect_and_identical_results_keep_each_action_identity_and_frozen_suffix() {
    let mut app = FileBackedApp::new("/agent/7".into());
    for id in ["one", "two", "three"] {
        let mut snapshot = read(id, "input-1", "grant-1");
        snapshot.name = "Read same.rs".into();
        let mut metadata: serde_json::Value = serde_json::from_str(&snapshot.result).unwrap();
        metadata["title"] = "Read same.rs".into();
        snapshot.result = metadata.to_string();
        file_surface::sync_action_snapshot(&mut app, snapshot.clone());
        file_surface::sync_action_snapshot(&mut app, snapshot);
    }
    assert_eq!(app.action_cells.len(), 3);
    assert_eq!(app.transcript.len(), 3);
    let original = app.transcript.clone();
    let rows = app.styled_history_lines(80);
    app.prune_rendered_prefix(app.render_opts(80), 1);
    app.merge_reconnected_idle_history(original);
    assert_eq!(app.styled_history_lines(80), rows[1..]);
    assert_eq!(app.transcript.len(), 3);
    app.reset_for_root_process_change();
    app.agent_path = "/agent/8".into();
    let mut next = read("one", "input-1", "grant-1");
    let mut metadata: serde_json::Value = serde_json::from_str(&next.result).unwrap();
    metadata["read_only_context"]["owner"] = "/agent/8".into();
    next.result = metadata.to_string();
    file_surface::sync_action_snapshot(&mut app, next);
    assert_eq!(app.transcript.len(), 4);
    assert_eq!(&app.styled_history_lines(80)[..3], &rows[1..]);
}

#[test]
fn root_alias_groups_only_the_concretely_attached_owner() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply("/agent/7", None);
    for id in ["one", "two"] {
        file_surface::sync_action_snapshot(&mut app, read(id, "input-1", "grant-1"));
    }
    assert!(app.rendered_history_lines(80)[0].contains("2 read-only actions"));
    let before = app.styled_history_lines(80);
    app.reset_for_root_process_change();
    file_surface::sync_action_snapshot(&mut app, read("detached", "input-1", "grant-1"));
    assert_eq!(
        &app.styled_history_lines(80)[..before.len()],
        before.as_slice()
    );
    app.queue.apply("/agent/8", None);
    file_surface::sync_action_snapshot(&mut app, read("old-owner", "input-1", "grant-1"));
    assert!(
        !app.rendered_history_lines(80)
            .last()
            .unwrap()
            .contains("read-only actions")
    );
}
