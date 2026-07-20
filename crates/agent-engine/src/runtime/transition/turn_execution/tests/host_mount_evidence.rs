use super::*;

#[tokio::test]
async fn invalid_mount_host_location_never_enters_machine_tape() {
    let raw_host_path = "/Users/example/private-project";
    let generate_calls = Arc::new(AtomicUsize::new(0));
    let provider = SequenceMockProvider::new(
        vec![
            GenerationResponse {
                content: String::new(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![ToolCall {
                    id: Some("call-invalid-mount".to_string()),
                    name: "request_mount".to_string(),
                    arguments: json!({
                        "namespace_path": "/mnt/project",
                        "host_path": raw_host_path,
                        "access": "read_only",
                        "reason": "Read project files"
                    }),
                }],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
            GenerationResponse {
                content: "The invalid request was rejected.".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            },
        ],
        Arc::clone(&generate_calls),
    );
    let mut state = create_test_state_with_provider(provider);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let outcome = run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Mount my project")]),
        &mut emit,
        &cancel,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome, TurnExecutionOutcome::Finished);
    assert_eq!(generate_calls.load(Ordering::SeqCst), 2);
    let machine_tape = serde_json::to_string(state.machine.messages()).unwrap();
    assert!(!machine_tape.contains(raw_host_path));
    assert!(!machine_tape.contains("host_path"));
    let persisted_request = state
        .machine
        .messages()
        .iter()
        .find_map(|message| match message {
            Message::Assistant { tool_requests, .. } => tool_requests.first(),
            _ => None,
        })
        .expect("assistant Tool request is retained without Host-owned values");
    assert_eq!(
        persisted_request.arguments,
        json!({"invalid_request": true})
    );
}
