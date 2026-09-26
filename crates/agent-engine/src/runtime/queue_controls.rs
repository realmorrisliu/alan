//! File-native controls applied to the Machine queue without starting another turn.
use alan_agent_protocol::{Op, Submission};
use tokio_util::sync::CancellationToken;

use super::{transition::NamespaceAgentFiles, turn_input::TurnInputBroker};

pub(super) async fn handle(
    submission: &Submission,
    queue: &TurnInputBroker,
    files: &NamespaceAgentFiles,
    cancel: Option<&CancellationToken>,
) -> bool {
    let result = match &submission.op {
        Op::InterruptSubmission { submission_id } => match queue.interrupt(submission_id) {
            Ok(true) => {
                if let Some(cancel) = cancel {
                    cancel.cancel();
                }
                Ok(())
            }
            Ok(false) => {
                super::ui_surfaces::error_notice(
                    files,
                    "Queued input cancelled before execution",
                    Some(submission_id),
                )
                .await
            }
            Err(error) => Err(error),
        },
        Op::ContinueQueue => queue.continue_queue(),
        Op::DiscardQueue => match queue.discard_queue() {
            Ok(discarded) => {
                for input in discarded {
                    if let Err(error) = super::ui_surfaces::error_notice(
                        files,
                        "Queued input discarded without execution",
                        Some(&input.id),
                    )
                    .await
                    {
                        tracing::warn!(%error, "failed to publish discarded input result");
                    }
                }
                Ok(())
            }
            Err(error) => Err(error),
        },
        _ => return false,
    };
    queue.record_activity();
    let notice = match result {
        Ok(()) if queue.is_paused() => {
            "Input queue paused; continue or discard explicitly".to_owned()
        }
        Ok(()) => "Input queue resumed".to_owned(),
        Err(error) => format!("Queue control rejected: {error}"),
    };
    if let Err(error) = super::ui_surfaces::warning(files, notice).await {
        tracing::warn!(%error, "failed to publish queue control notice");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_machine::AgentMachine;
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use std::sync::Arc;

    #[tokio::test]
    async fn file_controls_pause_active_input_and_discard_later_inputs_with_their_ids() {
        let mut ns = Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let environment =
            super::super::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        let files = environment.agent_files();
        let mut machine = AgentMachine::new();
        let queue = machine.input_broker();
        let active = uuid::Uuid::new_v4().to_string();
        let later = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("must not run")],
            mode: alan_agent_protocol::InputMode::FollowUp,
        });
        let later_id = later.id.clone();
        queue.push_outer_submission(later);
        machine.accept_submission(&active);
        let cancel = CancellationToken::new();
        shell
            .write(
                "/agent/1/machine/ctl",
                format!("queue-v1 interrupt {active}").as_bytes(),
            )
            .await
            .unwrap();
        let control = files
            .read_next_machine_control_submission()
            .await
            .unwrap()
            .unwrap();
        assert!(handle(&control, &queue, &files, Some(&cancel)).await);
        assert!(cancel.is_cancelled());
        assert!(queue.is_paused());
        assert!(queue.pop_outer().is_none());
        machine.reset_turn();
        machine.finish_submission();
        shell
            .write("/agent/1/machine/ctl", b"queue-v1 discard")
            .await
            .unwrap();
        let control = files
            .read_next_machine_control_submission()
            .await
            .unwrap()
            .unwrap();
        assert!(handle(&control, &queue, &files, None).await);
        assert!(queue.pop_outer().is_none());
        assert!(!queue.is_paused());
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert!(events.lines().any(|line| matches!(
            serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
            alan_agent_protocol::UiEvent::Error { submission_id: Some(id), .. } if id == later_id
        )));
    }
}
