//! Process-owned mentionability only; Skill IDs confer no execution authority.
use super::FileBackedApp;
#[cfg(test)]
use super::FileBackedEvent;
use crate::completion::{CompletionCandidate, CompletionKind};
use alan_agent_protocol::UiSkillSnapshot;

#[derive(Clone, Default)]
pub(super) struct SkillProjection {
    pub owner: String,
    revision: u64,
}
impl FileBackedApp {
    /// Revoke event authority as well as rows. Only pinned hydration may restore it.
    pub(super) fn invalidate_skill_owner(&mut self) {
        self.apply_skills(&self.skills.owner.clone(), None);
        self.skills = SkillProjection::default();
    }
    #[cfg(test)]
    pub(super) fn set_skill_candidates(&mut self, skills: Vec<CompletionCandidate>) {
        self.completion_sources.skills = skills;
    }
    pub(super) fn apply_skills(&mut self, owner: &str, snapshot: Option<UiSkillSnapshot>) {
        if self.skills.owner != owner {
            self.skills = SkillProjection {
                owner: owner.into(),
                revision: 0,
            };
        }
        let next = snapshot
            .filter(|s| s.is_valid() && s.process_path == owner.replace("/agent/", "/proc/"));
        if let Some(s) = &next {
            if s.publication_version < self.skills.revision {
                return;
            }
            // Equal-version invalidations clear; equal-version known cannot revive stale choices.
            if s.known && s.publication_version == self.skills.revision {
                return;
            }
            self.skills.revision = s.publication_version;
        }
        self.completion_sources.skills = next
            .filter(|s| s.known)
            .map(|s| {
                s.mentionable_skill_ids
                    .into_iter()
                    .map(|id| CompletionCandidate::new(id, None))
                    .collect()
            })
            .unwrap_or_default();
        if self
            .completion
            .as_ref()
            .is_some_and(|c| c.kind == CompletionKind::Skill)
        {
            self.refresh_completion();
        }
    }
}
pub(super) async fn read_skills(shell: &alan_shell::Shell, owner: &str) -> Option<UiSkillSnapshot> {
    let path = format!("{owner}/machine/ui/skills");
    let text = super::action_detail_io::reference::document_with_budget(shell, &path, 1048576)
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}
#[cfg(test)]
pub(super) async fn dispatch_skill_event(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    event: FileBackedEvent,
) {
    match event {
        FileBackedEvent::SkillsChanged { owner } if owner == app.skills.owner => {
            app.apply_skills(&owner, read_skills(shell, &owner).await)
        }
        FileBackedEvent::SkillsUnavailable { owner } if owner == app.skills.owner => {
            app.apply_skills(&owner, None)
        }
        _ => {}
    }
}
