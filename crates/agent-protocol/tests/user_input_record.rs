use alan_agent_protocol::{InputIntent, InputMode, UserInputRecord, parse_input_prefix};

#[test]
fn records_preserve_identity_body_intent_and_scheduling_independently() {
    for (input, intent, body) in [
        (
            "!  printf ':!\\n'\nnext",
            InputIntent::Command,
            "  printf ':!\\n'\nnext",
        ),
        (":!literal", InputIntent::ForceAgent, "!literal"),
        (
            "explain ! literally",
            InputIntent::Agent,
            "explain ! literally",
        ),
        (" !!literal", InputIntent::Agent, " !!literal"),
    ] {
        assert_eq!(parse_input_prefix(input), (intent, body));
        for mode in [InputMode::Steer, InputMode::FollowUp, InputMode::NextTurn] {
            let record = UserInputRecord::new(intent, mode, body);
            let encoded = record.encode_payload().unwrap();
            assert_eq!(
                UserInputRecord::decode_payload(&encoded).unwrap(),
                Some(record)
            );
        }
    }
}

#[test]
fn invalid_records_never_fall_back_to_legacy_prose() {
    for legacy in [b"ordinary text".as_slice(), b"{\"body\":\"JSON data\"}"] {
        assert_eq!(UserInputRecord::decode_payload(legacy).unwrap(), None);
    }
    for invalid in [b"alan-input-v2\n{}".as_slice(), b"alan-input-v1\nnot JSON"] {
        assert!(UserInputRecord::decode_payload(invalid).is_err());
    }
    let valid = UserInputRecord::new(InputIntent::Command, InputMode::FollowUp, "pwd");
    for field in ["version", "submission_id", "body", "unknown"] {
        let mut value = serde_json::to_value(&valid).unwrap();
        value[field] = match field {
            "version" => 2.into(),
            "submission_id" => "not-a-uuid".into(),
            "body" => " \n".into(),
            _ => true.into(),
        };
        let payload = format!("alan-input-v1\n{value}");
        assert!(
            UserInputRecord::decode_payload(payload.as_bytes()).is_err(),
            "{field}"
        );
    }
    let mut empty = valid;
    empty.body.clear();
    assert!(empty.encode_payload().is_err());
}

#[test]
fn submission_intent_round_trips_and_legacy_submissions_default_to_agent() {
    use alan_agent_protocol::{ContentPart, Op, Submission};
    let mut submission = Submission::new(Op::Input {
        parts: vec![ContentPart::text("!literal")],
        mode: InputMode::Steer,
    });
    submission.intent = InputIntent::ForceAgent;
    let mut value = serde_json::to_value(&submission).unwrap();
    let decoded: Submission = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.intent, InputIntent::ForceAgent);
    assert_eq!(decoded.id, submission.id);
    value.as_object_mut().unwrap().remove("intent");
    let legacy: Submission = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.intent, InputIntent::Agent);
}
