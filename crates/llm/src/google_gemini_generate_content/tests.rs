use super::*;
use crate::ReasoningEffort;

#[test]
fn test_gemini_client_with_params() {
    let client = GoogleGeminiGenerateContentClient::with_params(
        "test-project",
        "us-central1",
        "gemini-2.0-flash",
    );
    assert_eq!(
        client.endpoint_url(),
        "https://us-central1-aiplatform.googleapis.com/v1/projects/test-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent"
    );
    assert_eq!(
        client.stream_endpoint_url(),
        "https://us-central1-aiplatform.googleapis.com/v1/projects/test-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:streamGenerateContent?alt=sse"
    );
}

#[test]
fn test_global_vs_regional_vs_express_urls() {
    let global = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: Some("my-proj".to_string()),
        location: "global".to_string(),
        model: "gemini-3-pro-preview".to_string(),
        auth: VertexAuth::Adc,
        base_url: None,
    })
    .unwrap();
    assert_eq!(
        global.endpoint_url(),
        "https://aiplatform.googleapis.com/v1/projects/my-proj/locations/global/publishers/google/models/gemini-3-pro-preview:generateContent"
    );
    assert!(!global.endpoint_url().contains("global-aiplatform"));

    let regional = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: Some("my-proj".to_string()),
        location: "europe-west4".to_string(),
        model: "gemini-2.5-pro".to_string(),
        auth: VertexAuth::Adc,
        base_url: None,
    })
    .unwrap();
    assert_eq!(
        regional.endpoint_url(),
        "https://europe-west4-aiplatform.googleapis.com/v1/projects/my-proj/locations/europe-west4/publishers/google/models/gemini-2.5-pro:generateContent"
    );

    let express = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: "gemini-2.5-flash".to_string(),
        auth: VertexAuth::ApiKey("vk-test-key".to_string()),
        base_url: None,
    })
    .unwrap();
    assert_eq!(
        express.endpoint_url(),
        "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:generateContent"
    );
    assert_eq!(
        express.stream_endpoint_url(),
        "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
    );
}

#[test]
fn test_from_vertex_config_validates_auth_requirements() {
    let missing_project =
        GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
            project_id: None,
            location: "us-central1".to_string(),
            model: "gemini-2.5-pro".to_string(),
            auth: VertexAuth::Adc,
            base_url: None,
        });
    assert!(missing_project.is_err());

    let empty_key = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: "gemini-2.5-pro".to_string(),
        auth: VertexAuth::ApiKey("   ".to_string()),
        base_url: None,
    });
    assert!(empty_key.is_err());
}

#[test]
fn test_part_text() {
    let part = Part::text("Hello, world!");
    assert_eq!(part.text, Some("Hello, world!".to_string()));
    assert!(part.function_call.is_none());
    assert!(part.function_response.is_none());
    assert!(!part.is_thought());
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

    assert_eq!(config.include_thoughts, Some(true));
    assert_eq!(config.thinking_level.as_deref(), Some("medium"));
    assert_eq!(config.thinking_budget, None);
}

#[test]
fn test_build_gemini_thinking_config_maps_gemini_25_effort_to_budget() {
    let config = build_gemini_thinking_config("gemini-2.5-flash", Some(ReasoningEffort::High))
        .unwrap()
        .unwrap();

    assert_eq!(config.include_thoughts, Some(true));
    assert_eq!(config.thinking_level, None);
    assert_eq!(config.thinking_budget, Some(8192));
}

#[test]
fn test_build_gemini_thinking_config_omits_budget_when_effort_unset() {
    let config = build_gemini_thinking_config("gemini-2.5-flash", None)
        .unwrap()
        .unwrap();
    assert_eq!(config.include_thoughts, Some(true));
    assert_eq!(config.thinking_budget, None);
    assert_eq!(config.thinking_level, None);
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
        tool_config: None,
        generation_config: Some(GenerationConfig {
            temperature: Some(0.7),
            max_output_tokens: Some(100),
            top_p: None,
            top_k: None,
            thinking_config: Some(ThinkingConfig {
                include_thoughts: Some(true),
                thinking_level: Some("low".to_string()),
                thinking_budget: None,
            }),
            ..Default::default()
        }),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("contents"));
    assert!(json.contains("generationConfig"));
    assert!(json.contains("thinkingConfig"));
    assert!(json.contains("includeThoughts"));
    assert!(json.contains("thinkingLevel"));
    assert!(json.contains("0.7"));
}

#[test]
fn test_generate_content_response_deserialization_and_usage_mapping() {
    let json = r#"{
        "responseId": "resp-gemini-1",
        "candidates": [{
            "content": {
                "role": "model",
                "parts": [
                    {"text": "thinking...", "thought": true},
                    {"text": "Hello!", "thoughtSignature": "sig-final"}
                ]
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 100,
            "cachedContentTokenCount": 40,
            "candidatesTokenCount": 25,
            "thoughtsTokenCount": 15,
            "totalTokenCount": 140
        }
    }"#;

    let response: GoogleGeminiGenerateContentResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.response_id.as_deref(), Some("resp-gemini-1"));
    assert_eq!(response.candidates.len(), 1);
    let usage = response.usage_metadata.unwrap().to_token_usage();
    assert_eq!(usage.prompt_tokens, 100);
    assert_eq!(usage.cached_prompt_tokens, Some(40));
    assert_eq!(usage.reasoning_tokens, Some(15));
    assert_eq!(usage.completion_tokens, 40);
    assert_eq!(usage.total_tokens, 140);
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
fn function_declaration_serializes_full_json_schema() {
    let declaration = FunctionDeclaration {
        name: "test_function".to_string(),
        description: "A test function".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "arg1": {"type": "string"}
            },
            "oneOf": [
                {"properties": {"arg1": {"const": "submit"}}},
                {"not": {"required": ["arg1"]}}
            ],
            "additionalProperties": false
        }),
    };

    assert_eq!(declaration.name, "test_function");
    assert_eq!(declaration.description, "A test function");
    let wire = serde_json::to_value(&declaration).unwrap();
    assert_eq!(wire["parametersJsonSchema"], declaration.parameters);
    assert!(wire.get("parameters").is_none());
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

#[test]
fn rejects_unqualified_extended_reasoning_efforts() {
    for effort in [ReasoningEffort::Max, ReasoningEffort::Ultra] {
        assert!(build_gemini_thinking_config("gemini-2.5-flash", Some(effort)).is_err());
    }
}

#[tokio::test]
async fn express_mode_model_catalog_returns_none_while_baseline_exposes_gemini_25_and_3_models() {
    let client = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: "gemini-2.5-pro".to_string(),
        auth: VertexAuth::ApiKey("test-express-key".to_string()),
        base_url: None,
    })
    .unwrap();
    assert!(
        client.model_catalog().await.unwrap().is_none(),
        "Express Mode skips publisher listing so callers fall back to curated baseline"
    );

    let baseline = crate::gemini_catalog::vertex_baseline_catalog();
    let slugs: Vec<&str> = baseline.iter().map(|m| m.slug.as_str()).collect();
    assert!(slugs.contains(&"gemini-2.5-pro"));
    assert!(slugs.contains(&"gemini-2.5-flash"));
    assert!(slugs.contains(&"gemini-2.5-flash-lite"));
    assert!(slugs.contains(&"gemini-3-pro-preview"));
    assert!(slugs.contains(&"gemini-3-flash-preview"));
    assert!(slugs.contains(&"gemini-3.1-pro-preview"));
    assert!(slugs.contains(&"gemini-3.1-flash-lite-preview"));
}
