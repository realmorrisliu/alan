//! Retain directory outcome evidence without repeating its cwd effect.
use super::super::transition::{
    NamespaceActionRecord,
    directory_control::{cd_action_record, select_project_directory},
};
use super::*;

impl RuntimeSubmissionQueues {
    fn retain_directory_action(&mut self, id: &str, record: NamespaceActionRecord) {
        if !self
            .pending_directory_actions
            .iter()
            .any(|(existing, _)| existing == id)
        {
            self.pending_directory_actions.push_back((
                id.into(),
                super::super::transition::PendingActionPublication::new(record),
            ));
        }
    }

    pub(super) async fn handle_directory_selection(
        &mut self,
        state: &mut RuntimeLoopState,
        submission: &Submission,
    ) -> bool {
        if !matches!(
            submission.op,
            alan_agent_protocol::Op::SelectProjectDirectory { .. }
        ) {
            return false;
        }
        if !self
            .pending_directory_actions
            .iter()
            .any(|(id, _)| id == &submission.id)
        {
            let result = if self.pending_directory_actions.is_empty() {
                select_project_directory(state, submission)
            } else {
                cd_action_record(
                    &submission.id,
                    "Select Process directory",
                    Err(anyhow::anyhow!(
                        "previous directory outcome is not yet published"
                    )),
                    state.process_path(),
                )
            };
            match result {
                Ok(record) => self.retain_directory_action(&submission.id, record),
                Err(error) => tracing::error!(%error, "Invalid directory control owner"),
            }
        }
        self.publish_directory_actions(&state.agent_files()).await;
        true
    }

    pub(super) async fn reject_active_directory(
        &mut self,
        files: &NamespaceAgentFiles,
        submission: &Submission,
    ) {
        // This Process path was validated before Runtime Ready; rejection has no cwd effect.
        if let Ok(record) = cd_action_record(
            &submission.id,
            "Select Process directory",
            Err(anyhow::anyhow!("directory selection requires settled work")),
            self.model_process_path.clone(),
        ) {
            self.retain_directory_action(&submission.id, record);
            self.publish_directory_actions(files).await;
        }
    }

    pub(super) async fn publish_directory_actions(&mut self, files: &NamespaceAgentFiles) {
        if std::time::Instant::now() < self.directory_publication_retry {
            return;
        }
        while let Some((id, pending)) = self.pending_directory_actions.front_mut() {
            if let Err(error) = files.publish_action(pending).await {
                tracing::warn!(%error, submission_id=%id, "Directory result retained for publication retry");
                self.directory_publication_retry = std::time::Instant::now()
                    + crate::runtime::turn_input::NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL;
                break;
            }
            self.pending_directory_actions.pop_front();
        }
    }
}
