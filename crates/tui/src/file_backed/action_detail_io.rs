//! Fresh, read-only retained Action detail IO. AgentFS owns the catalog and evidence.
use super::app::{FileBackedApp, FileBackedEvent};
use super::file_surface::{ActionSnapshot, action_snapshot_to_history_cell, read_action_ids};
use ratatui::text::Line;
mod presentation;
pub(super) mod reference;
use reference::resolve as resolve_reference;

/// A selectable reference to one Process-owned Action; it carries no result or authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActionEntry {
    pub owner: String,
    pub id: String,
}

fn concrete_owner(owner: &str) -> bool {
    owner
        .strip_prefix("/agent/")
        .and_then(|pid| pid.parse::<u64>().ok())
        .is_some_and(|pid| pid > 0 && owner == format!("/agent/{pid}"))
}

pub(super) fn start_pending_for_pid(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    pid: Option<u64>,
) {
    let owner = pid
        .and_then(|pid| super::root_agent_path_for_pid(&app.agent_path, pid))
        .unwrap_or_else(|| app.agent_path.clone());
    start_pending_at(shell, app, tx, &owner);
}

#[cfg(test)]
pub(super) fn start_pending(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    let path = app.agent_path.clone();
    start_pending_at(shell, app, tx, &path);
}

pub(super) fn start_pending_at(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    owner_path: &str,
) {
    if !app.modal.active || !app.modal.pending {
        return;
    }
    app.modal.pending = false;
    // Root is a moving alias, not an authorized asynchronous evidence owner.
    if owner_path == "/agent/root" {
        app.modal.rows = vec![Line::from(
            "Root unavailable: close and reopen Action details to retry",
        )];
        return;
    }
    if app.modal.plan_mode {
        super::plan_detail_io::start(shell, app, tx, owner_path);
        return;
    }
    let shell = shell.clone();
    let tx = tx.clone();
    app.modal.owner_path = owner_path.into();
    let path = owner_path.to_string();
    let generation = app.modal.generation;
    let selected = app.modal.actions.get(app.modal.selected).cloned();
    let mut observed = app
        .projected_actions
        .keys()
        .filter(|(owner, _)| owner != &path && concrete_owner(owner))
        .map(|(owner, id)| ActionEntry {
            owner: owner.clone(),
            id: id.clone(),
        })
        .collect::<Vec<_>>();
    observed.sort_by_key(|entry| {
        (
            entry
                .owner
                .strip_prefix("/agent/")
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            super::file_surface::request_sort_key(&entry.id),
        )
    });
    tokio::spawn(async move {
        let (mut actions, warning) = match read_action_ids(&shell, &path).await {
            Ok(ids) => {
                observed.extend(ids.into_iter().map(|id| ActionEntry {
                    owner: path.clone(),
                    id,
                }));
                (Ok(observed), None)
            }
            Err(error) if !observed.is_empty() => (
                Ok(observed),
                Some(format!(
                    "Current Process Action catalog unavailable ({path}); observed references only: {error}"
                )),
            ),
            Err(error) => (Err(error.to_string()), None),
        };
        // A previously selected reference stays explicit if its file disappears.
        if let (Ok(entries), Some(selected)) = (&mut actions, &selected)
            && !entries.contains(selected)
        {
            entries.push(selected.clone());
        }
        let selected = selected.or_else(|| actions.as_ref().ok().and_then(|v| v.last().cloned()));
        if tx
            .send(FileBackedEvent::ActionDetails {
                path: path.clone(),
                generation,
                actions: actions.clone(),
                selected: selected.clone(),
                rows: warning.iter().cloned().map(Line::from).collect(),
            })
            .await
            .is_err()
        {
            return;
        }
        let mut rows = match &selected {
            Some(entry) => read_detail(&shell, &entry.owner, &entry.id).await,
            None => vec![Line::from("No retained Actions")],
        };
        if let Some(warning) = warning {
            rows.insert(0, Line::from(warning));
        }
        let _ = tx
            .send(FileBackedEvent::ActionDetails {
                path,
                generation,
                actions,
                selected,
                rows,
            })
            .await;
    });
}

pub(super) async fn field(shell: &alan_shell::Shell, path: &str) -> Result<String, String> {
    let stat = shell
        .stat(path)
        .await
        .map_err(|e| format!("unavailable: {e:?}"))?;
    if stat.length > reference::DISPLAY_BYTES {
        return Err(
            "display bound: field exceeds 262144 bytes; original remains in AgentFS".into(),
        );
    }
    reference::range(shell, path, 0, stat.length).await
}

pub(super) async fn read_detail(
    shell: &alan_shell::Shell,
    path: &str,
    id: &str,
) -> Vec<Line<'static>> {
    let base = format!("{path}/actions/{id}");
    // Output is the original evidence owner; do not silently substitute metadata.
    let output = field(shell, &format!("{base}/output")).await;
    let result = field(shell, &format!("{base}/result")).await;
    let name = field(shell, &format!("{base}/name")).await;
    let status = field(shell, &format!("{base}/status")).await;
    let snapshot = ActionSnapshot {
        id: id.into(),
        name: name.clone().unwrap_or_default(),
        status: status.clone().unwrap_or_default(),
        output: String::new(),
        result: result.clone().unwrap_or_default(),
    };
    let mut rows = Vec::new();
    let mut readable = false;
    for (label, value) in [
        ("Name", name),
        ("Status", status),
        ("Original output", output),
        ("Result metadata", result),
    ] {
        rows.push(Line::from(label));
        match value {
            Err(error) => rows.push(Line::from(error)),
            Ok(text) if text.is_empty() => rows.push(Line::from("(empty)")),
            Ok(text) => {
                if text.contains("[REDACTED reason=") {
                    rows.push(Line::from("Redacted evidence (not truncation)"));
                }
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    match value.get("type").and_then(|v| v.as_str()) {
                        Some("evidence_retention_expired") => rows.push(Line::from(format!(
                            "Evidence retention expired: {}",
                            value.get("cause").unwrap_or(&serde_json::Value::Null)
                        ))),
                        Some("evidence_projection") => {
                            rows.push(Line::from("Evidence projection: bounded preview; resolving retained reference"));
                            let resolved = resolve_reference(shell, &value, path).await;
                            readable |= resolved.readable;
                            rows.extend(resolved.rows);
                        }
                        _ => {}
                    }
                }
                let acquired = presentation::acquired_content(&mut rows, &text);
                readable |= label == "Original output" && acquired;
            }
        }
    }
    let mut header = vec![Line::from(format!("Action {id} · {path}"))];
    if let Some(cell) = action_snapshot_to_history_cell(&snapshot) {
        let mut detail = crate::history::action_detail(&cell);
        if readable {
            detail.truncate(1);
        } else if serde_json::from_str::<serde_json::Value>(&snapshot.result)
            .ok()
            .is_some_and(|value| value["type"] == "evidence_projection")
        {
            detail.insert(1.min(detail.len()), Line::from("Bounded preview"));
        }
        header.extend(detail);
    }
    header.extend(rows);
    header
}
