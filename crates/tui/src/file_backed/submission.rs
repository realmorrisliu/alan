use alan_agent_protocol::{UI_ACTIVITY_VERSION, UiActivitySnapshot};
use anyhow::{Result, ensure};

use super::file_surface;

pub(super) async fn prepare_root_agent_submission(
    shell: &alan_shell::Shell,
    agent_path: &str,
) -> Result<()> {
    let activity = file_surface::read_activity_snapshot(shell, agent_path).await?;
    require_input_identity(&activity)
}

pub(super) fn require_input_identity(activity: &UiActivitySnapshot) -> Result<()> {
    ensure!(
        activity.version == UI_ACTIVITY_VERSION,
        "Root Agent activity protocol does not support input identity; restart the Host before submitting"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_protocol::UiActivityState;

    #[test]
    fn admission_requires_identity_but_accepts_busy_and_paused_queues() {
        for state in [
            UiActivityState::Idle,
            UiActivityState::Running,
            UiActivityState::Paused,
        ] {
            let mut activity = UiActivitySnapshot::idle();
            activity.state = state;
            activity.queue_paused = true;
            assert!(require_input_identity(&activity).is_ok());
            activity.version = 1;
            assert!(
                require_input_identity(&activity)
                    .unwrap_err()
                    .to_string()
                    .contains("restart the Host")
            );
        }
    }
    #[tokio::test]
    async fn interactive_and_redirected_clients_submit_while_another_input_is_running() {
        use super::super::{StdioTaskWaitContext, submit_stdio_task, tail};
        use alan_agent_protocol::{InputIntent, UiSubmission};
        let (shell, _root, _namespace, _pid) = super::super::stdio_tests::live_root_agent().await;
        let mut activity = UiActivitySnapshot::running(1);
        activity.active_submission = Some(UiSubmission {
            submission_id: "foreign-input".into(),
            intent: InputIntent::Agent,
        });
        shell
            .write(
                "/agent/root/machine/ui/activity",
                &serde_json::to_vec(&activity).unwrap(),
            )
            .await
            .unwrap();
        let attachment = tail::prepare_stdio_tail_attachment(&shell, "/agent/root")
            .await
            .unwrap();
        let redirected = StdioTaskWaitContext::new("repeat");
        let tty = async {
            prepare_root_agent_submission(&shell, "/agent/root")
                .await
                .unwrap();
            file_surface::write_agent_submission(
                &shell,
                "/agent/root",
                InputIntent::Agent,
                "repeat",
            )
            .await
            .unwrap()
        };
        let (interactive_id, redirected_result) =
            tokio::join!(tty, submit_stdio_task(&shell, &redirected, &attachment));
        redirected_result.unwrap();
        assert_ne!(interactive_id, redirected.record.submission_id);
        let input = String::from_utf8(shell.cat("/agent/root/io/input").await.unwrap()).unwrap();
        assert_eq!(input.matches(&interactive_id).count(), 1);
        assert_eq!(input.matches(&redirected.record.submission_id).count(), 1);
        tail::close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
            .await
            .unwrap();
    }
}
