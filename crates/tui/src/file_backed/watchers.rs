use super::*;

pub(super) struct AgentWatchers {
    pub(super) recovery: Option<(alan_shell::Tail, alan_shell::Tail)>,
    shutdown: tokio::sync::watch::Sender<bool>,
    tasks: Vec<tokio::task::JoinHandle<Result<()>>>,
    pub(super) root_agent_pid: Option<u64>,
    pub(super) pid_refresh_failed: bool,
    pub(super) pending_root_agent_pid_result: Option<Result<Option<u64>, String>>,
    pub(super) pending_terminal_events: VecDeque<FileBackedEvent>,
}

impl AgentWatchers {
    pub(super) fn start(
        tails: file_surface::WatchTails,
        agent_path: &str,
        tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    ) -> Self {
        let root_agent_pid = tails.root_agent_pid;
        let action_agent_path = root_agent_pid
            .and_then(|pid| root_agent_path_for_pid(agent_path, pid))
            .unwrap_or_else(|| agent_path.to_string());
        let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
        let tasks = vec![
            tokio::spawn(spawn_output_tail(
                tails.output,
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_request_watch(
                tails.requests,
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(queue::watch_queue(
                tails.queue_events,
                action_agent_path.clone(),
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_action_watch(
                tails.actions,
                action_agent_path.clone(),
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_ui_watch(
                tails.ui,
                action_agent_path,
                tx.clone(),
                shutdown_rx.clone(),
            )),
            tokio::spawn(spawn_tape_watch(tails.tape, tx, shutdown_rx)),
        ];
        Self {
            recovery: Some((tails.recovery_ui, tails.recovery_tape)),
            shutdown,
            tasks,
            root_agent_pid,
            pid_refresh_failed: false,
            pending_root_agent_pid_result: None,
            pending_terminal_events: VecDeque::new(),
        }
    }

    pub(super) async fn refresh_root_agent_attachment(
        &mut self,
        shell: &alan_shell::Shell,
        agent_path: &str,
        app: &mut FileBackedApp,
        rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
        pending_turns: &mut VecDeque<PendingRootAgentTurn>,
        tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    ) -> bool {
        let current_pid = match self.pending_root_agent_pid_result.take() {
            Some(result) => result,
            None => current_root_agent_pid(shell)
                .await
                .map_err(|error| format!("{error:#}")),
        };
        match current_pid {
            Ok(Some(pid))
                if self.root_agent_pid == Some(pid)
                    && (app.skills.owner.is_empty() || app.model.owner.is_empty()) =>
            {
                // Repin revoked observations only after descriptor hydration and identity recheck.
                let owner = format!("/agent/{pid}");
                let snapshot = skills::read_skills(shell, &owner).await;
                let model_snapshot = model::read_model(shell, &owner).await;
                if current_root_agent_pid(shell).await.ok().flatten() == Some(pid) {
                    app.apply_skills(&owner, snapshot);
                    app.model.apply(&owner, model_snapshot);
                    app.reconcile_model_chooser();
                    self.pid_refresh_failed = false;
                }
                false
            }
            Ok(Some(pid)) if self.root_agent_pid != Some(pid) => {
                app.invalidate_skill_owner();
                app.invalidate_model_owner();
                if app.pending_project_control.is_some() {
                    app.fail_project_control(
                        "Root owner changed; project control invalidated, effects uncertain".into(),
                    );
                }
                let pending_count = pending_turns.len();
                // Tape history outlives the local pending-input lock.
                let history_restored = if let Some((_, tape)) = &self.recovery {
                    previous_input::restore_tape_history(app, tape, app.tape_consumed_offset).await
                } else {
                    false
                };
                let submitted = pending_turns
                    .iter()
                    .map(|turn| (turn.submission_id.clone(), turn.input.clone()))
                    .collect::<Vec<_>>();
                let retained = self.stop_for_input(&submitted).await;
                app.expected_terminal_error = None;
                let queued = discard_superseded_attachment_events(
                    rx,
                    &mut self.pending_terminal_events,
                    &submitted,
                );
                for event in retained.0.iter().chain(&queued.0) {
                    interrupt::observe_root_agent_completion(pending_turns, event, app);
                }
                // The detached Process's companion terminal errors are discarded with its events.
                app.expected_terminal_error = None;
                if !history_restored {
                    for turn in &submitted {
                        if let Some((_, input, answer)) = retained
                            .1
                            .iter()
                            .chain(&queued.1)
                            .find(|(id, _, _)| id.as_str() == turn.0.as_str())
                        {
                            previous_input::restore_answer(app, input, answer.clone());
                        }
                    }
                }
                let submitted = pending_turns.iter().cloned().collect::<Vec<_>>();
                match reattach_to_current_agent(shell, agent_path, app, &submitted).await {
                    Ok((tails, settled_ids)) => {
                        pending_turns.retain(|turn| !settled_ids.contains(&turn.submission_id));
                        self.pid_refresh_failed = false;
                        let pending_terminal_events =
                            std::mem::take(&mut self.pending_terminal_events);
                        *self = Self::start(tails, agent_path, tx.clone());
                        self.pending_terminal_events = pending_terminal_events;
                        pending_turns.len() < pending_count
                    }
                    Err(err) => {
                        let owner = app.queue.owner.clone();
                        app.queue.apply(&owner, None);
                        self.root_agent_pid = None;
                        if !self.pid_refresh_failed {
                            app.push_error(format!("Root Agent reattach failed: {err:#}"));
                        }
                        self.pid_refresh_failed = true;
                        pending_turns.len() < pending_count
                    }
                }
            }
            Ok(pid) => {
                if pid.is_none() {
                    app.invalidate_skill_owner();
                    app.invalidate_model_owner();
                }
                self.root_agent_pid = pid;
                self.pid_refresh_failed = false;
                false
            }
            Err(err) if !self.pid_refresh_failed => {
                app.invalidate_skill_owner();
                app.invalidate_model_owner();
                self.pid_refresh_failed = true;
                app.push_error(format!("Root Agent identity refresh failed: {err}"));
                false
            }
            Err(_) => false,
        }
    }

    pub(super) async fn stop(&mut self) {
        self.stop_for_input(&[]).await;
    }

    pub(super) async fn stop_for_input(
        &mut self,
        submitted: &[(String, String)],
    ) -> (Vec<UiEvent>, Vec<(String, String, String)>) {
        let _ = self.shutdown.send(true);
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
        let Some((ui, tape)) = self.recovery.take() else {
            return (Vec::new(), Vec::new());
        };
        let outcome = previous_input::snapshot(&ui, &tape, submitted).await;
        let _ = ui.close().await;
        let _ = tape.close().await;
        outcome
    }
}
