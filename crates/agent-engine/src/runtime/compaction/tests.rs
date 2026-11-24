use super::*;
use std::sync::{Arc, Mutex};

use alan_agent_protocol::Event;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use alan_llmfs::LlmFs;
use tokio::sync::mpsc;

use crate::{
    agent_machine::AgentMachine,
    config::Config,
    runtime::{
        NamespaceRuntimeEnvironment, RuntimeConfig, prompt_cache::PromptAssemblyCache,
        transition::RuntimeLoopState,
    },
};

struct RecordingProvider {
    requests: Arc<Mutex<Vec<GenerationRequest>>>,
    response: String,
}

#[async_trait::async_trait]
impl LlmProvider for RecordingProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }

    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        self.requests.lock().unwrap().push(request);
        let (tx, rx) = mpsc::channel(4);
        let response = self.response.clone();
        tokio::spawn(async move {
            let _ = tx
                .send(StreamChunk {
                    text: Some(response),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "recording"
    }
}

fn namespace_state_with_provider(provider: impl LlmProvider + 'static) -> RuntimeLoopState {
    let llmfs = Arc::new(LlmFs::new());
    llmfs.register_connection("default", Box::new(provider));

    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));

    RuntimeLoopState {
        machine: AgentMachine::new(),
        environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        core_config: Config::default(),
        runtime_config: RuntimeConfig {
            compaction_trigger_messages: 1,
            compaction_keep_last: 1,
            ..RuntimeConfig::default()
        },
        prompt_cache: PromptAssemblyCache::new(Vec::new()),
    }
}

#[tokio::test]
async fn compaction_generation_uses_namespace_llmfs() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut state = namespace_state_with_provider(RecordingProvider {
        requests: Arc::clone(&requests),
        response: "namespace compaction summary".to_string(),
    });
    state.core_config.memory.enabled = false;
    state.machine.add_user_message("first detail to compact");
    state.machine.add_assistant_message("first answer", None);
    state.machine.add_user_message("second detail to compact");
    state.machine.add_assistant_message("second answer", None);

    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let runtime = crate::runtime::transition::compaction_runtime(&mut state);
    let outcome = maybe_compact_context_with_cancel(
        runtime,
        &mut emit,
        &CompactionRequest::manual(None),
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    assert!(matches!(outcome, CompactionOutcome::Applied { .. }));
    assert_eq!(
        state.machine.tape_summary(),
        Some("namespace compaction summary")
    );
    let recorded = requests.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].max_tokens, Some(COMPACTION_MAX_TOKENS));
    assert_eq!(
        recorded[0].system_prompt.as_deref(),
        Some(prompts::COMPACT_PROMPT)
    );
    assert!(
        recorded[0]
            .messages
            .iter()
            .any(|message| message.content.contains("first detail to compact"))
    );
    let last = recorded[0].messages.last().unwrap();
    assert_eq!(last.role, crate::llm::MessageRole::User);
    assert_eq!(last.content, COMPACTION_TRAILING_DIRECTIVE);
}

struct SequencedRecordingProvider {
    requests: Arc<Mutex<Vec<GenerationRequest>>>,
    responses: Arc<Mutex<std::collections::VecDeque<String>>>,
}

#[async_trait::async_trait]
impl LlmProvider for SequencedRecordingProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        unimplemented!()
    }

    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        unimplemented!()
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        self.requests.lock().unwrap().push(request);
        let response = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_default();
        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            let _ = tx
                .send(StreamChunk {
                    text: Some(response),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "sequenced_recording"
    }
}

#[tokio::test]
async fn empty_summary_retries_with_trimming_and_protects_directive() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let responses = Arc::new(Mutex::new(std::collections::VecDeque::from(vec![
        String::new(),
        "   \n  ".to_string(),
        "recovered summary after empty responses".to_string(),
    ])));
    let mut state = namespace_state_with_provider(SequencedRecordingProvider {
        requests: Arc::clone(&requests),
        responses,
    });
    state.core_config.memory.enabled = false;
    state
        .machine
        .push_tape_message(crate::tape::Message::user("msg-1"));
    state
        .machine
        .push_tape_message(crate::tape::Message::assistant_with_tools(
            "",
            vec![crate::tape::ToolRequest {
                id: "call_1".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path": "a.rs"}),
            }],
        ));
    state
        .machine
        .push_tape_message(crate::tape::Message::tool_text("call_1", "file contents"));
    state
        .machine
        .push_tape_message(crate::tape::Message::user("msg-2"));
    state
        .machine
        .push_tape_message(crate::tape::Message::assistant("keep-last-msg"));

    let mut emit = |_event: Event| async {};
    let runtime = crate::runtime::transition::compaction_runtime(&mut state);
    let outcome = maybe_compact_context_with_cancel(
        runtime,
        &mut emit,
        &CompactionRequest::manual(Some("preserve call_1".to_string())),
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    match outcome {
        CompactionOutcome::Applied(applied) => {
            assert_eq!(applied.result, CompactionResult::Retry);
            assert_eq!(applied.retry_count, 2);
        }
        other => panic!("expected Applied after empty-summary retries, got {other:?}"),
    }

    let recorded = requests.lock().unwrap();
    assert_eq!(recorded.len(), 3);
    for req in recorded.iter() {
        assert_eq!(req.max_tokens, Some(COMPACTION_MAX_TOKENS));
        assert!(req.messages.iter().all(|m| {
            m.tool_calls.is_none()
                && m.tool_call_id.is_none()
                && m.thinking.is_none()
                && !matches!(m.role, crate::llm::MessageRole::Tool)
        }));
        // Focus Context message and trailing User directive are always preserved
        assert_eq!(req.messages[0].role, crate::llm::MessageRole::Context);
        let last = req.messages.last().unwrap();
        assert_eq!(last.role, crate::llm::MessageRole::User);
        assert_eq!(last.content, COMPACTION_TRAILING_DIRECTIVE);
    }
    assert_eq!(recorded[0].messages.len(), 6);
    assert_eq!(recorded[1].messages.len(), 5);
    assert_eq!(recorded[2].messages.len(), 4);
}

#[tokio::test]
async fn persistent_empty_summary_exhausts_retries_without_mutating_tape_below_hard_limit() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let responses = Arc::new(Mutex::new(std::collections::VecDeque::from(vec![
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ])));
    let mut state = namespace_state_with_provider(SequencedRecordingProvider {
        requests: Arc::clone(&requests),
        responses,
    });
    state.core_config.memory.enabled = false;
    state.machine.add_user_message("u1");
    state.machine.add_assistant_message("a1", None);
    state.machine.add_user_message("u2");
    state.machine.add_assistant_message("a2", None);

    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };
    let runtime = crate::runtime::transition::compaction_runtime(&mut state);
    let outcome = maybe_compact_context_with_cancel(
        runtime,
        &mut emit,
        &CompactionRequest::manual(None),
        &CancellationToken::new(),
    )
    .await
    .unwrap();

    match outcome {
        CompactionOutcome::Failed(failed) => {
            // 3 history messages before keep_last=1 -> trims down to 1 remaining -> 2 retries
            assert_eq!(failed.retry_count, 2);
            assert_eq!(failed.result, CompactionResult::Failure);
        }
        other => panic!("expected Failed after exhausting retries, got {other:?}"),
    }
    assert!(state.machine.tape_summary().is_none());
    assert_eq!(state.machine.tape_len(), 4);
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Warning { message }
            if message.contains("after 2 retry attempt(s): compaction summary was empty")
                && message.contains("Preserving existing context.")
    )));
}
