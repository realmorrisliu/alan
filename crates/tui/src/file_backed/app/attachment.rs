//! Root attachment lifecycle and stream replay reset.
use super::*;
impl FileBackedApp {
    pub(in crate::file_backed) fn seed_reconciler_from_tape_history(&mut self, raw: &str) {
        self.reconciler = StreamReconciler::new();
        self.pending_remote_turn_start = None;
        for line in raw.lines() {
            let Ok(record) = serde_json::from_str::<TapeRecordV1>(line) else {
                continue;
            };
            if record.kind == "message" {
                self.reconciler.on_hydrated_message_record(&record.role);
            }
        }
    }
    pub(in crate::file_backed) fn reset_for_root_process_change(&mut self) {
        self.lose_model_receipts(&self.model.owner.clone());
        self.model_chooser.active = false;
        self.model_chooser.explicit = false;
        self.model_chooser.catalog = None;
        self.model = super::super::model::ModelProjection::default();
        self.skills = super::super::skills::SkillProjection::default();
        self.completion_sources.skills.clear();
        self.queue = super::super::queue::QueueProjection::default();
        let mut project_notice = if self.retained_project_grant().is_some() {
            self.fence_project_control();
            self.notice.take()
        } else {
            None
        };
        if self.uncertain_project_mount.is_some() {
            let unknown = "project outcome unknown; detached Root; new selections blocked; retry only with the original Root or /quit exits";
            project_notice = Some(match project_notice {
                Some(notice) => format!("{notice}; {unknown}"),
                None => unknown.into(),
            });
        }
        self.tape_consumed_offset = 0;
        self.action_cells.clear();
        self.modal.active = false;
        self.modal.generation += 1;
        self.modal.rows.clear();
        self.modal.ids.clear();
        self.modal.plans.clear();
        self.modal.plan_mode = false;
        self.activity = UiActivitySnapshot::idle();
        self.plan = UiPlanSnapshot::empty();
        self.plan_revision = 0;
        self.thinking = UiThinkingSnapshot::idle();
        self.running_tools.clear();
        self.pending_yield = None;
        self.form = None;
        self.completion = None;
        let model_notice: Option<String> = self
            .model_chooser
            .uncertain
            .as_ref()
            .map(|_| "model selection outcome uncertain; detached Root; no retry".into());
        self.notice = match (project_notice, model_notice) {
            (Some(project), Some(model)) => Some(format!("{project}; {model}")),
            (project, model) => project.or(model),
        };
        self.reconciler = StreamReconciler::new();
        self.pending_remote_turn_start = None;
    }
}
