use super::*;

impl FileBackedApp {
    pub(in crate::file_backed) fn handle_command(
        &mut self,
        text: &str,
    ) -> Option<FileBackedAction> {
        let command = text.strip_prefix('/')?;
        let name = command.split_whitespace().next().unwrap_or("");
        match name {
            "login" => self.login_command(command.strip_prefix("login").unwrap_or("")),
            "settings" => self.settings_command(command.strip_prefix("settings").unwrap_or("")),
            "status" => self.model_status_command(),
            "doctor" => self.doctor_command(),
            "issue" => self.issue_command(command.strip_prefix("issue").unwrap_or("")),
            "bug" => self.issue_command(command.strip_prefix("bug").unwrap_or("")),
            "model" => self.model_command(command.strip_prefix("model").unwrap_or("").trim()),
            "resume" => self.resume_command(command.strip_prefix("resume").unwrap_or("")),
            "fork" => self.fork_command(command.strip_prefix("fork").unwrap_or("")),
            "permissions" => {
                self.permissions_command(command.strip_prefix("permissions").unwrap_or(""))
            }
            "summary" => self.summary_command(command.strip_prefix("summary").unwrap_or("")),
            "streams" => self.streams_command(command.strip_prefix("streams").unwrap_or("")),
            "focus" => self.focus_command(command.strip_prefix("focus").unwrap_or("")),
            "root" => self.root_command(command.strip_prefix("root").unwrap_or("")),
            "new" => self.new_session_command(),
            "tape" if command.trim() == "tape" => {
                self.modal.active = true;
                self.modal
                    .reset_for_mode(action_modal::ModalMode::Tape, false);
                self.modal.owner_path = self.agent_path.clone();
                self.modal.rows = super::super::tape_view::build_tape_lines_from_app(self);
                Some(FileBackedAction::ReadTape)
            }
            "man" => {
                let topic = command.strip_prefix("man").unwrap_or("").trim().to_string();
                let (owner, content) = super::super::man_view::fallback_content(&topic);
                self.modal.active = true;
                self.modal
                    .reset_for_mode(action_modal::ModalMode::Man, false);
                self.modal.owner_path = owner;
                self.modal.rows = super::super::man_view::format_man_lines(&content, self.theme);
                Some(FileBackedAction::ReadMan {
                    topic,
                    generation: self.modal.generation,
                })
            }
            "project" if command.trim() == "project" => {
                if let Some((_, retained)) = self.uncertain_project_mount.as_ref() {
                    if self.project_recovery_boundary_available(false) {
                        return Some(FileBackedAction::Project(retained.clone()));
                    }
                    self.notice = Some(Notice::warning(
                        "project outcome unknown; wait, then /project retries the same selection",
                    ));
                    return None;
                }
                if !self.project_boundary_available(false) {
                    self.notice =
                        Some("wait for the current Agent turn before selecting a project".into());
                } else if self.project.is_some() {
                    self.notice = Some("revoke the active project before selecting another".into());
                } else {
                    self.project_selection = Some(ProjectAccess::ReadWrite);
                    self.input_intent = InputIntent::Command;
                    self.composer.set_text(
                        self.project_candidate
                            .as_ref()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                    self.notice = Some(
                        format!(
                            "project path · {} · Enter approve · Tab toggle · Esc cancel",
                            ProjectAccess::ReadWrite.label()
                        )
                        .into(),
                    );
                }
                None
            }
            "project" if command.trim() == "project revoke" => {
                if !self.project_recovery_boundary_available(false) {
                    self.notice =
                        Some("wait for the current Agent turn before revoking a project".into());
                    None
                } else if self.project_cleanup.is_some() && self.pending_project_control.is_none() {
                    self.project_cleanup_attempted = false;
                    self.notice = Some("explicit candidate cleanup retry requested".into());
                    None
                } else if let Some(grant_id) = self.retained_project_grant() {
                    Some(FileBackedAction::Project(ProjectControl::Revoke {
                        grant_id,
                    }))
                } else {
                    Some(FileBackedAction::RevokeCurrentProject)
                }
            }
            "project" => {
                self.notice = Some("usage: /project or /project revoke".into());
                None
            }
            "quit" => {
                self.should_quit = true;
                Some(FileBackedAction::Quit)
            }
            "compact" => Some(FileBackedAction::MachineCtl {
                command: "compact".to_string(),
                success_notice: "compact requested".to_string(),
            }),
            "rollback" => Some(FileBackedAction::MachineCtl {
                command: "rollback".to_string(),
                success_notice: "rollback requested".to_string(),
            }),
            "continue" | "discard" if command.trim() == name => {
                self.auto_continue_on_empty_cancel = false;
                Some(FileBackedAction::MachineCtl {
                    command: format!("queue-v1 {name}"),
                    success_notice: format!("queue {name} requested"),
                })
            }
            "clear" => {
                self.clear_local_receipts();
                self.transcript.clear();
                self.turn_prompts.clear();
                self.action_cells.clear();
                self.pending_remote_turn_start = None;
                self.clear_late_thinking_latch();
                self.scrollback_front_is_partial = false;
                if self.confirmation.is_none() {
                    self.staged_confirmation_outcome = None;
                }
                None
            }
            "help" => {
                self.notice = Some(super::super::help::help_text().into());
                None
            }
            _ => {
                self.notice = Some(Notice::warning(format!("unknown command: /{name}")));
                None
            }
        }
    }

    pub(in crate::file_backed) fn permissions_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        let arg = argument.trim().to_ascii_lowercase();
        match arg.as_str() {
            "" => self.toggle_trust_posture(),
            "governed" => self.set_trust_posture(false),
            "auto-approve" | "auto_approve" | "trusted" | "confined" | "confined-trusted"
            | "confined/trusted" | "confined_trusted" => self.set_trust_posture(true),
            _ => {
                self.notice = Some(Notice::warning(
                    "usage: /permissions [governed|auto-approve] (or press Shift+Tab to cycle)",
                ));
                None
            }
        }
    }

    pub(in crate::file_backed) fn toggle_trust_posture(&mut self) -> Option<FileBackedAction> {
        let currently_confined_trusted = self.trust_posture_label.as_deref().is_some_and(|l| {
            let trimmed = l.trim();
            trimmed.eq_ignore_ascii_case("auto-approve")
                || trimmed.eq_ignore_ascii_case("auto_approve")
                || trimmed.eq_ignore_ascii_case("confined/trusted")
                || trimmed.eq_ignore_ascii_case("confined-trusted")
                || trimmed.eq_ignore_ascii_case("confined_trusted")
        });
        self.set_trust_posture(!currently_confined_trusted)
    }

    fn set_trust_posture(&mut self, confined_trusted: bool) -> Option<FileBackedAction> {
        if self
            .trust_posture_label
            .as_deref()
            .is_some_and(presentation::is_unsandboxed_posture)
        {
            self.notice = Some(Notice::warning(
                "Started with --unsafe-host-account; runtime trust posture switching is disabled.",
            ));
            return None;
        }
        if self.turn_active() || self.pending_yield.is_some() {
            self.notice = Some(Notice::warning(
                "Wait for or interrupt the active turn before switching trust posture.",
            ));
            return None;
        }
        if confined_trusted {
            self.trust_posture_label = Some("auto-approve".to_string());
            self.refresh_welcome_header();
            let msg = "Permissions: auto-approve (OS sandbox active; Shift+Tab cycles).";
            self.notice = Some(msg.into());
            Some(FileBackedAction::MachineCtl {
                command: "trust-posture confined-trusted".to_string(),
                success_notice: msg.to_string(),
            })
        } else {
            self.trust_posture_label = None;
            self.refresh_welcome_header();
            let msg = "Permissions: governed (OS sandbox + approvals; Shift+Tab cycles).";
            self.notice = Some(msg.into());
            Some(FileBackedAction::MachineCtl {
                command: "trust-posture governed".to_string(),
                success_notice: msg.to_string(),
            })
        }
    }

    fn new_session_command(&mut self) -> Option<FileBackedAction> {
        if self.turn_active() || self.pending_yield.is_some() {
            self.notice = Some(Notice::warning(
                "Wait for or interrupt the active turn before /new.",
            ));
            return None;
        }
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
        self.usage = super::super::usage::UsageProjection::default();
        self.budget = super::super::budget::BudgetProjection::default();
        self.reconciler = StreamReconciler::new();
        self.pending_remote_turn_start = None;
        self.clear_late_thinking_latch();
        self.scrollback_front_is_partial = false;
        self.last_input_failed = false;
        self.last_diagnostic_notice = None;
        self.auto_continue_on_empty_cancel = false;
        self.input_intent = InputIntent::Agent;
        self.history_draft_intent = None;
        self.notice = Some("Started a fresh session.".into());
        Some(FileBackedAction::MachineCtl {
            command: "reset-session".to_string(),
            success_notice: "Started a fresh session.".to_string(),
        })
    }
}
