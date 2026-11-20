//! Interactive `/fork [query]` prompt selector backed by `machine/ctl` `rollback <N>`.

use alan_agent_protocol::{InputIntent, UiPlanSnapshot, UiThinkingSnapshot};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::completion::{self, CompletionCandidate, CompletionKind, CompletionState};
use crate::history::{HistoryCell, clean_text};
use crate::reconcile::StreamReconciler;

use super::FileBackedApp;
use super::app::{FileBackedAction, Notice};

/// One user prompt eligible as a `/fork` rollback target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_backed) struct ForkPromptEntry {
    pub text: String,
    pub is_command: bool,
    pub turns_from_end: usize,
}

/// Interactive state for `/fork [query]`.
#[derive(Clone, Default)]
pub(in crate::file_backed) struct ForkPicker {
    pub active: bool,
    pub query: String,
    pub entries: Vec<ForkPromptEntry>,
    pub filtered: Vec<usize>,
    pub selected: usize,
}

impl ForkPicker {
    pub(in crate::file_backed) fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.entries.clear();
        self.filtered.clear();
        self.selected = 0;
    }
}

impl FileBackedApp {
    pub(in crate::file_backed) fn collect_fork_entries(&self) -> Vec<ForkPromptEntry> {
        if !self.turn_prompts.is_empty() {
            return self
                .turn_prompts
                .iter()
                .rev()
                .enumerate()
                .map(|(idx, (text, is_command))| ForkPromptEntry {
                    text: text.clone(),
                    is_command: *is_command,
                    turns_from_end: idx + 1,
                })
                .collect();
        }
        let mut entries = Vec::new();
        for (idx, cell) in self.transcript.iter().enumerate().rev() {
            if self.queued_receipt_at(idx) {
                continue;
            }
            if let Some((text, is_command, _)) = cell.input_source() {
                entries.push(ForkPromptEntry {
                    text: text.to_string(),
                    is_command,
                    turns_from_end: entries.len() + 1,
                });
            }
        }
        entries
    }

    pub(in crate::file_backed) fn fork_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        if self.turn_active() || self.pending_yield.is_some() || !self.running_tools.is_empty() {
            self.notice = Some(Notice::warning(
                "Wait for or interrupt the active turn before /fork.",
            ));
            return None;
        }
        if self.project_selection.is_some() || self.pending_project_control.is_some() {
            self.notice = Some(Notice::warning(
                "Finish or cancel the active project selection before /fork.",
            ));
            return None;
        }
        let entries = self.collect_fork_entries();
        if entries.is_empty() {
            self.notice = Some("No earlier prompt in this session to fork from.".into());
            return None;
        }
        self.modal.active = false;
        self.model_chooser.active = false;
        self.resume_picker.close();
        self.login.close();
        self.completion = None;
        self.fork_picker = ForkPicker {
            active: true,
            query: argument.trim().to_string(),
            entries,
            filtered: Vec::new(),
            selected: 0,
        };
        self.refresh_fork_completion();
        None
    }

    pub(in crate::file_backed) fn fork_picker_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
        if !self.fork_picker.active {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') if key.code == KeyCode::Esc || ctrl => {
                self.fork_picker.close();
                self.completion = None;
                self.notice = Some("Fork cancelled.".into());
                None
            }
            KeyCode::Up if !self.fork_picker.filtered.is_empty() => {
                let len = self.fork_picker.filtered.len();
                self.fork_picker.selected = if self.fork_picker.selected == 0 {
                    len - 1
                } else {
                    self.fork_picker.selected - 1
                };
                self.refresh_fork_completion();
                None
            }
            KeyCode::Down if !self.fork_picker.filtered.is_empty() => {
                let len = self.fork_picker.filtered.len();
                self.fork_picker.selected = (self.fork_picker.selected + 1) % len;
                self.refresh_fork_completion();
                None
            }
            KeyCode::Backspace if !self.fork_picker.query.is_empty() => {
                self.fork_picker.query.pop();
                self.fork_picker.selected = 0;
                self.refresh_fork_completion();
                None
            }
            KeyCode::Char('u') if ctrl && !self.fork_picker.query.is_empty() => {
                self.fork_picker.query.clear();
                self.fork_picker.selected = 0;
                self.refresh_fork_completion();
                None
            }
            KeyCode::Char(ch) if !ctrl && !alt && !ch.is_control() => {
                self.fork_picker.query.push(ch);
                self.fork_picker.selected = 0;
                self.refresh_fork_completion();
                None
            }
            KeyCode::Enter => {
                let entry_idx = *self.fork_picker.filtered.get(self.fork_picker.selected)?;
                let entry = self.fork_picker.entries.get(entry_idx)?.clone();
                self.fork_picker.close();
                self.completion = None;
                self.apply_fork_rollback(&entry);
                Some(FileBackedAction::MachineCtl {
                    command: format!("rollback {}", entry.turns_from_end),
                    success_notice: format!(
                        "Forked from {} turn{} back — edit prompt and press Enter to submit.",
                        entry.turns_from_end,
                        if entry.turns_from_end == 1 { "" } else { "s" }
                    ),
                })
            }
            _ => None,
        }
    }

    fn apply_fork_rollback(&mut self, entry: &ForkPromptEntry) {
        let turns = entry.turns_from_end;
        let keep_prompts = self.turn_prompts.len().saturating_sub(turns);
        self.turn_prompts.truncate(keep_prompts);

        let mut remaining = turns;
        let mut cut_idx = None;
        for (idx, cell) in self.transcript.iter().enumerate().rev() {
            if self.queued_receipt_at(idx) {
                continue;
            }
            if cell.input_source().is_some() {
                remaining -= 1;
                if remaining == 0 {
                    cut_idx = Some(idx);
                    break;
                }
            }
        }
        let cut = cut_idx.unwrap_or_else(|| {
            usize::from(matches!(
                self.transcript.first(),
                Some(HistoryCell::Welcome(_))
            ))
        });
        self.transcript.truncate(cut);
        self.action_cells.retain(|_, idx| *idx < cut);
        self.local_inputs
            .retain(|_, input| input.cell.is_some_and(|idx| idx < cut));
        self.projected_actions.clear();
        self.running_tools.clear();
        self.plan = UiPlanSnapshot::empty();
        self.plan_owners.clear();
        self.plan_revision = 0;
        self.thinking = UiThinkingSnapshot::idle();
        self.reconciler = StreamReconciler::new();
        self.pending_remote_turn_start = None;
        self.last_input_failed = false;
        self.auto_continue_on_empty_cancel = false;

        self.input_intent = if entry.is_command {
            InputIntent::Command
        } else {
            InputIntent::Agent
        };
        self.history_draft_intent = None;
        self.composer.set_text(&entry.text);
    }

    fn refresh_fork_completion(&mut self) {
        let query = self.fork_picker.query.clone();
        let mut scored: Vec<(i32, usize)> = self
            .fork_picker
            .entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                completion::fuzzy_score(&entry.text, &query).map(|score| (score, idx))
            })
            .collect();
        if !query.is_empty() {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        }
        self.fork_picker.filtered = scored.into_iter().map(|(_, idx)| idx).collect();
        if self.fork_picker.filtered.is_empty() {
            self.fork_picker.selected = 0;
        } else {
            self.fork_picker.selected = self
                .fork_picker
                .selected
                .min(self.fork_picker.filtered.len() - 1);
        }
        let matches = self
            .fork_picker
            .filtered
            .iter()
            .map(|&idx| {
                let entry = &self.fork_picker.entries[idx];
                let preview = clean_text(&entry.text)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                let label = format!("-{}t", entry.turns_from_end);
                let detail = if entry.is_command {
                    format!("! {preview}")
                } else {
                    preview
                };
                CompletionCandidate {
                    value: entry.turns_from_end.to_string(),
                    label,
                    detail: Some(detail),
                }
            })
            .collect();
        self.completion = Some(CompletionState {
            kind: CompletionKind::Command,
            token_start: 0,
            query: query.clone(),
            matches,
            selected: self.fork_picker.selected,
        });
        let filter_part = if query.is_empty() {
            String::new()
        } else {
            format!(" · filter: \"{query}\"")
        };
        self.notice = Some(
            format!("Fork from prompt{filter_part} · ↑↓ select · type to filter · Enter rollback & edit · Esc cancel")
                .into(),
        );
    }
}
