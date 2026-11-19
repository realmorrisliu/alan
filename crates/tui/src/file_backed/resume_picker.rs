//! Bounded `/resume [query]` rollout picker backed by `/agent/history/{catalog,ctl}`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use serde::Deserialize;
use unicode_width::UnicodeWidthStr;

use super::FileBackedApp;
use super::app::{FileBackedAction, FileBackedEvent, Notice};
use crate::history::clean_text;

pub(in crate::file_backed) const HISTORY_CATALOG_PATH: &str = "/agent/history/catalog";
pub(in crate::file_backed) const HISTORY_CTL_PATH: &str = "/agent/history/ctl";

/// Client projection of `/agent/history/catalog`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub(in crate::file_backed) struct RolloutHistoryCatalogView {
    #[serde(default)]
    pub current_rollout_id: Option<String>,
    #[serde(default)]
    pub active_project_cwd: Option<String>,
    #[serde(default)]
    pub active_project_fingerprint: Option<String>,
    #[serde(default)]
    pub excluded_cross_project_count: usize,
    #[serde(default)]
    pub entries: Vec<RolloutHistoryItem>,
    #[serde(default)]
    pub diagnostics: Vec<HistoryDiagnosticView>,
}

/// One discoverable retained rollout within the active project boundary.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::file_backed) struct RolloutHistoryItem {
    pub rollout_id: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub project_cwd: Option<String>,
    #[serde(default)]
    pub project_label: Option<String>,
    #[serde(default)]
    pub project_fingerprint: Option<String>,
    #[serde(default)]
    pub turn_count: usize,
    #[serde(default)]
    pub last_status: String,
    #[serde(default)]
    pub goal_snippet: String,
    #[serde(default)]
    pub has_pending_checkpoint: bool,
    #[serde(default)]
    pub has_unknown_effects: bool,
    #[serde(default)]
    pub corrupt_lines: usize,
}

/// Non-fatal per-file diagnostic returned with the history catalog.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(in crate::file_backed) struct HistoryDiagnosticView {
    pub file_name: String,
    pub message: String,
}

/// Interactive state for `/resume [query]`.
#[derive(Clone, Default)]
pub(in crate::file_backed) struct ResumePicker {
    pub active: bool,
    pub loading: bool,
    pub generation: u64,
    pub query: String,
    pub selected: usize,
    pub catalog: Option<RolloutHistoryCatalogView>,
    pub filtered: Vec<usize>,
    pub error: Option<String>,
}

impl ResumePicker {
    pub(in crate::file_backed) fn open(&mut self, query: &str) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.active = true;
        self.loading = true;
        self.query = query.trim().to_string();
        self.selected = 0;
        self.catalog = None;
        self.filtered.clear();
        self.error = None;
        self.generation
    }

    pub(in crate::file_backed) fn close(&mut self) {
        self.active = false;
        self.loading = false;
        self.query.clear();
        self.selected = 0;
    }

    pub(in crate::file_backed) fn apply_loaded(
        &mut self,
        generation: u64,
        result: Result<RolloutHistoryCatalogView, String>,
    ) {
        if generation != self.generation || !self.active {
            return;
        }
        self.loading = false;
        match result {
            Ok(catalog) => {
                self.error = None;
                self.catalog = Some(catalog);
                self.recompute_filter();
            }
            Err(error) => {
                self.catalog = None;
                self.filtered.clear();
                self.selected = 0;
                self.error = Some(error);
            }
        }
    }

    pub(in crate::file_backed) fn recompute_filter(&mut self) {
        let Some(catalog) = self.catalog.as_ref() else {
            self.filtered.clear();
            self.selected = 0;
            return;
        };
        let terms: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        self.filtered = catalog
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry_matches_terms(entry, &terms))
            .map(|(index, _)| index)
            .collect();
        let total = self.total_rows();
        if total == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(total - 1);
        }
    }

    /// Total selectable rows when catalog is ready (filtered rollouts + `"Start fresh session"`).
    pub(in crate::file_backed) fn total_rows(&self) -> usize {
        if self.loading {
            0
        } else if self.catalog.is_some() {
            self.filtered.len() + 1
        } else {
            // Even when catalog load failed, allow choosing fresh start or Esc cancel.
            1
        }
    }

    /// Return `Some(Some(rollout_id))` to resume, `Some(None)` for fresh, or `None` if unavailable.
    pub(in crate::file_backed) fn selected_target(&self) -> Option<Option<String>> {
        if self.loading {
            return None;
        }
        if let Some(catalog) = self.catalog.as_ref()
            && let Some(&entry_idx) = self.filtered.get(self.selected)
        {
            return catalog
                .entries
                .get(entry_idx)
                .map(|entry| Some(entry.rollout_id.clone()));
        }
        if self.selected == self.filtered.len() {
            return Some(None);
        }
        None
    }
}

fn entry_matches_terms(entry: &RolloutHistoryItem, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {} {}",
        entry.rollout_id,
        entry.goal_snippet,
        entry.project_label.as_deref().unwrap_or(""),
        entry.last_status,
        entry.updated_at,
    )
    .to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term.as_str()))
}

impl FileBackedApp {
    pub(in crate::file_backed) fn resume_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        if self.turn_active() || self.pending_yield.is_some() || !self.running_tools.is_empty() {
            self.notice = Some(Notice::warning(
                "Wait for or interrupt the active turn before /resume.",
            ));
            return None;
        }
        if self.project_selection.is_some() || self.pending_project_control.is_some() {
            self.notice = Some(Notice::warning(
                "Finish or cancel the active project selection before /resume.",
            ));
            return None;
        }
        self.modal.active = false;
        self.model_chooser.active = false;
        self.completion = None;
        let generation = self.resume_picker.open(argument);
        Some(FileBackedAction::ReadHistoryCatalog { generation })
    }

    pub(in crate::file_backed) fn resume_picker_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
        if !self.resume_picker.active {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let total = self.resume_picker.total_rows();
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') if key.code == KeyCode::Esc || ctrl => {
                self.resume_picker.close();
                self.notice = Some("Resume cancelled.".into());
            }
            KeyCode::Up if total > 0 => {
                self.resume_picker.selected = self.resume_picker.selected.saturating_sub(1);
            }
            KeyCode::Down if total > 0 => {
                self.resume_picker.selected = (self.resume_picker.selected + 1).min(total - 1);
            }
            KeyCode::PageUp if total > 0 => {
                self.resume_picker.selected = self.resume_picker.selected.saturating_sub(8);
            }
            KeyCode::PageDown if total > 0 => {
                self.resume_picker.selected = (self.resume_picker.selected + 8).min(total - 1);
            }
            KeyCode::Backspace if !self.resume_picker.query.is_empty() => {
                self.resume_picker.query.pop();
                self.resume_picker.selected = 0;
                self.resume_picker.recompute_filter();
            }
            KeyCode::Char('u') if ctrl && !self.resume_picker.query.is_empty() => {
                self.resume_picker.query.clear();
                self.resume_picker.selected = 0;
                self.resume_picker.recompute_filter();
            }
            KeyCode::Char(ch) if !ctrl && !alt && !ch.is_control() => {
                self.resume_picker.query.push(ch);
                self.resume_picker.selected = 0;
                self.resume_picker.recompute_filter();
            }
            KeyCode::Enter => {
                let target = self.resume_picker.selected_target()?;
                self.resume_picker.close();
                self.notice = Some(match target.as_deref() {
                    Some(id) => format!("Switching Root Agent to rollout {id}…").into(),
                    None => "Starting fresh Root Agent rollout…".into(),
                });
                return Some(FileBackedAction::SwitchRollout { rollout_id: target });
            }
            _ => {}
        }
        None
    }

    pub(in crate::file_backed) fn reset_for_rollout_switch(&mut self, rollout_id: Option<&str>) {
        self.rollout_switching = true;
        self.clear_local_receipts();
        self.transcript.clear();
        self.action_cells.clear();
        self.projected_actions.clear();
        self.running_tools.clear();
        self.plan = alan_agent_protocol::UiPlanSnapshot::empty();
        self.plan_owners.clear();
        self.plan_revision = 0;
        self.thinking = alan_agent_protocol::UiThinkingSnapshot::idle();
        self.usage = super::usage::UsageProjection::default();
        self.reconciler = crate::reconcile::StreamReconciler::new();
        self.pending_remote_turn_start = None;
        self.scrollback_front_is_partial = false;
        self.pending_yield = None;
        self.form = None;
        self.completion = None;
        self.tape_consumed_offset = 0;
        self.last_input_failed = false;
        self.auto_continue_on_empty_cancel = false;
        self.notice = Some(match rollout_id {
            Some(id) => format!("Resumed rollout {id}; reattaching Root Agent…").into(),
            None => "Started fresh session rollout; reattaching Root Agent…".into(),
        });
    }
}

pub(in crate::file_backed) fn spawn_load_catalog(
    shell: &alan_shell::Shell,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    generation: u64,
) {
    let shell = shell.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let result = match shell.cat(HISTORY_CATALOG_PATH).await {
            Ok(bytes) => serde_json::from_slice::<RolloutHistoryCatalogView>(&bytes)
                .map_err(|err| format!("invalid history catalog JSON: {err}")),
            Err(err) => Err(format!("history catalog unavailable: {err:#}")),
        };
        let _ = tx
            .send(FileBackedEvent::HistoryCatalogLoaded { generation, result })
            .await;
    });
}

pub(in crate::file_backed) fn spawn_switch_rollout(
    shell: &alan_shell::Shell,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    rollout_id: Option<String>,
) {
    let shell = shell.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let command = match rollout_id.as_deref() {
            Some(id) => format!("resume {id}"),
            None => "fresh".to_string(),
        };
        let result = shell
            .write(HISTORY_CTL_PATH, command.as_bytes())
            .await
            .map_err(|err| format!("{err:?}"));
        let _ = tx
            .send(FileBackedEvent::HistorySwitchCompleted { rollout_id, result })
            .await;
    });
}

/// Render `/resume [query]` modal lines for tests and full-screen draw.
pub(in crate::file_backed) fn render_lines(
    app: &FileBackedApp,
    width: usize,
) -> Vec<Line<'static>> {
    let theme = app.theme;
    let picker = &app.resume_picker;
    let scope = picker
        .catalog
        .as_ref()
        .and_then(|c| c.active_project_cwd.as_deref())
        .or_else(|| app.project.as_ref().map(|p| p.namespace_path.as_str()))
        .unwrap_or("/");
    let header_title = if picker.query.is_empty() {
        format!("Resume Session · project {}", clean_text(scope))
    } else {
        format!(
            "Resume Session · project {} · filter \"{}\"",
            clean_text(scope),
            clean_text(&picker.query)
        )
    };
    let hint = if width >= 60 {
        "↑↓ select · Type to filter · Backspace · Enter switch · Esc cancel"
    } else {
        "↑↓ · filter · Enter · Esc"
    };
    let mut rows = vec![
        Line::styled(clamp_to_width(&header_title, width), theme.accent_bold()),
        Line::styled(clamp_to_width(hint, width), theme.muted_style()),
    ];

    if picker.loading {
        rows.push(Line::styled(
            "Loading retained rollout history…",
            theme.muted_style(),
        ));
        return rows;
    }

    if let Some(error) = picker.error.as_deref() {
        rows.push(Line::styled(
            clamp_to_width(
                &format!("Catalog unavailable: {}", clean_text(error)),
                width,
            ),
            theme.block_style(),
        ));
    }

    if let Some(catalog) = picker.catalog.as_ref() {
        if catalog.excluded_cross_project_count > 0 {
            rows.push(Line::styled(
                clamp_to_width(
                    &format!(
                        "Cross-project isolation: excluded {} rollout(s) from other workspaces.",
                        catalog.excluded_cross_project_count
                    ),
                    width,
                ),
                theme.muted_style(),
            ));
        }
        for diag in catalog.diagnostics.iter().take(3) {
            rows.push(Line::styled(
                clamp_to_width(
                    &format!(
                        "Diagnostic ({}): {}",
                        clean_text(&diag.file_name),
                        clean_text(&diag.message)
                    ),
                    width,
                ),
                theme.wait_style(),
            ));
        }
        if !picker.query.is_empty() && picker.filtered.is_empty() {
            rows.push(Line::styled(
                format!("No rollouts match \"{}\".", clean_text(&picker.query)),
                theme.muted_style(),
            ));
        }
        for (visible_idx, &entry_idx) in picker.filtered.iter().enumerate() {
            let entry = &catalog.entries[entry_idx];
            let selected = visible_idx == picker.selected;
            let (prefix_style, left_style, detail_style) = theme.completion_row_styles(selected);
            let prefix = if selected {
                if theme.is_dumb { "> " } else { "▶ " }
            } else {
                "  "
            };
            let current_tag = if catalog.current_rollout_id.as_deref() == Some(&entry.rollout_id) {
                " · active"
            } else {
                ""
            };
            let project = entry
                .project_label
                .as_deref()
                .map(clean_text)
                .unwrap_or_else(|| "unmounted".to_string());
            let mut flags = Vec::new();
            if entry.has_pending_checkpoint {
                flags.push("pending-checkpoint".to_string());
            }
            if entry.has_unknown_effects {
                flags.push("unknown-effects".to_string());
            }
            if entry.corrupt_lines > 0 {
                flags.push(format!("{} corrupt line(s)", entry.corrupt_lines));
            }
            let flag_suffix = if flags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", flags.join(", "))
            };
            let summary = format!(
                "{}{} · {} · {} · {}t · {}{}",
                clean_text(&entry.rollout_id),
                current_tag,
                clean_text(&entry.updated_at),
                project,
                entry.turn_count,
                clean_text(&entry.last_status),
                flag_suffix,
            );
            rows.push(Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(
                    clamp_to_width(&summary, width.saturating_sub(2)),
                    left_style,
                ),
            ]));
            let goal = if entry.goal_snippet.trim().is_empty() {
                "(no user prompt recorded)".to_string()
            } else {
                clean_text(&entry.goal_snippet)
            };
            rows.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(clamp_to_width(&goal, width.saturating_sub(4)), detail_style),
            ]));
        }
    }

    let fresh_selected = picker.selected == picker.filtered.len();
    let (prefix_style, left_style, detail_style) = theme.completion_row_styles(fresh_selected);
    let prefix = if fresh_selected {
        if theme.is_dumb { "> " } else { "▶ " }
    } else {
        "  "
    };
    rows.push(Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled("Start fresh session", left_style),
        Span::styled(
            clamp_to_width(" — begin a new Root rollout", width.saturating_sub(22)),
            detail_style,
        ),
    ]));
    rows
}

pub(in crate::file_backed) fn draw(frame: &mut Frame<'_>, app: &FileBackedApp) {
    let area = frame.area();
    let lines = render_lines(app, area.width as usize);
    let height = area.height as usize;
    let skip = if height == 0 || lines.len() <= height {
        0
    } else {
        // Keep the highlighted entry near the viewport bottom when scrolling down.
        let approx_selected_row = 2 + app.resume_picker.selected.saturating_mul(2);
        approx_selected_row
            .saturating_add(3)
            .saturating_sub(height)
            .min(lines.len().saturating_sub(height))
    };
    let visible: Vec<Line<'static>> = lines.into_iter().skip(skip).take(height).collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn clamp_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}");
        if UnicodeWidthStr::width(candidate.as_str()) >= max_width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_catalog() -> RolloutHistoryCatalogView {
        RolloutHistoryCatalogView {
            current_rollout_id: Some("rollout-2".to_string()),
            active_project_cwd: Some("/mnt/project".to_string()),
            active_project_fingerprint: Some("fp-123".to_string()),
            excluded_cross_project_count: 2,
            entries: vec![
                RolloutHistoryItem {
                    rollout_id: "rollout-2".to_string(),
                    file_name: Some("rollout-2.jsonl".to_string()),
                    updated_at: "2026-11-19T09:00:00Z".to_string(),
                    project_cwd: Some("/mnt/project".to_string()),
                    project_label: Some("alan".to_string()),
                    project_fingerprint: Some("fp-123".to_string()),
                    turn_count: 5,
                    last_status: "idle".to_string(),
                    goal_snippet: "Wire parallel subagents".to_string(),
                    has_pending_checkpoint: false,
                    has_unknown_effects: false,
                    corrupt_lines: 0,
                },
                RolloutHistoryItem {
                    rollout_id: "rollout-1".to_string(),
                    file_name: Some("rollout-1.jsonl".to_string()),
                    updated_at: "2026-11-18T18:00:00Z".to_string(),
                    project_cwd: Some("/mnt/project".to_string()),
                    project_label: Some("alan".to_string()),
                    project_fingerprint: Some("fp-123".to_string()),
                    turn_count: 2,
                    last_status: "paused".to_string(),
                    goal_snippet: "Debug auth login token flow".to_string(),
                    has_pending_checkpoint: true,
                    has_unknown_effects: true,
                    corrupt_lines: 3,
                },
            ],
            diagnostics: vec![HistoryDiagnosticView {
                file_name: "rollout-1.jsonl".to_string(),
                message: "skipped 3 corrupt line(s)".to_string(),
            }],
        }
    }

    #[test]
    fn resume_picker_filters_by_query_and_handles_selection_fresh_and_cancel() {
        let mut app = FileBackedApp::new("/agent/root".to_string());
        let action = app.handle_command("/resume auth");
        assert!(matches!(
            action,
            Some(FileBackedAction::ReadHistoryCatalog { .. })
        ));
        assert!(app.resume_picker.active);
        let generation = app.resume_picker.generation;
        app.dispatch(FileBackedEvent::HistoryCatalogLoaded {
            generation,
            result: Ok(sample_catalog()),
        });
        assert_eq!(app.resume_picker.filtered, vec![1]);
        assert_eq!(
            app.resume_picker.selected_target(),
            Some(Some("rollout-1".to_string()))
        );

        let picked = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(
            picked,
            Some(FileBackedAction::SwitchRollout { rollout_id: Some(ref id) }) if id == "rollout-1"
        ));
        assert!(!app.resume_picker.active);

        // Selecting Start fresh session after moving Down reaches fresh rollout target.
        app.handle_command("/resume");
        let gen2 = app.resume_picker.generation;
        app.dispatch(FileBackedEvent::HistoryCatalogLoaded {
            generation: gen2,
            result: Ok(sample_catalog()),
        });
        assert_eq!(app.resume_picker.filtered, vec![0, 1]);
        app.handle_key(KeyEvent::from(KeyCode::Down));
        app.handle_key(KeyEvent::from(KeyCode::Down));
        let fresh = app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(matches!(
            fresh,
            Some(FileBackedAction::SwitchRollout { rollout_id: None })
        ));

        // Esc cancels without emitting a switch action.
        app.handle_command("/resume");
        let gen3 = app.resume_picker.generation;
        app.dispatch(FileBackedEvent::HistoryCatalogLoaded {
            generation: gen3,
            result: Ok(sample_catalog()),
        });
        assert!(app.handle_key(KeyEvent::from(KeyCode::Esc)).is_none());
        assert!(!app.resume_picker.active);
    }

    #[test]
    fn resume_picker_renders_diagnostics_isolation_and_flags() {
        let mut app = FileBackedApp::new("/agent/root".to_string());
        app.handle_command("/resume");
        let generation = app.resume_picker.generation;
        app.dispatch(FileBackedEvent::HistoryCatalogLoaded {
            generation,
            result: Ok(sample_catalog()),
        });
        let text = render_lines(&app, 100)
            .into_iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("excluded 2 rollout(s)"));
        assert!(text.contains("Diagnostic (rollout-1.jsonl)"));
        assert!(text.contains("pending-checkpoint"));
        assert!(text.contains("unknown-effects"));
        assert!(text.contains("3 corrupt line(s)"));
        assert!(text.contains("Start fresh session"));
    }
}
