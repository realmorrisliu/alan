//! FileLlmProvider reads its generations from llmfs files (the engine-rewiring
//! seam of the Plan 9 program): the agent engine calls the same `LlmProvider`
//! trait, but the provider now does clone-via-open → write request → read the
//! `events` token stream over aP. Proven end-to-end against a real `alan-llmfs`
//! backed by the mock provider — no engine loop changes, no real API key.

use std::sync::Arc;

use alan_ap::InProcessTransport;
use alan_llm::mock::MockLlmProvider;
use alan_llm::{GenerationRequest, LlmProvider};
use alan_llmfs::LlmFs;
use alan_llmfs_client::FileLlmProvider;

fn llmfs_transport() -> InProcessTransport {
    let fs = LlmFs::new();
    fs.register_connection("default", Box::new(MockLlmProvider::new()));
    InProcessTransport::new(Arc::new(fs))
}

#[tokio::test]
async fn generate_reads_the_response_through_files() {
    let mut provider = FileLlmProvider::new(llmfs_transport(), "default");
    let response = provider
        .generate(GenerationRequest::new().with_user_message("hi"))
        .await
        .expect("generation over files");
    assert!(
        response.content.contains("Mock response"),
        "got: {:?}",
        response.content
    );
}

#[tokio::test]
async fn generate_stream_forwards_tokens_from_the_events_file() {
    let mut provider = FileLlmProvider::new(llmfs_transport(), "default");
    let mut rx = provider
        .generate_stream(GenerationRequest::new().with_user_message("hi"))
        .await
        .expect("stream over files");

    let mut text = String::new();
    let mut saw_final = false;
    while let Some(chunk) = rx.recv().await {
        if let Some(t) = chunk.text {
            text.push_str(&t);
        }
        if chunk.is_finished {
            saw_final = true;
            break;
        }
    }
    assert!(text.contains("Mock response"), "streamed text: {text:?}");
    assert!(saw_final, "stream must end with a finished chunk");
}

#[tokio::test]
async fn the_engine_sees_it_as_an_ordinary_llm_provider() {
    // The whole point: an engine holding `&mut dyn LlmProvider` is unchanged —
    // it just happens to read its LLM from files now.
    let mut provider = FileLlmProvider::new(llmfs_transport(), "default");
    let dynamic: &mut dyn LlmProvider = &mut provider;
    assert_eq!(dynamic.provider_name(), "llmfs");
    let reply = dynamic
        .chat(Some("be brief"), "hello")
        .await
        .expect("chat over files");
    assert!(reply.contains("Mock response"), "got: {reply:?}");
}
