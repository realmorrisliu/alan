use alan_llm::{
    GenerationRequest, LlmProvider, Message, MessageContentPart, OpenAiChatCompletionsClient,
};

#[tokio::test]
async fn openai_chat_rejects_url_backed_document_before_dispatch() {
    let mut message = Message::user("");
    message.content_parts = vec![MessageContentPart::Attachment {
        hash: "doc_hash".to_string(),
        mime_type: "application/pdf".to_string(),
        metadata: serde_json::json!({"file_url": "https://example.com/spec.pdf"}),
    }];
    let request = GenerationRequest {
        messages: vec![message],
        ..GenerationRequest::new()
    };
    let mut provider = OpenAiChatCompletionsClient::official_with_params(
        "unused",
        "https://example.invalid/v1",
        "gpt-5.4",
    );

    let error = provider.generate(request).await.unwrap_err();

    assert!(error.to_string().contains("file_id"));
    assert!(error.to_string().contains("file_data"));
}
