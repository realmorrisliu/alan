use alan_llm::{
    AnthropicMessagesClient, GenerationRequest, GoogleGeminiGenerateContentClient, LlmProvider,
    Message, MessageContentPart, OpenAiChatCompletionsClient, OpenAiResponsesClient,
    OpenRouterClient,
};

fn request_with_attachment(metadata: serde_json::Value) -> GenerationRequest {
    let mut message = Message::user("");
    message.content_parts = vec![MessageContentPart::Attachment {
        hash: "doc_hash".to_string(),
        mime_type: "application/pdf".to_string(),
        metadata,
    }];
    GenerationRequest {
        messages: vec![message],
        ..GenerationRequest::new()
    }
}

#[tokio::test]
async fn openai_chat_rejects_url_backed_document_before_dispatch() {
    let mut provider = OpenAiChatCompletionsClient::official_with_params(
        "unused",
        "https://example.invalid/v1",
        "gpt-5.4",
    );

    let error = provider
        .generate(request_with_attachment(serde_json::json!({
            "file_url": "https://example.com/spec.pdf"
        })))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("file_id"));
    assert!(error.to_string().contains("file_data"));
}

#[tokio::test]
async fn openai_responses_rejects_unreferenced_document_before_dispatch() {
    let mut provider =
        OpenAiResponsesClient::with_params("unused", "https://example.invalid/v1", "gpt-5.4");

    let error = provider
        .generate(request_with_attachment(serde_json::json!({})))
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("cannot represent document attachment")
    );
}

#[tokio::test]
async fn anthropic_rejects_unreferenced_document_before_dispatch() {
    let mut provider = AnthropicMessagesClient::with_params(
        "unused",
        "https://example.invalid/v1",
        "claude-sonnet-4-5",
    );

    let error = provider
        .generate(request_with_attachment(serde_json::json!({})))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("cannot represent attachment"));
}

#[tokio::test]
async fn openrouter_rejects_attachment_before_dispatch() {
    let mut provider =
        OpenRouterClient::with_params("unused", "https://example.invalid/v1", "openai/gpt-5.4")
            .unwrap();

    let error = provider
        .generate(request_with_attachment(serde_json::json!({
            "file_id": "file_123"
        })))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("cannot represent attachment"));
}

#[tokio::test]
async fn gemini_rejects_attachment_before_dispatch() {
    let mut provider =
        GoogleGeminiGenerateContentClient::with_params("unused", "us-central1", "gemini-2.5-pro");

    let error = provider
        .generate(request_with_attachment(serde_json::json!({
            "file_id": "file_123"
        })))
        .await
        .unwrap_err();

    assert!(error.to_string().contains("cannot represent attachment"));
}
