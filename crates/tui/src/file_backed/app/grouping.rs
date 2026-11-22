//! Group projections retain individual Action cells and freeze before physical drains.
use super::*;
use crate::history::{ActionHistory, ToolStatus};

impl FileBackedApp {
    pub(in crate::file_backed) fn action_owner(&self) -> &str {
        if self.agent_path == "/agent/root" && !self.queue.owner.is_empty() {
            &self.queue.owner
        } else {
            &self.agent_path
        }
    }

    fn eligible_context(
        &self,
        index: usize,
    ) -> Option<&alan_agent_protocol::ActionReadOnlyContext> {
        match self.transcript.get(index)? {
            HistoryCell::Tool {
                status: ToolStatus::Complete,
                action:
                    Some(ActionHistory {
                        owner,
                        read_only: Some(context),
                        frozen_rows: None,
                        ..
                    }),
                ..
            } if owner == &context.owner => Some(context),
            _ => None,
        }
    }

    fn group_end(&self, start: usize) -> usize {
        let Some(context) = self.eligible_context(start) else {
            return start + 1;
        };
        let mut end = start + 1;
        while self.eligible_context(end) == Some(context) {
            end += 1;
        }
        end
    }

    fn has_trailing_assistant_gap(&self, index: usize) -> bool {
        let unfrozen_tool_or_plan = match self.transcript.get(index) {
            Some(HistoryCell::Tool {
                action: Some(action),
                ..
            }) => action.frozen_rows.is_none(),
            Some(HistoryCell::Tool { action: None, .. } | HistoryCell::Plan { .. }) => true,
            _ => false,
        };
        unfrozen_tool_or_plan
            && matches!(
                self.transcript.get(index + 1),
                Some(HistoryCell::Assistant(_) | HistoryCell::AssistantTail { .. })
            )
    }

    /// Project one standalone transcript cell into styled lines. Shared by both
    /// [`Self::history_row_projection`] and [`Self::prune_rendered_prefix`] so
    /// scrollback pruning can never drift from rendered row counts.
    pub(super) fn single_cell_rows(&self, index: usize, opts: RenderOpts) -> Vec<Line<'static>> {
        let mut cell_rows = match &self.transcript[index] {
            HistoryCell::Tool {
                action: Some(action),
                ..
            } if action.frozen_rows.is_some() => {
                HistoryCell::Styled(action.frozen_rows.clone().unwrap()).render_styled_lines(opts)
            }
            HistoryCell::Tool {
                action: Some(_), ..
            } => crate::history::action_summary_with_theme(
                &self.transcript[index],
                opts.width,
                opts.theme,
            ),
            cell if self.action_cells.values().any(|i| *i == index) => {
                crate::history::action_summary_with_theme(cell, opts.width, opts.theme)
            }
            cell => cell.render_styled_lines(opts),
        };
        if self.has_trailing_assistant_gap(index) {
            cell_rows.push(Line::default());
        }
        cell_rows
    }

    pub(super) fn history_row_projection(&self, opts: RenderOpts) -> Vec<Vec<Line<'static>>> {
        let mut rows = vec![Vec::new(); self.transcript.len()];
        let mut index = 0;
        while index < self.transcript.len() {
            let end = self.group_end(index);
            if end - index > 1 {
                rows[index].push(crate::history::group_summary_header_with_theme(
                    end - index,
                    opts.width,
                    opts.theme,
                ));
                for (member, output) in rows.iter_mut().enumerate().take(end).skip(index) {
                    output.push(crate::history::group_summary_member_with_theme(
                        &self.transcript[member],
                        opts.width,
                        opts.theme,
                    ));
                }
                if self.has_trailing_assistant_gap(end - 1) {
                    rows[end - 1].push(Line::default());
                }
            } else {
                rows[index] = self.single_cell_rows(index, opts);
            }
            index = end;
        }
        rows
    }

    pub(super) fn freeze_committing_actions(&mut self, opts: RenderOpts, count: usize) {
        let rows = self.history_row_projection(opts);
        let mut seen = 0;
        let mut index = 0;
        while index < rows.len() && seen < count {
            let end = self.group_end(index);
            for (member, output) in rows.iter().enumerate().take(end).skip(index) {
                if let HistoryCell::Tool {
                    action: Some(action),
                    ..
                } = &mut self.transcript[member]
                {
                    action.frozen_rows = Some(output.clone());
                    if action.read_only.is_some()
                        && let Some(projection) = self
                            .projected_actions
                            .get_mut(&(action.owner.clone(), action.id.clone()))
                    {
                        projection.1 = true;
                    }
                }
                seen += output.len();
            }
            index = end;
        }
    }
}
