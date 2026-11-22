//! Interactive `/settings` menu, two-step reset confirmation, and First-Run Setup handoff.

use alan_agent_protocol::{InputIntent, UiPlanSnapshot, UiThinkingSnapshot};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::completion::{CompletionCandidate, CompletionKind, CompletionState};
use crate::reconcile::StreamReconciler;

use super::FileBackedApp;
use super::app::{FileBackedAction, Notice};
use super::settings::{SettingsControl, SettingsResetScope, SettingsResult};

const SETTINGS_MENU_ITEMS: [(&str, &str); 5] = [
    (
        "Connections & Login",
        "connect or switch model providers (/login)",
    ),
    (
        "Model & Reasoning",
        "choose model and reasoning effort (/model)",
    ),
    (
        "Trust Posture",
        "cycle governed vs auto-approve (/permissions)",
    ),
    (
        "Reset connections & settings",
        "clear providers, secrets, grants, history (keeps rollouts & memory)",
    ),
    (
        "Reset ALL Alan data (factory reset)",
        "wipe all Alan services, memory, and rollouts (keeps user files & gcloud)",
    ),
];

#[derive(Clone, Default)]
pub(in crate::file_backed) struct SettingsFlowState {
    pub generation: u64,
    phase: SettingsPhase,
}

#[derive(Clone, Default)]
enum SettingsPhase {
    #[default]
    Idle,
    Menu {
        selected: usize,
    },
    ConfirmReset {
        scope: SettingsResetScope,
        /// Defaults to `1` (`Cancel`) so accidental Enter cannot destroy state.
        selected: usize,
    },
    Applying,
}

impl SettingsFlowState {
    pub(in crate::file_backed) fn is_active(&self) -> bool {
        !matches!(self.phase, SettingsPhase::Idle)
    }

    pub(in crate::file_backed) fn close(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.phase = SettingsPhase::Idle;
    }

    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }
}

impl FileBackedApp {
    /// Entry point for `/settings [reset [--all]]`.
    pub(in crate::file_backed) fn settings_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        if self.turn_active() || self.pending_yield.is_some() || !self.running_tools.is_empty() {
            self.notice = Some(Notice::warning(
                "Wait for or interrupt the active turn before /settings.",
            ));
            return None;
        }
        if self.project_selection.is_some() || self.pending_project_control.is_some() {
            self.notice = Some(Notice::warning(
                "Finish or cancel the active project selection before /settings.",
            ));
            return None;
        }

        let trimmed = argument.trim();
        let initial_phase = match trimmed {
            "" => SettingsPhase::Menu { selected: 0 },
            "reset" => SettingsPhase::ConfirmReset {
                scope: SettingsResetScope::Settings,
                selected: 1,
            },
            "reset --all" => SettingsPhase::ConfirmReset {
                scope: SettingsResetScope::All,
                selected: 1,
            },
            _ => {
                self.notice = Some(Notice::warning(
                    "usage: /settings | /settings reset | /settings reset --all",
                ));
                return None;
            }
        };

        self.modal.active = false;
        self.model_chooser.active = false;
        self.resume_picker.close();
        self.fork_picker.close();
        self.login.close();
        self.pending_login_submission = None;
        self.completion = None;

        self.settings.next_generation();
        self.settings.phase = initial_phase;
        self.refresh_settings_view();
        None
    }

    pub(in crate::file_backed) fn settings_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
        if !self.settings.is_active() {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match &mut self.settings.phase {
            SettingsPhase::Idle => None,
            SettingsPhase::Applying => {
                if key.code == KeyCode::Esc || (ctrl && key.code == KeyCode::Char('c')) {
                    self.settings.close();
                    self.completion = None;
                    self.notice = Some("Cancelled pending reset view.".into());
                }
                None
            }
            SettingsPhase::Menu { selected } => match key.code {
                KeyCode::Esc | KeyCode::Char('c') if key.code == KeyCode::Esc || ctrl => {
                    self.settings.close();
                    self.completion = None;
                    self.notice = None;
                    None
                }
                KeyCode::Up => {
                    let count = SETTINGS_MENU_ITEMS.len();
                    *selected = if *selected == 0 {
                        count - 1
                    } else {
                        *selected - 1
                    };
                    self.refresh_settings_view();
                    None
                }
                KeyCode::Down => {
                    *selected = (*selected + 1) % SETTINGS_MENU_ITEMS.len();
                    self.refresh_settings_view();
                    None
                }
                KeyCode::Enter => {
                    let choice = *selected;
                    match choice {
                        0 => {
                            self.settings.close();
                            self.completion = None;
                            self.login_command("")
                        }
                        1 => {
                            self.settings.close();
                            self.completion = None;
                            self.model_command("")
                        }
                        2 => {
                            self.settings.close();
                            self.completion = None;
                            self.toggle_trust_posture()
                        }
                        3 => {
                            self.settings.phase = SettingsPhase::ConfirmReset {
                                scope: SettingsResetScope::Settings,
                                selected: 1,
                            };
                            self.refresh_settings_view();
                            None
                        }
                        4 => {
                            self.settings.phase = SettingsPhase::ConfirmReset {
                                scope: SettingsResetScope::All,
                                selected: 1,
                            };
                            self.refresh_settings_view();
                            None
                        }
                        _ => None,
                    }
                }
                _ => None,
            },
            SettingsPhase::ConfirmReset { scope, selected } => match key.code {
                KeyCode::Esc | KeyCode::Char('c') if key.code == KeyCode::Esc || ctrl => {
                    self.settings.phase = SettingsPhase::Menu { selected: 0 };
                    self.refresh_settings_view();
                    None
                }
                KeyCode::Up | KeyCode::Down => {
                    *selected = (*selected + 1) % 2;
                    self.refresh_settings_view();
                    None
                }
                KeyCode::Enter => {
                    if *selected != 0 {
                        self.settings.phase = SettingsPhase::Menu { selected: 0 };
                        self.refresh_settings_view();
                        self.notice = Some("Reset cancelled.".into());
                        return None;
                    }
                    let chosen_scope = *scope;
                    if !self.settings_available {
                        self.settings.close();
                        self.completion = None;
                        self.notice = Some(Notice::warning(
                            "In-TUI reset is unavailable in this Host; run `alan reset` from your terminal.",
                        ));
                        return None;
                    }
                    let generation = self.settings.next_generation();
                    self.settings.phase = SettingsPhase::Applying;
                    self.completion = None;
                    self.notice = Some("Resetting Alan stores and reloading runtime…".into());
                    Some(FileBackedAction::SettingsControl {
                        generation,
                        control: SettingsControl::Reset {
                            scope: chosen_scope,
                        },
                    })
                }
                _ => None,
            },
        }
    }

    pub(in crate::file_backed) fn apply_settings_result(
        &mut self,
        generation: u64,
        result: Result<SettingsResult, String>,
    ) -> (bool, Option<FileBackedAction>) {
        if generation != self.settings.generation
            || !matches!(self.settings.phase, SettingsPhase::Applying)
        {
            return (false, None);
        }
        self.settings.close();
        self.completion = None;

        match result {
            Ok(SettingsResult::ResetCompleted { scope, .. }) => {
                self.reset_local_state_after_store_wipe();
                if self.login_available {
                    let setup_action = self.start_first_run_setup();
                    return (true, setup_action);
                }
                let scope_label = match scope {
                    SettingsResetScope::Settings => "Connections and settings",
                    SettingsResetScope::All => "All Alan stores",
                };
                self.notice = Some(
                    format!(
                        "{scope_label} reset. Run `/login` or `alan login` to connect a provider."
                    )
                    .into(),
                );
                (true, None)
            }
            Err(error) => {
                self.notice = Some(Notice::error(format!("Settings reset failed: {error}")));
                (false, None)
            }
        }
    }

    fn reset_local_state_after_store_wipe(&mut self) {
        self.clear_local_receipts();
        self.transcript.clear();
        self.turn_prompts.clear();
        self.action_cells.clear();
        self.projected_actions.clear();
        self.running_tools.clear();
        self.plan = UiPlanSnapshot::empty();
        self.plan_owners.clear();
        self.plan_revision = 0;
        self.thinking = UiThinkingSnapshot::idle();
        self.usage = super::usage::UsageProjection::default();
        self.reconciler = StreamReconciler::new();
        self.pending_remote_turn_start = None;
        self.scrollback_front_is_partial = false;
        self.last_input_failed = false;
        self.auto_continue_on_empty_cancel = false;
        self.input_intent = InputIntent::Agent;
        self.history_draft_intent = None;
        self.effective_model = None;
        self.connection_configured = Some(false);
        self.model.snapshot = None;
        self.model_chooser = Default::default();
        self.maybe_insert_welcome_header();
    }

    fn refresh_settings_view(&mut self) {
        match &self.settings.phase {
            SettingsPhase::Menu { selected } => {
                let matches = SETTINGS_MENU_ITEMS
                    .iter()
                    .map(|(label, detail)| {
                        CompletionCandidate::new(*label, Some((*detail).to_string()))
                    })
                    .collect();
                self.completion = Some(CompletionState {
                    kind: CompletionKind::Command,
                    token_start: 0,
                    query: String::new(),
                    matches,
                    selected: *selected,
                });
                self.notice = Some("Settings · ↑↓ move · Enter select · Esc close".into());
            }
            SettingsPhase::ConfirmReset { scope, selected } => {
                let (title, detail, confirm_label) = match scope {
                    SettingsResetScope::Settings => (
                        "Confirm Reset Connections & Settings",
                        "Clears providers, secrets/OAuth, project grants, UI history & updater cache. Keeps rollouts, memory, and ~/.config/gcloud.",
                        "Yes, reset connections & settings",
                    ),
                    SettingsResetScope::All => (
                        "Confirm Factory Reset (ALL Alan Data)",
                        "Wipes ALL Alan System Store services, rollouts, and Memory Stores + Host credentials. Keeps user project folders & ~/.config/gcloud.",
                        "Yes, factory reset all Alan data",
                    ),
                };
                self.completion = Some(CompletionState {
                    kind: CompletionKind::Command,
                    token_start: 0,
                    query: String::new(),
                    matches: vec![
                        CompletionCandidate::new(
                            confirm_label,
                            Some("proceed with reset and launch setup".to_string()),
                        ),
                        CompletionCandidate::new("Cancel", Some("return to /settings".to_string())),
                    ],
                    selected: *selected,
                });
                self.notice = Some(Notice::warning(format!(
                    "{title}\n{detail} (↑↓ choose · Enter confirm · Esc back)"
                )));
            }
            SettingsPhase::Idle | SettingsPhase::Applying => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_protocol::{UiModelBinding, UiModelControlSource, UiModelSnapshot};

    fn test_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn settings_reset_defaults_to_cancel_and_launches_setup_after_confirm() {
        let mut app = FileBackedApp::new("/agent/1".into());
        app.login_available = true;
        app.settings_available = true;
        app.connection_configured = Some(true);
        app.effective_model = Some("gpt-6.1-sol".into());
        app.model.apply(
            "/agent/1",
            Some(UiModelSnapshot {
                known: true,
                publication_version: 1,
                process_path: "/proc/1".into(),
                catalog: None,
                selected_next: Some(UiModelBinding {
                    profile: "chatgpt-main".into(),
                    provider: "chatgpt".into(),
                    model: "gpt-6.1-sol".into(),
                    reasoning: Default::default(),
                    control_source: UiModelControlSource::ProviderDefault,
                }),
                active: None,
                admitted: vec![],
                ..Default::default()
            }),
        );

        app.handle_command("/settings");
        assert!(app.settings.is_active());
        assert_eq!(app.completion.as_ref().unwrap().matches.len(), 5);

        // Move to "Reset connections & settings" (index 3) and press Enter.
        for _ in 0..3 {
            app.handle_key(test_key(KeyCode::Down));
        }
        assert!(app.handle_key(test_key(KeyCode::Enter)).is_none());

        // Confirmation defaults to Cancel (index 1); pressing Enter cancels safely.
        assert_eq!(app.completion.as_ref().unwrap().selected, 1);
        assert!(app.handle_key(test_key(KeyCode::Enter)).is_none());
        assert!(
            app.notice
                .as_ref()
                .is_some_and(|n| n.contains("Reset cancelled"))
        );

        // Enter Reset confirmation via direct shortcut, move to Confirm (index 0), press Enter.
        app.handle_command("/settings reset");
        assert_eq!(app.completion.as_ref().unwrap().selected, 1);
        app.handle_key(test_key(KeyCode::Up));
        assert_eq!(app.completion.as_ref().unwrap().selected, 0);

        let Some(FileBackedAction::SettingsControl {
            generation,
            control:
                SettingsControl::Reset {
                    scope: SettingsResetScope::Settings,
                },
        }) = app.handle_key(test_key(KeyCode::Enter))
        else {
            panic!("expected SettingsControl::Reset action");
        };

        let (was_reset, next_action) = app.apply_settings_result(
            generation,
            Ok(SettingsResult::ResetCompleted {
                scope: SettingsResetScope::Settings,
                removed_count: 5,
            }),
        );
        assert!(was_reset);
        assert!(app.is_connection_unconfigured());
        assert!(app.login.is_active());
        assert!(app.login.first_run_setup);
        assert!(matches!(
            next_action,
            Some(FileBackedAction::LoginListProviders { .. })
        ));
    }

    #[test]
    fn first_run_setup_can_be_skipped_with_esc_and_shows_hint() {
        let mut app = FileBackedApp::new("/agent/1".into());
        app.login_available = true;
        app.connection_configured = Some(false);

        let action = app.start_first_run_setup();
        assert!(matches!(
            action,
            Some(FileBackedAction::LoginListProviders { .. })
        ));
        assert!(app.login.first_run_setup);
        assert!(
            app.notice
                .as_ref()
                .is_some_and(|n| n.contains("First-Time Setup"))
        );

        let cancel_action = app.handle_key(test_key(KeyCode::Esc));
        assert!(matches!(cancel_action, Some(FileBackedAction::CancelLogin)));
        assert!(!app.login.is_active());
        assert!(!app.login.first_run_setup);
        assert!(
            app.notice
                .as_ref()
                .is_some_and(|n| n.contains("Setup skipped"))
        );
    }
}
