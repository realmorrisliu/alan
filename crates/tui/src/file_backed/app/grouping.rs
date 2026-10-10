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

    pub(super) fn history_row_projection(&self, opts: RenderOpts) -> Vec<Vec<Line<'static>>> {
        let mut rows = vec![Vec::new(); self.transcript.len()];
        let mut index = 0;
        while index < self.transcript.len() {
            let end = self.group_end(index);
            if end - index > 1 {
                rows[index].push(crate::history::group_summary_header(
                    end - index,
                    opts.width,
                ));
                for (member, output) in rows.iter_mut().enumerate().take(end).skip(index) {
                    output.push(crate::history::group_summary_member(
                        &self.transcript[member],
                        opts.width,
                    ));
                }
            } else {
                rows[index] = match &self.transcript[index] {
                    HistoryCell::Tool {
                        action: Some(action),
                        ..
                    } if action.frozen_rows.is_some() => {
                        HistoryCell::Styled(action.frozen_rows.clone().unwrap())
                            .render_styled_lines(opts)
                    }
                    HistoryCell::Tool {
                        action: Some(_), ..
                    } => crate::history::action_summary(&self.transcript[index], opts.width),
                    cell if self.action_cells.values().any(|i| *i == index) => {
                        crate::history::action_summary(cell, opts.width)
                    }
                    cell => cell.render_styled_lines(opts),
                };
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
