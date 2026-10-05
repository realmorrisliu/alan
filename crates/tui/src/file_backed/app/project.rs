use super::*;

pub(super) fn default_commands() -> Vec<CompletionCandidate> {
    [
        ("project", "select or revoke a project directory"),
        ("compact", "summarize context"),
        ("rollback", "undo the last turn"),
        ("continue", "continue paused input"),
        ("discard", "discard paused input"),
        ("clear", "clear the transcript"),
        ("help", "show key bindings"),
        ("quit", "exit alan"),
        ("model", "choose selected-next from Process catalog"),
        ("status", "inspect Process model bindings"),
    ]
    .into_iter()
    .map(|(value, detail)| CompletionCandidate::new(value, Some(detail.to_string())))
    .collect()
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;

#[derive(Clone)]
pub(in crate::file_backed) struct PendingProjectControl {
    pub owner: String,
    pub id: String,
    pub target: String,
    pub revoke: Option<String>,
    pub fenced: bool,
}

impl FileBackedApp {
    pub(in crate::file_backed) fn project_boundary_available(&self, admitted: bool) -> bool {
        self.project_recovery_boundary_available(admitted)
            && self.pending_project_control.is_none()
            && self.retained_project_mount.is_none()
            && self.ready_project_revoke.is_none()
            && self.project_cleanup.is_none()
    }

    pub(in crate::file_backed) fn project_recovery_boundary_available(
        &self,
        admitted: bool,
    ) -> bool {
        !admitted
            && !self.project_host_pending
            && self.queue.project_safe()
            && !self.modal.active
            && self.activity.state != UiActivityState::Running
            && self.activity.waiting_submission_ids.is_empty()
            && self.pending_yield.is_none()
            && self.form.is_none()
            && self.response_in_flight.is_none()
            && self.running_tools.is_empty()
    }

    pub(in crate::file_backed) fn set_project_candidate(
        &mut self,
        candidate: Option<std::path::PathBuf>,
    ) {
        self.project_candidate = candidate;
    }

    pub(in crate::file_backed) fn stage_project_control(
        &mut self,
        owner: String,
        id: String,
        mount: Option<(ProjectMountReceipt, std::path::PathBuf)>,
        revoke: Option<String>,
    ) {
        let target = mount
            .as_ref()
            .map_or("/", |(r, _)| r.namespace_path.as_str())
            .to_owned();
        if let Some(mount) = mount {
            // Only one candidate is admitted at a time. Recovery controls do
            // not replace its authority with a request-history chain.
            self.retained_project_mount = Some(mount);
        }
        // A retry supersedes only disposition evidence, never candidate authority.
        self.ready_project_revoke = None;
        self.pending_project_control = Some(PendingProjectControl {
            owner,
            id,
            target,
            revoke,
            fenced: false,
        });
        self.project_selection = None;
        self.notice = Some("project directory selection pending; authorization alone does not select cwd or continue paused work".into());
        self.refresh_completion();
    }

    pub(in crate::file_backed) fn fail_project_control(&mut self, message: String) {
        // A transport error or owner replacement does not prove the selector
        // was rejected. Retain candidate authority: it may already be cwd.
        self.notice = Some(format!(
            "{message}; grant retained, cwd unconfirmed; /project revoke explicitly leaves current Root cwd before cleanup"
        ));
    }

    pub(in crate::file_backed) fn fence_project_control(&mut self) {
        if let Some(pending) = self.pending_project_control.as_mut() {
            pending.fenced = true;
        }
        self.fail_project_control("Root changed; old owner evidence fenced".into());
    }

    pub(in crate::file_backed) fn retained_project_grant(&self) -> Option<String> {
        self.retained_project_mount
            .as_ref()
            .map(|(receipt, _)| receipt.grant_id.clone())
            .or_else(|| {
                self.pending_project_control
                    .as_ref()
                    .and_then(|control| control.revoke.clone())
            })
            .or_else(|| self.project_cleanup.clone())
            .or_else(|| self.project.as_ref().map(|p| p.grant_id.clone()))
    }

    pub(in crate::file_backed) fn project_revoked(&mut self) {
        let grant = self.retained_project_grant();
        self.pending_project_control = None;
        self.project_cleanup = None;
        self.project_cleanup_attempted = false;
        if self
            .project
            .as_ref()
            .is_some_and(|p| Some(&p.grant_id) == grant.as_ref())
        {
            self.project = None;
            self.set_file_candidates(Vec::new());
        }
        self.retained_project_mount = None;
        self.ready_project_revoke = None;
    }

    pub(in crate::file_backed) fn take_project_cleanup(&mut self) -> Option<String> {
        if self.project_cleanup_attempted {
            return None;
        }
        let grant = self.project_cleanup.clone()?;
        self.project_cleanup_attempted = true;
        Some(grant)
    }

    pub(in crate::file_backed) fn finish_project_cleanup(&mut self, grant: &str, confirmed: bool) {
        if confirmed && self.project_cleanup.as_deref() == Some(grant) {
            self.project_cleanup = None;
        }
    }

    pub(in crate::file_backed) fn take_ready_project_revoke(&mut self) -> Option<String> {
        self.ready_project_revoke.take()
    }

    pub(in crate::file_backed) fn observe_project_action(
        &mut self,
        owner: &str,
        snapshot: &ActionSnapshot,
    ) {
        let Some(pending) = self.pending_project_control.as_ref() else {
            return;
        };
        if pending.fenced || snapshot.name != "cd" || pending.owner != owner {
            return;
        }
        let Ok(result) = serde_json::from_str::<serde_json::Value>(&snapshot.result) else {
            return;
        };
        if result["call_id"].as_str() != Some(pending.id.as_str()) {
            return;
        }
        if !matches!(snapshot.status.trim(), "completed" | "failed" | "rejected") {
            return;
        }
        let cwd = result["outcome"]["cwd"].as_str();
        let target = &pending.target;
        if snapshot.status.trim() != "completed"
            || result["exit_code"].as_i64() != Some(0)
            || result["outcome"]["success"].as_bool() != Some(true)
            || cwd.is_none_or(|cwd| std::path::Path::new(cwd) != std::path::Path::new(target))
        {
            // Only correlated explicit failure proves cleanup is safe. A
            // successful but unexpected cwd is not rejection evidence.
            if matches!(snapshot.status.trim(), "failed" | "rejected")
                && result["exit_code"].as_i64().is_some_and(|code| code != 0)
                && result["outcome"]["success"].as_bool() == Some(false)
                && pending.revoke.is_none()
                && let Some((receipt, _)) = self.retained_project_mount.as_ref()
                && self
                    .project
                    .as_ref()
                    .is_none_or(|old| old.grant_id != receipt.grant_id)
            {
                self.project_cleanup = Some(receipt.grant_id.clone());
                self.project_cleanup_attempted = false;
                self.pending_project_control = None;
                self.retained_project_mount = None;
            }
            self.fail_project_control("project cwd selection failed or returned an unexpected cwd; previous binding retained".into());
            return;
        }
        let cwd = cwd.unwrap().to_string();
        let pending = self.pending_project_control.take().unwrap();
        self.namespace_cwd = std::path::PathBuf::from(&cwd);
        if pending.revoke.is_none()
            && let Some((receipt, root)) = self.retained_project_mount.take()
        {
            self.set_file_candidates(crate::build_file_index(&root, crate::FILE_INDEX_LIMIT));
            self.project = Some(receipt);
        }
        if pending.revoke.is_some() {
            self.ready_project_revoke = pending.revoke.clone();
            let mut pending = pending;
            pending.fenced = true;
            self.pending_project_control = Some(pending);
        }
        self.notice = Some(format!(
            "project cwd: {cwd}; paused work requires explicit /continue"
        ));
    }

    pub(in crate::file_backed) fn observe_action_cwd(&mut self, snapshot: &ActionSnapshot) {
        if self.pending_project_control.is_some() {
            return;
        }
        // Engine directory controls already carry this title; ordinary cd
        // carries its command. No wire-ID mutation or request history needed.
        if serde_json::from_str::<serde_json::Value>(&snapshot.result)
            .ok()
            .is_some_and(|result| result["title"].as_str() == Some("Select Process directory"))
        {
            return;
        }
        if snapshot.name == "cd"
            && snapshot.status.trim() == "completed"
            && let Ok(result) = serde_json::from_str::<serde_json::Value>(&snapshot.result)
            && let Some(cwd) = result["outcome"]["cwd"].as_str()
        {
            self.namespace_cwd = cwd.into();
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
