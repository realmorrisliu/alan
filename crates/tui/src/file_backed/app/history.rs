use super::*;

impl FileBackedApp {
    pub(in crate::file_backed) fn upsert_action_cell(
        &mut self,
        action_id: String,
        cell: HistoryCell,
    ) {
        if let Some(index) = self.action_cells.get(&action_id).copied()
            && let Some(existing) = self.transcript.get_mut(index)
        {
            if matches!(
                existing,
                HistoryCell::Styled(_)
                    | HistoryCell::Tool {
                        action: Some(crate::history::ActionHistory {
                            frozen_rows: Some(_),
                            read_only: None,
                            ..
                        }),
                        ..
                    }
            ) {
                return;
            }
            if !matches!(
                existing,
                HistoryCell::Tool {
                    action: Some(crate::history::ActionHistory {
                        frozen_rows: Some(_),
                        ..
                    }),
                    ..
                }
            ) {
                *existing = cell;
                return;
            }
        }
        self.mark_pending_remote_turn_start_if_unbounded();
        let index = self.transcript.len();
        self.transcript.push(cell);
        self.action_cells.insert(action_id, index);
    }

    pub(super) fn shift_action_cells_for_insert(&mut self, inserted_at: usize) {
        for input in self.local_inputs.values_mut() {
            if let Some(index) = &mut input.cell
                && *index >= inserted_at
            {
                *index += 1;
            }
        }
        for index in self.action_cells.values_mut() {
            if *index >= inserted_at {
                *index += 1;
            }
        }
    }
    pub(in crate::file_backed) fn prune_rendered_prefix(
        &mut self,
        opts: RenderOpts,
        lines_to_prune: usize,
    ) -> usize {
        self.freeze_committing_actions(opts, lines_to_prune);
        let mut remaining = lines_to_prune;
        let mut index = 0;
        while remaining > 0 && index < self.transcript.len() {
            let rows = self.single_cell_rows(index, opts);
            let cell_lines = rows.len();
            let keep_source = Some(index) == self.current_assistant_cell();
            for input in self.local_inputs.values_mut() {
                if input.cell == Some(index) && remaining >= cell_lines {
                    input.committed = true;
                    input.release_terminal_source();
                }
            }
            if cell_lines > remaining || keep_source {
                let count = remaining.min(cell_lines);
                if count > 0 {
                    if let HistoryCell::Tool {
                        action: Some(action),
                        ..
                    } = &mut self.transcript[index]
                    {
                        action.frozen_rows = Some(rows.into_iter().skip(count).collect());
                    } else if self.transcript[index].input_source().is_some()
                        || self.transcript[index].assistant_source().is_some()
                    {
                        if !self.transcript[index].trim_rendered_prefix(opts, count) {
                            break;
                        }
                    } else {
                        self.transcript[index] =
                            HistoryCell::Styled(rows.into_iter().skip(count).collect());
                    }
                    if let Some((_, _, cut)) = self.transcript[index].input_source() {
                        for input in self.local_inputs.values_mut() {
                            if input.cell == Some(index) {
                                input.source_cut = cut;
                            }
                        }
                    }
                    remaining -= count;
                    self.scrollback_front_is_partial = true;
                }
                if cell_lines > count {
                    break;
                }
                // A fully committed assistant stays in chronology as an empty
                // source-aware tail. It owns no rows and cannot block later cells.
                index += 1;
            } else {
                self.transcript.remove(index);
                for input in self.local_inputs.values_mut() {
                    if let Some(cell) = &mut input.cell {
                        if *cell == index {
                            input.cell = None;
                        } else if *cell > index {
                            *cell -= 1;
                        }
                    }
                }
                self.action_cells.retain(|_, cell_index| {
                    if *cell_index == index {
                        return false;
                    }
                    if *cell_index > index {
                        *cell_index -= 1;
                    }
                    true
                });
                if let Some(boundary) = &mut self.pending_remote_turn_start
                    && *boundary > index
                {
                    *boundary -= 1;
                }
                if index == 0 {
                    self.scrollback_front_is_partial = false;
                }
                remaining -= cell_lines;
            }
        }
        lines_to_prune - remaining
    }

    pub(in crate::file_backed) fn push_output(&mut self, text: String) {
        match self.reconciler.on_stream(text) {
            StreamAction::Drop => {}
            StreamAction::Append(text) => self.append_to_open_assistant_cell(text),
            StreamAction::StartNew(text) => {
                self.mark_pending_remote_turn_start_if_unbounded();
                self.transcript.push(HistoryCell::Assistant(text));
            }
        }
    }

    pub(in crate::file_backed) fn push_turn_preview_cell(&mut self, cell: HistoryCell) {
        self.mark_pending_remote_turn_start_if_unbounded();
        self.transcript.push(cell);
    }

    pub(in crate::file_backed) fn insert_user_boundary(&mut self, cell: HistoryCell) {
        let index = self
            .pending_remote_turn_start
            .take()
            .unwrap_or(self.transcript.len())
            .min(self.transcript.len());
        self.transcript.insert(index, cell);
        self.shift_action_cells_for_insert(index);
    }

    pub(in crate::file_backed) fn flush_held_stream_after_boundary(&mut self) {
        if let Some(stream) = self.reconciler.take_flushed_stream() {
            self.transcript.push(HistoryCell::Assistant(stream));
        }
    }

    pub(in crate::file_backed) fn append_to_open_assistant_cell(&mut self, text: String) {
        if let Some(index) = self.current_assistant_cell()
            && let Some(
                HistoryCell::Assistant(existing)
                | HistoryCell::AssistantTail { text: existing, .. },
            ) = self.transcript.get_mut(index)
        {
            existing.push_str(&text);
            return;
        }
        self.mark_pending_remote_turn_start_if_unbounded();
        self.transcript.push(HistoryCell::Assistant(text));
    }

    pub(in crate::file_backed) fn mark_pending_remote_turn_start_if_unbounded(&mut self) {
        if self.pending_remote_turn_start.is_some() {
            return;
        }
        if self.reconciler.awaiting_boundary() || !self.current_turn_has_user_boundary() {
            self.pending_remote_turn_start = Some(self.transcript.len());
        }
    }

    pub(in crate::file_backed) fn queued_receipt_at(&self, index: usize) -> bool {
        self.local_inputs
            .values()
            .any(|input| input.cell == Some(index) && !input.tape_seen)
    }

    pub(in crate::file_backed) fn remove_receipt_cell(&mut self, index: usize) {
        self.transcript.remove(index);
        for input in self.local_inputs.values_mut() {
            if let Some(cell) = &mut input.cell {
                if *cell == index {
                    input.cell = None;
                } else if *cell > index {
                    *cell -= 1;
                }
            }
        }
        self.action_cells.retain(|_, cell| {
            if *cell == index {
                return false;
            }
            if *cell > index {
                *cell -= 1;
            }
            true
        });
        if let Some(boundary) = &mut self.pending_remote_turn_start
            && *boundary > index
        {
            *boundary -= 1;
        }
    }

    pub(in crate::file_backed) fn current_turn_has_user_boundary(&self) -> bool {
        if self.active_turn_has_user_boundary {
            return true;
        }
        for (index, cell) in self.transcript.iter().enumerate().rev() {
            if self.queued_receipt_at(index) {
                continue;
            }
            match cell {
                HistoryCell::User(_) | HistoryCell::Command(_) | HistoryCell::InputTail { .. } => {
                    return true;
                }
                HistoryCell::Assistant(_) | HistoryCell::AssistantTail { .. } => return false,
                _ => {}
            }
        }
        false
    }

    /// The index of the current turn's assistant cell: the most recent
    /// `Assistant` cell with no user message or yield after it. Interposed
    /// plan/notice cells are scanned over; a boundary stops the scan.
    pub(in crate::file_backed) fn current_assistant_cell(&self) -> Option<usize> {
        if self.pending_yield.is_some() {
            return None;
        }
        for (idx, cell) in self.transcript.iter().enumerate().rev() {
            if self.queued_receipt_at(idx) {
                continue;
            }
            match cell {
                HistoryCell::Assistant(_) | HistoryCell::AssistantTail { .. } => return Some(idx),
                HistoryCell::User(_)
                | HistoryCell::Command(_)
                | HistoryCell::InputTail { .. }
                | HistoryCell::PendingYield(_)
                | HistoryCell::ConfirmationReceipt { .. }
                | HistoryCell::RoutingReceipt { .. } => {
                    return None;
                }
                _ => {}
            }
        }
        None
    }

    /// Reconcile a live `machine/tape` record with the transcript. All the
    /// matching/suppression/echo logic lives in [`StreamReconciler`]; this
    /// only locates the current-turn cell and applies the returned decision.
    pub(in crate::file_backed) fn apply_tape_record(&mut self, record: TapeRecordV1) {
        self.tape_consumed_offset = self.tape_consumed_offset.max(record.end_offset);
        if record.kind != "message" {
            return;
        }
        match record.role.as_str() {
            "user" => {
                let ids: Vec<_> = self
                    .local_inputs
                    .iter()
                    .filter(|(_, input)| input.owner == self.queue.owner)
                    .map(|(id, _)| id)
                    .filter(|id| record.belongs_to(id))
                    .cloned()
                    .collect();
                let local = !ids.is_empty();
                let duplicate = ids.iter().any(|id| {
                    self.local_inputs
                        .get(id)
                        .is_some_and(|input| input.tape_seen || input.committed)
                });
                if ids.iter().any(|id| {
                    self.local_inputs
                        .get(id)
                        .is_some_and(|input| input.tape_seen)
                }) {
                    return;
                }
                let cut = ids
                    .iter()
                    .filter_map(|id| self.local_inputs.get(id))
                    .map(|input| input.source_cut)
                    .max()
                    .unwrap_or((0, 0));
                let mut cells: Vec<_> = ids
                    .iter()
                    .filter_map(|id| self.local_inputs.get(id).and_then(|input| input.cell))
                    .collect();
                cells.sort_unstable();
                cells.dedup();
                for index in cells.into_iter().rev() {
                    self.remove_receipt_cell(index);
                }
                for id in &ids {
                    if let Some(input) = self.local_inputs.get_mut(id) {
                        input.tape_seen = true;
                    }
                }
                self.reconciler.on_user_record();
                self.clear_late_thinking_latch();
                self.active_turn_has_user_boundary = true;
                self.expected_terminal_error = None;
                if !local || !duplicate {
                    let index = self
                        .pending_remote_turn_start
                        .unwrap_or(self.transcript.len());
                    let is_cmd =
                        record.input_intent == Some(alan_agent_protocol::InputIntent::Command);
                    self.turn_prompts.push((record.content.clone(), is_cmd));
                    self.insert_user_boundary(record.into_user_cell().with_input_cut(cut));
                    for id in &ids {
                        if let Some(input) = self.local_inputs.get_mut(id) {
                            input.cell = Some(index);
                        }
                    }
                }
                for input in self.local_inputs.values_mut() {
                    input.release_terminal_source();
                }
                self.flush_held_stream_after_boundary();
            }
            "assistant" => {
                for (id, input) in &mut self.local_inputs {
                    if input.owner == self.queue.owner
                        && !input.tape_seen
                        && input.body.trim().is_empty()
                        && record.belongs_to(id)
                    {
                        // A correlated assistant record settles any local text-less attachment input
                        // whose user tape line was omitted by the runtime.
                        input.tape_seen = true;
                    }
                }
                let idx = self.current_assistant_cell();
                let preview =
                    idx.and_then(|i| self.transcript[i].assistant_source().map(str::to_string));
                match self
                    .reconciler
                    .on_assistant_record(record.content, preview.as_deref())
                {
                    AssistantDecision::Drop => {}
                    AssistantDecision::ReplacePreview(content) => {
                        if let Some(index) = idx {
                            self.transcript[index].replace_assistant_source(content);
                        }
                    }
                    AssistantDecision::Push(content) => {
                        self.transcript.push(HistoryCell::Assistant(content))
                    }
                }
                let already_has_thinking = self.current_assistant_cell().is_some_and(|i| {
                    i > 0 && matches!(self.transcript[i - 1], HistoryCell::Thinking { .. })
                });
                self.closed_assistant_accepts_late_thinking =
                    !self.ui_turn_closed && !already_has_thinking;
                self.active_turn_has_user_boundary = false;
            }
            _ => {}
        }
    }

    pub(in crate::file_backed) fn push_error(&mut self, message: String) {
        self.transcript.push(HistoryCell::Error(
            super::notice::humanize_protocol_message(&message),
        ));
    }

    pub(in crate::file_backed) fn apply_ui_event(&mut self, event: UiEvent) {
        let paired_notice = matches!(&event, UiEvent::Notice { snapshot }
            if snapshot.kind == UiNoticeKind::Error
                && self.expected_terminal_error.as_deref() == Some(snapshot.message.as_str()));
        let expected_error = if matches!(event, UiEvent::Error { .. }) {
            self.expected_terminal_error.take()
        } else {
            None
        };
        match event {
            UiEvent::InputCompleted { status, .. } => {
                self.last_input_failed = status == UiInputStatus::Failed;
                self.closed_assistant_accepts_late_thinking = false;
                self.ui_turn_closed = true;
                self.active_turn_has_user_boundary = false;
            }
            UiEvent::Activity { snapshot } => self.apply_ui_activity_snapshot(snapshot),
            UiEvent::Plan { snapshot } => self.apply_ui_plan_snapshot(snapshot),
            UiEvent::Thinking { snapshot } => self.apply_ui_thinking_snapshot(snapshot),
            UiEvent::Notice { .. } if paired_notice => {}
            UiEvent::Notice { snapshot } => self.apply_ui_notice_snapshot(snapshot),
            UiEvent::Error { message, .. } => {
                let humanized = super::notice::humanize_protocol_message(&message);
                if self.notice.as_ref().is_some_and(|notice| {
                    notice.runtime_owned
                        && notice.kind == UiNoticeKind::Error
                        && (&**notice == message.as_str() || &**notice == humanized.as_str())
                }) {
                    self.notice = None;
                }
                let matches_expected = expected_error.as_deref().is_some_and(|expected| {
                    expected == message.as_str()
                        || expected.strip_prefix("Error handling submission: ")
                            == Some(message.as_str())
                });
                if !matches_expected {
                    self.push_error(message);
                }
            }
        }
    }

    pub(in crate::file_backed) fn clear_late_thinking_latch(&mut self) {
        self.closed_assistant_accepts_late_thinking = false;
        self.ui_turn_closed = false;
    }

    pub(in crate::file_backed) fn apply_ui_activity_snapshot(
        &mut self,
        snapshot: UiActivitySnapshot,
    ) {
        if snapshot.state == UiActivityState::Idle {
            self.closed_assistant_accepts_late_thinking = false;
            self.active_turn_has_user_boundary = false;
            if self.activity.state != UiActivityState::Idle {
                self.ui_turn_closed = true;
            }
            if matches!(self.thinking.state, UiThinkingState::Streaming) {
                self.thinking = UiThinkingSnapshot::idle();
            }
        }
        self.activity = snapshot;
    }

    pub(in crate::file_backed) fn apply_ui_plan_snapshot(&mut self, snapshot: UiPlanSnapshot) {
        let changed = self.plan != snapshot;
        self.plan = snapshot.clone();
        if changed {
            let owner = if self.queue.owner.is_empty() {
                &self.agent_path
            } else {
                &self.queue.owner
            }
            .clone();
            if !self.plan_owners.contains(&owner) {
                self.plan_owners.push(owner.clone());
            }
            self.plan_revision += 1;
            self.push_turn_preview_cell(HistoryCell::Plan {
                snapshot,
                owner,
                revision: self.plan_revision,
            });
        }
    }

    /// If the current generation's assistant text raced ahead of `machine/ui/events`
    /// (`UiThinkingState::Complete`) on the separate `io/output` tail, return the
    /// index of that unpruned assistant cell so completed thinking inserts above it.
    fn thinking_insert_index(&self) -> Option<usize> {
        let awaiting_boundary = self.reconciler.awaiting_boundary();
        if awaiting_boundary && !self.closed_assistant_accepts_late_thinking {
            return None;
        }
        for (idx, cell) in self.transcript.iter().enumerate().rev() {
            if !awaiting_boundary && self.queued_receipt_at(idx) {
                continue;
            }
            return match cell {
                HistoryCell::Assistant(_)
                | HistoryCell::AssistantTail {
                    committed: (0, 0), ..
                } => Some(idx),
                _ => None,
            };
        }
        None
    }

    pub(in crate::file_backed) fn apply_ui_thinking_snapshot(
        &mut self,
        snapshot: UiThinkingSnapshot,
    ) {
        let changed = self.thinking != snapshot;
        self.thinking = snapshot.clone();
        if changed
            && matches!(snapshot.state, UiThinkingState::Complete)
            && !snapshot.text.trim().is_empty()
        {
            let cell = HistoryCell::Thinking {
                text: snapshot.text,
                duration_secs: snapshot.duration_secs.unwrap_or(0),
            };
            let insert_at = self.thinking_insert_index();
            self.closed_assistant_accepts_late_thinking = false;
            if let Some(index) = insert_at {
                self.transcript.insert(index, cell);
                self.shift_action_cells_for_insert(index);
            } else {
                self.push_turn_preview_cell(cell);
            }
        }
    }

    pub(in crate::file_backed) fn apply_ui_notice_snapshot(&mut self, snapshot: UiNoticeSnapshot) {
        self.notice = Notice::runtime(snapshot);
    }
}
