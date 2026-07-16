use super::*;

#[test]
fn test_gemini_client_with_params() {
    let client = GoogleGeminiGenerateContentClient::with_params(
        "test-project",
        "us-central1",
        "gemini-2.0-flash",
    );
    drop(client);
}

#[test]
fn test_part_text() {
    let part = Part::text("Hello, world!");
    assert_eq!(part.text, Some("Hello, world!".to_string()));
    assert!(part.function_call.is_none());
    assert!(part.function_response.is_none());
}

#[test]
fn test_part_function_call() {
    let args = serde_json::json!({"query": "test"});
    let part = Part::function_call("web_search", args.clone());
    assert!(part.text.is_none());
    assert!(part.function_call.is_some());
    let function_call = part.function_call.unwrap();
    assert_eq!(function_call.name, "web_search");
    assert_eq!(function_call.args, args);
}

#[test]
fn test_part_function_response() {
    let response = serde_json::json!({"result": "success"});
    let part = Part::function_response("my_tool", response.clone());
    assert!(part.text.is_none());
    assert!(part.function_response.is_some());
    let function_response = part.function_response.unwrap();
    assert_eq!(function_response.name, "my_tool");
    assert_eq!(function_response.response, response);
}

#[test]
fn test_content_user() {
    let content = Content::user(vec![Part::text("Hello")]);
    assert_eq!(content.role, Some("user".to_string()));
    assert_eq!(content.parts.len(), 1);
}

#[test]
fn test_content_model() {
    let content = Content::model(vec![Part::text("Response")]);
    assert_eq!(content.role, Some("model".to_string()));
    assert_eq!(content.parts.len(), 1);
}

#[test]
fn test_content_function() {
    let content = Content::function(vec![Part::function_response("tool", serde_json::json!({}))]);
    assert_eq!(content.role, Some("function".to_string()));
    assert_eq!(content.parts.len(), 1);
}

#[test]
fn test_generation_config_default() {
    let config = GenerationConfig::default();
    assert!(config.temperature.is_none());
    assert!(config.max_output_tokens.is_none());
    assert!(config.top_p.is_none());
    assert!(config.top_k.is_none());
    assert!(config.thinking_config.is_none());
}

#[test]
fn test_build_gemini_thinking_config_maps_gemini_3_effort_to_level() {
    let config =
        build_gemini_thinking_config("gemini-3-flash-preview", Some(ReasoningEffort::Medium))
            .unwrap()
            .unwrap();

    assert_eq!(config.thinking_level.as_deref(), Some("medium"));
    assert_eq!(config.thinking_budget, None);
}

#[test]
fn test_build_gemini_thinking_config_maps_gemini_25_effort_to_budget() {
    let config = build_gemini_thinking_config("gemini-2.5-flash", Some(ReasoningEffort::High))
        .unwrap()
        .unwrap();

    assert_eq!(config.thinking_level, None);
    assert_eq!(config.thinking_budget, Some(8192));
}

#[test]
fn test_build_gemini_thinking_config_omits_budget_when_effort_unset() {
    let config = build_gemini_thinking_config("gemini-2.5-flash", None).unwrap();
    assert!(config.is_none());
}

#[test]
fn test_select_primary_candidate_prefers_index_zero() {
    let candidates = vec![
        Candidate {
            content: Some(Content::model(vec![Part::text("secondary")])),
            finish_reason: Some("STOP".to_string()),
            index: Some(2),
            safety_ratings: vec![],
        },
        Candidate {
            content: Some(Content::model(vec![Part::text("primary")])),
            finish_reason: Some("STOP".to_string()),
            index: Some(0),
            safety_ratings: vec![],
        },
    ];

    let selected = select_primary_candidate(&candidates).expect("expected candidate");
    assert_eq!(selected.index, Some(0));
    assert_eq!(
        selected
            .content
            .as_ref()
            .and_then(|content| content.parts.first())
            .and_then(|part| part.text.as_deref()),
        Some("primary")
    );
}

#[test]
fn test_generate_content_request_serialization() {
    let request = GoogleGeminiGenerateContentRequest {
        contents: vec![Content::user(vec![Part::text("Hello")])],
        system_instruction: None,
        tools: None,
        generation_config: Some(GenerationConfig {
            temperature: Some(0.7),
            max_output_tokens: Some(100),
            top_p: None,
            top_k: None,
            thinking_config: Some(ThinkingConfig {
                thinking_level: Some("low".to_string()),
                thinking_budget: None,
            }),
        }),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("contents"));
    assert!(json.contains("generationConfig"));
    assert!(json.contains("thinkingConfig"));
    assert!(json.contains("thinkingLevel"));
    assert!(json.contains("0.7"));
}

#[test]
fn test_generate_content_response_deserialization() {
    let json = r#"{
        "candidates": [{
            "content": {
                "role": "model",
                "parts": [{"text": "Hello!"}]
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 10,
            "candidatesTokenCount": 5,
            "totalTokenCount": 15
        }
    }"#;

    let response: GoogleGeminiGenerateContentResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.candidates.len(), 1);
    assert!(response.usage_metadata.is_some());
    let usage = response.usage_metadata.unwrap();
    assert_eq!(usage.prompt_token_count, Some(10));
    assert_eq!(usage.total_token_count, Some(15));
}

#[test]
fn test_safety_rating() {
    let json = r#"{
        "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT",
        "probability": "NEGLIGIBLE"
    }"#;

    let rating: SafetyRating = serde_json::from_str(json).unwrap();
    assert_eq!(rating.category, "HARM_CATEGORY_SEXUALLY_EXPLICIT");
    assert_eq!(rating.probability, "NEGLIGIBLE");
}

#[test]
fn test_prompt_feedback_deserialization() {
    let json = r#"{
        "blockReason": "SAFETY",
        "safetyRatings": [
            {"category": "HARM_CATEGORY_HARASSMENT", "probability": "HIGH"}
        ]
    }"#;

    let feedback: PromptFeedback = serde_json::from_str(json).unwrap();
    assert_eq!(feedback.block_reason, Some("SAFETY".to_string()));
    assert_eq!(feedback.safety_ratings.len(), 1);
}

#[test]
fn test_function_declaration() {
    let declaration = FunctionDeclaration {
        name: "test_function".to_string(),
        description: "A test function".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "arg1": {"type": "string"}
            }
        }),
    };

    assert_eq!(declaration.name, "test_function");
    assert_eq!(declaration.description, "A test function");
}

#[test]
fn test_tool_serialization() {
    let tool = Tool {
        function_declarations: vec![FunctionDeclaration {
            name: "func1".to_string(),
            description: "Function 1".to_string(),
            parameters: serde_json::json!({}),
        }],
    };

    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("functionDeclarations"));
}
