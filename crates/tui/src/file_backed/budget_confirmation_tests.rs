use super::*;
use crate::history::{HistoryCell, RenderOpts};
use alan_agent_protocol::ContentPart;

#[test]
fn budget_confirmation_uses_one_prompt_and_safe_confirmation_controls() {
    let summary = "Budget reached\nUsed: models 1/1\nAdditional allowance: 1 model call";
    let pending = request_snapshot_to_pending_yield(RequestSnapshot {
        id: "budget-request".into(),
        kind: "task_budget_extension".into(),
        prompt: summary.into(),
        options: serde_json::json!({
            "checkpoint_type": "task_budget_extension",
            "details": {"budget": {"model_calls": 1}},
            "options": ["approve", "reject"],
            "default_option": "reject"
        })
        .to_string(),
        status: "pending".into(),
    })
    .unwrap();
    assert_eq!(pending.kind, YieldKind::Confirmation);
    let rendered = HistoryCell::PendingYield(pending.clone())
        .render_lines(RenderOpts::new(160, false))
        .join("\n");
    assert_eq!(rendered.matches("Used: models 1/1").count(), 1);
    assert_eq!(
        pending.resume_content("").unwrap(),
        vec![ContentPart::structured(
            serde_json::json!({"choice": "reject"})
        )]
    );
    assert!(pending.resume_content("maybe").is_err());
    let mut app = FileBackedApp::new("/agent/root".into());
    app.set_pending_yield(pending);
    assert!(matches!(
        app.confirmation_keypress(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE)),
        Some(FileBackedAction::Resume { response, .. }) if response == r#"{"choice":"reject"}"#
    ));
}
