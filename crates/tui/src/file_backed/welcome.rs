use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::history::WelcomeBanner;
use crate::theme::Theme;

use super::app::FileBackedApp;

const RABBIT_LEFT: [&str; 4] = [
    "  (\\_/)  +-------+",
    " =(o.o)= | /proc |",
    "  / >[]  |_______|",
    " (__)__) ==o===o==",
];

/// Return `true` when `ALAN_NO_WELCOME` disables the empty-session welcome header.
pub(in crate::file_backed) fn is_disabled() -> bool {
    std::env::var_os("ALAN_NO_WELCOME").is_some_and(|value| {
        let raw = value.to_string_lossy();
        let trimmed = raw.trim();
        !trimmed.is_empty() && trimmed != "0" && !trimmed.eq_ignore_ascii_case("false")
    })
}

/// Build a [`WelcomeBanner`] snapshot from the hydrated application state.
pub(in crate::file_backed) fn from_app(app: &FileBackedApp) -> WelcomeBanner {
    let project = app
        .project
        .as_ref()
        .map(|p| super::model::safe(&p.label))
        .or_else(|| {
            app.project_candidate
                .as_ref()
                .and_then(|p| p.file_name().and_then(|n| n.to_str()))
                .map(super::model::safe)
        })
        .unwrap_or_else(|| "no project".to_string());
    let model = app
        .model
        .prompt_binding(false)
        .map(|b| super::model::safe(&b.model))
        .or_else(|| app.effective_model.as_deref().map(super::model::safe))
        .unwrap_or_else(|| {
            if app.is_connection_unconfigured() {
                "Not connected".to_string()
            } else {
                "model unset".to_string()
            }
        });
    let sandbox = app
        .sandbox_backend
        .clone()
        .unwrap_or_else(|| "os-sandbox".to_string());
    let trust_posture = app.trust_posture_label.clone();
    WelcomeBanner {
        project,
        model,
        sandbox,
        trust_posture,
    }
}

fn posture_badge_span(
    banner: &WelcomeBanner,
    theme: Theme,
    max_width: usize,
) -> Option<Span<'static>> {
    let raw = banner.trust_posture.as_deref()?;
    let label = super::model::safe(raw);
    let unsandboxed = super::app::presentation::is_unsandboxed_posture(&label);
    let text = if unsandboxed && theme.is_dumb {
        format!("! {label}")
    } else {
        label
    };
    let clamped = clamp_width(&text, max_width);
    if clamped.is_empty() {
        return None;
    }
    let style = if unsandboxed {
        theme.bold_if_capable(theme.block_style())
    } else {
        theme.run_style()
    };
    Some(Span::styled(clamped, style))
}

/// Render the width-responsive scrollable welcome header (`4` lines wide, `2` lines narrow).
pub(crate) fn render_banner_lines(
    banner: &WelcomeBanner,
    width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let unconfigured = banner.model == "Not connected";
    if width >= 64 {
        let right_budget = width.saturating_sub(21).max(1);
        let hints_raw = if unconfigured {
            "/login to connect a provider · /help · /man · : task"
        } else {
            ": task · !shell · /help · /model · /resume · /man"
        };
        let hints = clamp_width(hints_raw, right_budget);
        let tagline = clamp_width("Everything is a file. Even an agent.", right_budget);
        let title = clamp_width("ALAN(1) · alan9", right_budget);
        let summary_line = if let Some(badge) = posture_badge_span(banner, theme, right_budget) {
            let badge_w = badge.content.width();
            let prefix_budget = right_budget.saturating_sub(badge_w + 3);
            let prefix = clamp_width(
                &format!("{} · {}", banner.project, banner.model),
                prefix_budget,
            );
            let mut spans = vec![
                Span::styled(" =(o.o)= | ", theme.muted_style()),
                Span::styled("/proc", theme.accent_style()),
                Span::styled(" |", theme.muted_style()),
                Span::raw("   "),
            ];
            if !prefix.is_empty() {
                spans.push(Span::styled(prefix, Style::default()));
                spans.push(Span::raw(" · "));
            }
            spans.push(badge);
            Line::from(spans)
        } else {
            let summary = clamp_width(
                &format!("{} · {} · {}", banner.project, banner.model, banner.sandbox),
                right_budget,
            );
            Line::from(vec![
                Span::styled(" =(o.o)= | ", theme.muted_style()),
                Span::styled("/proc", theme.accent_style()),
                Span::styled(" |", theme.muted_style()),
                Span::raw("   "),
                Span::styled(summary, Style::default()),
            ])
        };

        vec![
            Line::from(vec![
                Span::styled(RABBIT_LEFT[0], theme.muted_style()),
                Span::raw("   "),
                Span::styled(title, theme.accent_bold()),
            ]),
            summary_line,
            Line::from(vec![
                Span::styled(RABBIT_LEFT[2], theme.muted_style()),
                Span::raw("   "),
                Span::styled(hints, theme.muted_style()),
            ]),
            Line::from(vec![
                Span::styled(RABBIT_LEFT[3], theme.muted_style()),
                Span::raw("   "),
                Span::styled(tagline, theme.faint_style()),
            ]),
        ]
    } else {
        let marker = if theme.is_dumb { "* " } else { "■ " };
        let lead = format!("{marker}ALAN");
        let tail_budget = width.saturating_sub(lead.width());
        let line_top = if let Some(badge) = posture_badge_span(banner, theme, tail_budget) {
            let badge_w = badge.content.width();
            let prefix = clamp_width(
                &format!(" · {}", banner.project),
                tail_budget.saturating_sub(badge_w + 3),
            );
            Line::from(vec![
                Span::styled(lead, theme.accent_bold()),
                Span::styled(prefix, Style::default()),
                Span::raw(" · "),
                badge,
            ])
        } else {
            let summary = clamp_width(
                &format!(" · {} · {}", banner.project, banner.model),
                tail_budget,
            );
            Line::from(vec![
                Span::styled(lead, theme.accent_bold()),
                Span::styled(summary, Style::default()),
            ])
        };
        let hints_raw = if unconfigured {
            "/login to connect · /help · /man"
        } else {
            ": task · !shell · /help · /model · /resume · /man"
        };
        let hints = clamp_width(hints_raw, width);
        vec![line_top, Line::styled(hints, theme.muted_style())]
    }
}

fn clamp_width(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w >= max_width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}
