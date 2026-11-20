//! `/summary [show|edit <text>|delete]` inspection and revision-safe control via `/agent/history/summary*`.

use ratatui::text::Line;
use serde::Deserialize;

use super::FileBackedApp;
use super::app::{FileBackedAction, FileBackedEvent, Notice};
use crate::history::{HistoryCell, clean_text};

pub(in crate::file_backed) const HISTORY_SUMMARY_PATH: &str = "/agent/history/summary";
pub(in crate::file_backed) const HISTORY_SUMMARY_CTL_PATH: &str = "/agent/history/summary_ctl";

/// Client projection of `/agent/history/summary`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::file_backed) struct ProjectSummaryViewDto {
    pub exists: bool,
    pub deleted: bool,
    pub project_key: String,
    #[serde(default)]
    pub project_cwd: Option<String>,
    #[serde(default)]
    pub summary: Option<ProjectSummaryDto>,
    #[serde(default)]
    pub markdown_recall_active: bool,
    pub retention_note: String,
}

/// Summary fields projected from Process Memory Store.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::file_backed) struct ProjectSummaryDto {
    pub goal: String,
    pub progress: String,
    pub verification: String,
    #[serde(default)]
    pub source_rollouts: Vec<String>,
    pub revision: u64,
    #[serde(default)]
    pub updated_at: String,
    pub origin: String,
    #[serde(default)]
    pub deleted: bool,
}

impl FileBackedApp {
    pub(in crate::file_backed) fn summary_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        let trimmed = argument.trim();
        if trimmed.is_empty() || trimmed == "show" {
            self.notice = Some("Loading project continuity summary…".into());
            return Some(FileBackedAction::InspectProjectSummary);
        }
        if trimmed == "delete" {
            self.notice = Some("Deleting derived project continuity summary…".into());
            return Some(FileBackedAction::DeleteProjectSummary {
                expected_revision: self.last_summary_revision,
            });
        }
        if let Some(rest) = trimmed.strip_prefix("edit ") {
            let text = rest.trim();
            if text.is_empty() {
                self.notice = Some(Notice::warning("usage: /summary [show|edit <text>|delete]"));
                return None;
            }
            self.notice = Some("Updating project continuity summary…".into());
            return Some(FileBackedAction::EditProjectSummary {
                text: text.to_string(),
                expected_revision: self.last_summary_revision,
            });
        }
        self.notice = Some(Notice::warning("usage: /summary [show|edit <text>|delete]"));
        None
    }

    pub(in crate::file_backed) fn apply_summary_result(
        &mut self,
        action_label: Option<&str>,
        result: Result<ProjectSummaryViewDto, String>,
    ) {
        match result {
            Ok(view) => {
                self.last_summary_revision = view.summary.as_ref().map(|s| s.revision);
                let lines = format_summary_lines(&view, self.theme);
                self.transcript.push(HistoryCell::Styled(lines));
                self.notice = Some(match action_label {
                    Some("edited") => format!(
                        "Project continuity summary updated (revision {}).",
                        self.last_summary_revision.unwrap_or(0)
                    )
                    .into(),
                    Some("deleted") => clean_text(&view.retention_note).into(),
                    _ => clean_text(&view.retention_note).into(),
                });
            }
            Err(error) => {
                self.notice = Some(Notice::warning(format!("project summary: {error}")));
            }
        }
    }
}

pub(in crate::file_backed) fn spawn_inspect(
    shell: &alan_shell::Shell,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    let shell = shell.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = read_summary_dto(&shell).await;
        let _ = tx
            .send(FileBackedEvent::ProjectSummaryResult {
                action_label: None,
                result,
            })
            .await;
    });
}

pub(in crate::file_backed) fn spawn_edit(
    shell: &alan_shell::Shell,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    text: String,
    expected_revision: Option<u64>,
) {
    let shell = shell.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "action": "edit",
            "text": text,
            "expected_revision": expected_revision,
        });
        let result = match shell
            .write(HISTORY_SUMMARY_CTL_PATH, payload.to_string().as_bytes())
            .await
        {
            Ok(()) => read_summary_dto(&shell).await,
            Err(err) => Err(format!(
                "edit rejected ({err:?}); run `/summary show` to inspect latest revision"
            )),
        };
        let _ = tx
            .send(FileBackedEvent::ProjectSummaryResult {
                action_label: Some("edited"),
                result,
            })
            .await;
    });
}

pub(in crate::file_backed) fn spawn_delete(
    shell: &alan_shell::Shell,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    expected_revision: Option<u64>,
) {
    let shell = shell.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "action": "delete",
            "expected_revision": expected_revision,
        });
        let result = match shell
            .write(HISTORY_SUMMARY_CTL_PATH, payload.to_string().as_bytes())
            .await
        {
            Ok(()) => read_summary_dto(&shell).await,
            Err(err) => Err(format!(
                "delete rejected ({err:?}); run `/summary show` to inspect latest revision"
            )),
        };
        let _ = tx
            .send(FileBackedEvent::ProjectSummaryResult {
                action_label: Some("deleted"),
                result,
            })
            .await;
    });
}

async fn read_summary_dto(shell: &alan_shell::Shell) -> Result<ProjectSummaryViewDto, String> {
    let bytes = shell
        .cat(HISTORY_SUMMARY_PATH)
        .await
        .map_err(|err| format!("summary unavailable: {err:?}"))?;
    serde_json::from_slice::<ProjectSummaryViewDto>(&bytes)
        .map_err(|err| format!("invalid project summary JSON: {err}"))
}

fn format_summary_lines(
    view: &ProjectSummaryViewDto,
    theme: crate::theme::Theme,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        clean_text(&format!(
            "Project Continuity Summary · {}{}",
            view.project_key,
            view.project_cwd
                .as_deref()
                .map(|c| format!(" ({c})"))
                .unwrap_or_default()
        )),
        theme.accent_bold(),
    )];
    if view.deleted {
        let rev = view.summary.as_ref().map_or(0, |s| s.revision);
        lines.push(Line::styled(
            format!("Status: deleted (revision {rev})"),
            theme.wait_style(),
        ));
    } else if let Some(summary) = view.summary.as_ref().filter(|_| view.exists) {
        lines.push(Line::styled(
            clean_text(&format!(
                "Revision {} · origin {} · recall {}",
                summary.revision,
                summary.origin,
                if view.markdown_recall_active {
                    "active"
                } else {
                    "inactive"
                }
            )),
            theme.muted_style(),
        ));
        lines.push(Line::from(format!("Goal: {}", clean_text(&summary.goal))));
        lines.push(Line::from(format!(
            "Progress: {}",
            clean_text(&summary.progress)
        )));
        lines.push(Line::from(format!(
            "Verification: {}",
            clean_text(&summary.verification)
        )));
        let rollouts = if summary.source_rollouts.is_empty() {
            "none".to_string()
        } else {
            summary.source_rollouts.join(", ")
        };
        lines.push(Line::styled(
            clean_text(&format!("Source rollouts: {rollouts}")),
            theme.muted_style(),
        ));
    } else {
        lines.push(Line::styled(
            "Status: no summary recorded yet",
            theme.muted_style(),
        ));
    }
    lines.push(Line::styled(
        clean_text(&view.retention_note),
        theme.muted_style(),
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_slash_command_routes_show_edit_and_delete() {
        let mut app = FileBackedApp::new("/agent/root".to_string());
        assert!(matches!(
            app.handle_command("/summary"),
            Some(FileBackedAction::InspectProjectSummary)
        ));
        assert!(matches!(
            app.handle_command("/summary show"),
            Some(FileBackedAction::InspectProjectSummary)
        ));

        app.dispatch(FileBackedEvent::ProjectSummaryResult {
            action_label: None,
            result: Ok(ProjectSummaryViewDto {
                exists: true,
                deleted: false,
                project_key: "repo-123".to_string(),
                project_cwd: Some("/mnt/repo".to_string()),
                summary: Some(ProjectSummaryDto {
                    goal: "Ship M3 continuity".to_string(),
                    progress: "Summary ready".to_string(),
                    verification: "Tests pass".to_string(),
                    source_rollouts: vec!["r-1".to_string()],
                    revision: 4,
                    updated_at: "2026-11-19T10:00:00Z".to_string(),
                    origin: "generated".to_string(),
                    deleted: false,
                }),
                markdown_recall_active: true,
                retention_note: "Derived project continuity summary only.".to_string(),
            }),
        });
        assert_eq!(app.last_summary_revision, Some(4));

        let edit = app.handle_command("/summary edit goal: Updated goal");
        assert!(matches!(
            edit,
            Some(FileBackedAction::EditProjectSummary {
                ref text,
                expected_revision: Some(4)
            }) if text == "goal: Updated goal"
        ));

        let del = app.handle_command("/summary delete");
        assert!(matches!(
            del,
            Some(FileBackedAction::DeleteProjectSummary {
                expected_revision: Some(4)
            })
        ));
    }
}
