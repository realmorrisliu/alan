//! Interactive `/issue` (`/bug`) intake, redacted diagnostics, draft preview, and tiered submission.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use alan_ap::ErrorCode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::FileBackedApp;
use super::app::{FileBackedAction, FileBackedEvent, Notice};
use crate::history::{HistoryCell, clean_text};

mod diagnostics;

use diagnostics::{
    RedactedDiagnostics, build_prefilled_issue_url, is_valid_repo_slug, sanitize_multiline,
    sanitize_single_line,
};

/// Default target repository used by `/issue` and `/bug` when no override is configured.
pub(in crate::file_backed) const DEFAULT_ISSUE_REPO: &str = "realmorrisliu/alan";

/// Control request sent from `/issue` to the Host adapter when `/mnt/github` is unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueControl {
    /// Check authenticated `gh` CLI availability and create an issue in `repo`.
    Submit {
        /// Target repository in `owner/repo` form.
        repo: String,
        /// Issue title confirmed by the user.
        title: String,
        /// Issue body confirmed by the user.
        body: String,
    },
}

/// Outcome returned by [`IssueControlHandler`] for Tier 2 (`gh` CLI) or Tier 3 fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueSubmitResult {
    /// Issue was created via authenticated `gh issue create`.
    CreatedGhCli {
        /// Created GitHub issue number when parsed from the returned URL.
        number: Option<u64>,
        /// Created GitHub issue HTML URL.
        url: String,
    },
    /// Authenticated `gh` CLI is unavailable; TUI should present the Tier 3 web URL fallback.
    GhUnavailable {
        /// Human-readable reason `gh` could not be used.
        reason: String,
    },
}

/// Future returned by [`IssueControlHandler`].
pub type IssueControlFuture =
    Pin<Box<dyn Future<Output = Result<IssueSubmitResult, anyhow::Error>> + Send + 'static>>;

/// Host-local `/issue` submission handler supplied by the CLI to the TUI.
pub type IssueControlHandler =
    Arc<dyn Fn(IssueControl) -> IssueControlFuture + Send + Sync + 'static>;

/// Final outcome across the three `/issue` submission tiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_backed) enum IssueSubmissionOutcome {
    /// Tier 1: Written to `/mnt/github/repos/<owner>/<repo>/issues/new` and read back.
    MountedGithubFs { number: Option<u64>, url: String },
    /// Tier 2: Created via Host `gh issue create`.
    GhCli { number: Option<u64>, url: String },
    /// Tier 3: Neither `/mnt/github` nor authenticated `gh` was available; prefilled web URL emitted.
    WebFallback { url: String, reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(in crate::file_backed) enum IssueFieldFocus {
    #[default]
    Title,
    Body,
}

#[derive(Clone, Default)]
pub(in crate::file_backed) struct IssueFlowState {
    pub generation: u64,
    phase: IssuePhase,
}

#[derive(Clone)]
struct IssueDraft {
    repo: String,
    title: String,
    title_cursor: usize,
    body: String,
    body_cursor: usize,
    focus: IssueFieldFocus,
    warning: Option<String>,
}

#[derive(Clone, Default)]
enum IssuePhase {
    #[default]
    Idle,
    Editing(IssueDraft),
    Submitting {
        draft: IssueDraft,
        submitted_title: String,
    },
}

impl IssueFlowState {
    pub(in crate::file_backed) fn is_active(&self) -> bool {
        !matches!(self.phase, IssuePhase::Idle)
    }

    pub(in crate::file_backed) fn is_editing(&self) -> bool {
        matches!(self.phase, IssuePhase::Editing { .. })
    }

    pub(in crate::file_backed) fn close(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.phase = IssuePhase::Idle;
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }
}

impl FileBackedApp {
    pub(in crate::file_backed) fn set_issue_repo(&mut self, repo: Option<String>) {
        if let Some(repo) = repo.filter(|r| is_valid_repo_slug(r)) {
            self.issue_repo = repo;
        }
    }

    /// Entry point for `/issue [optional title]` and `/bug [optional title]`.
    pub(in crate::file_backed) fn issue_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        if self.project_selection.is_some() || self.pending_project_control.is_some() {
            self.notice = Some(Notice::warning(
                "Finish or cancel the active project selection before /issue.",
            ));
            return None;
        }

        // Snapshot diagnostics BEFORE clearing/changing notice or modals.
        let diagnostics = RedactedDiagnostics::from_app(self);

        self.modal.active = false;
        self.model_chooser.active = false;
        self.resume_picker.close();
        self.fork_picker.close();
        self.login.close();
        self.settings.close();
        self.pending_login_submission = None;
        self.completion = None;

        let initial_title = sanitize_single_line(argument);
        let (body, body_cursor) = diagnostics.format_issue_body();
        let focus = if initial_title.is_empty() {
            IssueFieldFocus::Title
        } else {
            IssueFieldFocus::Body
        };
        let title_cursor = initial_title.len();
        let repo = if is_valid_repo_slug(&self.issue_repo) {
            self.issue_repo.clone()
        } else {
            DEFAULT_ISSUE_REPO.to_string()
        };

        self.issue.next_generation();
        self.issue.phase = IssuePhase::Editing(IssueDraft {
            repo,
            title: initial_title,
            title_cursor,
            body,
            body_cursor,
            focus,
            warning: None,
        });
        self.notice = None;
        None
    }

    pub(in crate::file_backed) fn issue_paste(&mut self, text: &str) {
        let IssuePhase::Editing(IssueDraft {
            title,
            title_cursor,
            body,
            body_cursor,
            focus,
            warning,
            ..
        }) = &mut self.issue.phase
        else {
            return;
        };
        *warning = None;
        match focus {
            IssueFieldFocus::Title => {
                let clean = sanitize_single_line(text);
                if !clean.is_empty() {
                    insert_at_cursor(title, title_cursor, &clean);
                }
            }
            IssueFieldFocus::Body => {
                let clean = sanitize_multiline(text);
                if !clean.is_empty() {
                    insert_at_cursor(body, body_cursor, &clean);
                }
            }
        }
    }

    pub(in crate::file_backed) fn issue_key(&mut self, key: KeyEvent) -> Option<FileBackedAction> {
        if !self.issue.is_active() {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c')) {
            let was_submitting = matches!(self.issue.phase, IssuePhase::Submitting { .. });
            self.issue.close();
            self.notice = Some(if was_submitting {
                "Issue submission cancelled.".into()
            } else {
                "Issue draft cancelled.".into()
            });
            return was_submitting.then_some(FileBackedAction::CancelIssue);
        }

        let IssuePhase::Editing(IssueDraft {
            repo,
            title,
            title_cursor,
            body,
            body_cursor,
            focus,
            warning,
        }) = &mut self.issue.phase
        else {
            return None;
        };

        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                *focus = match *focus {
                    IssueFieldFocus::Title => IssueFieldFocus::Body,
                    IssueFieldFocus::Body => IssueFieldFocus::Title,
                };
                *warning = None;
            }
            KeyCode::Enter if !shift && !alt && !ctrl => {
                let trimmed_title = title.trim().to_string();
                if trimmed_title.is_empty() {
                    *focus = IssueFieldFocus::Title;
                    *warning = Some("Title is required before submitting (Esc to cancel).".into());
                    return None;
                }
                *warning = None;
                let repo = repo.clone();
                let submitted_body = body.trim_end().to_string();
                let IssuePhase::Editing(draft) = std::mem::take(&mut self.issue.phase) else {
                    unreachable!("checked Editing phase above");
                };
                let generation = self.issue.next_generation();
                self.issue.phase = IssuePhase::Submitting {
                    draft,
                    submitted_title: trimmed_title.clone(),
                };
                return Some(FileBackedAction::SubmitIssue {
                    generation,
                    repo,
                    title: trimmed_title,
                    body: submitted_body,
                });
            }
            KeyCode::Enter if shift || alt => {
                *warning = None;
                if *focus == IssueFieldFocus::Body {
                    insert_at_cursor(body, body_cursor, "\n");
                } else {
                    *focus = IssueFieldFocus::Body;
                }
            }
            KeyCode::Char('j') if ctrl && !alt => {
                *warning = None;
                if *focus == IssueFieldFocus::Body {
                    insert_at_cursor(body, body_cursor, "\n");
                } else {
                    *focus = IssueFieldFocus::Body;
                }
            }
            KeyCode::Up if !ctrl && !alt => {
                *warning = None;
                if *focus == IssueFieldFocus::Body && !move_cursor_vertical(body, body_cursor, -1) {
                    *focus = IssueFieldFocus::Title;
                }
            }
            KeyCode::Down if !ctrl && !alt => {
                *warning = None;
                if *focus == IssueFieldFocus::Title {
                    *focus = IssueFieldFocus::Body;
                } else {
                    move_cursor_vertical(body, body_cursor, 1);
                }
            }
            _ => {
                let (buf, cur, single_line) = match focus {
                    IssueFieldFocus::Title => (title, title_cursor, true),
                    IssueFieldFocus::Body => (body, body_cursor, false),
                };
                if handle_text_edit_key(buf, cur, key, single_line) {
                    *warning = None;
                }
            }
        }
        None
    }

    pub(in crate::file_backed) fn apply_issue_submit_result(
        &mut self,
        generation: u64,
        result: Result<IssueSubmissionOutcome, String>,
    ) {
        if generation != self.issue.generation || !self.issue.is_active() {
            return;
        }
        match result {
            Ok(outcome) => {
                self.issue.close();
                match outcome {
                    IssueSubmissionOutcome::MountedGithubFs { number, url } => {
                        let num = number.map_or_else(String::new, |n| format!("#{n} "));
                        let summary =
                            format!("Issue {num}created via /mnt/github: {}", clean_text(&url));
                        self.transcript.push(HistoryCell::Styled(vec![Line::styled(
                            summary.clone(),
                            self.theme.run_style(),
                        )]));
                        self.notice = Some(summary.into());
                    }
                    IssueSubmissionOutcome::GhCli { number, url } => {
                        let num = number.map_or_else(String::new, |n| format!("#{n} "));
                        let summary =
                            format!("Issue {num}created via gh CLI: {}", clean_text(&url));
                        self.transcript.push(HistoryCell::Styled(vec![Line::styled(
                            summary.clone(),
                            self.theme.run_style(),
                        )]));
                        self.notice = Some(summary.into());
                    }
                    IssueSubmissionOutcome::WebFallback { url, reason } => {
                        let clean_url = clean_text(&url);
                        let clean_reason = clean_text(&reason);
                        self.transcript.push(HistoryCell::Styled(vec![
                            Line::styled(
                                "Issue draft ready (browser URL fallback)",
                                self.theme.accent_bold(),
                            ),
                            Line::styled(
                                format!("  Reason: {clean_reason}"),
                                self.theme.muted_style(),
                            ),
                            Line::from(format!("  Open: {clean_url}")),
                        ]));
                        self.notice = Some(
                            format!("Issue fallback URL ({clean_reason}): {clean_url}").into(),
                        );
                    }
                }
            }
            Err(error) => {
                match std::mem::take(&mut self.issue.phase) {
                    IssuePhase::Submitting { mut draft, .. } => {
                        self.issue.next_generation();
                        draft.warning = Some(format!(
                            "Submission failed: {} (Enter to retry, Esc to cancel)",
                            sanitize_single_line(&clean_text(&error))
                        ));
                        self.issue.phase = IssuePhase::Editing(draft);
                    }
                    _ => self.issue.close(),
                }
                let msg = format!("issue submission failed: {error}");
                self.push_error(msg.clone());
                self.notice = Some(Notice::error(msg));
            }
        }
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "forwards file-backed event loop refs out of file_backed.rs"
)]
pub(in crate::file_backed) fn dispatch_issue_action(
    shell: &alan_shell::Shell,
    handler: Option<IssueControlHandler>,
    tx: &mpsc::Sender<FileBackedEvent>,
    generation: u64,
    repo: String,
    title: String,
    body: String,
    active_issue_task: &mut Option<(u64, tokio::task::AbortHandle)>,
) {
    if let Some((_, prev)) = active_issue_task.take() {
        prev.abort();
    }
    *active_issue_task = Some((
        generation,
        spawn_submit(shell, handler, tx, generation, repo, title, body),
    ));
}

pub(in crate::file_backed) fn handle_issue_completed(
    app: &mut FileBackedApp,
    generation: u64,
    result: Result<IssueSubmissionOutcome, String>,
    active_issue_task: &mut Option<(u64, tokio::task::AbortHandle)>,
) {
    if active_issue_task
        .as_ref()
        .is_some_and(|(task_gen, _)| *task_gen == generation)
    {
        *active_issue_task = None;
    }
    app.apply_issue_submit_result(generation, result);
}

fn spawn_submit(
    shell: &alan_shell::Shell,
    handler: Option<IssueControlHandler>,
    tx: &mpsc::Sender<FileBackedEvent>,
    generation: u64,
    repo: String,
    title: String,
    body: String,
) -> tokio::task::AbortHandle {
    let shell = shell.clone();
    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        let result = submit_issue_tiered(&shell, handler, &repo, &title, &body).await;
        let _ = tx
            .send(FileBackedEvent::IssueSubmitted { generation, result })
            .await;
    });
    handle.abort_handle()
}

pub(in crate::file_backed) async fn submit_issue_tiered(
    shell: &alan_shell::Shell,
    handler: Option<IssueControlHandler>,
    repo: &str,
    title: &str,
    body: &str,
) -> Result<IssueSubmissionOutcome, String> {
    if !is_valid_repo_slug(repo) {
        return Err(format!("invalid repository slug `{repo}`"));
    }
    let issues_new_path = format!("/mnt/github/repos/{repo}/issues/new");
    if let Ok(stat) = shell.stat(&issues_new_path).await
        && stat.writable
    {
        let payload = serde_json::json!({ "title": title, "body": body });
        let payload_bytes = serde_json::to_vec(&payload).map_err(|err| err.to_string())?;
        match shell.write(&issues_new_path, &payload_bytes).await {
            Ok(()) => {
                let receipt = shell.cat(&issues_new_path).await.map_err(|err| {
                    format!("created issue via {issues_new_path}, but receipt read failed: {err:?}")
                })?;
                let parsed: MountedIssueReceipt = serde_json::from_slice(&receipt).map_err(|err| {
                    format!("created issue via {issues_new_path}, but receipt JSON was invalid: {err}")
                })?;
                let url = parsed.url.trim().to_string();
                if url.is_empty() {
                    return Err(format!(
                        "created issue via {issues_new_path}, but receipt URL was empty"
                    ));
                }
                return Ok(IssueSubmissionOutcome::MountedGithubFs {
                    number: parsed.number,
                    url,
                });
            }
            Err(ErrorCode::NotFound | ErrorCode::NoAccess) => {}
            Err(err) => {
                return Err(format!(
                    "failed to write issue to {issues_new_path}: {err:?}"
                ));
            }
        }
    }

    let fallback_reason = if let Some(handler) = handler {
        match handler(IssueControl::Submit {
            repo: repo.to_string(),
            title: title.to_string(),
            body: body.to_string(),
        })
        .await
        {
            Ok(IssueSubmitResult::CreatedGhCli { number, url }) => {
                return Ok(IssueSubmissionOutcome::GhCli { number, url });
            }
            Ok(IssueSubmitResult::GhUnavailable { reason }) => reason,
            Err(err) => return Err(format!("{err:#}")),
        }
    } else {
        "`gh` CLI host adapter unavailable".to_string()
    };

    Ok(IssueSubmissionOutcome::WebFallback {
        url: build_prefilled_issue_url(repo, title, body),
        reason: fallback_reason,
    })
}

#[derive(Deserialize, Serialize)]
struct MountedIssueReceipt {
    #[serde(default)]
    number: Option<u64>,
    url: String,
}

fn insert_at_cursor(buf: &mut String, cursor: &mut usize, text: &str) {
    let pos = (*cursor).min(buf.len());
    buf.insert_str(pos, text);
    *cursor = pos + text.len();
}

fn handle_text_edit_key(
    buf: &mut String,
    cursor: &mut usize,
    key: KeyEvent,
    single_line: bool,
) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let pos = (*cursor).min(buf.len());
    match key.code {
        KeyCode::Left if !ctrl && !alt => {
            if pos > 0 {
                *cursor = buf[..pos].char_indices().last().map_or(0, |(i, _)| i);
            }
            true
        }
        KeyCode::Right if !ctrl && !alt => {
            if pos < buf.len() {
                *cursor = pos + buf[pos..].chars().next().map_or(0, char::len_utf8);
            }
            true
        }
        KeyCode::Home | KeyCode::Char('a') if key.code == KeyCode::Home || (ctrl && !alt) => {
            *cursor = buf[..pos].rfind('\n').map_or(0, |i| i + 1);
            true
        }
        KeyCode::End | KeyCode::Char('e') if key.code == KeyCode::End || (ctrl && !alt) => {
            *cursor = buf[pos..].find('\n').map_or(buf.len(), |off| pos + off);
            true
        }
        KeyCode::Backspace => {
            if pos > 0 {
                let prev = buf[..pos].char_indices().last().map_or(0, |(i, _)| i);
                buf.drain(prev..pos);
                *cursor = prev;
            }
            true
        }
        KeyCode::Delete => {
            if pos < buf.len() {
                let len = buf[pos..].chars().next().map_or(0, char::len_utf8);
                buf.drain(pos..pos + len);
            }
            true
        }
        KeyCode::Char('u') if ctrl && !alt => {
            let line_start = buf[..pos].rfind('\n').map_or(0, |i| i + 1);
            buf.drain(line_start..pos);
            *cursor = line_start;
            true
        }
        KeyCode::Char('k') if ctrl && !alt => {
            let line_end = buf[pos..].find('\n').map_or(buf.len(), |off| pos + off);
            buf.drain(pos..line_end);
            true
        }
        KeyCode::Char(ch) if !ctrl && !alt && !ch.is_control() => {
            if single_line && matches!(ch, '\n' | '\r') {
                return false;
            }
            let mut utf8 = [0u8; 4];
            insert_at_cursor(buf, cursor, ch.encode_utf8(&mut utf8));
            true
        }
        _ => false,
    }
}

fn move_cursor_vertical(buf: &str, cursor: &mut usize, delta: isize) -> bool {
    let pos = (*cursor).min(buf.len());
    let cur_start = buf[..pos].rfind('\n').map_or(0, |i| i + 1);
    let col = buf[cur_start..pos].width();
    if delta < 0 {
        if cur_start == 0 {
            return false;
        }
        let prev_end = cur_start - 1;
        let prev_start = buf[..prev_end].rfind('\n').map_or(0, |i| i + 1);
        *cursor = prev_start + byte_for_col(&buf[prev_start..prev_end], col);
        true
    } else {
        let Some(off) = buf[pos..].find('\n') else {
            return false;
        };
        let next_start = pos + off + 1;
        let next_end = buf[next_start..]
            .find('\n')
            .map_or(buf.len(), |o| next_start + o);
        *cursor = next_start + byte_for_col(&buf[next_start..next_end], col);
        true
    }
}

fn byte_for_col(line: &str, target_col: usize) -> usize {
    let mut used = 0usize;
    for (idx, ch) in line.char_indices() {
        let w = ch.width().unwrap_or(0);
        if used + w > target_col {
            return idx;
        }
        used += w;
    }
    line.len()
}

pub(in crate::file_backed) fn draw(frame: &mut Frame<'_>, app: &FileBackedApp) {
    let area = frame.area();
    let width = area.width as usize;
    let height = area.height as usize;
    let (lines, cursor, body_start_row) = build_view(app, width);
    let skip = if height == 0 || lines.len() <= height {
        0
    } else if let Some((_, cy)) = cursor {
        let cy = cy as usize;
        if cy >= height {
            (cy + 1)
                .saturating_sub(height)
                .min(body_start_row.max(cy + 1 - height))
        } else {
            0
        }
    } else {
        0
    };
    let visible: Vec<Line<'static>> = lines.into_iter().skip(skip).take(height).collect();
    frame.render_widget(Paragraph::new(visible), area);
    if let Some((cx, cy)) = cursor
        && height > 0
        && (cy as usize) >= skip
        && (cy as usize) < skip + height
    {
        frame.set_cursor_position((
            area.x + cx.min(area.width.saturating_sub(1)),
            area.y + (cy - skip as u16).min(area.height - 1),
        ));
    }
}

#[cfg(test)]
pub(in crate::file_backed) fn render_lines(
    app: &FileBackedApp,
    width: usize,
) -> Vec<Line<'static>> {
    build_view(app, width).0
}

fn build_view(
    app: &FileBackedApp,
    width: usize,
) -> (Vec<Line<'static>>, Option<(u16, u16)>, usize) {
    let theme = app.theme;
    let width = width.max(16);
    match &app.issue.phase {
        IssuePhase::Idle => (Vec::new(), None, 0),
        IssuePhase::Submitting {
            draft,
            submitted_title,
        } => {
            let lines = vec![
                Line::styled(
                    format!("Submitting Issue · {}", clean_text(&draft.repo)),
                    theme.accent_bold(),
                ),
                Line::styled("Please wait… (Esc to close)", theme.muted_style()),
                Line::default(),
                Line::from(format!("Title: {}", clean_text(submitted_title))),
            ];
            (lines, None, 0)
        }
        IssuePhase::Editing(IssueDraft {
            repo,
            title,
            title_cursor,
            body,
            body_cursor,
            focus,
            warning,
        }) => {
            let hint = if width >= 68 {
                "Tab switch field · Enter submit · Shift+Enter/Ctrl+J newline · Esc cancel"
            } else {
                "Tab field · Enter submit · Esc cancel"
            };
            let mut rows = vec![
                Line::styled(
                    format!("New Issue · {}", clean_text(repo)),
                    theme.accent_bold(),
                ),
                Line::styled(hint, theme.muted_style()),
                Line::styled(
                    "Redacted diagnostics only (no transcript, project code, or secrets)",
                    theme.muted_style(),
                ),
                Line::styled(
                    if theme.is_dumb { "-" } else { "─" }.repeat(width),
                    theme.rule_style(),
                ),
            ];
            if let Some(warn) = warning {
                rows.push(Line::styled(format!("! {warn}"), theme.wait_style()));
            }
            let title_focused = *focus == IssueFieldFocus::Title;
            let title_prefix = if title_focused {
                "▶ Title: "
            } else {
                "  Title: "
            };
            let title_style = if title_focused {
                theme.accent_bold()
            } else {
                theme.muted_style()
            };
            let mut cursor_pos = None;
            let wrapped_title =
                wrap_logical_line(title, width.saturating_sub(title_prefix.width()).max(1));
            for (idx, seg) in wrapped_title.iter().enumerate() {
                let prefix = if idx == 0 { title_prefix } else { "         " };
                let row_y = rows.len();
                if title_focused
                    && cursor_pos.is_none()
                    && *title_cursor >= seg.start
                    && *title_cursor <= seg.end
                {
                    let col = seg.text[..(*title_cursor - seg.start).min(seg.text.len())].width();
                    cursor_pos = Some(((prefix.width() + col) as u16, row_y as u16));
                }
                let content_span = if title.is_empty() && !title_focused && idx == 0 {
                    Span::styled("(enter issue title)", theme.muted_style())
                } else {
                    Span::raw(seg.text.clone())
                };
                rows.push(Line::from(vec![
                    Span::styled(prefix, title_style),
                    content_span,
                ]));
            }
            let body_focused = *focus == IssueFieldFocus::Body;
            let body_header = if body_focused { "▶ Body:" } else { "  Body:" };
            let body_style = if body_focused {
                theme.accent_bold()
            } else {
                theme.muted_style()
            };
            rows.push(Line::styled(body_header, body_style));
            let body_start_row = rows.len();
            let gutter = if theme.is_dumb { "  | " } else { "  │ " };
            let gutter_w = gutter.width();
            let body_avail = width.saturating_sub(gutter_w).max(1);
            let mut offset = 0usize;
            let mut lines_iter = body.split('\n').peekable();
            while let Some(logical) = lines_iter.next() {
                let line_start = offset;
                let line_end = line_start + logical.len();
                for seg in wrap_logical_line(logical, body_avail) {
                    let row_y = rows.len();
                    let abs_start = line_start + seg.start;
                    let abs_end = line_start + seg.end;
                    if body_focused
                        && cursor_pos.is_none()
                        && *body_cursor >= abs_start
                        && *body_cursor <= abs_end
                    {
                        let col =
                            seg.text[..(*body_cursor - abs_start).min(seg.text.len())].width();
                        cursor_pos = Some(((gutter_w + col) as u16, row_y as u16));
                    }
                    rows.push(Line::from(vec![
                        Span::styled(gutter, theme.rule_style()),
                        Span::raw(seg.text),
                    ]));
                }
                offset = line_end + usize::from(lines_iter.peek().is_some());
            }
            (rows, cursor_pos, body_start_row)
        }
    }
}

struct WrappedSeg {
    text: String,
    start: usize,
    end: usize,
}

fn wrap_logical_line(line: &str, max_width: usize) -> Vec<WrappedSeg> {
    if line.is_empty() {
        return vec![WrappedSeg {
            text: String::new(),
            start: 0,
            end: 0,
        }];
    }
    let max_width = max_width.max(1);
    let mut segs = Vec::new();
    let mut seg_start = 0usize;
    let mut used = 0usize;
    for (idx, ch) in line.char_indices() {
        let w = ch.width().unwrap_or(0);
        if used + w > max_width && idx > seg_start {
            segs.push(WrappedSeg {
                text: line[seg_start..idx].to_string(),
                start: seg_start,
                end: idx,
            });
            seg_start = idx;
            used = w;
        } else {
            used += w;
        }
    }
    segs.push(WrappedSeg {
        text: line[seg_start..].to_string(),
        start: seg_start,
        end: line.len(),
    });
    segs
}

#[cfg(test)]
#[path = "issue_flow_tests.rs"]
mod tests;
