use alan_agent::AgentWorkspaceSnapshots;
use alan_kernel::{
    ConversationBlock, ConversationBlockKind, TaskTreeNode, ViewModel, ViewSnapshot,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use serde_json::Value;

use crate::history::RenderOpts;

/// Render Agent Workspace semantic snapshots into the current terminal transcript shape.
#[must_use]
pub fn render_agent_workspace_snapshots(
    snapshots: &AgentWorkspaceSnapshots,
    opts: RenderOpts,
) -> Vec<String> {
    render_agent_workspace_snapshot_lines(snapshots, opts)
        .into_iter()
        .map(line_to_plain_text)
        .collect()
}

/// Render Agent Workspace semantic snapshots into Ratatui lines.
#[must_use]
pub fn render_agent_workspace_snapshot_lines(
    snapshots: &AgentWorkspaceSnapshots,
    opts: RenderOpts,
) -> Vec<Line<'static>> {
    render_agent_workspace_snapshot_lines_from(snapshots, opts, 0)
}

/// Render Agent Workspace semantic snapshots after skipping host-scrolled conversation blocks.
#[must_use]
pub fn render_agent_workspace_snapshot_lines_from(
    snapshots: &AgentWorkspaceSnapshots,
    opts: RenderOpts,
    conversation_blocks_to_skip: usize,
) -> Vec<Line<'static>> {
    let mut lines =
        render_conversation_from(&snapshots.conversation, opts, conversation_blocks_to_skip);
    let task_lines = render_snapshot_lines(&snapshots.task_tree, opts);
    if !task_lines.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.extend(task_lines);
    }
    lines
}

/// Build a Ratatui paragraph for the first Agent Workspace transcript slice.
#[must_use]
pub fn agent_workspace_paragraph(
    snapshots: &AgentWorkspaceSnapshots,
    opts: RenderOpts,
) -> Paragraph<'static> {
    Paragraph::new(render_agent_workspace_snapshot_lines(snapshots, opts))
        .wrap(Wrap { trim: false })
}

/// Render one typed Alan Kernel view snapshot into terminal host lines.
#[must_use]
pub fn render_snapshot_lines(snapshot: &ViewSnapshot, opts: RenderOpts) -> Vec<Line<'static>> {
    match &snapshot.model {
        ViewModel::Conversation(_) => render_conversation(snapshot, opts),
        ViewModel::TaskTree(_) => render_task_tree(snapshot, opts),
        ViewModel::Form(_) => render_form(snapshot, opts),
        ViewModel::CommandPalette(_) => render_command_palette(snapshot, opts),
        ViewModel::Dynamic(payload) => wrap_with_prefix(
            "view",
            &format!(
                "unsupported view {} v{}",
                payload.schema_id, payload.schema_version
            ),
            opts.width,
            Style::default().fg(Color::Yellow),
        ),
        _ => wrap_with_prefix(
            "view",
            "semantic view model is not rendered by this terminal adapter yet",
            opts.width,
            Style::default().fg(Color::Yellow),
        ),
    }
}

fn render_conversation(snapshot: &ViewSnapshot, opts: RenderOpts) -> Vec<Line<'static>> {
    render_conversation_from(snapshot, opts, 0)
}

fn render_conversation_from(
    snapshot: &ViewSnapshot,
    opts: RenderOpts,
    blocks_to_skip: usize,
) -> Vec<Line<'static>> {
    let ViewModel::Conversation(model) = &snapshot.model else {
        return Vec::new();
    };

    model
        .blocks
        .iter()
        .skip(blocks_to_skip)
        .flat_map(|block| render_conversation_block_lines(block, opts))
        .collect()
}

/// Render one conversation block using the terminal host's current wrapping rules.
#[must_use]
pub fn render_conversation_block_lines(
    block: &ConversationBlock,
    opts: RenderOpts,
) -> Vec<Line<'static>> {
    let prefix = conversation_prefix(&block.kind);
    let body = if block.kind == ConversationBlockKind::Thinking && !opts.expand_thinking {
        "thinking (ctrl+r to expand)"
    } else {
        block.text.as_str()
    };
    wrap_with_prefix(prefix, body, opts.width, conversation_style(&block.kind))
}

fn render_task_tree(snapshot: &ViewSnapshot, opts: RenderOpts) -> Vec<Line<'static>> {
    let ViewModel::TaskTree(model) = &snapshot.model else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    for root in &model.roots {
        push_task_node(&mut lines, root, 0, opts.width);
    }
    lines
}

fn render_form(snapshot: &ViewSnapshot, opts: RenderOpts) -> Vec<Line<'static>> {
    let ViewModel::Form(model) = &snapshot.model else {
        return Vec::new();
    };

    let mut lines = wrap_with_prefix(
        "form",
        &model.title,
        opts.width,
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    );
    for field in &model.fields {
        let required = if field.required { "*" } else { "" };
        let body = format!(
            "{required}{}: {}",
            field.label,
            compact_json_value(&field.value)
        );
        lines.extend(wrap_with_prefix(
            "form",
            &body,
            opts.width,
            Style::default().fg(Color::Magenta),
        ));
    }
    lines
}

fn render_command_palette(snapshot: &ViewSnapshot, opts: RenderOpts) -> Vec<Line<'static>> {
    let ViewModel::CommandPalette(model) = &snapshot.model else {
        return Vec::new();
    };

    let mut lines = wrap_with_prefix(
        "cmd",
        &format!("query: {}", model.query),
        opts.width,
        Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
    );
    for entry in &model.entries {
        let disabled = if entry.enabled { "" } else { " (disabled)" };
        let subtitle = entry
            .subtitle
            .as_ref()
            .map(|subtitle| format!(" - {subtitle}"))
            .unwrap_or_default();
        lines.extend(wrap_with_prefix(
            "cmd",
            &format!("{}{}{}", entry.title, subtitle, disabled),
            opts.width,
            Style::default().fg(Color::Blue),
        ));
    }
    lines
}

fn push_task_node(lines: &mut Vec<Line<'static>>, node: &TaskTreeNode, depth: usize, width: usize) {
    let indent = "  ".repeat(depth);
    let body = format!("{indent}[{}] {}", node.status, node.label);
    lines.extend(wrap_with_prefix(
        "task",
        &body,
        width,
        Style::default().fg(Color::Green),
    ));
    for child in &node.children {
        push_task_node(lines, child, depth + 1, width);
    }
}

fn conversation_prefix(kind: &ConversationBlockKind) -> &'static str {
    match kind {
        ConversationBlockKind::User => "you",
        ConversationBlockKind::Assistant => "alan",
        ConversationBlockKind::Thinking => "thinking",
        ConversationBlockKind::Tool => "tool",
        ConversationBlockKind::Yield => "input",
        ConversationBlockKind::Error => "error",
        ConversationBlockKind::Artifact => "artifact",
    }
}

fn conversation_style(kind: &ConversationBlockKind) -> Style {
    match kind {
        ConversationBlockKind::User => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        ConversationBlockKind::Thinking => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
        ConversationBlockKind::Tool | ConversationBlockKind::Artifact => {
            Style::default().fg(Color::Blue)
        }
        ConversationBlockKind::Yield => Style::default().fg(Color::Magenta),
        ConversationBlockKind::Error => Style::default().fg(Color::Red),
        ConversationBlockKind::Assistant => Style::default(),
    }
}

fn compact_json_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn wrap_with_prefix(prefix: &str, body: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    let width = width.max(16);
    let body_width = width.saturating_sub(prefix.len() + 3).max(8);
    body.split('\n')
        .flat_map(|segment| {
            let wrapped = textwrap::wrap(segment, body_width);
            if wrapped.is_empty() {
                vec![String::new()]
            } else {
                wrapped.into_iter().map(|line| line.into_owned()).collect()
            }
        })
        .enumerate()
        .map(|(idx, line)| {
            if idx == 0 {
                Line::styled(format!("{prefix}> {line}"), style)
            } else {
                Line::styled(
                    format!("{:width$}  {line}", "", width = prefix.len()),
                    style,
                )
            }
        })
        .collect()
}

fn line_to_plain_text(line: Line<'static>) -> String {
    line.spans
        .into_iter()
        .map(|span| span.content.into_owned())
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use alan_agent::{AgentWorkspaceProjector, AgentWorkspaceSessionMetadata};
    use alan_kernel::ViewModel;
    use alan_protocol::{Event, EventEnvelope};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    #[test]
    fn renders_conversation_and_task_snapshots() {
        let mut projector =
            AgentWorkspaceProjector::new(AgentWorkspaceSessionMetadata::new("session-1"));
        projector.apply_envelope(&envelope_with_event(1, Event::TurnStarted {}));
        projector.apply_envelope(&envelope_with_event(
            2,
            Event::TextDelta {
                chunk: "semantic transcript".to_string(),
                is_final: false,
            },
        ));
        projector.apply_envelope(&envelope_with_event(
            3,
            Event::Yield {
                request_id: "approval-1".to_string(),
                kind: alan_protocol::YieldKind::Confirmation,
                payload: serde_json::json!({"message": "Approve?"}),
            },
        ));

        let lines =
            render_agent_workspace_snapshots(&projector.snapshots(), RenderOpts::new(80, false));

        assert!(lines.iter().any(|line| line == "alan> semantic transcript"));
        assert!(lines.iter().any(|line| line.contains("[yielded]")));
        assert!(lines.iter().any(|line| line.contains("Turn t-1")));
    }

    #[test]
    fn renders_form_and_command_palette_semantic_snapshots() {
        let projector =
            AgentWorkspaceProjector::new(AgentWorkspaceSessionMetadata::new("session-1"));
        let snapshots = projector.snapshots();

        let form_lines =
            render_snapshot_lines(&snapshots.approval_form, RenderOpts::new(80, false))
                .into_iter()
                .map(line_to_plain_text)
                .collect::<Vec<_>>();
        assert!(form_lines.iter().any(|line| line.contains("Approval")));

        let command_lines =
            render_snapshot_lines(&snapshots.command_palette, RenderOpts::new(80, false))
                .into_iter()
                .map(line_to_plain_text)
                .collect::<Vec<_>>();
        assert!(
            command_lines
                .iter()
                .any(|line| line.contains("Submit Turn"))
        );
    }

    #[test]
    fn paragraph_renders_semantic_snapshot_with_ratatui_test_backend() {
        let mut projector =
            AgentWorkspaceProjector::new(AgentWorkspaceSessionMetadata::new("session-1"));
        projector.apply_envelope(&envelope_with_event(
            1,
            Event::TextDelta {
                chunk: "ratatui semantic output".to_string(),
                is_final: false,
            },
        ));

        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    agent_workspace_paragraph(&projector.snapshots(), RenderOpts::new(80, false)),
                    frame.area(),
                );
            })
            .expect("draw");
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("ratatui semantic output"));
    }

    #[test]
    fn dynamic_view_uses_bounded_fallback() {
        let projector =
            AgentWorkspaceProjector::new(AgentWorkspaceSessionMetadata::new("session-1"));
        let mut snapshot = projector.snapshots().conversation;
        snapshot.model = ViewModel::Dynamic(alan_kernel::DynamicViewPayload {
            schema_id: "example.dynamic".to_string(),
            schema_version: 7,
            payload: serde_json::json!({"ignored": true}),
        });

        let lines = render_snapshot_lines(&snapshot, RenderOpts::new(80, false))
            .into_iter()
            .map(line_to_plain_text)
            .collect::<Vec<_>>();

        assert_eq!(lines[0], "view> unsupported view example.dynamic v7");
    }

    fn envelope_with_event(sequence: u64, event: Event) -> EventEnvelope {
        EventEnvelope {
            event_id: format!("e-{sequence}"),
            sequence,
            session_id: "s-1".into(),
            submission_id: None,
            turn_id: "t-1".into(),
            item_id: "i-1".into(),
            timestamp_ms: sequence,
            event,
        }
    }
}
