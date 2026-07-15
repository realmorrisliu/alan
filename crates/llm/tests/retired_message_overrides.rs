use alan_llm::{
    AnthropicMessagesClient, GenerationRequest, LlmProvider, OpenAiChatCompletionsClient,
    OpenAiResponsesClient,
};

fn request_with_retired_override(key: &str) -> GenerationRequest {
    GenerationRequest::new()
        .with_user_message("hello")
        .with_extra_param(key, serde_json::json!([]))
}

async fn assert_rejected_before_dispatch(provider: &mut dyn LlmProvider, key: &str) {
    let error = match provider.generate(request_with_retired_override(key)).await {
        Ok(_) => panic!("{key} should be rejected before provider dispatch"),
        Err(error) => error,
    };
    assert!(error.to_string().contains(key));
    assert!(error.to_string().contains("Message::content_parts"));
}

#[tokio::test]
async fn openai_responses_rejects_retired_input_override() {
    let mut provider =
        OpenAiResponsesClient::with_params("unused", "https://example.invalid/v1", "gpt-5.4");
    assert_rejected_before_dispatch(&mut provider, "responses_input_items").await;
}

#[tokio::test]
async fn openai_chat_rejects_retired_input_override() {
    let mut provider = OpenAiChatCompletionsClient::official_with_params(
        "unused",
        "https://example.invalid/v1",
        "gpt-5.4",
    );
    assert_rejected_before_dispatch(&mut provider, "chat_completions_messages").await;
}

#[tokio::test]
async fn anthropic_rejects_retired_input_override() {
    let mut provider = AnthropicMessagesClient::with_params(
        "unused",
        "https://example.invalid/v1",
        "claude-sonnet-4-5",
    );
    assert_rejected_before_dispatch(&mut provider, "anthropic_messages").await;
}
