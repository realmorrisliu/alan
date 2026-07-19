use super::*;
use crate::agent_machine::DeferredRuntimeAction;
use crate::runtime::{RuntimeConfig, memory_promotion};
use alan_agent_protocol::{ContentPart, Op};
use alan_ap::InProcessTransport;
use alan_llm::{
    GenerationRequest, GenerationResponse, LlmProvider, MockLlmProvider, StreamChunk, TokenUsage,
    ToolCallDelta,
};
use anyhow::anyhow;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

struct PackageTestTool {
    name: &'static str,
    description: &'static str,
}

impl crate::tools::Tool for PackageTestTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    fn execute(
        &self,
        _arguments: serde_json::Value,
        _ctx: &crate::tools::ToolContext,
    ) -> crate::tools::ToolResult {
        Box::pin(async { Ok(serde_json::json!({"ok": true})) })
    }
}

fn single_file_fs(name: &str, bytes: &[u8]) -> Arc<alan_ap::reference::MemFs> {
    Arc::new(alan_ap::reference::MemFs::with_read_only_file(name, bytes))
}

fn namespace_environment_for_test() -> NamespaceRuntimeEnvironment {
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
        alan_kernel::Namespace::new(),
    )));
    crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
}

fn make_deferred_action_for_test() -> DeferredRuntimeAction {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");

    let mut machine = AgentMachine::new();
    machine.add_user_message("My name is Morris.");

    machine.begin_turn(0);

    let mut core_config = crate::Config::default();
    core_config.memory.enabled = true;
    core_config.memory.store_dir = Some(memory_dir);
    let runtime_config = RuntimeConfig::from(&core_config);

    let state = RuntimeLoopState {
        machine,
        environment: namespace_environment_for_test(),
        core_config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    memory_promotion::build_turn_memory_promotion_job(&state, "queue ordering test")
        .map(DeferredRuntimeAction::TurnMemoryPromotion)
        .expect("build deferred memory promotion job")
}

fn queue_item_kinds(queue: &VecDeque<QueuedRuntimeItem>) -> Vec<&'static str> {
    queue
        .iter()
        .map(|item| match item {
            QueuedRuntimeItem::Submission(_) => "submission",
            QueuedRuntimeItem::Deferred(_) => "deferred",
        })
        .collect()
}

#[tokio::test]
async fn namespace_discovery_ignores_incomplete_tool_packages() {
    let manifest = crate::runtime::ToolPackageManifest::from_tool(
        &PackageTestTool {
            name: "hidden",
            description: "Hidden Tool",
        },
        30,
    )
    .unwrap();
    let manifest_fs = single_file_fs("manifest", &serde_json::to_vec(&manifest).unwrap());

    let mut mounts = alan_kernel::Namespace::new();
    mounts.mount(
        "/bin/ordinary",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        alan_kernel::Access::ReadOnly,
    );
    mounts.mount(
        "/lib/exec/hidden",
        InProcessTransport::new(manifest_fs),
        alan_kernel::Access::ReadOnly,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(mounts)));
    let environment = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    assert!(
        environment
            .discover_tool_packages()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn namespace_discovery_rejects_invalid_mounted_manifest() {
    let mut mounts = alan_kernel::Namespace::new();
    mounts.mount(
        "/bin/broken",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        alan_kernel::Access::ReadOnly,
    );
    mounts.mount(
        "/lib/exec/broken",
        InProcessTransport::new(single_file_fs("manifest", b"{}")),
        alan_kernel::Access::ReadOnly,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(mounts)));
    let environment = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    assert!(environment.discover_tool_packages().await.is_err());
}

#[test]
fn test_should_requeue_deferred_action_only_after_cancelled_exit() {
    assert!(should_requeue_deferred_action(
        true,
        DeferredRuntimeActionExit::Cancelled
    ));
    assert!(!should_requeue_deferred_action(
        true,
        DeferredRuntimeActionExit::Completed
    ));
    assert!(!should_requeue_deferred_action(
        false,
        DeferredRuntimeActionExit::Cancelled
    ));
}

fn mock_generation_response(content: impl Into<String>) -> GenerationResponse {
    GenerationResponse {
        content: content.into(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            cached_prompt_tokens: None,
            completion_tokens: 5,
            total_tokens: 15,
            reasoning_tokens: None,
        }),
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

async fn wait_for_ui_turn_completion(
    tail: &mut alan_shell::Tail,
    timeout: Duration,
) -> Vec<alan_agent_protocol::UiEvent> {
    tokio::time::timeout(timeout, async {
        let mut pending = String::new();
        let mut events = Vec::new();
        let mut saw_running = false;
        loop {
            pending.push_str(&String::from_utf8(tail.read(4096).await.unwrap()).unwrap());
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].to_string();
                pending.drain(..=newline);
                let event: alan_agent_protocol::UiEvent = serde_json::from_str(&line).unwrap();
                if let alan_agent_protocol::UiEvent::Activity { snapshot } = &event {
                    saw_running |= matches!(
                        snapshot.state,
                        alan_agent_protocol::UiActivityState::Running
                    );
                    if saw_running
                        && matches!(snapshot.state, alan_agent_protocol::UiActivityState::Idle)
                    {
                        events.push(event);
                        return events;
                    }
                }
                events.push(event);
            }
        }
    })
    .await
    .expect("turn UI stream did not reach idle")
}

fn response_stream(response: GenerationResponse) -> tokio::sync::mpsc::Receiver<StreamChunk> {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(async move {
        if !response.content.is_empty()
            || response
                .thinking
                .as_deref()
                .is_some_and(|value| !value.is_empty())
            || response
                .thinking_signature
                .as_deref()
                .is_some_and(|value| !value.is_empty())
            || !response.redacted_thinking.is_empty()
        {
            let mut redacted = response.redacted_thinking.into_iter();
            let _ = tx
                .send(StreamChunk {
                    text: (!response.content.is_empty()).then_some(response.content),
                    thinking: response.thinking,
                    thinking_signature: response.thinking_signature,
                    redacted_thinking: redacted.next(),
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
            for redacted in redacted {
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: Some(redacted),
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
            }
        }

        let tool_calls = response.tool_calls;
        for (index, tool_call) in tool_calls.iter().enumerate() {
            let arguments =
                serde_json::to_string(&tool_call.arguments).unwrap_or_else(|_| "{}".into());
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: Some(ToolCallDelta {
                        index,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        arguments_delta: Some(arguments.clone()),
                        arguments: Some(arguments),
                    }),
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
        }

        let finish_reason = response.finish_reason.unwrap_or_else(|| {
            if tool_calls.is_empty() {
                "stop".to_string()
            } else {
                "tool_calls".to_string()
            }
        });
        let _ = tx
            .send(StreamChunk {
                text: None,
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: response.usage,
                provider_response_id: response.provider_response_id,
                provider_response_status: response.provider_response_status,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some(finish_reason),
            })
            .await;
    });
    rx
}

struct ShutdownDrainMemoryPromotionProvider {
    call_count: Arc<Mutex<usize>>,
    deferred_delay: Duration,
}

#[async_trait]
impl LlmProvider for ShutdownDrainMemoryPromotionProvider {
    async fn generate(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<GenerationResponse> {
        let current_call = {
            let mut guard = self.call_count.lock().unwrap();
            let current = *guard;
            *guard += 1;
            current
        };

        match current_call {
            0 => Ok(mock_generation_response("Noted.")),
            1 => {
                tokio::time::sleep(self.deferred_delay).await;
                Ok(mock_generation_response(
                    serde_json::json!({
                        "writes": [
                            {
                                "kind": "user_identity",
                                "target": "USER.md",
                                "confidence": "high",
                                "disposition": "promote_now",
                                "observation": "Name: Morris",
                                "evidence": ["My name is Morris."],
                                "promotion_rationale": "Direct user-stated stable identity detail."
                            }
                        ]
                    })
                    .to_string(),
                ))
            }
            _ => Ok(mock_generation_response(
                serde_json::json!({ "writes": [] }).to_string(),
            )),
        }
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Err(anyhow!(
            "ShutdownDrainMemoryPromotionProvider does not implement chat"
        ))
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        Ok(response_stream(self.generate(request).await?))
    }

    fn provider_name(&self) -> &'static str {
        "shutdown_drain_memory_promotion"
    }
}

#[path = "engine_runtime_tests.rs"]
mod runtime;
#[path = "engine_startup_tests.rs"]
mod startup;
