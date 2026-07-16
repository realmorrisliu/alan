use super::*;

#[test]
fn test_generation_request_builder() {
    let request = GenerationRequest::new()
        .with_system_prompt("You are helpful")
        .with_user_message("Hello")
        .with_temperature(0.7)
        .with_max_tokens(100);

    assert_eq!(request.system_prompt, Some("You are helpful".to_string()));
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].content, "Hello");
    assert_eq!(request.temperature, Some(0.7));
    assert_eq!(request.max_tokens, Some(100));
}

#[test]
fn test_generation_request_builder_responses_helpers() {
    let request = GenerationRequest::new()
        .with_previous_response_id("resp_prev")
        .with_store(true)
        .with_background(true)
        .with_include(["reasoning.encrypted_content"])
        .with_context_management_compact_threshold(8192);

    assert_eq!(
        request.extra_params.get("previous_response_id"),
        Some(&serde_json::json!("resp_prev"))
    );
    assert_eq!(
        request.extra_params.get("store"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        request.extra_params.get("background"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        request.extra_params.get("include"),
        Some(&serde_json::json!(["reasoning.encrypted_content"]))
    );
    assert_eq!(
        request.extra_params.get("context_management"),
        Some(&serde_json::json!({ "compact_threshold": 8192 }))
    );
}

#[test]
fn test_generation_request_reasoning_controls_set_canonical_controls() {
    let cleared = GenerationRequest::new().with_reasoning_controls(ReasoningControls::default());

    assert_eq!(cleared.reasoning, ReasoningControls::default());

    let efforted = GenerationRequest::new().with_reasoning_controls(ReasoningControls {
        effort: Some(ReasoningEffort::High),
    });

    assert_eq!(efforted.reasoning.effort, Some(ReasoningEffort::High));
}

#[test]
fn test_message_helpers() {
    let sys = Message::system("System prompt");
    assert_eq!(sys.role, MessageRole::System);
    assert_eq!(sys.content, "System prompt");

    let user = Message::user("User message");
    assert_eq!(user.role, MessageRole::User);

    let assistant = Message::assistant("Assistant reply");
    assert_eq!(assistant.role, MessageRole::Assistant);

    let tool = Message::tool("call-123", "Tool result");
    assert_eq!(tool.role, MessageRole::Tool);
    assert_eq!(tool.tool_call_id, Some("call-123".to_string()));
}

#[test]
fn test_tool_definition_builder() {
    let tool = ToolDefinition::new("search", "Search the web")
        .with_string_param("query", "The search query");

    assert_eq!(tool.name, "search");
    assert_eq!(tool.description, "Search the web");
    assert!(tool.parameters["properties"].get("query").is_some());
    assert!(
        tool.parameters["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("query"))
    );
}

#[test]
fn test_tool_call_builder() {
    let call = ToolCall::new("my_tool", serde_json::json!({"arg": "value"})).with_id("call-123");

    assert_eq!(call.name, "my_tool");
    assert_eq!(call.id, Some("call-123".to_string()));
    assert_eq!(call.arguments["arg"], "value");
}

#[test]
fn test_factory_config() {
    let gemini =
        factory::ProviderConfig::google_gemini_generate_content("my-project", "gemini-pro");
    assert_eq!(
        gemini.provider_type,
        factory::ProviderType::GoogleGeminiGenerateContent
    );
    assert_eq!(gemini.project_id, Some("my-project".to_string()));

    let chatgpt = factory::ProviderConfig::chatgpt("gpt-5.3-codex");
    assert_eq!(
        chatgpt.provider_type,
        factory::ProviderType::ChatgptResponses
    );
    assert_eq!(
        chatgpt.base_url,
        Some("https://chatgpt.com/backend-api/codex".to_string())
    );
    assert_eq!(chatgpt.api_key, None);

    let openai_responses = factory::ProviderConfig::openai_responses("sk-xxx", "gpt-5.4");
    assert_eq!(
        openai_responses.provider_type,
        factory::ProviderType::OpenAiResponses
    );
    assert_eq!(openai_responses.api_key, Some("sk-xxx".to_string()));

    let openai_chat_completions =
        factory::ProviderConfig::openai_chat_completions("sk-openai-chat", "gpt-4.1");
    assert_eq!(
        openai_chat_completions.provider_type,
        factory::ProviderType::OpenAiChatCompletions
    );
    assert_eq!(
        openai_chat_completions.api_key,
        Some("sk-openai-chat".to_string())
    );

    let openai_chat_completions_compatible =
        factory::ProviderConfig::openai_chat_completions_compatible("sk-compat", "qwen3.5-plus");
    assert_eq!(
        openai_chat_completions_compatible.provider_type,
        factory::ProviderType::OpenAiChatCompletionsCompatible
    );
    assert_eq!(
        openai_chat_completions_compatible.api_key,
        Some("sk-compat".to_string())
    );

    let anthropic_messages =
        factory::ProviderConfig::anthropic_messages("sk-ant", "claude-3-5-sonnet");
    assert_eq!(
        anthropic_messages.provider_type,
        factory::ProviderType::AnthropicMessages
    );
    assert_eq!(anthropic_messages.api_key, Some("sk-ant".to_string()));

    let openrouter = factory::ProviderConfig::openrouter("sk-or-xxx", "anthropic/claude-3-opus")
        .with_http_referer("https://alan.local")
        .with_x_title("alan")
        .with_app_categories(["cli-agent"]);
    assert_eq!(openrouter.provider_type, factory::ProviderType::OpenRouter);
    assert_eq!(openrouter.api_key, Some("sk-or-xxx".to_string()));
    assert_eq!(
        openrouter.base_url,
        Some("https://openrouter.ai/api/v1".to_string())
    );
    assert_eq!(
        openrouter.http_referer.as_deref(),
        Some("https://alan.local")
    );
    assert_eq!(openrouter.x_title.as_deref(), Some("alan"));
    assert_eq!(
        openrouter.app_categories,
        Some(vec!["cli-agent".to_string()])
    );
}

#[test]
fn test_provider_capabilities_distinguish_provider_families() {
    let chatgpt = factory::ProviderType::ChatgptResponses.capabilities();
    let openai_responses = factory::ProviderType::OpenAiResponses.capabilities();
    let openai_chat = factory::ProviderType::OpenAiChatCompletions.capabilities();
    let openrouter = factory::ProviderType::OpenRouter.capabilities();
    let anthropic = factory::ProviderType::AnthropicMessages.capabilities();

    assert_eq!(
        chatgpt.compatibility_tier,
        CompatibilityTier::TierCBestEffortCompatible
    );
    assert!(!chatgpt.supports_server_managed_continuation);
    assert!(!chatgpt.supports_provider_compaction);
    assert_eq!(
        chatgpt.instruction_role,
        InstructionRole::ResponsesInstructions
    );

    assert!(openai_responses.supports_server_managed_continuation);
    assert!(openai_responses.supports_background_execution);
    assert!(openai_responses.supports_retrieve_cancel);
    assert!(openai_responses.supports_provider_compaction);
    assert_eq!(
        openai_responses.instruction_role,
        InstructionRole::ResponsesInstructions
    );

    assert_eq!(
        openai_chat.compatibility_tier,
        CompatibilityTier::TierBFullFidelityStateless
    );
    assert_eq!(openai_chat.instruction_role, InstructionRole::Developer);
    assert!(openai_chat.supports_multimodal_input);
    assert!(!openai_chat.supports_server_managed_continuation);

    assert_eq!(
        openrouter.compatibility_tier,
        CompatibilityTier::TierCBestEffortCompatible
    );
    assert!(openrouter.supports_streaming_text);
    assert!(openrouter.supports_streaming_tool_calls);
    assert!(openrouter.supports_provider_response_id);
    assert!(openrouter.supports_reasoning_text);
    assert!(!openrouter.supports_server_managed_continuation);
    assert!(!openrouter.supports_provider_compaction);

    assert_eq!(
        anthropic.compatibility_tier,
        CompatibilityTier::TierBFullFidelityStateless
    );
    assert_eq!(anthropic.instruction_role, InstructionRole::AnthropicSystem);
    assert!(anthropic.supports_multimodal_input);
    assert!(anthropic.supports_document_input);
    assert!(anthropic.supports_redacted_thinking);
}

#[tokio::test]
async fn test_mock_provider() {
    use crate::LlmProvider;

    let mut mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: "Test response".to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: vec![],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });

    let request = GenerationRequest::new().with_user_message("Hello");
    let response: GenerationResponse = LlmProvider::generate(&mut mock, request).await.unwrap();

    assert_eq!(response.content, "Test response");

    // Verify request was recorded
    let recorded: Vec<GenerationRequest> = mock.recorded_requests();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].messages[0].content, "Hello");
}

#[tokio::test]
async fn test_mock_provider_multiple_responses() {
    use crate::LlmProvider;

    let mut mock = MockLlmProvider::new().with_responses(vec![
        GenerationResponse {
            content: "First".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
        GenerationResponse {
            content: "Second".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    ]);

    let r1: GenerationResponse = LlmProvider::generate(&mut mock, GenerationRequest::new())
        .await
        .unwrap();
    let r2: GenerationResponse = LlmProvider::generate(&mut mock, GenerationRequest::new())
        .await
        .unwrap();

    assert_eq!(r1.content, "First");
    assert_eq!(r2.content, "Second");
}

#[tokio::test]
async fn test_mock_provider_chat() {
    use crate::LlmProvider;

    let mut mock = MockLlmProvider::new();
    let response: String = LlmProvider::chat(&mut mock, Some("System"), "Hello")
        .await
        .unwrap();

    assert!(response.contains("Mock response to:"));
    assert!(response.contains("Hello"));
}

#[tokio::test]
async fn test_mock_provider_stream() {
    use crate::LlmProvider;

    let mut mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: "Streamed".to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: vec![],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });

    let mut rx: tokio::sync::mpsc::Receiver<StreamChunk> =
        LlmProvider::generate_stream(&mut mock, GenerationRequest::new())
            .await
            .unwrap();
    let first_chunk: StreamChunk = rx.recv().await.unwrap();
    let final_chunk: StreamChunk = rx.recv().await.unwrap();

    assert_eq!(first_chunk.text, Some("Streamed".to_string()));
    assert!(!first_chunk.is_finished);
    assert!(final_chunk.is_finished);
    assert_eq!(final_chunk.finish_reason.as_deref(), Some("stop"));
}
