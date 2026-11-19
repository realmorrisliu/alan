mod action_summary;
mod highlight;
mod literal;
mod markdown;
mod table;
mod yield_validation;
#[cfg(test)]
pub(crate) use action_summary::action_summary;
pub(crate) use action_summary::{
    action_summary_with_theme, group_summary_header_with_theme, group_summary_member_with_theme,
};

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use alan_agent_protocol::{
    ActionReadOnlyContext, DiffLine, PlanItemStatus, StructuredInputQuestion,
    ToolResultPresentation, UiPlanSnapshot, YieldKind,
};

use crate::theme::{DotState, Theme};
use crate::transcript_ui::{
    INLINE_COMMAND_PROMPT_PREFIX, INLINE_PROMPT_CONTINUATION, INLINE_PROMPT_PREFIX,
};

/// Options controlling how transcript cells render.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    pub width: usize,
    pub expand_thinking: bool,
    pub theme: Theme,
}

impl RenderOpts {
    pub fn new(width: usize, expand_thinking: bool) -> Self {
        Self {
            width,
            expand_thinking,
            theme: Theme::default(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }
}

/// Compact metadata rendered as the initial scrollable empty-session welcome header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelcomeBanner {
    pub project: String,
    pub model: String,
    pub sandbox: String,
}

/// Permanent transcript content. Ephemeral activity (running tools, streaming
/// thinking, transient notices, turn state) is projected by the file-backed transcript owner and is
/// rendered in the live region rather than committed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryCell {
    Rendered(Vec<String>),
    /// Already committed prefix removed; retain semantic spans, not prefix guesses.
    Styled(Vec<Line<'static>>),
    /// Streaming Markdown retains its source and parse context after a partial drain.
    AssistantTail {
        text: String,
        /// Stable source-byte end and expanded-character slot of committed content.
        committed: (usize, usize),
    },
    User(String),
    Command(String),
    /// Literal input retains its role and original source after physical drain.
    InputTail {
        text: String,
        command: bool,
        committed: (usize, usize),
    },
    Assistant(String),
    /// Completed thinking, collapsed to a one-line summary by default.
    Thinking {
        text: String,
        duration_secs: u64,
    },
    /// A completed tool call.
    Tool {
        /// Durable identity and optional renderer projection for an AgentFS Action.
        action: Option<ActionHistory>,
        title: String,
        status: ToolStatus,
        preview: Option<String>,
        presentation: Option<ToolResultPresentation>,
    },
    Plan {
        snapshot: UiPlanSnapshot,
        owner: String,
        revision: usize,
    },
    PendingYield(PendingYieldCell),
    /// A fatal (non-recoverable) error.
    Error(String),
    /// Scrollable empty-session welcome header.
    Welcome(WelcomeBanner),
}

/// Presentation state belonging to one retained AgentFS Action, not an execution record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionHistory {
    pub owner: String,
    pub id: String,
    pub read_only: Option<ActionReadOnlyContext>,
    /// Rows freeze when native scrollback consumes any part of this group/member.
    pub frozen_rows: Option<Vec<Line<'static>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Complete,
    Failed,
    Rejected,
    Cancelled,
}

impl ToolStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Complete => "completed",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    fn dot(self, theme: Theme) -> (&'static str, Style) {
        let state = match self {
            Self::Complete => DotState::Run,
            Self::Failed => DotState::Block,
            Self::Rejected | Self::Cancelled => DotState::Wait,
        };
        theme.status_dot(state)
    }

    fn style(self, theme: Theme) -> Style {
        match self {
            Self::Complete => theme.muted_style(),
            Self::Failed => theme.block_style(),
            Self::Rejected | Self::Cancelled => theme.wait_style(),
        }
    }
}

/// A tool call that is still running; shown in the live region only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningTool {
    pub id: String,
    pub title: String,
    pub tail_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingYieldCell {
    pub request_id: String,
    pub kind: YieldKind,
    pub title: String,
    pub prompt: Option<String>,
    pub options: Vec<String>,
    /// The option submitted on a blank confirmation reply (the runtime's
    /// `default_option`), which may not be the first option.
    pub default_option: Option<String>,
    pub questions: Vec<StructuredInputQuestion>,
    /// Policy capability for an escalation (e.g. `network`, `write`).
    pub capability: Option<String>,
    /// Human-readable policy reason for the escalation.
    pub reason: Option<String>,
    /// Structured preview of the operation being approved (diff/command).
    pub presentation: Option<ToolResultPresentation>,
}

impl HistoryCell {
    pub(crate) fn input_source(&self) -> Option<(&str, bool, (usize, usize))> {
        match self {
            Self::User(text) => Some((text, false, (0, 0))),
            Self::Command(text) => Some((text, true, (0, 0))),
            Self::InputTail {
                text,
                command,
                committed,
            } => Some((text, *command, *committed)),
            _ => None,
        }
    }

    pub(crate) fn with_input_cut(mut self, committed: (usize, usize)) -> Self {
        if committed != (0, 0)
            && let Some((text, command, _)) = self.input_source()
        {
            self = Self::InputTail {
                text: text.to_owned(),
                command,
                committed,
            };
        }
        self
    }

    pub(crate) fn assistant_source(&self) -> Option<&str> {
        match self {
            Self::Assistant(text) | Self::AssistantTail { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Replace reconciled source without replaying already committed content.
    pub(crate) fn replace_assistant_source(&mut self, source: String) {
        match self {
            Self::Assistant(text) | Self::AssistantTail { text, .. } => *text = source,
            _ => {}
        }
    }

    pub fn render_lines(&self, opts: RenderOpts) -> Vec<String> {
        if matches!(
            self,
            Self::Assistant(_) | Self::Styled(_) | Self::AssistantTail { .. } | Self::Welcome(_)
        ) {
            return self
                .render_styled_lines(opts)
                .iter()
                .map(ToString::to_string)
                .collect();
        }
        let width = opts.width.max(16);
        if let Some((text, command, committed)) = self.input_source() {
            return literal::project(
                text,
                width,
                if command {
                    INLINE_COMMAND_PROMPT_PREFIX
                } else {
                    INLINE_PROMPT_PREFIX
                },
                committed,
            )
            .into_iter()
            .map(|(line, _)| line.to_string())
            .collect();
        }
        if let Self::Rendered(lines) = self {
            return lines
                .iter()
                .flat_map(|line| textwrap::wrap(line, width))
                .map(Into::into)
                .collect();
        }

        if let Self::Plan {
            snapshot, revision, ..
        } = self
        {
            let completed = snapshot
                .items
                .iter()
                .filter(|item| item.status == PlanItemStatus::Completed)
                .count();
            let current = snapshot
                .items
                .iter()
                .find(|item| item.status == PlanItemStatus::InProgress);
            let step = current.map_or(String::new(), |item| {
                format!(" · {}", action_summary::bounded(&item.content, 256))
            });
            let summary = format!(
                "Plan {revision} · {completed}/{} completed{step}",
                snapshot.items.len()
            );
            return vec![
                action_summary::summary_row(
                    &format!("{}{summary}", opts.theme.gutter()),
                    width,
                    metadata_style(opts.theme),
                )
                .to_string(),
            ];
        }

        if let Self::Thinking {
            text,
            duration_secs,
        } = self
        {
            if !opts.expand_thinking {
                let summary = format!("· thinking · {duration_secs}s (ctrl+r to expand)");
                return textwrap::wrap(&summary, width)
                    .into_iter()
                    .map(Into::into)
                    .collect();
            }
            let gutter = opts.theme.gutter();
            let mut rows = vec![format!("{gutter}thinking · {duration_secs}s")];
            let body_width = width.saturating_sub(gutter.len()).max(8);
            rows.extend(
                wrap_plain_text(text, body_width)
                    .into_iter()
                    .map(|line| format!("{gutter}{line}")),
            );
            return rows;
        }

        let body = match self {
            Self::Rendered(_)
            | Self::Styled(_)
            | Self::AssistantTail { .. }
            | Self::InputTail { .. }
            | Self::Plan { .. }
            | Self::Thinking { .. }
            | Self::Welcome(_) => {
                unreachable!("handled above")
            }
            Self::User(_) | Self::Command(_) => unreachable!("literal inputs handled above"),
            Self::Assistant(text) => return wrap_plain_text(text, width),
            Self::Tool {
                title,
                status,
                preview,
                presentation,
                ..
            } => {
                let mut body = format!("{title} · {}", status.label());
                if let Some(presentation) = presentation {
                    for line in presentation_lines(presentation) {
                        body.push_str(&format!("\n  {line}"));
                    }
                } else if let Some(preview) =
                    preview.as_deref().filter(|preview| !preview.is_empty())
                {
                    body.push_str(&format!("\n  {preview}"));
                }
                body
            }
            Self::PendingYield(pending) => format!("Waiting for input\n{}", pending.render_body()),
            Self::Error(message) => format!("Error: {message}"),
        };

        wrap_plain_text(&body, width)
    }

    /// Project typed transcript roles and Markdown directly into Ratatui spans.
    pub fn render_styled_lines(&self, opts: RenderOpts) -> Vec<Line<'static>> {
        let width = opts.width.max(16);
        let theme = opts.theme;
        match self {
            Self::Welcome(banner) => {
                crate::file_backed::welcome::render_banner_lines(banner, opts.width, theme)
            }
            Self::Assistant(text) => markdown::project(text, width, (0, 0), theme)
                .into_iter()
                .map(|(line, _)| line)
                .collect(),
            Self::AssistantTail { text, committed } => {
                markdown::project(text, width, *committed, theme)
                    .into_iter()
                    .map(|(line, _)| line)
                    .collect()
            }
            Self::Styled(lines) => wrap_styled_lines(lines.clone(), width),
            Self::Tool {
                title,
                status,
                presentation: Some(presentation @ ToolResultPresentation::Diff { .. }),
                ..
            } => {
                let (dot, dot_style) = status.dot(theme);
                let mut lines = vec![Line::from(vec![
                    Span::styled(format!("{dot} "), dot_style),
                    Span::styled(
                        format!("{} · {}", clean_text(title), status.label()),
                        theme.bold_if_capable(status.style(theme)),
                    ),
                ])];
                lines.extend(
                    presentation_rows_with_theme(presentation, false, theme)
                        .into_iter()
                        .map(|(gstyle, text, style)| {
                            Line::from(vec![
                                Span::raw("  "),
                                Span::styled(theme.gutter(), gstyle),
                                Span::styled(clean_text(&text), style),
                            ])
                        }),
                );
                wrap_styled_lines(lines, width)
            }
            Self::Plan {
                snapshot, revision, ..
            } => {
                let completed = snapshot
                    .items
                    .iter()
                    .filter(|item| item.status == PlanItemStatus::Completed)
                    .count();
                let current = snapshot
                    .items
                    .iter()
                    .find(|item| item.status == PlanItemStatus::InProgress);
                let step = current.map_or(String::new(), |item| {
                    format!(" · {}", action_summary::bounded(&item.content, 256))
                });
                let summary = format!(
                    "Plan {revision} · {completed}/{} completed{step}",
                    snapshot.items.len()
                );
                let gutter_style =
                    if !snapshot.items.is_empty() && completed == snapshot.items.len() {
                        theme.run_style()
                    } else {
                        theme.accent_style()
                    };
                wrap_styled_lines(
                    [Line::from(vec![
                        Span::styled(theme.gutter(), gutter_style),
                        Span::styled(clean_text(&summary), metadata_style(theme)),
                    ])],
                    width,
                )
            }
            Self::Thinking {
                text,
                duration_secs,
            } => {
                let body_style = theme.italic_if_capable(metadata_style(theme));
                if !opts.expand_thinking {
                    let summary = format!("· thinking · {duration_secs}s (ctrl+r to expand)");
                    return wrap_plain_text(&summary, width)
                        .into_iter()
                        .map(|row| Line::styled(clean_text(&row), body_style))
                        .collect();
                }
                let gutter = theme.gutter();
                let body_width = width.saturating_sub(gutter.len()).max(8);
                let mut rows = vec![Line::from(vec![
                    Span::styled(gutter, theme.muted_style()),
                    Span::styled(format!("thinking · {duration_secs}s"), body_style),
                ])];
                for part in wrap_plain_text(&clean_text(text), body_width) {
                    rows.push(Line::from(vec![
                        Span::styled(gutter, theme.rule_style()),
                        Span::styled(part, body_style),
                    ]));
                }
                rows
            }
            Self::PendingYield(pending) => render_pending_yield_styled_lines(pending, width, theme),
            _ => {
                let style = match self {
                    Self::User(_) | Self::Command(_) | Self::InputTail { .. } => {
                        theme.accent_bold()
                    }
                    Self::Tool { status, .. } => status.style(theme),
                    Self::Error(_) => theme.bold_if_capable(theme.block_style()),
                    _ => Style::default(),
                };
                self.render_lines(opts)
                    .into_iter()
                    .map(|text| Line::styled(clean_text(&text), style))
                    .collect()
            }
        }
    }

    pub fn trim_rendered_prefix(&mut self, opts: RenderOpts, lines_to_trim: usize) -> bool {
        if lines_to_trim == 0 {
            return true;
        }

        if let Some((text, command, cut)) = self.input_source() {
            let prefix = if command {
                INLINE_COMMAND_PROMPT_PREFIX
            } else {
                INLINE_PROMPT_PREFIX
            };
            let rows = literal::project(text, opts.width.max(16), prefix, cut);
            let committed = rows
                .iter()
                .take(lines_to_trim)
                .next_back()
                .map_or(cut, |(_, key)| *key);
            *self = Self::InputTail {
                text: text.to_owned(),
                command,
                committed,
            };
            return true;
        }
        if let Some(text) = self.assistant_source() {
            let cut = match &*self {
                Self::AssistantTail { committed, .. } => *committed,
                _ => (0, 0),
            };
            let rows = markdown::project(text, opts.width.max(16), cut, opts.theme);
            let committed = rows
                .iter()
                .take(lines_to_trim)
                .next_back()
                .map_or(cut, |(_, key)| *key);
            *self = Self::AssistantTail {
                text: text.to_string(),
                committed,
            };
            return true;
        }
        let remaining = self
            .render_styled_lines(opts)
            .into_iter()
            .skip(lines_to_trim)
            .collect();
        *self = Self::Styled(remaining);
        true
    }
}

fn metadata_style(theme: Theme) -> Style {
    if theme.is_dumb {
        Style::default()
    } else {
        theme.muted_style().add_modifier(Modifier::DIM)
    }
}

// Strip terminal controls and expand tabs before Ratatui filters control graphemes.
// No tab-stop convention exists in this renderer; use four copyable spaces per tab.
pub(crate) fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .flat_map(|c| {
            if c == '\t' {
                "    ".chars().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub(crate) fn wrap_styled_lines(
    lines: impl IntoIterator<Item = Line<'static>>,
    width: usize,
) -> Vec<Line<'static>> {
    let mut output = Vec::new();
    for line in lines {
        let mut row = Vec::new();
        let mut cells = 0;
        for span in &line.spans {
            for grapheme in span.styled_graphemes(line.style) {
                let size = unicode_width::UnicodeWidthStr::width(grapheme.symbol);
                if cells + size > width.max(1) && cells > 0 {
                    output.push(Line::from(std::mem::take(&mut row)));
                    cells = 0;
                }
                row.push(Span::styled(grapheme.symbol.to_string(), grapheme.style));
                cells += size;
            }
        }
        output.push(Line::from(row));
    }
    output
}

impl PendingYieldCell {
    fn default_choice_ref(&self) -> Option<&str> {
        self.default_option
            .as_deref()
            .or_else(|| self.options.first().map(String::as_str))
    }

    fn format_option_hint(&self, index: usize, option: &str) -> String {
        let is_default = self.default_choice_ref() == Some(option);
        if is_default {
            format!("[{}] {option} (default)", index + 1)
        } else {
            format!("[{}] {option}", index + 1)
        }
    }

    fn render_body(&self) -> String {
        let mut body = self.title.clone();
        if let Some(prompt) = &self.prompt {
            body.push_str(&format!(" - {prompt}"));
        }
        if !self.options.is_empty() {
            let formatted = self
                .options
                .iter()
                .enumerate()
                .map(|(idx, opt)| self.format_option_hint(idx, opt))
                .collect::<Vec<_>>()
                .join(" · ");
            body.push_str(&format!("\n  {formatted}"));
        }
        if let Some(capability) = &self.capability {
            body.push_str(&format!("\n  capability: {capability}"));
        }
        if let Some(reason) = &self.reason {
            body.push_str(&format!("\n  why: {reason}"));
        }
        if let Some(presentation) = &self.presentation {
            for line in presentation_lines(presentation) {
                body.push_str(&format!("\n  {line}"));
            }
        }
        for question in &self.questions {
            body.push_str(&format!(
                "\n  {} [{}]: {}",
                question.id,
                yield_validation::structured_kind_label(question.kind),
                question.prompt
            ));
            if !question.options.is_empty() {
                let labels = question
                    .options
                    .iter()
                    .map(|option| format!("{}={}", option.value, option.label))
                    .collect::<Vec<_>>()
                    .join(", ");
                body.push_str(&format!(" ({labels})"));
            }
        }
        body
    }
}

fn render_pending_yield_styled_lines(
    pending: &PendingYieldCell,
    width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let wait_gutter = theme.wait_style();
    let gutter = theme.gutter();
    let mut lines = vec![Line::from(vec![
        Span::styled(gutter, wait_gutter),
        Span::styled(
            "Waiting for input · ",
            theme.bold_if_capable(theme.wait_style()),
        ),
        Span::styled(clean_text(&pending.title), theme.accent_bold()),
    ])];
    if let Some(prompt) = &pending.prompt {
        for segment in prompt.split('\n') {
            lines.push(Line::from(vec![
                Span::styled(gutter, wait_gutter),
                Span::raw(clean_text(segment)),
            ]));
        }
    }
    if let Some(capability) = &pending.capability {
        lines.push(Line::from(vec![
            Span::styled(gutter, wait_gutter),
            Span::styled(
                format!("capability: {}", clean_text(capability)),
                metadata_style(theme),
            ),
        ]));
    }
    if let Some(reason) = &pending.reason {
        lines.push(Line::from(vec![
            Span::styled(gutter, wait_gutter),
            Span::styled(format!("why: {}", clean_text(reason)), theme.wait_style()),
        ]));
    }
    if let Some(presentation) = &pending.presentation {
        for (row_gutter_style, text, style) in
            presentation_rows_with_theme(presentation, false, theme)
        {
            let bar_style = if row_gutter_style == Style::default() && !theme.is_dumb {
                wait_gutter
            } else {
                row_gutter_style
            };
            lines.push(Line::from(vec![
                Span::styled(gutter, bar_style),
                Span::styled(clean_text(&text), style),
            ]));
        }
    }
    if !pending.options.is_empty() {
        let mut spans = vec![Span::styled(gutter, wait_gutter)];
        let default_opt = pending.default_choice_ref();
        for (idx, option) in pending.options.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(" · ", metadata_style(theme)));
            }
            spans.push(Span::styled(format!("[{}] ", idx + 1), theme.accent_bold()));
            let label_style = match option.as_str() {
                "approve" => theme.bold_if_capable(theme.run_style()),
                "allow_session" => theme.accent_bold(),
                "reject" => theme.block_style(),
                _ => Style::default(),
            };
            spans.push(Span::styled(clean_text(option), label_style));
            if default_opt == Some(option.as_str()) {
                spans.push(Span::styled(" (default)", metadata_style(theme)));
            }
        }
        lines.push(Line::from(spans));
    }
    for question in &pending.questions {
        lines.push(Line::from(vec![
            Span::styled(gutter, wait_gutter),
            Span::styled(clean_text(&question.id), theme.accent_bold()),
            Span::styled(
                format!(
                    " [{}]: ",
                    yield_validation::structured_kind_label(question.kind)
                ),
                metadata_style(theme),
            ),
            Span::raw(clean_text(&question.prompt)),
        ]));
        if !question.options.is_empty() {
            let labels = question
                .options
                .iter()
                .map(|option| format!("{}={}", option.value, option.label))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(Line::from(vec![
                Span::styled(gutter, wait_gutter),
                Span::styled(
                    format!("  options: {}", clean_text(&labels)),
                    metadata_style(theme),
                ),
            ]));
        }
    }
    wrap_styled_lines(lines, width)
}

/// Maximum lines shown for a tool presentation before collapsing.
const PRESENTATION_MAX_LINES: usize = 40;

/// Render a presentation primitive into transcript lines (one renderer per form).
fn presentation_lines(presentation: &ToolResultPresentation) -> Vec<String> {
    presentation_rows_with_theme(presentation, false, Theme::default())
        .into_iter()
        .map(|(_, text, _)| text)
        .collect()
}

fn presentation_rows_with_theme(
    presentation: &ToolResultPresentation,
    detail: bool,
    theme: Theme,
) -> Vec<(Style, String, Style)> {
    if let ToolResultPresentation::Diff { path, hunks } = presentation {
        let mut rows = vec![(theme.rule_style(), path.clone(), metadata_style(theme))];
        for hunk in hunks {
            if let Some(header) = &hunk.header {
                rows.push((theme.rule_style(), header.clone(), metadata_style(theme)));
            }
            for line in &hunk.lines {
                let (marker, text, gstyle, style) = match line {
                    DiffLine::Added { text } => {
                        ("+", text, theme.diff_add_style(), theme.diff_add_style())
                    }
                    DiffLine::Removed { text } => (
                        "-",
                        text,
                        theme.diff_remove_style(),
                        theme.diff_remove_style(),
                    ),
                    DiffLine::Context { text } => (" ", text, theme.rule_style(), Style::default()),
                };
                rows.push((gstyle, format!("{marker}{text}"), style));
            }
        }
        if !detail && rows.len() > PRESENTATION_MAX_LINES {
            let hidden = rows.len() - PRESENTATION_MAX_LINES;
            rows.truncate(PRESENTATION_MAX_LINES);
            rows.push((
                theme.rule_style(),
                format!("… +{hidden} more lines"),
                metadata_style(theme),
            ));
        }
        return rows;
    }
    let mut lines = match presentation {
        ToolResultPresentation::Diff { .. } => unreachable!("handled above"),
        ToolResultPresentation::FileContent {
            path,
            lines,
            truncated,
        } => {
            let suffix = if *truncated { ", truncated" } else { "" };
            vec![format!("{path} ({lines} lines{suffix})")]
        }
        ToolResultPresentation::Command {
            cmdline,
            exit_code,
            stdout,
            stderr,
            truncated,
        } => {
            let mut lines = vec![format!("$ {cmdline}")];
            if detail {
                lines.push("stdout".into());
            }
            lines.extend(stdout.lines().map(str::to_string));
            if detail {
                lines.push("stderr".into());
            }
            lines.extend(stderr.lines().map(str::to_string));
            if let Some(code) = exit_code {
                lines.push(format!("exit {code}"));
            }
            if *truncated {
                lines.push("(output truncated)".to_string());
            }
            lines
        }
        ToolResultPresentation::Listing { rows } => rows.clone(),
        ToolResultPresentation::PlainText { body } => body.lines().map(str::to_string).collect(),
    };

    if !detail && lines.len() > PRESENTATION_MAX_LINES {
        let hidden = lines.len() - PRESENTATION_MAX_LINES;
        lines.truncate(PRESENTATION_MAX_LINES);
        lines.push(format!("… +{hidden} more lines"));
    }
    lines
        .into_iter()
        .map(|text| (theme.rule_style(), text, Style::default()))
        .collect()
}

pub(crate) fn action_detail_with_theme(cell: &HistoryCell, theme: Theme) -> Vec<Line<'static>> {
    if let HistoryCell::Tool {
        title,
        presentation: Some(presentation),
        ..
    } = cell
    {
        let mut rows = vec![Line::styled(clean_text(title), theme.accent_bold())];
        let is_diff = matches!(presentation, ToolResultPresentation::Diff { .. });
        rows.extend(
            presentation_rows_with_theme(presentation, true, theme)
                .into_iter()
                .map(|(gstyle, text, style)| {
                    if is_diff {
                        Line::from(vec![
                            Span::styled(theme.gutter(), gstyle),
                            Span::styled(clean_text(&text), style),
                        ])
                    } else {
                        Line::styled(clean_text(&text), style)
                    }
                }),
        );
        rows
    } else {
        cell.render_styled_lines(RenderOpts::new(16384, false).with_theme(theme))
    }
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    text.split('\n')
        .flat_map(|segment| {
            let wrapped = textwrap::wrap(segment, width);
            if wrapped.is_empty() {
                vec![String::new()]
            } else {
                wrapped.into_iter().map(Into::into).collect()
            }
        })
        .collect()
}
