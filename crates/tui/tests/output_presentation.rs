use alan_agent_protocol::{
    PlanItem, PlanItemStatus, ToolResultPresentation, UiPlanSnapshot, YieldKind,
};
use alan_tui::history::{HistoryCell, PendingYieldCell, RenderOpts, ToolStatus};

#[test]
fn semantic_output_has_no_generated_role_labels_and_keeps_literal_content() {
    let literal = "server> ready; a > b; tool> literal";
    let cells = [
        HistoryCell::Tool {
            action: None,
            title: "Read src/main.rs".into(),
            status: ToolStatus::Complete,
            preview: Some(literal.into()),
            presentation: None,
        },
        HistoryCell::Thinking {
            text: literal.into(),
            duration_secs: 2,
        },
        HistoryCell::Plan {
            snapshot: UiPlanSnapshot::new(
                None,
                vec![PlanItem {
                    id: "step-1".into(),
                    status: PlanItemStatus::InProgress,
                    content: literal.into(),
                }],
            ),
            owner: "/agent/1".into(),
            revision: 1,
        },
        HistoryCell::Error(literal.into()),
        HistoryCell::PendingYield(PendingYieldCell {
            request_id: "request-1".into(),
            kind: YieldKind::Confirmation,
            title: "Approve read".into(),
            prompt: Some(literal.into()),
            options: vec!["approve".into(), "reject".into()],
            default_option: Some("reject".into()),
            questions: vec![],
            capability: Some("read".into()),
            reason: Some("explicit grant required".into()),
            presentation: None,
        }),
    ];
    for width in [48, 80, 120] {
        for cell in &cells {
            let rows = cell.render_styled_lines(RenderOpts::new(width, true));
            let text = rows
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            for prefix in ["tool> ", "plan> ", "thinking> ", "input> ", "error> "] {
                assert!(!text.lines().any(|line| line.starts_with(prefix)), "{text}");
            }
            assert!(text.contains("server> ready"), "{text}");
        }
        let diff = HistoryCell::Tool {
            action: None,
            title: "Edit src/界.rs".into(),
            status: ToolStatus::Failed,
            preview: None,
            presentation: Some(ToolResultPresentation::Diff {
                path: "src/界.rs".into(),
                hunks: vec![],
            }),
        };
        let text = diff
            .render_styled_lines(RenderOpts::new(width, false))
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!text.contains("tool>"), "{text}");
        assert!(
            text.contains("failed"),
            "failure needs textual status: {text}"
        );
    }
}
