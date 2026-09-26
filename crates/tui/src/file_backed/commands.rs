//! Renderer commands only write existing Machine controls; the Machine owns execution.
use super::app::{FileBackedAction, FileBackedApp};
use crate::completion::CompletionCandidate;

pub(super) fn default_commands() -> Vec<CompletionCandidate> {
    [
        ("compact", "summarize context"),
        ("rollback", "rewind the last turn context"),
        ("clear", "clear the transcript"),
        ("help", "show key bindings"),
        ("quit", "exit alan"),
        ("continue", "run paused queued inputs"),
        ("discard", "discard paused queued inputs"),
    ]
    .into_iter()
    .map(|(value, detail)| CompletionCandidate::new(value, Some(detail.to_string())))
    .collect()
}

impl FileBackedApp {
    pub(super) fn handle_command(&mut self, text: &str) -> Option<FileBackedAction> {
        let command = text.strip_prefix('/')?;
        let name = command.split_whitespace().next().unwrap_or("");
        match name {
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
            "continue" | "discard" => {
                if command.split_whitespace().count() != 1 {
                    self.notice = Some(format!("usage: /{name}"));
                    return None;
                }
                Some(FileBackedAction::MachineCtl {
                    command: format!("queue-v1 {name}"),
                    success_notice: format!("queue {name} requested"),
                })
            }
            "clear" => {
                self.transcript.clear();
                self.action_cells.clear();
                self.tape_user_cells.clear();
                self.pending_command_actions.clear();
                self.pending_remote_turn_start = None;
                self.scrollback_front_is_partial = false;
                None
            }
            "help" => {
                self.notice = Some(
                    "/continue /discard paused queue · /compact /rollback /clear /quit · ctrl+r toggle thinking · ctrl+c/esc interrupt"
                        .to_string(),
                );
                None
            }
            _ => {
                self.notice = Some(format!("unknown command: /{name}"));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_backed::file_surface::write_machine_ctl;
    use crate::history::PendingYieldCell;
    use alan_agent_protocol::YieldKind;

    #[tokio::test]
    async fn queue_commands_write_controls_without_submitting_or_claiming_completion() {
        let (shell, _root, _namespace, _pid) =
            crate::file_backed::stdio_tests::live_root_agent().await;
        let mut app = FileBackedApp::new("/agent/root".into());
        app.activity.queue_paused = true;
        for verb in ["continue", "discard"] {
            assert!(default_commands().iter().any(|c| c.value == verb));
            app.composer.set_text(format!("/{verb} extra"));
            assert!(app.handle_submit().is_none());
            assert_eq!(
                app.notice.as_deref(),
                Some(format!("usage: /{verb}").as_str())
            );
            app.composer.set_text(format!("/{verb}"));
            assert!(!app.enter_submits_agent_task());
            let Some(FileBackedAction::MachineCtl {
                command,
                success_notice,
            }) = app.handle_submit()
            else {
                panic!("queue command did not reach Machine controls");
            };
            assert_eq!(command, format!("queue-v1 {verb}"));
            assert_eq!(success_notice, format!("queue {verb} requested"));
            write_machine_ctl(&shell, &app.agent_path, &command)
                .await
                .unwrap();
            assert!(
                app.activity.queue_paused,
                "only runtime evidence changes queue state"
            );
        }
        let events = String::from_utf8(shell.cat("/agent/root/events").await.unwrap()).unwrap();
        assert!(events.contains("ctl:queue-v1 continue"));
        assert!(events.contains("ctl:queue-v1 discard"));
        assert!(shell.cat("/agent/root/io/input").await.unwrap().is_empty());
        assert!(app.transcript.is_empty());
    }

    #[test]
    fn pending_responses_and_explicit_intents_take_precedence_over_queue_commands() {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.set_pending_yield(PendingYieldCell {
            request_id: "request".into(),
            kind: YieldKind::StructuredInput,
            title: "Reply".into(),
            prompt: None,
            options: Vec::new(),
            default_option: None,
            questions: Vec::new(),
            capability: None,
            reason: None,
            presentation: None,
        });
        app.composer.set_text("/discard");
        assert!(
            matches!(app.handle_submit(), Some(FileBackedAction::Resume { response, .. }) if response == "/discard")
        );
        app.pending_yield = None;
        for prefix in ['!', ':'] {
            app.composer.set_text("");
            app.composer.insert_text(&format!("{prefix}/continue"));
            assert!(matches!(
                app.handle_submit(),
                Some(FileBackedAction::Submit(_))
            ));
        }
    }
}
