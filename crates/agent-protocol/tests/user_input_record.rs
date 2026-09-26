use alan_agent_protocol::{InputIntent, InputMode, Op, UserInputRecord};

#[test]
fn encoded_user_input_preserves_identity_intent_body_and_scheduling() {
    let record = UserInputRecord::new(
        InputIntent::Command,
        InputMode::FollowUp,
        "  printf 'a\\nb'  ",
    );
    let id = record.submission_id.clone();
    let payload = record.encode_payload().unwrap();
    let decoded = UserInputRecord::decode_payload(&payload)
        .unwrap()
        .expect("versioned record");

    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.submission_id, id);
    assert_eq!(decoded.intent, InputIntent::Command);
    assert_eq!(decoded.mode, InputMode::FollowUp);
    assert_eq!(decoded.body, "  printf 'a\\nb'  ");
    let submission = decoded.into_submission().unwrap();
    assert_eq!(submission.id, id);
    assert_eq!(submission.intent, InputIntent::Command);
    assert!(matches!(
        submission.op,
        Op::Input {
            mode: InputMode::FollowUp,
            ..
        }
    ));
}
