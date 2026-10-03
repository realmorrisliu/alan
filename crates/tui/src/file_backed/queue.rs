//! Consumer of the existing payload-free Machine projection; never queue authority.
use super::{FileBackedApp, FileBackedEvent};
use alan_agent_protocol::UiQueueSnapshot;

#[derive(Clone, Default)]
pub(super) struct QueueProjection {
    pub owner: String,
    pub snapshot: Option<UiQueueSnapshot>,
    revision: u64,
    hint: Option<(String, String)>,
}

impl QueueProjection {
    pub fn apply(&mut self, owner: &str, snapshot: Option<UiQueueSnapshot>) {
        if self.owner != owner {
            self.owner = owner.into();
            self.revision = 0;
            self.snapshot = None;
            self.hint = None;
        }
        let snapshot = snapshot.filter(UiQueueSnapshot::is_valid);
        let Some(next) = snapshot.as_ref() else {
            self.snapshot = None;
            return;
        };
        if next.known && next.revision < self.revision {
            return;
        }
        if next.known {
            self.revision = next.revision;
        }
        self.snapshot = snapshot;
    }

    // Valid unknown is not known-empty. It permits only a guarded control
    // request; Runtime remains the final admission/cwd authority. Read failures
    // and malformed documents never provide this authorization boundary.
    pub fn project_safe(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|q| {
            (!q.known && q.revision == 0 && self.revision == 0)
                || (q.known
                    && q.active_submission_ids.is_empty()
                    && q.uncertain_submission_ids.is_empty()
                    && !q.deferred
                    && (q.pending_submission_ids.is_empty() || q.paused))
        })
    }

    pub fn label(&self) -> String {
        match &self.snapshot {
            None => "queue unknown".into(),
            Some(q) if !q.known => "queue unknown".into(),
            Some(q) => format!(
                "{}queued {}{}{}{}",
                if q.paused { "paused · " } else { "" },
                q.pending_submission_ids.len(),
                if q.active_submission_ids.is_empty() {
                    ""
                } else {
                    " · active"
                },
                if q.deferred { " · deferred" } else { "" },
                if q.uncertain_submission_ids.is_empty() {
                    ""
                } else {
                    " · uncertain"
                },
            ),
        }
    }
}

pub(super) async fn read_queue(shell: &alan_shell::Shell, owner: &str) -> Option<UiQueueSnapshot> {
    let bytes = shell.cat(&format!("{owner}/machine/ui/queue")).await.ok()?;
    serde_json::from_slice::<UiQueueSnapshot>(&bytes)
        .ok()
        .filter(UiQueueSnapshot::is_valid)
}

#[derive(Clone)]
pub(super) struct LocalInput {
    pub owner: String,
    pub body: String,
    intent: alan_agent_protocol::InputIntent,
    draft_revision: u64,
    pub acknowledged: bool,
    pub cell: Option<usize>,
    pub tape_seen: bool,
    pub committed: bool,
    pub source_cut: (usize, usize),
    pub terminal: bool,
}

impl LocalInput {
    pub(super) fn release_terminal_source(&mut self) {
        if self.terminal && (self.cell.is_some() || self.committed) {
            self.body = String::new();
        }
    }
}

impl FileBackedApp {
    pub(super) fn clear_local_receipts(&mut self) {
        for input in self.local_inputs.values_mut() {
            if input.acknowledged || input.cell.is_some() {
                input.committed = true;
            }
            input.cell = None;
            input.release_terminal_source();
        }
    }

    pub(super) fn restore_receipts_after_hydration(&mut self, raw: &str) {
        let records: Vec<super::TapeRecordV1> = raw
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        for input in self.local_inputs.values_mut() {
            input.cell = None;
        }
        let mut index = 0;
        let mut previous_assistant = false;
        for record in &records {
            if record.kind != "message" {
                continue;
            }
            match record.role.as_str() {
                "user" => {
                    let matches: Vec<_> = self
                        .local_inputs
                        .iter()
                        .filter(|(id, input)| {
                            input.owner == self.queue.owner && record.belongs_to(id)
                        })
                        .map(|(id, _)| id.clone())
                        .collect();
                    for id in matches {
                        let input = self.local_inputs.get_mut(&id).expect("matched local input");
                        input.tape_seen = true;
                        input.cell = Some(index);
                        if let Some(cell) = self.transcript.get_mut(index) {
                            if input.committed {
                                *cell = crate::history::HistoryCell::Styled(Vec::new());
                            } else {
                                *cell = cell.clone().with_input_cut(input.source_cut);
                            }
                        }
                    }
                    index += 1;
                    previous_assistant = false;
                }
                "assistant" => {
                    if !previous_assistant {
                        index += 1;
                    }
                    previous_assistant = true;
                }
                _ => {}
            }
        }
        let ids: Vec<_> = self
            .queue
            .snapshot
            .as_ref()
            .filter(|q| q.known)
            .map(|q| {
                q.active_submission_ids
                    .iter()
                    .chain(&q.pending_submission_ids)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            let Some(input) = self.local_inputs.get_mut(&id) else {
                continue;
            };
            if input.owner != self.queue.owner || input.terminal {
                continue;
            }
            input.tape_seen = records.iter().any(|record| {
                record.kind == "message" && record.role == "user" && record.belongs_to(&id)
            });
            if input.acknowledged && !input.tape_seen && !input.committed && input.cell.is_none() {
                input.cell = Some(self.transcript.len());
                self.transcript.push(
                    (if input.intent == alan_agent_protocol::InputIntent::Command {
                        crate::history::HistoryCell::Command(input.body.clone())
                    } else {
                        crate::history::HistoryCell::User(input.body.clone())
                    })
                    .with_input_cut(input.source_cut),
                );
            }
        }
        // Exact-ID receipts remain available for repeated late Tape/hydration.
        for input in self.local_inputs.values_mut() {
            input.release_terminal_source();
        }
    }

    pub(super) fn track_local_input(
        &mut self,
        id: &str,
        owner: String,
        body: String,
        intent: alan_agent_protocol::InputIntent,
    ) {
        self.local_inputs.insert(
            id.into(),
            LocalInput {
                owner,
                body,
                intent,
                draft_revision: self.composer.draft_revision(),
                acknowledged: false,
                cell: None,
                tape_seen: false,
                committed: false,
                source_cut: (0, 0),
                terminal: false,
            },
        );
    }

    pub(super) fn refresh_local_input_hint(&mut self, id: &str, owner: &str) {
        let Some(input) = self
            .local_inputs
            .get(id)
            .filter(|input| input.owner == owner && !input.terminal)
        else {
            return;
        };
        let acknowledged = input.acknowledged;
        let state = match self.queue.snapshot.as_ref().filter(|q| q.known) {
            Some(q)
                if q.uncertain_submission_ids
                    .iter()
                    .any(|candidate| candidate == id) =>
            {
                "uncertain · outcome unknown"
            }
            Some(q)
                if q.active_submission_ids
                    .iter()
                    .any(|candidate| candidate == id) =>
            {
                "active · admitted to queue"
            }
            Some(q)
                if q.pending_submission_ids
                    .iter()
                    .any(|candidate| candidate == id)
                    && q.paused =>
            {
                "paused · /continue resumes · /discard removes queued input"
            }
            Some(q)
                if q.pending_submission_ids
                    .iter()
                    .any(|candidate| candidate == id) =>
            {
                "queued"
            }
            _ => "unconfirmed · outcome unknown",
        };
        let notice = format!(
            "input {id} {} · {state}",
            if acknowledged {
                "acknowledged"
            } else {
                "unconfirmed"
            }
        );
        self.queue.hint = Some((id.into(), notice.clone()));
        self.notice = Some(notice);
    }

    pub(super) fn refresh_queue_hint(&mut self) {
        let Some((id, notice)) = self.queue.hint.clone() else {
            return;
        };
        if self.notice.as_ref() != Some(&notice) {
            return;
        }
        let owner = self.queue.owner.clone();
        let live = self
            .local_inputs
            .get(&id)
            .is_some_and(|input| input.owner == owner && !input.terminal);
        if !live {
            let notice = self.queue.label();
            self.notice = Some(notice.clone());
            // Keep ownership of this label so a later queue observation can refresh it too.
            self.queue.hint = Some((id, notice));
        } else {
            self.refresh_local_input_hint(&id, &owner);
        }
    }

    pub(super) fn acknowledge_local_input(&mut self, id: &str, owner: &str) {
        let Some(input) = self.local_inputs.get_mut(id) else {
            return;
        };
        if input.owner != owner || input.acknowledged {
            return;
        }
        input.acknowledged = true;
        let body = input.body.clone();
        let intent = input.intent;
        let consume = self.composer.draft_revision() == input.draft_revision
            && self.composer.text() == body
            && self.input_intent == intent;
        if !input.tape_seen {
            input.cell = Some(self.transcript.len());
            self.transcript
                .push(if intent == alan_agent_protocol::InputIntent::Command {
                    crate::history::HistoryCell::Command(body.clone())
                } else {
                    crate::history::HistoryCell::User(body.clone())
                });
        }
        if consume {
            self.accept_input();
        } else {
            self.composer.remember_input(&body, intent);
        }
        self.refresh_local_input_hint(id, owner);
    }
}

pub(super) async fn dispatch_queue_event(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    pending: &std::collections::VecDeque<super::interrupt::PendingRootAgentTurn>,
    event: FileBackedEvent,
) {
    match event {
        FileBackedEvent::QueueChanged { owner } if owner == app.queue.owner => {
            app.queue.apply(&owner, read_queue(shell, &owner).await);
            app.refresh_queue_hint();
            confirm_local_receipts(app, pending);
        }
        FileBackedEvent::QueueUnavailable { owner } if owner == app.queue.owner => {
            app.queue.apply(&owner, None);
            app.refresh_queue_hint();
        }
        _ => {}
    }
}

pub(super) fn confirm_local_receipts(
    app: &mut FileBackedApp,
    pending: &std::collections::VecDeque<super::interrupt::PendingRootAgentTurn>,
) {
    let owner = app.queue.owner.clone();
    for turn in pending {
        app.refresh_local_input_hint(&turn.submission_id, &owner);
    }
    let Some(q) = app.queue.snapshot.as_ref().filter(|q| q.known) else {
        return;
    };
    let ids: Vec<_> = pending
        .iter()
        .filter(|turn| {
            app.queue.owner
                == turn
                    .submitted_process
                    .map_or_else(|| app.agent_path.clone(), |pid| format!("/agent/{pid}"))
                && (q.pending_submission_ids.contains(&turn.submission_id)
                    || q.active_submission_ids.contains(&turn.submission_id))
        })
        .map(|turn| turn.submission_id.clone())
        .collect();
    let owner = app.queue.owner.clone();
    for id in ids {
        app.acknowledge_local_input(&id, &owner);
    }
}

/// Subscribe before snapshot read. The existing tail includes earlier bytes;
/// harmless repeated refreshes are revision-checked by the consumer.
pub(super) async fn watch_queue(
    mut tail: alan_shell::Tail,
    owner: String,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut pending = Vec::new();
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { break; }
            }
            result = tail.read(4096) => {
                let Ok(bytes) = result else {
                    let _ = super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::SkillsUnavailable { owner: owner.clone() }).await;
                    let _ = super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::ModelUnavailable { owner: owner.clone() }).await;
                    if !super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::QueueUnavailable { owner: owner.clone() }).await { break; }
                    break;
                };
                if bytes.is_empty() {
                    let _ = super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::SkillsUnavailable { owner: owner.clone() }).await;
                    let _ = super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::ModelUnavailable { owner: owner.clone() }).await;
                    let _ = super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::QueueUnavailable { owner: owner.clone() }).await;
                    break;
                }
                pending.extend(bytes);
                while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                    let line: Vec<_> = pending.drain(..=end).collect();
                    if line == b"ui:skills\n" && !super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::SkillsChanged { owner: owner.clone() }).await {
                        let _ = tail.close().await;
                        return Ok(());
                    }
                    if line == b"ui:models\n" && !super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::ModelChanged { owner: owner.clone() }).await {
                        let _ = tail.close().await;
                        return Ok(());
                    }
                    if line == b"ui:queue\n" && !super::file_surface::send_event_or_shutdown(&tx, &mut shutdown, FileBackedEvent::QueueChanged { owner: owner.clone() }).await {
                        let _ = tail.close().await;
                        return Ok(());
                    }
                }
                // Agent event records are bounded tokens; never retain arbitrary payloads.
                if pending.len() > 4096 { pending.clear(); }
            }
        }
    }
    let _ = tail.close().await;
    Ok(())
}
