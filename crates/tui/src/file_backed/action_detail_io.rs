//! Fresh, read-only retained Action detail IO. AgentFS owns the catalog and evidence.
use super::app::{FileBackedApp, FileBackedEvent};
use super::file_surface::{ActionSnapshot, action_snapshot_to_history_cell, read_action_ids};
use ratatui::text::Line;
mod reference;
use reference::resolve as resolve_reference;

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
    let shell = shell.clone();
    let tx = tx.clone();
    app.modal.owner_path = owner_path.into();
    let path = owner_path.to_string();
    let generation = app.modal.generation;
    let selected = app.modal.ids.get(app.modal.selected).cloned();
    tokio::spawn(async move {
        let ids = read_action_ids(&shell, &path)
            .await
            .map_err(|e| e.to_string());
        let id = selected.or_else(|| ids.as_ref().ok().and_then(|v| v.first().cloned()));
        if tx
            .send(FileBackedEvent::ActionDetails {
                path: path.clone(),
                generation,
                ids: ids.clone(),
                id: id.clone(),
                rows: Vec::new(),
            })
            .await
            .is_err()
        {
            return;
        }
        let rows = match &id {
            Some(id) => read_detail(&shell, &path, id).await,
            None => vec![Line::from("No retained Actions")],
        };
        let _ = tx
            .send(FileBackedEvent::ActionDetails {
                path,
                generation,
                ids,
                id,
                rows,
            })
            .await;
    });
}

async fn field(shell: &alan_shell::Shell, path: &str) -> Result<String, String> {
    let stat = shell
        .stat(path)
        .await
        .map_err(|e| format!("unavailable: {e:?}"))?;
    if stat.length > 262144 {
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
    let mut rows = vec![Line::from(format!("Action {id}"))];
    if let Some(cell) = action_snapshot_to_history_cell(&snapshot) {
        rows.extend(crate::history::action_detail(&cell));
    }
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
                            rows.extend(resolve_reference(shell, &value, path).await);
                        }
                        _ => {}
                    }
                }
                rows.extend(
                    text.lines()
                        .map(|s| Line::from(crate::history::clean_text(s))),
                );
            }
        }
    }
    rows
}
