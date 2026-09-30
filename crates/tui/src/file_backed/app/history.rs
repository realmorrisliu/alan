use super::*;

impl FileBackedApp {
    pub(in crate::file_backed) fn prune_rendered_prefix(
        &mut self,
        opts: RenderOpts,
        lines_to_prune: usize,
    ) -> usize {
        let mut remaining = lines_to_prune;
        let mut index = 0;
        while remaining > 0 && index < self.transcript.len() {
            let is_action = self.action_cells.values().any(|i| *i == index)
                && matches!(self.transcript[index], HistoryCell::Tool { .. });
            let rows = if is_action {
                crate::history::action_summary(&self.transcript[index], opts.width)
            } else {
                self.transcript[index].render_styled_lines(opts)
            };
            let cell_lines = rows.len();
            let keep_source = Some(index) == self.current_assistant_cell();
            if cell_lines > remaining || keep_source {
                let count = remaining.min(cell_lines);
                if count > 0 {
                    if is_action {
                        self.transcript[index] =
                            HistoryCell::Styled(rows.into_iter().skip(count).collect());
                    } else if !self.transcript[index].trim_rendered_prefix(opts, count) {
                        break;
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

    pub(in crate::file_backed) fn current_turn_has_user_boundary(&self) -> bool {
        for cell in self.transcript.iter().rev() {
            match cell {
                HistoryCell::User(_) | HistoryCell::Command(_) => return true,
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
        for (idx, cell) in self.transcript.iter().enumerate().rev() {
            match cell {
                HistoryCell::Assistant(_) | HistoryCell::AssistantTail { .. } => return Some(idx),
                HistoryCell::User(_) | HistoryCell::Command(_) | HistoryCell::PendingYield(_) => {
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
                self.reconciler.on_user_record();
                self.insert_user_boundary(record.into_user_cell());
                self.flush_held_stream_after_boundary();
            }
            "assistant" => {
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
            }
            _ => {}
        }
    }

    pub(in crate::file_backed) fn push_error(&mut self, message: String) {
        self.transcript.push(HistoryCell::Error(message));
    }

    pub(in crate::file_backed) fn apply_ui_event(&mut self, event: UiEvent) {
        let paired_notice = matches!(&event, UiEvent::Notice { snapshot }
            if snapshot.kind == UiNoticeKind::Error
                && self.expected_terminal_error.as_deref() == Some(snapshot.message.as_str()));
        let expected_error = if matches!(event, UiEvent::InputCompleted { .. }) || paired_notice {
            None
        } else {
            self.expected_terminal_error.take()
        };
        match event {
            UiEvent::InputCompleted { status, .. } => {
                self.last_input_failed = status == UiInputStatus::Failed;
            }
            UiEvent::Activity { snapshot } => self.apply_ui_activity_snapshot(snapshot),
            UiEvent::Plan { snapshot } => self.apply_ui_plan_snapshot(snapshot),
            UiEvent::Thinking { snapshot } => self.apply_ui_thinking_snapshot(snapshot),
            UiEvent::Notice { snapshot } => self.apply_ui_notice_snapshot(snapshot),
            UiEvent::Error { message, .. } => {
                if expected_error.as_deref() != Some(message.as_str()) {
                    self.push_error(message);
                }
            }
        }
    }

    pub(in crate::file_backed) fn apply_ui_activity_snapshot(
        &mut self,
        snapshot: UiActivitySnapshot,
    ) {
        self.activity = snapshot;
    }

    pub(in crate::file_backed) fn apply_ui_plan_snapshot(&mut self, snapshot: UiPlanSnapshot) {
        let changed = self.plan != snapshot;
        self.plan = snapshot.clone();
        if changed && !snapshot.items.is_empty() {
            self.push_turn_preview_cell(HistoryCell::Plan(
                snapshot
                    .items
                    .into_iter()
                    .map(|item| crate::history::PlanLine {
                        status: item.status,
                        content: item.content,
                    })
                    .collect(),
            ));
        }
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
            self.push_turn_preview_cell(HistoryCell::Thinking {
                text: snapshot.text,
                duration_secs: snapshot.duration_secs.unwrap_or(0),
            });
        }
    }

    pub(in crate::file_backed) fn apply_ui_notice_snapshot(&mut self, snapshot: UiNoticeSnapshot) {
        self.notice = match snapshot.kind {
            UiNoticeKind::None => None,
            _ if snapshot.message.trim().is_empty() => None,
            _ => Some(snapshot.message),
        };
    }
}
