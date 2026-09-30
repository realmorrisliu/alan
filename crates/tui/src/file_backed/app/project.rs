use super::*;

impl FileBackedApp {
    pub(in crate::file_backed) fn set_project_candidate(
        &mut self,
        candidate: Option<std::path::PathBuf>,
    ) {
        self.project_candidate = candidate;
    }

    pub(in crate::file_backed) fn project_mounted(
        &mut self,
        project: ProjectMountReceipt,
        submission_id: String,
    ) {
        self.project_selection = None;
        self.composer.set_text("");
        self.input_intent = InputIntent::Agent;
        self.pending_project_cwd = Some(project.namespace_path.clone());
        self.pending_project_cwd_submission_id = Some(submission_id);
        self.pending_project_revoke = None;
        self.ready_project_revoke = None;
        self.project = Some(project);
        self.last_input_failed = false;
        self.refresh_completion();
    }

    pub(in crate::file_backed) fn project_revoked(&mut self) {
        self.project = None;
        self.pending_project_cwd = None;
        self.pending_project_cwd_submission_id = None;
        self.pending_project_revoke = None;
        self.ready_project_revoke = None;
        self.set_file_candidates(Vec::new());
    }

    pub(in crate::file_backed) fn begin_project_revoke(
        &mut self,
        grant_id: String,
        submission_id: String,
    ) {
        self.pending_project_revoke = Some((grant_id, submission_id));
    }

    pub(in crate::file_backed) fn take_ready_project_revoke(&mut self) -> Option<String> {
        self.ready_project_revoke.take()
    }

    pub(in crate::file_backed) fn observe_action_cwd(&mut self, snapshot: &ActionSnapshot) {
        if snapshot.name != "cd" {
            return;
        }
        let Ok(result) = serde_json::from_str::<serde_json::Value>(&snapshot.result) else {
            return;
        };
        let submission_id = result["call_id"].as_str();
        match snapshot.status.trim() {
            "completed" => {
                let Some(cwd) = result["outcome"]["cwd"].as_str() else {
                    return;
                };
                self.namespace_cwd = std::path::PathBuf::from(cwd);
                if self.pending_project_cwd_submission_id.as_deref() == submission_id
                    && self.pending_project_cwd.as_deref().is_some_and(|pending| {
                        std::path::Path::new(pending) == std::path::Path::new(cwd)
                    })
                {
                    self.pending_project_cwd = None;
                    self.pending_project_cwd_submission_id = None;
                    self.notice = Some(format!("project cwd: {cwd}"));
                }
                if let Some((grant_id, pending_id)) = self.pending_project_revoke.as_ref()
                    && Some(pending_id.as_str()) == submission_id
                    && std::path::Path::new(cwd) == std::path::Path::new("/")
                {
                    self.ready_project_revoke = Some(grant_id.clone());
                    self.pending_project_revoke = None;
                }
            }
            "failed" => {
                if self.pending_project_cwd_submission_id.as_deref() == submission_id {
                    self.pending_project_cwd = None;
                    self.pending_project_cwd_submission_id = None;
                    self.last_input_failed = true;
                    self.notice =
                        Some("project mounted, but cwd selection failed; inspect details".into());
                }
                if self
                    .pending_project_revoke
                    .as_ref()
                    .is_some_and(|(_, pending_id)| Some(pending_id.as_str()) == submission_id)
                {
                    self.pending_project_revoke = None;
                    self.last_input_failed = true;
                    self.notice = Some("could not leave project cwd; grant remains active".into());
                }
            }
            _ => {}
        }
    }

    pub(in crate::file_backed) fn handle_project_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<FileBackedAction> {
        match key.code {
            KeyCode::Esc => {
                self.cancel_project_selection();
                None
            }
            KeyCode::Tab => {
                if let Some(access) = self.project_selection.as_mut() {
                    *access = access.toggle();
                    self.notice = Some(format!(
                        "project path · {} · Enter approve · Tab toggle · Esc cancel",
                        access.label()
                    ));
                }
                None
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                let path = self.composer.text().trim();
                if path.is_empty() {
                    self.notice = Some("enter a project directory path".into());
                    return None;
                }
                Some(FileBackedAction::Project(ProjectControl::Mount {
                    host_path: std::path::PathBuf::from(path),
                    access: self.project_selection.unwrap_or(ProjectAccess::ReadOnly),
                }))
            }
            KeyCode::Enter => {
                self.notice = Some("project path must be a single line".into());
                None
            }
            _ => {
                if matches!(
                    self.composer.handle_key(key),
                    ComposerKeyOutcome::Changed | ComposerKeyOutcome::Ignored
                ) {
                    None
                } else {
                    self.notice = Some("press Enter to mount the selected project".into());
                    None
                }
            }
        }
    }

    pub(in crate::file_backed) fn cancel_project_selection(&mut self) {
        self.project_selection = None;
        self.composer.set_text("");
        self.input_intent = InputIntent::Agent;
        self.notice = Some("project selection cancelled".into());
        self.refresh_completion();
    }
}
