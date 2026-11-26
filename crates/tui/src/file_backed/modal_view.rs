//! Full-screen inspector modal rendering (`Ctrl+O` Actions/Plans/Tape/Status & `Ctrl+T` Status & `/man`)
//! plus deterministic tab hit-testing for modal-only SGR mouse clicks.

use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::FileBackedApp;
use super::app::action_modal::ModalMode;

/// Tab descriptor shared between modal rendering and mouse click hit-testing.
pub(in crate::file_backed) struct ModalTabHit {
    pub mode: ModalMode,
    pub start_col: u16,
    pub end_col: u16,
}

const MODAL_TABS: &[(ModalMode, &str, &str)] = &[
    (ModalMode::Actions, "a", "Actions"),
    (ModalMode::Plans, "p", "Plans"),
    (ModalMode::Tape, "t", "Tape"),
    (ModalMode::Status, "s", "Status"),
];

/// Compute the tab bar spans and column hit-ranges for row 1 of the inspector modal.
pub(in crate::file_backed) fn modal_tab_bar(
    active_mode: ModalMode,
    theme: crate::theme::Theme,
    width: usize,
) -> (Line<'static>, Vec<ModalTabHit>) {
    let sep = if theme.is_dumb { " | " } else { " · " };
    let mut spans = Vec::new();
    let mut hits = Vec::new();
    let mut col = 0usize;

    for (idx, &(mode, key, name)) in MODAL_TABS.iter().enumerate() {
        let selected = mode == active_mode;
        let label = if selected {
            format!("[{key}:{name}]")
        } else {
            format!("{key}:{name}")
        };
        let sep_w = if idx > 0 { sep.width() } else { 0 };
        let lw = label.width();
        if col + sep_w + lw > width {
            break;
        }
        if idx > 0 {
            spans.push(Span::styled(sep.to_string(), theme.muted_style()));
            col += sep_w;
        }
        let style = if selected {
            theme.accent_bold()
        } else {
            theme.muted_style()
        };
        hits.push(ModalTabHit {
            mode,
            start_col: col as u16,
            end_col: (col + lw) as u16,
        });
        spans.push(Span::styled(label, style));
        col += lw;
    }

    let tails = [
        format!("{sep}↑↓/jk{sep}↔{sep}d{sep}Space/b{sep}Esc"),
        format!("{sep}↑↓{sep}↔{sep}Esc"),
        format!("{sep}Esc"),
    ];
    for tail in tails {
        if col + tail.width() <= width {
            spans.push(Span::styled(tail, theme.muted_style()));
            break;
        }
    }

    (Line::from(spans), hits)
}

/// Resolve a mouse click `(column, row)` in the active modal to a tab switch when row 1 is clicked.
pub(in crate::file_backed) fn hit_test_modal_tab(
    active_mode: ModalMode,
    theme: crate::theme::Theme,
    width: usize,
    column: u16,
    row: u16,
) -> Option<ModalMode> {
    if row != 1 {
        return None;
    }
    let (_, hits) = modal_tab_bar(active_mode, theme, width);
    hits.into_iter()
        .find(|hit| column >= hit.start_col && column < hit.end_col)
        .map(|hit| hit.mode)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CollapsedSection {
    None,
    RawBytes,
    ResultMetadata,
}

fn filter_modal_rows(
    rows: &[Line<'static>],
    show_raw_json: bool,
    theme: crate::theme::Theme,
) -> Vec<Line<'static>> {
    if show_raw_json {
        return rows.to_vec();
    }
    let mut filtered = Vec::with_capacity(rows.len());
    let mut collapsed = CollapsedSection::None;

    for row in rows {
        let text = row.to_string();
        if text == "Result metadata" {
            if collapsed != CollapsedSection::ResultMetadata {
                filtered.push(Line::styled(
                    "  [result metadata collapsed · press d to expand]",
                    theme.muted_style(),
                ));
                collapsed = CollapsedSection::ResultMetadata;
            }
            continue;
        }
        if text == "Original raw bytes" {
            if collapsed != CollapsedSection::RawBytes {
                filtered.push(Line::styled(
                    "  [raw JSON/bytes collapsed · press d to expand]",
                    theme.muted_style(),
                ));
                collapsed = CollapsedSection::RawBytes;
            }
            continue;
        }
        if matches!(
            text.as_str(),
            "Name" | "Status" | "Original output" | "Recovered reference (retained original)"
        ) {
            collapsed = CollapsedSection::None;
        }
        if collapsed != CollapsedSection::None {
            continue;
        }
        filtered.push(row.clone());
    }
    filtered
}

/// Draw the active inspector modal (`Actions`, `Plans`, `Tape`, `Status`, `Man`).
pub(in crate::file_backed) fn draw_modal(frame: &mut Frame<'_>, app: &FileBackedApp) {
    let area = frame.area();
    let width = area.width as usize;

    let header_line = match app.modal.mode {
        ModalMode::Actions => {
            let count = app.modal.actions.len();
            let owner = app
                .modal
                .actions
                .get(app.modal.selected)
                .map(|entry| entry.owner.as_str())
                .unwrap_or(&app.modal.owner_path);
            let idx = if count == 0 {
                0
            } else {
                app.modal.selected + 1
            };
            Line::styled(
                format!("{idx}/{count} · Action · {owner}"),
                app.theme.accent_bold(),
            )
        }
        ModalMode::Plans => {
            let count = app.modal.plans.len();
            let owner = app
                .modal
                .plans
                .get(app.modal.selected)
                .map(|entry| entry.owner.as_str())
                .unwrap_or(&app.modal.owner_path);
            let idx = if count == 0 {
                0
            } else {
                app.modal.selected + 1
            };
            Line::styled(
                format!("{idx}/{count} · Plan · {owner}"),
                app.theme.accent_bold(),
            )
        }
        ModalMode::Tape => Line::styled(
            format!("Turing Tape · {}", app.modal.owner_path),
            app.theme.accent_bold(),
        ),
        ModalMode::Status => Line::styled(
            format!("Status · {}", app.modal.owner_path),
            app.theme.accent_bold(),
        ),
        ModalMode::Man => Line::styled(
            format!("Manual · {}", app.modal.owner_path),
            app.theme.accent_bold(),
        ),
    };

    let (tab_line, _) = modal_tab_bar(app.modal.mode, app.theme, width);
    let mut rows = vec![header_line, tab_line];
    if app.modal.mode == ModalMode::Actions && !app.running_tools.is_empty() {
        let active_list: Vec<String> = app
            .running_tools
            .iter()
            .map(|tool| format!("{} · running", crate::history::clean_text(&tool.title)))
            .collect();
        rows.push(Line::styled(
            format!("Active · {}", active_list.join(" | ")),
            app.theme.accent_style(),
        ));
    }
    let header_rows = rows.len();
    let display_rows = if app.modal.mode == ModalMode::Actions {
        filter_modal_rows(&app.modal.rows, app.modal.show_raw_json, app.theme)
    } else {
        app.modal.rows.clone()
    };
    let detail = crate::history::wrap_styled_lines(display_rows, width);
    let start = app.modal.scroll.min(detail.len().saturating_sub(1));
    if detail.is_empty() {
        rows.push(Line::styled(
            "Loading retained details…",
            app.theme.muted_style(),
        ));
    }
    rows.extend(
        detail
            .into_iter()
            .skip(start)
            .take(area.height.saturating_sub(header_rows as u16) as usize),
    );
    frame.render_widget(Paragraph::new(rows), area);
}
