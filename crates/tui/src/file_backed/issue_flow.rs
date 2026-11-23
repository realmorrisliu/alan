//! Interactive `/issue` (`/bug`) intake, redacted diagnostics, draft preview, and tiered submission.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use alan_ap::ErrorCode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::FileBackedApp;
use super::app::{FileBackedAction, FileBackedEvent, Notice};
use crate::history::{HistoryCell, clean_text};

mod diagnostics;
mod render;

pub(in crate::file_backed) use render::draw;
#[cfg(test)]
use render::render_lines;

use diagnostics::{
    RedactedDiagnostics, build_prefilled_issue_url, is_valid_repo_slug,
    redact_diagnostic_multiline, redact_diagnostic_text, sanitize_multiline, sanitize_single_line,
};

/// Default target repository used by `/issue` and `/bug` when no override is configured.
pub(in crate::file_backed) const DEFAULT_ISSUE_REPO: &str = "realmorrisliu/alan";
const AUTO_ISSUE_LABEL: &str = "alan:auto";
const MAX_AI_TITLE_CHARS: usize = 120;
const MAX_AI_SUMMARY_CHARS: usize = 900;

/// Control request sent from `/issue` to the Host adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueControl {
    /// Ask the host adapter for a stateless one-shot redacted issue draft.
    Draft {
        /// Target repository in `owner/repo` form.
        repo: String,
        /// Optional one-line hint passed by the user after `/issue` or `/bug`.
        user_hint: Option<String>,
        /// Strictly redacted diagnostics context collected in the TUI.
        diagnostics_context: String,
        /// Active or configured profile hint when known.
        profile: Option<String>,
    },
    /// Check authenticated `gh` CLI availability and create an issue in `repo`.
    Submit {
        /// Target repository in `owner/repo` form.
        repo: String,
        /// Issue title confirmed by the user.
        title: String,
        /// Issue body confirmed by the user.
        body: String,
        /// Optional GitHub labels requested by the user (for example `["alan:auto"]`).
        labels: Vec<String>,
    },
}

/// Outcome returned by [`IssueControlHandler`] for `/issue` drafting and submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueSubmitResult {
    /// One-shot AI drafting produced a redacted title + summary proposal.
    Drafted {
        /// Concise proposed issue title.
        title: String,
        /// Concise markdown issue summary.
        summary: String,
    },
    /// One-shot AI drafting was unavailable or failed; TUI should keep deterministic fallback.
    DraftUnavailable {
        /// Human-readable reason AI drafting could not be completed.
        reason: String,
    },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(in crate::file_backed) enum IssueDraftSource {
    #[default]
    Manual,
    AiDrafted,
    DeterministicFallback,
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
    auto_label: bool,
    draft_source: IssueDraftSource,
    diagnostics: RedactedDiagnostics,
    warning: Option<String>,
}

#[derive(Clone, Default)]
enum IssuePhase {
    #[default]
    Idle,
    Drafting {
        repo: String,
        fallback: IssueDraft,
        started_at_ms: u64,
    },
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

    pub(in crate::file_backed) fn is_drafting(&self) -> bool {
        matches!(self.phase, IssuePhase::Drafting { .. })
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

        let clean_arg = sanitize_single_line(argument);
        let user_hint = (!clean_arg.is_empty()).then_some(clean_arg);
        let try_ai_draft = self.issue_available && !self.is_connection_unconfigured();
        let fallback_title = user_hint
            .clone()
            .unwrap_or_else(|| diagnostics.default_title());
        let fallback_summary = if user_hint.is_some() && !self.issue_available {
            String::new()
        } else {
            diagnostics.default_summary()
        };
        let (body, body_cursor) = diagnostics.format_issue_body_with_summary(&fallback_summary);
        let focus = if fallback_title.is_empty() {
            IssueFieldFocus::Title
        } else {
            IssueFieldFocus::Body
        };
        let title_cursor = fallback_title.len();
        let repo = if is_valid_repo_slug(&self.issue_repo) {
            self.issue_repo.clone()
        } else {
            DEFAULT_ISSUE_REPO.to_string()
        };
        let draft_source = if self.issue_available && diagnostics.has_real_failure() {
            IssueDraftSource::DeterministicFallback
        } else {
            IssueDraftSource::Manual
        };

        let fallback = IssueDraft {
            repo: repo.clone(),
            title: fallback_title,
            title_cursor,
            body,
            body_cursor,
            focus,
            auto_label: false,
            draft_source,
            diagnostics: diagnostics.clone(),
            warning: None,
        };

        let generation = self.issue.next_generation();
        self.notice = None;
        if try_ai_draft {
            let redacted_hint = user_hint
                .as_deref()
                .map(|hint| sanitize_single_line(&redact_diagnostic_text(hint)))
                .filter(|hint| !hint.is_empty());
            self.issue.phase = IssuePhase::Drafting {
                repo: repo.clone(),
                fallback,
                started_at_ms: super::unix_time_ms(),
            };
            Some(FileBackedAction::DraftIssue {
                generation,
                repo,
                user_hint: redacted_hint,
                diagnostics_context: diagnostics.to_draft_prompt_context(),
                profile: diagnostics.active_profile,
            })
        } else {
            self.issue.phase = IssuePhase::Editing(fallback);
            None
        }
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
        let is_ctrl_c = ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));

        if key.code == KeyCode::Esc || is_ctrl_c {
            if matches!(self.issue.phase, IssuePhase::Drafting { .. }) && !is_ctrl_c {
                let IssuePhase::Drafting { fallback, .. } = std::mem::take(&mut self.issue.phase)
                else {
                    unreachable!("checked Drafting phase above");
                };
                self.issue.next_generation();
                self.issue.phase = IssuePhase::Editing(fallback);
                self.notice = Some("Skipped AI draft — review redacted draft below.".into());
                return Some(FileBackedAction::CancelIssue);
            }
            let was_submitting = matches!(self.issue.phase, IssuePhase::Submitting { .. });
            let was_drafting = matches!(self.issue.phase, IssuePhase::Drafting { .. });
            self.issue.close();
            self.notice = Some(if was_submitting {
                "Issue submission cancelled.".into()
            } else {
                "Issue draft cancelled.".into()
            });
            return (was_submitting || was_drafting).then_some(FileBackedAction::CancelIssue);
        }

        let IssuePhase::Editing(IssueDraft {
            repo,
            title,
            title_cursor,
            body,
            body_cursor,
            focus,
            auto_label,
            warning,
            ..
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
            KeyCode::Char('l' | 'L') if ctrl && !alt => {
                *auto_label = !*auto_label;
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
                let labels = if *auto_label {
                    vec![AUTO_ISSUE_LABEL.to_string()]
                } else {
                    Vec::new()
                };
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
                    labels,
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

    pub(in crate::file_backed) fn apply_issue_draft_result(
        &mut self,
        generation: u64,
        result: Result<IssueSubmitResult, String>,
    ) {
        if generation != self.issue.generation
            || !matches!(self.issue.phase, IssuePhase::Drafting { .. })
        {
            return;
        }
        let IssuePhase::Drafting { mut fallback, .. } = std::mem::take(&mut self.issue.phase)
        else {
            unreachable!("checked Drafting phase above");
        };
        if let Ok(IssueSubmitResult::Drafted { title, summary }) = result {
            let clean_title: String = sanitize_single_line(&redact_diagnostic_text(&title))
                .chars()
                .take(MAX_AI_TITLE_CHARS)
                .collect();
            let final_title = if clean_title.trim().is_empty() {
                fallback.title.clone()
            } else {
                clean_title
            };
            let clean_summary = redact_diagnostic_multiline(&summary, MAX_AI_SUMMARY_CHARS);
            let effective_summary = if clean_summary.trim().is_empty() {
                fallback.diagnostics.default_summary()
            } else {
                clean_summary
            };
            let (body, body_cursor) = fallback
                .diagnostics
                .format_issue_body_with_summary(&effective_summary);
            fallback.title_cursor = final_title.len();
            fallback.title = final_title;
            fallback.body = body;
            fallback.body_cursor = body_cursor;
            fallback.focus = if fallback.title.is_empty() {
                IssueFieldFocus::Title
            } else {
                IssueFieldFocus::Body
            };
            fallback.draft_source = IssueDraftSource::AiDrafted;
        } else {
            fallback.draft_source =
                if !fallback.title.is_empty() || fallback.diagnostics.has_real_failure() {
                    IssueDraftSource::DeterministicFallback
                } else {
                    IssueDraftSource::Manual
                };
        }
        self.issue.phase = IssuePhase::Editing(fallback);
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
pub(in crate::file_backed) fn dispatch_issue_draft(
    handler: Option<IssueControlHandler>,
    tx: &mpsc::Sender<FileBackedEvent>,
    generation: u64,
    repo: String,
    user_hint: Option<String>,
    diagnostics_context: String,
    profile: Option<String>,
    active_issue_task: &mut Option<(u64, tokio::task::AbortHandle)>,
) {
    if let Some((_, prev)) = active_issue_task.take() {
        prev.abort();
    }
    *active_issue_task = Some((
        generation,
        spawn_draft(
            handler,
            tx,
            generation,
            repo,
            user_hint,
            diagnostics_context,
            profile,
        ),
    ));
}

pub(in crate::file_backed) fn handle_issue_drafted(
    app: &mut FileBackedApp,
    generation: u64,
    result: Result<IssueSubmitResult, String>,
    active_issue_task: &mut Option<(u64, tokio::task::AbortHandle)>,
) {
    if active_issue_task
        .as_ref()
        .is_some_and(|(task_gen, _)| *task_gen == generation)
    {
        *active_issue_task = None;
    }
    app.apply_issue_draft_result(generation, result);
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
    labels: Vec<String>,
    active_issue_task: &mut Option<(u64, tokio::task::AbortHandle)>,
) {
    if let Some((_, prev)) = active_issue_task.take() {
        prev.abort();
    }
    *active_issue_task = Some((
        generation,
        spawn_submit(shell, handler, tx, generation, repo, title, body, labels),
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

fn spawn_draft(
    handler: Option<IssueControlHandler>,
    tx: &mpsc::Sender<FileBackedEvent>,
    generation: u64,
    repo: String,
    user_hint: Option<String>,
    diagnostics_context: String,
    profile: Option<String>,
) -> tokio::task::AbortHandle {
    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        let result = match handler {
            Some(handler) => handler(IssueControl::Draft {
                repo,
                user_hint,
                diagnostics_context,
                profile,
            })
            .await
            .map_err(|err| format!("{err:#}")),
            None => Ok(IssueSubmitResult::DraftUnavailable {
                reason: "issue host adapter unavailable".to_string(),
            }),
        };
        let _ = tx
            .send(FileBackedEvent::IssueDrafted { generation, result })
            .await;
    });
    handle.abort_handle()
}

#[allow(
    clippy::too_many_arguments,
    reason = "spawns background tiered submission task with labels"
)]
fn spawn_submit(
    shell: &alan_shell::Shell,
    handler: Option<IssueControlHandler>,
    tx: &mpsc::Sender<FileBackedEvent>,
    generation: u64,
    repo: String,
    title: String,
    body: String,
    labels: Vec<String>,
) -> tokio::task::AbortHandle {
    let shell = shell.clone();
    let tx = tx.clone();
    let handle = tokio::spawn(async move {
        let result = submit_issue_tiered(&shell, handler, &repo, &title, &body, &labels).await;
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
    labels: &[String],
) -> Result<IssueSubmissionOutcome, String> {
    if !is_valid_repo_slug(repo) {
        return Err(format!("invalid repository slug `{repo}`"));
    }
    let issues_new_path = format!("/mnt/github/repos/{repo}/issues/new");
    if let Ok(stat) = shell.stat(&issues_new_path).await
        && stat.writable
    {
        let payload = if labels.is_empty() {
            serde_json::json!({ "title": title, "body": body })
        } else {
            serde_json::json!({ "title": title, "body": body, "labels": labels })
        };
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
            labels: labels.to_vec(),
        })
        .await
        {
            Ok(IssueSubmitResult::CreatedGhCli { number, url }) => {
                return Ok(IssueSubmissionOutcome::GhCli { number, url });
            }
            Ok(IssueSubmitResult::GhUnavailable { reason })
            | Ok(IssueSubmitResult::DraftUnavailable { reason }) => reason,
            Ok(IssueSubmitResult::Drafted { .. }) => {
                "unexpected draft response during issue submission".to_string()
            }
            Err(err) => return Err(format!("{err:#}")),
        }
    } else {
        "`gh` CLI host adapter unavailable".to_string()
    };

    Ok(IssueSubmissionOutcome::WebFallback {
        url: build_prefilled_issue_url(repo, title, body, labels),
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

#[cfg(test)]
#[path = "issue_flow_tests.rs"]
mod tests;
