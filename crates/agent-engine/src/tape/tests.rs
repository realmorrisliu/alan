use super::*;

fn msg(role: MessageRole, content: &str) -> Message {
    match role {
        MessageRole::User => Message::user(content),
        MessageRole::Assistant => Message::assistant(content),
        MessageRole::System => Message::system(content),
        MessageRole::Context => Message::context(content),
        MessageRole::Tool => {
            // For test convenience, create a tool message with a single text response
            Message::Tool {
                responses: vec![ToolResponse::text("test", content)],
            }
        }
    }
}

fn assistant_with_tool_request(id: &str, name: &str) -> Message {
    Message::assistant_with_tools(
        "",
        vec![ToolRequest {
            id: id.to_string(),
            name: name.to_string(),
            arguments: serde_json::json!({}),
        }],
    )
}

fn item(id: &str, content: &str) -> ContextItem {
    ContextItem {
        id: id.to_string(),
        kind: "test".to_string(),
        title: format!("Title {}", id),
        content: content.to_string(),
        fingerprint: fingerprint_context("test", &format!("Title {}", id), content),
    }
}

fn control_user_message() -> Message {
    Message::user_parts(vec![ContentPart::structured(serde_json::json!({
        "checkpoint_id": "tool_escalation_call-1",
        "checkpoint_type": "tool_escalation",
        "choice": "approve",
        "__alan_internal_control": {
            "kind": "tool_escalation_confirmation",
            "version": 1,
            "source": "runtime/submission_handlers"
        }
    }))])
}

fn effect_replay_control_user_message() -> Message {
    Message::user_parts(vec![ContentPart::structured(serde_json::json!({
        "checkpoint_id": "effect_replay_call-1",
        "checkpoint_type": "effect_replay_confirmation",
        "choice": "approve",
        "__alan_internal_control": {
            "kind": "effect_replay_confirmation",
            "version": 1,
            "source": "runtime/submission_handlers"
        }
    }))])
}

#[test]
fn test_messages_for_prompt_includes_summary() {
    let mut ctx = Tape::new();
    ctx.push(msg(MessageRole::User, "hello"));
    ctx.set_summary("short summary".to_string());

    let messages = ctx.messages_for_prompt();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role(), MessageRole::Context);
    assert!(messages[0].text_content().contains(SUMMARY_PREFIX));
    assert!(messages[0].text_content().contains("short summary"));
    assert_eq!(messages[1].role(), MessageRole::User);
}

#[test]
fn test_apply_context_items_computes_baseline_and_delta() {
    let mut ctx = Tape::new();

    let delta = ctx.apply_context_items(vec![item("a", "alpha"), item("b", "beta")]);
    assert!(delta.changed);
    assert_eq!(delta.revision, 1);
    assert_eq!(delta.added_ids, vec!["a", "b"]);
    assert!(delta.updated_ids.is_empty());
    assert!(delta.removed_ids.is_empty());
    assert_eq!(ctx.context_revision(), 1);

    let unchanged = ctx.apply_context_items(vec![item("a", "alpha"), item("b", "beta")]);
    assert!(!unchanged.changed);
    assert_eq!(unchanged.revision, 1);
    assert_eq!(ctx.context_revision(), 1);
}

#[test]
fn test_apply_context_items_detects_updates_removals_and_reorder() {
    let mut ctx = Tape::new();
    ctx.apply_context_items(vec![
        item("a", "alpha"),
        item("b", "beta"),
        item("c", "gamma"),
    ]);

    let delta = ctx.apply_context_items(vec![item("b", "beta"), item("a", "alpha2")]);
    assert!(delta.changed);
    assert_eq!(delta.revision, 2);
    assert_eq!(delta.updated_ids, vec!["a"]);
    assert_eq!(delta.removed_ids, vec!["c"]);
    assert!(delta.added_ids.is_empty());

    let reorder_only = ctx.apply_context_items(vec![item("a", "alpha2"), item("b", "beta")]);
    assert!(reorder_only.changed);
    assert!(reorder_only.reordered);
    assert!(reorder_only.added_ids.is_empty());
    assert!(reorder_only.updated_ids.is_empty());
    assert!(reorder_only.removed_ids.is_empty());
}

#[test]
fn test_apply_context_items_detects_content_change_with_stale_fingerprint() {
    let mut ctx = Tape::new();
    let original = item("a", "alpha");
    let stale_fingerprint = original.fingerprint.clone();
    ctx.apply_context_items(vec![original]);

    let delta = ctx.apply_context_items(vec![ContextItem {
        id: "a".to_string(),
        kind: "test".to_string(),
        title: "Title a".to_string(),
        content: "beta".to_string(),
        fingerprint: stale_fingerprint,
    }]);

    assert!(delta.changed);
    assert_eq!(delta.updated_ids, vec!["a"]);
    assert_eq!(
        ctx.context_items()[0].fingerprint,
        fingerprint_context("test", "Title a", "beta")
    );
}

#[test]
fn test_prompt_view_exposes_reference_context_snapshot_metadata() {
    let mut ctx = Tape::new();
    let delta = ctx.apply_context_items(vec![item("ctx_1", "important background")]);
    assert!(delta.changed);
    ctx.push(msg(MessageRole::User, "hello"));

    let view = ctx.prompt_view();
    assert_eq!(view.reference_context.item_count, 1);
    assert_eq!(view.reference_context.revision, 1);
    assert!(view.reference_context.delta.changed);
    assert_eq!(view.messages.len(), 2);
    assert_eq!(view.messages[0].role(), MessageRole::Context);
    assert_eq!(view.messages[1].role(), MessageRole::User);
}

#[test]
fn test_compact_keeps_complete_latest_user_turn_span_and_sets_summary() {
    let mut ctx = Tape::new();
    ctx.push(msg(MessageRole::User, "u1"));
    ctx.push(msg(MessageRole::Assistant, "a1"));
    ctx.push(msg(MessageRole::Tool, "tool1"));
    ctx.push(msg(MessageRole::User, "u2"));
    ctx.push(msg(MessageRole::Assistant, "a2"));
    ctx.push(msg(MessageRole::Tool, "tool2"));

    ctx.compact("summary".to_string(), 3);
    let messages = ctx.messages_for_prompt();
    assert_eq!(messages.len(), 4);
    assert!(messages[0].text_content().contains("summary"));
    assert_eq!(messages[1].text_content(), "u2");
    assert_eq!(messages[2].text_content(), "a2");
    assert_eq!(messages[3].text_content(), "tool2");
}

#[test]
fn test_semantic_message_spans_treat_control_preamble_as_control() {
    let spans = semantic_message_spans(&[
        msg(MessageRole::Assistant, "assistant preamble"),
        msg(MessageRole::Tool, "tool preamble"),
        msg(MessageRole::User, "u1"),
        msg(MessageRole::Assistant, "a1"),
    ]);

    assert_eq!(
        spans,
        vec![
            MessageSpan {
                start: 0,
                end: 2,
                kind: SpanKind::Control,
            },
            MessageSpan {
                start: 2,
                end: 4,
                kind: SpanKind::UserTurn,
            },
        ]
    );
}

#[test]
fn test_semantic_message_spans_do_not_start_new_turn_for_control_user_messages() {
    let spans = semantic_message_spans(&[
        msg(MessageRole::User, "u1"),
        msg(MessageRole::Assistant, "a1"),
        control_user_message(),
        msg(MessageRole::Assistant, "a2"),
        msg(MessageRole::User, "u2"),
    ]);

    assert_eq!(
        spans,
        vec![
            MessageSpan {
                start: 0,
                end: 4,
                kind: SpanKind::UserTurn,
            },
            MessageSpan {
                start: 4,
                end: 5,
                kind: SpanKind::UserTurn,
            },
        ]
    );
}

#[test]
fn test_semantic_message_spans_do_not_start_new_turn_for_effect_replay_controls() {
    let spans = semantic_message_spans(&[
        msg(MessageRole::User, "u1"),
        msg(MessageRole::Assistant, "a1"),
        effect_replay_control_user_message(),
        msg(MessageRole::Assistant, "a2"),
        msg(MessageRole::User, "u2"),
    ]);

    assert_eq!(
        spans,
        vec![
            MessageSpan {
                start: 0,
                end: 4,
                kind: SpanKind::UserTurn,
            },
            MessageSpan {
                start: 4,
                end: 5,
                kind: SpanKind::UserTurn,
            },
        ]
    );
}

#[test]
fn test_compact_preserves_reference_context_summary_message_order() {
    let mut ctx = Tape::new();
    ctx.apply_context_items(vec![item("ctx-1", "domain context")]);
    ctx.push(msg(MessageRole::User, "u1"));
    ctx.push(msg(MessageRole::Assistant, "a1"));
    ctx.push(msg(MessageRole::User, "u2"));
    ctx.push(msg(MessageRole::Assistant, "a2"));

    ctx.compact("summary".to_string(), 2);

    let prompt = ctx.messages_for_prompt();
    assert_eq!(prompt[0].role(), MessageRole::Context);
    assert!(prompt[0].text_content().contains("domain context"));
    assert_eq!(prompt[1].role(), MessageRole::Context);
    assert!(prompt[1].text_content().contains("summary"));
    assert_eq!(prompt[2].text_content(), "u2");
    assert_eq!(prompt[3].text_content(), "a2");
}

#[test]
fn test_compact_reduces_estimated_prompt_tokens_with_semantic_window() {
    let mut ctx = Tape::new();
    ctx.push(msg(MessageRole::User, "u1"));
    ctx.push(msg(MessageRole::Assistant, "a1"));
    ctx.push(msg(MessageRole::Tool, &"log line\n".repeat(200)));
    ctx.push(msg(MessageRole::User, "u2"));
    ctx.push(msg(MessageRole::Assistant, "a2"));

    let before = ctx.estimated_prompt_tokens();
    ctx.compact("short summary".to_string(), 1);
    let after = ctx.estimated_prompt_tokens();

    assert!(after < before);
}

#[test]
fn test_compaction_retention_start_uses_message_budget_not_span_count() {
    let messages = vec![
        msg(MessageRole::User, "u1"),
        msg(MessageRole::Assistant, "a1"),
        msg(MessageRole::Tool, "tool1"),
        msg(MessageRole::Assistant, "a1b"),
        msg(MessageRole::Tool, "tool1b"),
        msg(MessageRole::User, "u2"),
        msg(MessageRole::Assistant, "a2"),
        msg(MessageRole::Tool, "tool2"),
        msg(MessageRole::Assistant, "a2b"),
        msg(MessageRole::Tool, "tool2b"),
    ];

    assert_eq!(compaction_retention_start(&messages, 6), 5);
}

#[test]
fn test_compaction_retention_start_falls_back_when_latest_user_turn_exceeds_budget() {
    let messages = vec![
        msg(MessageRole::User, "u1"),
        msg(MessageRole::Assistant, "a1"),
        msg(MessageRole::Tool, "tool1"),
        msg(MessageRole::Assistant, "a1b"),
        msg(MessageRole::Tool, "tool1b"),
        msg(MessageRole::User, "u2"),
        msg(MessageRole::Assistant, "a2"),
        msg(MessageRole::Tool, "tool2"),
        msg(MessageRole::Assistant, "a2b"),
        msg(MessageRole::Tool, "tool2b"),
    ];

    assert_eq!(compaction_retention_start(&messages, 4), 6);
}

#[test]
fn test_compaction_retention_start_falls_back_for_single_large_span() {
    let messages = vec![
        msg(MessageRole::User, "u1"),
        msg(MessageRole::Assistant, "a1"),
        msg(MessageRole::Tool, "tool1"),
        msg(MessageRole::Assistant, "a1b"),
        msg(MessageRole::Tool, "tool1b"),
    ];

    assert_eq!(compaction_retention_start(&messages, 2), 3);
}

#[test]
fn test_compaction_retention_start_falls_back_for_large_recent_span_after_control_preamble() {
    let messages = vec![
        msg(MessageRole::Assistant, "assistant preamble"),
        msg(MessageRole::Tool, "tool preamble"),
        msg(MessageRole::User, "u1"),
        msg(MessageRole::Assistant, "a1"),
        msg(MessageRole::Tool, "tool1"),
        msg(MessageRole::Assistant, "a1b"),
        msg(MessageRole::Tool, "tool1b"),
    ];

    assert_eq!(compaction_retention_start(&messages, 2), 5);
}

#[test]
fn test_compaction_retention_start_preserves_assistant_tool_pairing_in_raw_tail_fallback() {
    let messages = vec![
        msg(MessageRole::User, "u1"),
        assistant_with_tool_request("call_1", "lookup"),
        Message::tool_text("call_1", "tool result"),
    ];

    assert_eq!(compaction_retention_start(&messages, 1), 1);
}

#[test]
fn test_compact_keeps_entire_trailing_tool_block_when_budget_is_smaller_than_block() {
    let mut ctx = Tape::new();
    ctx.push(msg(MessageRole::User, "u1"));
    ctx.push(Message::assistant_with_tools(
        "",
        vec![
            ToolRequest {
                id: "call_1".to_string(),
                name: "lookup".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolRequest {
                id: "call_2".to_string(),
                name: "lookup".to_string(),
                arguments: serde_json::json!({}),
            },
        ],
    ));
    ctx.push(Message::tool_text("call_1", "tool result 1"));
    ctx.push(Message::tool_text("call_2", "tool result 2"));

    ctx.compact("summary".to_string(), 1);

    assert_eq!(ctx.summary(), Some("summary"));
    assert_eq!(ctx.messages().len(), 3);
    assert!(matches!(
        &ctx.messages()[0],
        Message::Assistant { tool_requests, .. } if !tool_requests.is_empty()
    ));
    assert!(matches!(&ctx.messages()[1], Message::Tool { .. }));
    assert!(matches!(&ctx.messages()[2], Message::Tool { .. }));
}

#[test]
fn test_clear_resets_messages_summary_and_reference_context() {
    let mut ctx = Tape::new();
    ctx.apply_context_items(vec![item("x", "ctx")]);
    ctx.push(msg(MessageRole::User, "hello"));
    ctx.set_summary("summary".to_string());
    ctx.clear();

    let messages = ctx.messages_for_prompt();
    assert!(messages.is_empty());
    assert_eq!(ctx.context_revision(), 0);
    assert!(ctx.context_items().is_empty());
}

#[test]
fn test_clear_summary_preserves_messages() {
    let mut ctx = Tape::new();
    ctx.push(msg(MessageRole::User, "hello"));
    ctx.set_summary("summary".to_string());
    ctx.clear_summary();

    let messages = ctx.messages_for_prompt();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text_content(), "hello");
}

#[test]
fn test_replace_messages() {
    let mut ctx = Tape::new();
    ctx.push(msg(MessageRole::User, "old"));
    ctx.replace(vec![msg(MessageRole::Assistant, "new")]);
    let messages = ctx.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role(), MessageRole::Assistant);
}

#[test]
fn test_context_items_render_before_messages() {
    let mut ctx = Tape::new();
    ctx.apply_context_items(vec![ContextItem {
        id: "onboarding".to_string(),
        kind: "static".to_string(),
        title: "Onboarding".to_string(),
        content: "Follow the steps".to_string(),
        fingerprint: fingerprint_context("static", "Onboarding", "Follow the steps"),
    }]);
    ctx.push(msg(MessageRole::User, "hello"));

    let messages = ctx.messages_for_prompt();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role(), MessageRole::Context);
    assert!(messages[0].text_content().contains("Onboarding"));
    assert_eq!(messages[1].role(), MessageRole::User);
}

#[test]
fn test_estimated_prompt_tokens_includes_summary_and_context_items() {
    let mut ctx = Tape::new();
    ctx.apply_context_items(vec![ContextItem {
        id: "ctx_1".to_string(),
        kind: "domain".to_string(),
        title: "Domain".to_string(),
        content: "Important background".to_string(),
        fingerprint: fingerprint_context("domain", "Domain", "Important background"),
    }]);
    ctx.push(msg(MessageRole::User, "hello world"));
    ctx.set_summary("previous summary".to_string());

    let estimated = ctx.estimated_prompt_tokens();
    assert!(estimated > 0);

    let without_summary = {
        let mut clone = ctx.clone();
        clone.clear_summary();
        clone.estimated_prompt_tokens()
    };
    assert!(
        estimated > without_summary,
        "summary content should contribute to token estimate"
    );
}
