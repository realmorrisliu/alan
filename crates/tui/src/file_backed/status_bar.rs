//! Calm, high-density 2-row left/right-aligned status bar rendered below the composer.
//!
//! Zero Nerd Font glyphs. Pure ASCII fallback when `theme.is_dumb` is set.
//! Strictly omits host sandbox backend names (`seatbelt`, `landlock`) from daily status rows.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::theme::{DotState, Theme};

use super::app::presentation::is_unsandboxed_posture;
use super::{FileBackedApp, ProjectAccess, UiActivityState};

fn truncate_for_theme(value: &str, max_width: usize, is_dumb: bool) -> String {
    if value.width() <= max_width {
        return value.to_string();
    }
    if !is_dumb {
        return super::app::presentation::truncate_middle(value, max_width);
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }
    let left_limit = (max_width - 3) / 2;
    let right_limit = max_width - 3 - left_limit;
    let mut left = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let ch = if ch.is_ascii() { ch } else { '?' };
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > left_limit {
            break;
        }
        left.push(ch);
        used += w;
    }
    let mut right = String::new();
    used = 0;
    for ch in value.chars().rev() {
        let ch = if ch.is_ascii() { ch } else { '?' };
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > right_limit {
            break;
        }
        right.push(ch);
        used += w;
    }
    let right: String = right.chars().rev().collect();
    format!("{left}...{right}")
}

fn clamp_for_theme(value: &str, max_width: usize, is_dumb: bool) -> String {
    if !is_dumb {
        return crate::history::clamp_text_width(value, max_width);
    }
    let ascii: String = value
        .chars()
        .map(|ch| if ch.is_ascii() { ch } else { ' ' })
        .collect();
    if ascii.width() <= max_width {
        return ascii;
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width <= 3 {
        return ascii.chars().take(max_width).collect();
    }
    let mut out = String::new();
    for ch in ascii.chars() {
        if out.len() + 3 >= max_width {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[derive(Clone)]
struct Segment {
    text: String,
    style: Style,
}

impl Segment {
    fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    fn width(&self) -> usize {
        self.text.width()
    }
}

/// Render both rows of the status bar below the composer for `width` columns.
pub(in crate::file_backed) fn status_bar_lines(
    app: &FileBackedApp,
    width: usize,
) -> [Line<'static>; 2] {
    [row1_line(app, width), row2_line(app, width)]
}

/// Row 1 (always visible):
/// Left:  `● <project_or_focus>  <git_branch[*]>  <rw|ro>·<gov|trust>` (or bold red `! unsandboxed`)
/// Right: `<profile/>model · <effort>`
pub(in crate::file_backed) fn row1_line(app: &FileBackedApp, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }
    let theme = app.theme;
    let sep = if theme.is_dumb { " | " } else { " · " };
    let mid_dot = if theme.is_dumb { "/" } else { "·" };
    let unconfigured = app.is_connection_unconfigured();

    let dot_state = if app.expected_terminal_error.is_some() || app.last_input_failed {
        DotState::Block
    } else if app.pending_yield.is_some() || app.form.is_some() || unconfigured {
        DotState::Wait
    } else if app.turn_active() || app.project.is_some() {
        DotState::Run
    } else {
        DotState::Muted
    };
    let (dot, dot_style) = status_bar_dot(theme, dot_state);
    let dot_prefix = format!("{dot} ");
    let dot_width = dot_prefix.width();

    let base_location = app.project.as_ref().map_or_else(
        || {
            if app.namespace_cwd == std::path::Path::new("/") {
                "Select project".to_string()
            } else {
                app.namespace_cwd.display().to_string()
            }
        },
        |project| super::model::safe(&project.label),
    );
    let full_location = if let Some(stream_name) = app.streams.focused_stream.as_deref() {
        let safe_stream = super::model::safe(stream_name);
        if app.project.is_some() {
            format!("{base_location}{sep}focus:{safe_stream}")
        } else {
            format!("focus:{safe_stream}")
        }
    } else {
        base_location
    };
    let short_location = app
        .streams
        .focused_stream
        .as_deref()
        .map(|s| format!("focus:{}", super::model::safe(s)))
        .unwrap_or_else(|| full_location.clone());

    let git_seg = app.git_status.as_ref().map(|git| {
        let branch = super::model::safe(&git.branch);
        let label = if git.dirty {
            format!("{branch}*")
        } else {
            branch
        };
        let style = if git.dirty {
            theme.wait_style()
        } else {
            theme.run_style()
        };
        Segment::new(label, style)
    });

    let (posture_segs, urgent_unsandboxed) = row1_posture_segments(app, theme, mid_dot);

    let (right_full, right_no_profile, right_model_only, right_style) =
        row1_model_variants(app, theme, sep, unconfigured);

    let mut include_git = git_seg.is_some();
    let mut include_posture = !posture_segs.is_empty();
    let mut location = full_location;
    let mut right_text = right_full;

    let left_width_for = |loc: &str, git: bool, posture: bool| -> usize {
        let mut w = dot_width + loc.width();
        if git && let Some(ref g) = git_seg {
            w += 2 + g.width();
        }
        if posture && !posture_segs.is_empty() {
            w += 2 + segments_width(&posture_segs);
        }
        w
    };

    // Priority degradation for narrow terminals (<80 cols):
    // 1. Shorten <project · focus:x> -> focus:x if focused stream
    if left_width_for(&location, include_git, include_posture) + 2 + right_text.width() > width {
        location = short_location;
    }
    // 2. Drop <profile/> from right side -> model · effort
    if left_width_for(&location, include_git, include_posture) + 2 + right_text.width() > width {
        right_text = right_no_profile;
    }
    // 3. Drop git branch
    if left_width_for(&location, include_git, include_posture) + 2 + right_text.width() > width {
        include_git = false;
    }
    // 4. Drop non-urgent rw·gov/trust badge (NEVER drop bold red ! unsandboxed here)
    if !urgent_unsandboxed
        && left_width_for(&location, include_git, include_posture) + 2 + right_text.width() > width
    {
        include_posture = false;
    }
    // 5. Drop effort from right side -> model only
    if left_width_for(&location, include_git, include_posture) + 2 + right_text.width() > width {
        right_text = right_model_only;
    }

    // 6. Fit remaining left/right cleanly within width (preserving urgent ! unsandboxed).
    let posture_w = if include_posture && !posture_segs.is_empty() {
        2 + segments_width(&posture_segs)
    } else {
        0
    };
    let git_w = if include_git && let Some(ref g) = git_seg {
        2 + g.width()
    } else {
        0
    };
    let min_gap = usize::from(!right_text.is_empty());
    let right_w = right_text.width();
    let fixed_left = dot_width + git_w + posture_w;

    if fixed_left + location.width() + min_gap + right_w > width {
        // First shrink location while keeping right_text and urgent posture intact.
        let avail_for_loc = width.saturating_sub(fixed_left + min_gap + right_w);
        if avail_for_loc >= 4 {
            location = truncate_for_theme(&location, avail_for_loc, theme.is_dumb);
        } else if !urgent_unsandboxed && dot_width + right_w <= width {
            // Keep the full model identifier legible on very narrow panes (e.g. 16 cols)
            // rather than truncating the model to keep a generic location label.
            let remaining = width.saturating_sub(dot_width + right_w + min_gap);
            location = if remaining >= 4 {
                truncate_for_theme(&location, remaining, theme.is_dumb)
            } else {
                String::new()
            };
        } else if urgent_unsandboxed && fixed_left + min_gap <= width {
            // Shrink both location and right_text so `! unsandboxed` stays visible.
            let remaining = width.saturating_sub(dot_width + posture_w + min_gap);
            let loc_budget = (remaining / 3).min(location.width());
            location = if loc_budget >= 3 {
                truncate_for_theme(&location, loc_budget, theme.is_dumb)
            } else {
                String::new()
            };
            let right_budget = width.saturating_sub(
                dot_width
                    + location.width()
                    + if location.is_empty() {
                        segments_width(&posture_segs)
                    } else {
                        posture_w
                    }
                    + min_gap,
            );
            right_text = truncate_for_theme(&right_text, right_budget, theme.is_dumb);
        } else {
            let right_budget = width.saturating_sub(dot_width + min_gap + 4);
            if right_budget >= 6 {
                right_text = truncate_for_theme(&right_text, right_budget, theme.is_dumb);
                let loc_budget = width
                    .saturating_sub(dot_width + git_w + posture_w + min_gap + right_text.width());
                location = truncate_for_theme(&location, loc_budget, theme.is_dumb);
            } else {
                include_posture =
                    urgent_unsandboxed && dot_width + segments_width(&posture_segs) <= width;
                location = String::new();
                let r_budget = width.saturating_sub(
                    dot_width
                        + if include_posture {
                            segments_width(&posture_segs) + 1
                        } else {
                            0
                        },
                );
                right_text = truncate_for_theme(&right_text, r_budget, theme.is_dumb);
            }
        }
    }

    let mut left_spans = Vec::new();
    if width >= dot_width {
        left_spans.push(Span::styled(dot_prefix, dot_style));
    }
    if !location.is_empty() {
        left_spans.push(Span::styled(location.clone(), theme.accent_style()));
    }
    if include_git && let Some(g) = git_seg {
        if !location.is_empty() {
            left_spans.push(Span::raw("  "));
        }
        left_spans.push(Span::styled(g.text, g.style));
    }
    if include_posture && !posture_segs.is_empty() {
        if !location.is_empty() || include_git {
            left_spans.push(Span::raw("  "));
        }
        for seg in posture_segs {
            left_spans.push(Span::styled(seg.text, seg.style));
        }
    }

    let right_spans = if right_text.is_empty() {
        Vec::new()
    } else {
        vec![Span::styled(right_text, right_style)]
    };

    align_left_right(left_spans, right_spans, width)
}

/// Row 2 (hybrid tiering):
/// Left:  `ctx X% (used/max) · Y% hit · $cost` + conditional compact budget (`18m/30m 84/256t 43/64m`)
/// Right: conditional non-zero streams `Xa·Yi·Zin`, `queued N`, `cow:N`, non-default `no-resume`/`ephemeral`, subtle `^T status`
pub(in crate::file_backed) fn row2_line(app: &FileBackedApp, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }
    let theme = app.theme;
    let sep = if theme.is_dumb { " | " } else { " · " };
    let mid_dot = if theme.is_dumb { "/" } else { "·" };
    let running = matches!(app.activity.state, UiActivityState::Running);

    // --- Left segments ---
    let ctx_pct = app.usage.ctx_pct();
    let ctx_style = ratio_semantic_style(ctx_pct.map(|p| p as f64), theme);
    let ctx_full = app
        .usage
        .ctx_segment(true)
        .map(|t| Segment::new(t, ctx_style));
    let ctx_short = app
        .usage
        .ctx_segment(false)
        .map(|t| Segment::new(t, ctx_style));
    let hit_seg = app
        .usage
        .cache_hit_segment()
        .map(|t| Segment::new(t, theme.muted_style()));
    let cost_seg = app
        .usage
        .cost_segment()
        .map(|t| Segment::new(t, theme.muted_style()));

    let budget_ratio = app.budget.max_ratio_pct();
    let budget_style = ratio_semantic_style(budget_ratio, theme);
    let budget_seg = app
        .budget
        .compact_label(running)
        .map(|t| Segment::new(t, budget_style));

    // --- Right segments ---
    let streams_seg = app.streams.effective_counts().and_then(|c| {
        if c.running == 0 && c.idle == 0 && c.unread_inbox == 0 && c.paused_error == 0 {
            return None;
        }
        let label = if c.paused_error > 0 {
            format!(
                "{}a{mid_dot}{}i{mid_dot}{}p{mid_dot}{}in",
                c.running, c.idle, c.paused_error, c.unread_inbox
            )
        } else {
            format!(
                "{}a{mid_dot}{}i{mid_dot}{}in",
                c.running, c.idle, c.unread_inbox
            )
        };
        let style = if c.unread_inbox > 0 || c.paused_error > 0 {
            theme.wait_style()
        } else {
            theme.muted_style()
        };
        Some(Segment::new(label, style))
    });

    let queued_seg = app
        .queue
        .snapshot
        .as_ref()
        .filter(|q| q.known && !q.pending_submission_ids.is_empty())
        .map(|q| {
            Segment::new(
                format!("queued {}", q.pending_submission_ids.len()),
                theme.wait_style(),
            )
        });

    let cow_seg = (app.cow_branch_count > 0).then(|| {
        Segment::new(
            format!("cow:{}", app.cow_branch_count),
            theme.accent_style(),
        )
    });

    let ephemeral_seg = app
        .ephemeral
        .then(|| Segment::new("ephemeral", theme.wait_style()));
    let no_resume_seg = app
        .no_resume_update
        .then(|| Segment::new("no-resume", theme.muted_style()));

    let has_details = !app.projected_actions.is_empty() || !app.plan_owners.is_empty();
    let hint_levels: Vec<Option<Segment>> = if running {
        vec![
            Some(Segment::new(
                format!("Enter queue{sep}^S steer{sep}Esc stop{sep}^T status"),
                theme.faint_style(),
            )),
            Some(Segment::new(
                format!("^S steer{sep}Esc stop{sep}^T status"),
                theme.faint_style(),
            )),
            Some(Segment::new(
                format!("Esc stop{sep}^T status"),
                theme.faint_style(),
            )),
            Some(Segment::new("^T status", theme.faint_style())),
            None,
        ]
    } else if has_details {
        vec![
            Some(Segment::new(
                format!("^O details{sep}^T status"),
                theme.faint_style(),
            )),
            Some(Segment::new("^T status", theme.faint_style())),
            None,
        ]
    } else {
        vec![Some(Segment::new("^T status", theme.faint_style())), None]
    };

    // Assemble & degrade gracefully for narrow terminals.
    let mut use_ctx_full = true;
    let mut include_hit = hit_seg.is_some();
    let mut include_cost = cost_seg.is_some();
    let mut include_budget = budget_seg.is_some();
    let mut include_no_resume = no_resume_seg.is_some();
    let mut include_cow = cow_seg.is_some();
    let mut hint_idx = 0usize;

    let build_left = |full_ctx: bool, hit: bool, cost: bool, budget: bool| -> Vec<Segment> {
        let mut out = Vec::new();
        if let Some(c) = if full_ctx { &ctx_full } else { &ctx_short } {
            out.push(c.clone());
        }
        if hit && let Some(ref h) = hit_seg {
            out.push(h.clone());
        }
        if cost && let Some(ref c) = cost_seg {
            out.push(c.clone());
        }
        if budget && let Some(ref b) = budget_seg {
            out.push(b.clone());
        }
        out
    };

    let build_right = |cow: bool, no_resume: bool, h_idx: usize| -> Vec<Segment> {
        let mut out = Vec::new();
        if let Some(ref s) = streams_seg {
            out.push(s.clone());
        }
        if let Some(ref q) = queued_seg {
            out.push(q.clone());
        }
        if cow && let Some(ref c) = cow_seg {
            out.push(c.clone());
        }
        if let Some(ref e) = ephemeral_seg {
            out.push(e.clone());
        }
        if no_resume && let Some(ref nr) = no_resume_seg {
            out.push(nr.clone());
        }
        if let Some(Some(h)) = hint_levels.get(h_idx) {
            out.push(h.clone());
        }
        out
    };

    let fits = |left: &[Segment], right: &[Segment]| -> bool {
        let lw = joined_width(left, sep);
        let rw = joined_width(right, sep);
        let gap = if lw > 0 && rw > 0 { 2 } else { 0 };
        lw + gap + rw <= width
    };

    loop {
        let left = build_left(use_ctx_full, include_hit, include_cost, include_budget);
        let right = build_right(include_cow, include_no_resume, hint_idx);
        if fits(&left, &right) {
            break;
        }
        // Step 1: shorten hints down to `^T status` before dropping telemetry.
        if hint_idx + 2 < hint_levels.len() {
            hint_idx += 1;
            continue;
        }
        if include_hit {
            include_hit = false;
            continue;
        }
        if include_cost {
            include_cost = false;
            continue;
        }
        if use_ctx_full && ctx_short.is_some() {
            use_ctx_full = false;
            continue;
        }
        if include_no_resume {
            include_no_resume = false;
            continue;
        }
        if include_cow {
            include_cow = false;
            continue;
        }
        if hint_idx + 1 < hint_levels.len() {
            hint_idx += 1;
            continue;
        }
        if include_budget {
            include_budget = false;
            continue;
        }
        break;
    }

    let left_segs = build_left(use_ctx_full, include_hit, include_cost, include_budget);
    let right_segs = build_right(include_cow, include_no_resume, hint_idx);

    let mut left_spans = join_segments(left_segs, sep, theme.muted_style());
    let mut right_spans = join_segments(right_segs, sep, theme.muted_style());

    let lw = spans_width(&left_spans);
    let rw = spans_width(&right_spans);
    let gap = usize::from(lw > 0 && rw > 0);
    if lw + gap + rw > width {
        if lw >= width {
            left_spans = clamp_spans(left_spans, width, theme.is_dumb);
            right_spans.clear();
        } else {
            let rem = width.saturating_sub(lw + gap);
            right_spans = clamp_spans(right_spans, rem, theme.is_dumb);
        }
    }

    align_left_right(left_spans, right_spans, width)
}

fn row1_posture_segments(app: &FileBackedApp, theme: Theme, mid_dot: &str) -> (Vec<Segment>, bool) {
    let access_short = app.project.as_ref().map(|p| match p.access {
        ProjectAccess::ReadWrite => "rw",
        ProjectAccess::ReadOnly => "ro",
    });

    if let Some(raw) = app.trust_posture_label.as_deref() {
        let safe = super::model::safe(raw);
        if !safe.is_empty() && is_unsandboxed_posture(&safe) {
            let mut segs = Vec::new();
            if let Some(acc) = access_short {
                segs.push(Segment::new(format!("{acc} "), theme.muted_style()));
            }
            segs.push(Segment::new(
                "! unsandboxed",
                theme.bold_if_capable(theme.block_style()),
            ));
            return (segs, true);
        }
        if !safe.is_empty() {
            // Confined trusted posture (`auto-approve` / `trusted`).
            let trust_style = theme.run_style();
            let segs = match access_short {
                Some(acc) => vec![
                    Segment::new(format!("{acc}{mid_dot}"), theme.muted_style()),
                    Segment::new("trust", trust_style),
                ],
                None => vec![Segment::new("trust", trust_style)],
            };
            return (segs, false);
        }
    }

    let segs = match access_short {
        Some(acc) => vec![Segment::new(
            format!("{acc}{mid_dot}gov"),
            theme.muted_style(),
        )],
        None => vec![Segment::new("gov", theme.muted_style())],
    };
    (segs, false)
}

fn row1_model_variants(
    app: &FileBackedApp,
    theme: Theme,
    sep: &str,
    unconfigured: bool,
) -> (String, String, String, Style) {
    if unconfigured {
        return (
            format!("Not connected{sep}/login"),
            "Not connected".to_string(),
            "Not connected".to_string(),
            theme.wait_style(),
        );
    }
    let prefer_active = app.prefers_active_model();
    let Some(binding) = app.model.prompt_binding(prefer_active) else {
        let fallback = "model unknown".to_string();
        return (
            fallback.clone(),
            fallback.clone(),
            fallback,
            theme.muted_style(),
        );
    };
    let profile = super::model::safe(&binding.profile);
    let model = super::model::safe(&binding.model);
    let model_with_profile = if !profile.is_empty() && profile != model {
        format!("{profile}/{model}")
    } else {
        model.clone()
    };
    let (full, no_profile) = match binding.reasoning.effort {
        Some(effort) => (
            format!("{model_with_profile}{sep}{effort}"),
            format!("{model}{sep}{effort}"),
        ),
        None => (model_with_profile, model.clone()),
    };
    (full, no_profile, model, Style::default())
}

fn status_bar_dot(theme: Theme, state: DotState) -> (&'static str, Style) {
    if theme.is_dumb {
        let glyph = match state {
            DotState::Run => "*",
            DotState::Wait => "?",
            DotState::Block => "!",
            DotState::Muted => ".",
        };
        (glyph, Style::default())
    } else {
        theme.status_dot(state)
    }
}

pub(in crate::file_backed) fn ratio_semantic_style(pct: Option<f64>, theme: Theme) -> Style {
    match pct {
        Some(p) if p >= 90.0 => theme.block_style(),
        Some(p) if p >= 75.0 => theme.wait_style(),
        _ => theme.muted_style(),
    }
}

fn segments_width(segs: &[Segment]) -> usize {
    segs.iter().map(Segment::width).sum()
}

fn joined_width(segs: &[Segment], sep: &str) -> usize {
    if segs.is_empty() {
        return 0;
    }
    let sep_w = sep.width();
    segs.iter().map(Segment::width).sum::<usize>() + sep_w * segs.len().saturating_sub(1)
}

fn join_segments(segs: Vec<Segment>, sep: &str, sep_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(segs.len() * 2);
    for (idx, seg) in segs.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(sep.to_string(), sep_style));
        }
        spans.push(Span::styled(seg.text, seg.style));
    }
    spans
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.width()).sum()
}

fn clamp_spans(spans: Vec<Span<'static>>, max_width: usize, is_dumb: bool) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut used = 0usize;
    for span in spans {
        if used >= max_width {
            break;
        }
        let rem = max_width - used;
        let w = span.content.width();
        if w <= rem {
            used += w;
            out.push(span);
        } else {
            let clamped = clamp_for_theme(&span.content, rem, is_dumb);
            if !clamped.is_empty() {
                out.push(Span::styled(clamped, span.style));
            }
            break;
        }
    }
    out
}

fn align_left_right(
    mut left: Vec<Span<'static>>,
    right: Vec<Span<'static>>,
    width: usize,
) -> Line<'static> {
    if right.is_empty() {
        return Line::from(left);
    }
    let lw = spans_width(&left);
    let rw = spans_width(&right);
    let pad = width.saturating_sub(lw + rw);
    if pad > 0 {
        left.push(Span::raw(" ".repeat(pad)));
    } else if lw > 0 && lw + 1 + rw <= width {
        left.push(Span::raw(" "));
    }
    left.extend(right);
    Line::from(left)
}

#[cfg(test)]
#[path = "status_bar_tests.rs"]
mod tests;
