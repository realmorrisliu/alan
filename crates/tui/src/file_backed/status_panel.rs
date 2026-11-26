//! Read-only Status Panel (`Ctrl+T` & `Ctrl+O` -> `s` tab).
//!
//! Surfaces full task budget breakdown, mounted `/mnt/*` grants, active/installed `q`
//! fileserver packages, active CoW branches (`/mnt/branch`), project continuity `/summary`
//! revision/status, active Connection profile/catalog, System 1 stats placeholder slot,
//! and OS sandbox/host isolation diagnostics (the ONLY modal alongside `/doctor` that
//! mentions the host sandbox backend name).

use ratatui::text::{Line, Span};
use serde_json::Value;

use super::FileBackedApp;
use super::app::FileBackedEvent;
use super::model::safe;
use super::summary_view::{HISTORY_SUMMARY_PATH, ProjectSummaryViewDto};
use crate::history::clean_text;

/// Live namespace diagnostics fetched asynchronously when the Status panel opens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::file_backed) struct StatusDiagnostics {
    pub loaded: bool,
    pub mnt_entries: Vec<String>,
    pub host_grants: Vec<String>,
    pub fileserver_packages: Vec<String>,
    pub cow_branches: Vec<String>,
    pub summary_line: Option<String>,
}

/// Build immediate Status panel rows from locally observed state plus any cached diagnostics.
pub(in crate::file_backed) fn build_rows(app: &FileBackedApp) -> Vec<Line<'static>> {
    let theme = app.theme;
    let header_style = theme.accent_bold();
    let label_style = theme.muted_style();
    let mut rows = Vec::new();

    // 1. Task Budget Breakdown
    rows.push(Line::styled("Task Budget", header_style));
    let budget_style = super::status_bar::ratio_semantic_style(app.budget.max_ratio_pct(), theme);
    for line in app.budget.detail_lines() {
        rows.push(Line::styled(line, budget_style));
    }
    if let Some(usage) = app.usage.status() {
        rows.push(Line::styled(
            format!("  {}", clean_text(&usage)),
            label_style,
        ));
    }
    rows.push(Line::default());

    // 2. Active Connection Profile / Catalog
    rows.push(Line::styled("Connection & Model", header_style));
    match app.model.known() {
        Some(snapshot) => {
            let selected = snapshot.selected_next.as_ref().map_or_else(
                || "unknown".to_string(),
                |b| {
                    let effort = b
                        .reasoning
                        .effort
                        .map_or_else(|| "default".to_string(), |e| e.to_string());
                    format!(
                        "{} / {} / {} (effort: {effort})",
                        safe(&b.profile),
                        safe(&b.provider),
                        safe(&b.model)
                    )
                },
            );
            let active = snapshot.active.as_ref().map_or_else(
                || "idle".to_string(),
                |b| format!("{} / {}", safe(&b.profile), safe(&b.model)),
            );
            let catalog_summary = match &snapshot.catalog {
                None => "unavailable".to_string(),
                Some(c) => {
                    let profile_count = c.profiles.len().max(usize::from(!c.profile.is_empty()));
                    let model_count = if c.profiles.is_empty() {
                        c.models.len()
                    } else {
                        c.profiles.iter().map(|p| p.models.len()).sum()
                    };
                    format!("{profile_count} profile(s), {model_count} model(s)")
                }
            };
            rows.push(kv_row("selected", &selected, label_style));
            rows.push(kv_row("active", &active, label_style));
            rows.push(kv_row("catalog", &catalog_summary, label_style));
        }
        None => {
            let fallback = app
                .effective_model
                .as_deref()
                .map(|m| format!("{} (observation pending)", safe(m)))
                .unwrap_or_else(|| "unknown or unconfigured".to_string());
            rows.push(kv_row("model", &fallback, label_style));
        }
    }
    rows.push(Line::default());

    // 3. Mounted /mnt/* Grants & Active/Installed q Fileserver Packages
    rows.push(Line::styled("Mounts & Packages (/mnt/*)", header_style));
    if let Some(project) = &app.project {
        rows.push(kv_row(
            "project",
            &format!(
                "{} ({}) -> {}",
                safe(&project.label),
                project.access.label(),
                safe(&project.namespace_path)
            ),
            label_style,
        ));
    } else {
        rows.push(kv_row("project", "none mounted", label_style));
    }
    let diag = &app.status_diagnostics;
    if !diag.host_grants.is_empty() {
        for grant in &diag.host_grants {
            rows.push(kv_row("grant", grant, label_style));
        }
    } else if diag.loaded {
        rows.push(kv_row("grants", "none active", label_style));
    } else {
        rows.push(kv_row("grants", "loading…", label_style));
    }
    if !diag.mnt_entries.is_empty() {
        rows.push(kv_row("/mnt", &diag.mnt_entries.join(", "), label_style));
    }
    if !diag.fileserver_packages.is_empty() {
        for pkg in &diag.fileserver_packages {
            rows.push(kv_row("q pkg", pkg, label_style));
        }
    } else if diag.loaded {
        rows.push(kv_row(
            "q pkgs",
            "no fileserver packages installed",
            label_style,
        ));
    } else {
        rows.push(kv_row("q pkgs", "loading…", label_style));
    }
    rows.push(Line::default());

    // 4. Active CoW Branches (/mnt/branch)
    rows.push(Line::styled("CoW Branches (/mnt/branch)", header_style));
    if !diag.cow_branches.is_empty() {
        for branch in &diag.cow_branches {
            rows.push(kv_row("branch", branch, label_style));
        }
    } else if app.cow_branch_count > 0 {
        rows.push(kv_row(
            "active",
            &format!("{} branch(es)", app.cow_branch_count),
            label_style,
        ));
    } else if diag.loaded {
        rows.push(kv_row("branches", "none active", label_style));
    } else {
        rows.push(kv_row("branches", "loading…", label_style));
    }
    rows.push(Line::default());

    // 5. Project Continuity (/summary)
    rows.push(Line::styled("Project Continuity (/summary)", header_style));
    if let Some(summary_line) = &diag.summary_line {
        rows.push(kv_row("summary", summary_line, label_style));
    } else if let Some(rev) = app.last_summary_revision {
        rows.push(kv_row("summary", &format!("revision {rev}"), label_style));
    } else if diag.loaded {
        rows.push(kv_row(
            "summary",
            "unavailable or not recorded",
            label_style,
        ));
    } else {
        rows.push(kv_row("summary", "loading…", label_style));
    }
    if app.ephemeral || app.no_resume_update {
        let mut flags = Vec::new();
        if app.ephemeral {
            flags.push("ephemeral");
        }
        if app.no_resume_update {
            flags.push("no-resume");
        }
        rows.push(kv_row("session", &flags.join(", "), label_style));
    }
    rows.push(Line::default());

    // 6. System 1 Evaluator Stats (placeholder slot)
    rows.push(Line::styled("System 1 (Fast Evaluator)", header_style));
    rows.push(kv_row(
        "stats",
        "standby (telemetry slot ready)",
        label_style,
    ));
    rows.push(Line::default());

    // 7. OS Sandbox / Host Isolation Diagnostics
    // (The ONLY modal alongside /doctor that mentions the host sandbox backend name.)
    rows.push(Line::styled("OS Sandbox & Isolation", header_style));
    let trust = app
        .trust_posture_label
        .as_deref()
        .map(safe)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "governed".to_string());
    let backend = app
        .sandbox_backend
        .as_deref()
        .map(safe)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let isolation_style = if super::app::presentation::is_unsandboxed_posture(&trust)
        || super::app::presentation::is_unsandboxed_posture(&backend)
    {
        theme.bold_if_capable(theme.block_style())
    } else {
        theme.run_style()
    };
    rows.push(Line::from(vec![
        Span::styled(format!("  {:<12}", "posture"), label_style),
        Span::styled(clean_text(&trust), isolation_style),
    ]));
    rows.push(Line::from(vec![
        Span::styled(format!("  {:<12}", "backend"), label_style),
        Span::styled(clean_text(&backend), isolation_style),
    ]));
    rows.push(kv_row("process", &safe(&app.agent_path), label_style));

    rows
}

fn kv_row(label: &str, value: &str, label_style: ratatui::style::Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label:<12}"), label_style),
        Span::raw(clean_text(value)),
    ])
}

/// Spawn a bounded read-only async refresh of namespace diagnostics for the Status panel.
pub(in crate::file_backed) fn start(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    let generation = app.modal.generation;
    let shell = shell.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let diagnostics = collect_diagnostics(&shell).await;
        let _ = tx
            .send(FileBackedEvent::StatusPanelLoaded {
                generation,
                diagnostics,
            })
            .await;
    });
}

/// Lightweight read of active CoW branch count from `/mnt/branch/branches`.
pub(in crate::file_backed) async fn read_cow_branch_count(shell: &alan_shell::Shell) -> usize {
    let Ok(entries) = shell.ls("/mnt/branch/branches").await else {
        return 0;
    };
    let mut active = 0usize;
    for id in entries.into_iter().take(64) {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        let path = format!("/mnt/branch/branches/{id}");
        if let Ok(bytes) = shell.cat(&path).await
            && let Ok(val) = serde_json::from_slice::<Value>(&bytes)
        {
            let status = val
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("active");
            if matches!(status, "active" | "selected") {
                active += 1;
            }
        } else {
            active += 1;
        }
    }
    active
}

pub(in crate::file_backed) async fn collect_diagnostics(
    shell: &alan_shell::Shell,
) -> StatusDiagnostics {
    let mut mnt_entries: Vec<String> = shell
        .ls("/mnt")
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    mnt_entries.sort();

    // Host mounts under /mnt/host-mount/grants
    let mut host_grants = Vec::new();
    if let Ok(grant_ids) = shell.ls("/mnt/host-mount/grants").await {
        for grant_id in grant_ids.into_iter().take(32) {
            let grant_id = grant_id.trim();
            if grant_id.is_empty() {
                continue;
            }
            let record_path = format!("/mnt/host-mount/grants/{grant_id}/record");
            if let Ok(bytes) = shell.cat(&record_path).await
                && let Ok(val) = serde_json::from_slice::<Value>(&bytes)
            {
                let ns = val
                    .get("namespace_path")
                    .and_then(Value::as_str)
                    .unwrap_or(grant_id);
                let access = val
                    .get("access")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let state = val.get("state").and_then(Value::as_str).unwrap_or("active");
                host_grants.push(format!("{ns} ({access}, {state})"));
            } else {
                host_grants.push(grant_id.to_string());
            }
        }
    }

    // Installed / active q fileserver packages from /mnt/package/catalog
    let mut fileserver_packages = Vec::new();
    if let Ok(bytes) = shell.cat("/mnt/package/catalog").await
        && let Ok(val) = serde_json::from_slice::<Value>(&bytes)
        && let Some(pkgs) = val.get("packages").and_then(Value::as_object)
    {
        for (id, record) in pkgs {
            let state = record
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("installed");
            let exports = record.get("exports").and_then(Value::as_array);
            let mut found_fs = false;
            if let Some(exports) = exports {
                for exp in exports {
                    let kind = exp.get("kind").and_then(Value::as_str).unwrap_or("");
                    if kind == "fileserver" {
                        found_fs = true;
                        let svc = exp
                            .get("service_id")
                            .and_then(Value::as_str)
                            .unwrap_or(id.as_str());
                        let mounted = mnt_entries.iter().any(|e| e == svc);
                        let status = if mounted { "active" } else { state };
                        fileserver_packages.push(format!("{id} -> /mnt/{svc} ({status})"));
                    }
                }
            }
            if !found_fs && id.contains("fileserver") {
                fileserver_packages.push(format!("{id} ({state})"));
            }
        }
    }

    // Active CoW branches under /mnt/branch/branches
    let mut cow_branches = Vec::new();
    if let Ok(branch_ids) = shell.ls("/mnt/branch/branches").await {
        for branch_id in branch_ids.into_iter().take(32) {
            let branch_id = branch_id.trim();
            if branch_id.is_empty() {
                continue;
            }
            let path = format!("/mnt/branch/branches/{branch_id}");
            if let Ok(bytes) = shell.cat(&path).await
                && let Ok(val) = serde_json::from_slice::<Value>(&bytes)
            {
                let status = val
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("active");
                cow_branches.push(format!("{branch_id} ({status})"));
            } else {
                cow_branches.push(branch_id.to_string());
            }
        }
    }

    // Project continuity /summary revision & status
    let summary_line = match shell.cat(HISTORY_SUMMARY_PATH).await {
        Ok(bytes) => match serde_json::from_slice::<ProjectSummaryViewDto>(&bytes) {
            Ok(view) => {
                if view.deleted {
                    let rev = view.summary.as_ref().map_or(0, |s| s.revision);
                    Some(format!("deleted (rev {rev})"))
                } else if let Some(s) = view.summary.as_ref().filter(|_| view.exists) {
                    let recall = if view.markdown_recall_active {
                        "recall on"
                    } else {
                        "recall off"
                    };
                    Some(format!(
                        "rev {} · {} · {recall}",
                        s.revision,
                        safe(&s.origin)
                    ))
                } else {
                    Some("none recorded yet".to_string())
                }
            }
            Err(_) => Some("invalid summary projection".to_string()),
        },
        Err(_) => None,
    };

    StatusDiagnostics {
        loaded: true,
        mnt_entries,
        host_grants,
        fileserver_packages,
        cow_branches,
        summary_line,
    }
}
