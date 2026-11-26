use std::collections::BTreeMap;

use serde_json::json;

use super::*;

fn sample_request() -> DecisionRequest {
    let mut questions = BTreeMap::new();
    questions.insert(
        "safe".to_string(),
        DecisionQuestion::Predicate {
            prompt: "Does the action stay within policy?".to_string(),
        },
    );
    questions.insert(
        "verdict".to_string(),
        DecisionQuestion::Choice {
            prompt: "Select policy action.".to_string(),
            options: vec![
                DecisionOption {
                    id: "allow".to_string(),
                    description: "Safe to execute".to_string(),
                },
                DecisionOption {
                    id: "deny".to_string(),
                    description: "Violates policy".to_string(),
                },
                DecisionOption {
                    id: "escalate".to_string(),
                    description: "Needs human review".to_string(),
                },
            ],
        },
    );
    questions.insert(
        "risk".to_string(),
        DecisionQuestion::Score {
            prompt: "Rate operational risk.".to_string(),
            levels: vec![
                DecisionLevel {
                    id: "low".to_string(),
                    description: "Read-only".to_string(),
                    score: 0.0,
                },
                DecisionLevel {
                    id: "medium".to_string(),
                    description: "Reversible".to_string(),
                    score: 0.5,
                },
                DecisionLevel {
                    id: "high".to_string(),
                    description: "Destructive".to_string(),
                    score: 1.0,
                },
            ],
        },
    );
    DecisionRequest::new(json!({"tool": "bash", "cmd": "cargo test"}), questions)
        .with_instructions("Evaluate strictly against policy.")
}

fn sample_response() -> DecisionResponse {
    let mut answers = BTreeMap::new();
    answers.insert(
        "safe".to_string(),
        DecisionAnswer::Predicate {
            yes_probability: 0.95,
            confidence: 0.92,
        },
    );
    answers.insert(
        "verdict".to_string(),
        DecisionAnswer::Choice {
            choice: "allow".to_string(),
            probabilities: BTreeMap::from([
                ("allow".to_string(), 0.90),
                ("deny".to_string(), 0.03),
                ("escalate".to_string(), 0.07),
            ]),
            confidence: 0.90,
        },
    );
    answers.insert(
        "risk".to_string(),
        DecisionAnswer::Score {
            expected_score: 0.10,
            probabilities: BTreeMap::from([
                ("low".to_string(), 0.85),
                ("medium".to_string(), 0.10),
                ("high".to_string(), 0.05),
            ]),
            confidence: 0.88,
        },
    );
    DecisionResponse {
        model: "gemini-decision-1".to_string(),
        provider: "google_gemini".to_string(),
        calibration: CalibrationKind::Calibrated,
        answers,
        usage: Some(TokenUsage {
            prompt_tokens: 120,
            completion_tokens: 0,
            total_tokens: 120,
            ..Default::default()
        }),
    }
}

#[test]
fn valid_request_and_response_pass_and_round_trip_serde() {
    let request = sample_request();
    let original_state = request.state.clone();
    request.validate().unwrap();
    assert_eq!(request.state, original_state);

    let response = sample_response();
    request.validate_response(&response).unwrap();
    assert_eq!(response.answers["safe"].predicate_yes(), Some(true));
    assert_eq!(response.answers["verdict"].predicate_yes(), None);
    assert!((response.answers["risk"].confidence() - 0.88).abs() < 1e-12);

    let encoded_req = serde_json::to_string(&request).unwrap();
    let decoded_req: DecisionRequest = serde_json::from_str(&encoded_req).unwrap();
    assert_eq!(decoded_req, request);

    let encoded_resp = serde_json::to_string(&response).unwrap();
    assert!(encoded_resp.contains("\"calibration\":\"calibrated\""));
    let decoded_resp: DecisionResponse = serde_json::from_str(&encoded_resp).unwrap();
    assert_eq!(decoded_resp, response);
}

#[test]
fn request_enforces_question_count_bounds() {
    let empty = DecisionRequest::new(json!("state"), BTreeMap::new());
    assert!(empty.validate().is_err());

    let mut max_questions = BTreeMap::new();
    for i in 0..MAX_DECISION_QUESTIONS {
        max_questions.insert(
            format!("q_{i}"),
            DecisionQuestion::Predicate {
                prompt: format!("Question {i}?"),
            },
        );
    }
    let at_limit = DecisionRequest::new(json!({}), max_questions.clone());
    at_limit.validate().unwrap();

    max_questions.insert(
        "q_overflow".to_string(),
        DecisionQuestion::Predicate {
            prompt: "One too many".to_string(),
        },
    );
    let over_limit = DecisionRequest::new(json!({}), max_questions);
    assert!(over_limit.validate().is_err());
}

#[test]
fn request_rejects_invalid_ids_prompts_options_levels_and_non_finite_scores() {
    let mut bad_q_id = sample_request();
    bad_q_id.questions.insert(
        "../escape".to_string(),
        DecisionQuestion::Predicate {
            prompt: "Valid prompt".to_string(),
        },
    );
    assert!(bad_q_id.validate().is_err());

    let mut blank_prompt = sample_request();
    blank_prompt.questions.insert(
        "safe".to_string(),
        DecisionQuestion::Predicate {
            prompt: "   \n\t ".to_string(),
        },
    );
    assert!(blank_prompt.validate().is_err());

    let mut empty_options = sample_request();
    empty_options.questions.insert(
        "verdict".to_string(),
        DecisionQuestion::Choice {
            prompt: "Pick one".to_string(),
            options: Vec::new(),
        },
    );
    assert!(empty_options.validate().is_err());

    let mut duplicate_options = sample_request();
    duplicate_options.questions.insert(
        "verdict".to_string(),
        DecisionQuestion::Choice {
            prompt: "Pick one".to_string(),
            options: vec![
                DecisionOption {
                    id: "dup".to_string(),
                    description: "First".to_string(),
                },
                DecisionOption {
                    id: "dup".to_string(),
                    description: "Second".to_string(),
                },
            ],
        },
    );
    assert!(duplicate_options.validate().is_err());

    let mut empty_levels = sample_request();
    empty_levels.questions.insert(
        "risk".to_string(),
        DecisionQuestion::Score {
            prompt: "Rate".to_string(),
            levels: Vec::new(),
        },
    );
    assert!(empty_levels.validate().is_err());

    let mut duplicate_levels = sample_request();
    duplicate_levels.questions.insert(
        "risk".to_string(),
        DecisionQuestion::Score {
            prompt: "Rate".to_string(),
            levels: vec![
                DecisionLevel {
                    id: "same".to_string(),
                    description: "A".to_string(),
                    score: 0.0,
                },
                DecisionLevel {
                    id: "same".to_string(),
                    description: "B".to_string(),
                    score: 1.0,
                },
            ],
        },
    );
    assert!(duplicate_levels.validate().is_err());

    for bad_score in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut req = sample_request();
        req.questions.insert(
            "risk".to_string(),
            DecisionQuestion::Score {
                prompt: "Rate".to_string(),
                levels: vec![DecisionLevel {
                    id: "bad".to_string(),
                    description: "Non-finite".to_string(),
                    score: bad_score,
                }],
            },
        );
        assert!(req.validate().is_err());
    }

    let mut empty_instructions = sample_request();
    empty_instructions.instructions = Some("   ".to_string());
    assert!(empty_instructions.validate().is_err());

    let mut bad_override = sample_request();
    bad_override.model_override = Some("bad\nmodel".to_string());
    assert!(bad_override.validate().is_err());
}

#[test]
fn request_enforces_one_mib_size_cap() {
    let mut request = sample_request();
    request.state = json!("x".repeat(MAX_DECISION_REQUEST_BYTES + 1));
    assert!(request.validate().is_err());
}

#[test]
fn response_rejects_nan_infinity_and_out_of_range_probabilities() {
    let request = sample_request();

    for bad_val in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
        let mut resp = sample_response();
        resp.answers.insert(
            "safe".to_string(),
            DecisionAnswer::Predicate {
                yes_probability: bad_val,
                confidence: 0.5,
            },
        );
        assert!(request.validate_response(&resp).is_err());

        let mut resp_conf = sample_response();
        resp_conf.answers.insert(
            "safe".to_string(),
            DecisionAnswer::Predicate {
                yes_probability: 0.5,
                confidence: bad_val,
            },
        );
        assert!(request.validate_response(&resp_conf).is_err());
    }
}

#[test]
fn response_rejects_broken_distributions_choice_mismatch_and_score_contradictions() {
    let request = sample_request();

    // Missing question key
    let mut missing = sample_response();
    missing.answers.remove("risk");
    assert!(request.validate_response(&missing).is_err());

    // Extra question key
    let mut extra = sample_response();
    extra.answers.insert(
        "extra".to_string(),
        DecisionAnswer::Predicate {
            yes_probability: 0.5,
            confidence: 0.5,
        },
    );
    assert!(request.validate_response(&extra).is_err());

    // Variant mismatch
    let mut mismatch = sample_response();
    mismatch.answers.insert(
        "safe".to_string(),
        DecisionAnswer::Score {
            expected_score: 0.5,
            probabilities: BTreeMap::from([("low".to_string(), 1.0)]),
            confidence: 0.5,
        },
    );
    assert!(request.validate_response(&mismatch).is_err());

    // Choice outside submitted options
    let mut unknown_choice = sample_response();
    unknown_choice.answers.insert(
        "verdict".to_string(),
        DecisionAnswer::Choice {
            choice: "sudo".to_string(),
            probabilities: BTreeMap::from([
                ("allow".to_string(), 0.2),
                ("deny".to_string(), 0.2),
                ("sudo".to_string(), 0.6),
            ]),
            confidence: 0.6,
        },
    );
    assert!(request.validate_response(&unknown_choice).is_err());

    // Choice disagrees with argmax probability
    let mut non_argmax = sample_response();
    non_argmax.answers.insert(
        "verdict".to_string(),
        DecisionAnswer::Choice {
            choice: "deny".to_string(),
            probabilities: BTreeMap::from([
                ("allow".to_string(), 0.8),
                ("deny".to_string(), 0.1),
                ("escalate".to_string(), 0.1),
            ]),
            confidence: 0.8,
        },
    );
    assert!(request.validate_response(&non_argmax).is_err());

    // Probabilities do not sum to 1.0
    let mut bad_sum = sample_response();
    bad_sum.answers.insert(
        "verdict".to_string(),
        DecisionAnswer::Choice {
            choice: "allow".to_string(),
            probabilities: BTreeMap::from([
                ("allow".to_string(), 0.5),
                ("deny".to_string(), 0.1),
                ("escalate".to_string(), 0.1),
            ]),
            confidence: 0.5,
        },
    );
    assert!(request.validate_response(&bad_sum).is_err());

    // Score expected_score contradicts probability distribution
    let mut bad_expected = sample_response();
    bad_expected.answers.insert(
        "risk".to_string(),
        DecisionAnswer::Score {
            expected_score: 0.95,
            probabilities: BTreeMap::from([
                ("low".to_string(), 0.90),
                ("medium".to_string(), 0.05),
                ("high".to_string(), 0.05),
            ]),
            confidence: 0.90,
        },
    );
    assert!(request.validate_response(&bad_expected).is_err());

    // Score expected_score NaN
    let mut nan_score = sample_response();
    nan_score.answers.insert(
        "risk".to_string(),
        DecisionAnswer::Score {
            expected_score: f64::NAN,
            probabilities: BTreeMap::from([
                ("low".to_string(), 0.90),
                ("medium".to_string(), 0.05),
                ("high".to_string(), 0.05),
            ]),
            confidence: 0.90,
        },
    );
    assert!(request.validate_response(&nan_score).is_err());
}

#[test]
fn hundredths_rounded_simplex_and_negative_score_levels_are_accepted() {
    let mut questions = BTreeMap::new();
    questions.insert(
        "sentiment".to_string(),
        DecisionQuestion::Score {
            prompt: "Rate sentiment".to_string(),
            levels: vec![
                DecisionLevel {
                    id: "neg".to_string(),
                    description: "Negative".to_string(),
                    score: -1.0,
                },
                DecisionLevel {
                    id: "neu".to_string(),
                    description: "Neutral".to_string(),
                    score: 0.0,
                },
                DecisionLevel {
                    id: "pos".to_string(),
                    description: "Positive".to_string(),
                    score: 1.0,
                },
            ],
        },
    );
    let request = DecisionRequest::new(json!("neutral text"), questions);
    // 0.33 + 0.33 + 0.33 = 0.99, valid under hundredths rounding intervals
    let response = DecisionResponse {
        model: "jev-1.13.0".to_string(),
        provider: "typesafe".to_string(),
        calibration: CalibrationKind::Calibrated,
        answers: BTreeMap::from([(
            "sentiment".to_string(),
            DecisionAnswer::Score {
                expected_score: 0.0,
                probabilities: BTreeMap::from([
                    ("neg".to_string(), 0.33),
                    ("neu".to_string(), 0.33),
                    ("pos".to_string(), 0.33),
                ]),
                confidence: 0.34,
            },
        )]),
        usage: None,
    };
    request.validate_response(&response).unwrap();
}
