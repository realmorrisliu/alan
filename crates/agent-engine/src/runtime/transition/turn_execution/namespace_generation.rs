use alan_agent_protocol::Event;
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::runtime::transition::NamespaceGeneration;

/// File-native generation context for one logical turn.
///
/// Provider-specific capabilities are intentionally neutralized at the namespace boundary. The
/// Agent Execution Engine consumes the mounted LLM Connection through files and does not recover
/// provider-local continuation or compaction authority behind that boundary.
#[derive(Debug, Clone)]
pub(super) struct NamespaceTurnGeneration {
    provider: String,
    capabilities: crate::llm::ProviderCapabilities,
}

impl NamespaceTurnGeneration {
    pub(super) async fn load(generation: &NamespaceGeneration) -> Self {
        match generation.read_llm_connection_capabilities().await {
            Ok(info) => Self {
                provider: info.provider,
                capabilities: neutralize_namespace_capabilities(info.capabilities),
            },
            Err(error) => {
                warn!(
                    error = %error,
                    "Failed to read namespace LLM Connection capabilities; using neutral fallback"
                );
                Self {
                    provider: "namespace".to_string(),
                    capabilities: neutral_namespace_generation_capabilities(),
                }
            }
        }
    }

    pub(super) fn provider(&self) -> &str {
        &self.provider
    }

    pub(super) fn capabilities(&self) -> crate::llm::ProviderCapabilities {
        self.capabilities
    }

    pub(super) async fn generate(
        &self,
        generation: &NamespaceGeneration,
        request: crate::llm::GenerationRequest,
        timeout_secs: u64,
        cancel: &CancellationToken,
    ) -> Result<(crate::llm::GenerationResponse, Vec<String>)> {
        let max_retries = crate::retry::DEFAULT_MAX_RETRIES;
        let mut last_error = None;

        for attempt in 0..=max_retries {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("LLM request cancelled"));
            }

            let attempt_request = request.clone();
            let mut live_text_chunks = Vec::new();
            let mut collect_text = |event: Event| {
                if let Event::TextDelta {
                    chunk,
                    is_final: false,
                } = event
                    && !chunk.is_empty()
                {
                    live_text_chunks.push(chunk);
                }
                async {}
            };
            let result = generation
                .generate_with_text_events_controlled(
                    &attempt_request,
                    &mut collect_text,
                    timeout_secs,
                    cancel,
                )
                .await;

            match result {
                Ok((response, _saw_text_events)) => return Ok((response, live_text_chunks)),
                Err(error) => {
                    if !crate::retry::is_retryable(&error) || attempt >= max_retries {
                        return Err(error);
                    }
                    last_error = Some(error);
                    let delay = crate::retry::backoff_delay(attempt + 1);
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            return Err(anyhow::anyhow!("LLM request cancelled"));
                        }
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
    }
}

fn neutralize_namespace_capabilities(
    mut capabilities: crate::llm::ProviderCapabilities,
) -> crate::llm::ProviderCapabilities {
    capabilities.instruction_role = crate::llm::InstructionRole::System;
    capabilities.supports_server_managed_continuation = false;
    capabilities.supports_provider_compaction = false;
    capabilities
}

fn neutral_namespace_generation_capabilities() -> crate::llm::ProviderCapabilities {
    crate::llm::ProviderCapabilities {
        supports_streaming_text: true,
        supports_streaming_tool_calls: true,
        supports_provider_response_id: true,
        supports_provider_response_status: true,
        supports_reasoning_text: true,
        supports_reasoning_signature: true,
        supports_reasoning_effort_control: true,
        supports_redacted_thinking: true,
        supports_multimodal_input: false,
        supports_document_input: false,
        supports_cached_token_usage: true,
        supports_server_managed_continuation: false,
        supports_background_execution: false,
        supports_retrieve_cancel: false,
        supports_provider_compaction: false,
        instruction_role: crate::llm::InstructionRole::System,
        compatibility_tier: crate::llm::CompatibilityTier::TierBFullFidelityStateless,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_boundary_removes_provider_local_generation_authority() {
        let mut capabilities = neutral_namespace_generation_capabilities();
        capabilities.instruction_role = crate::llm::InstructionRole::ResponsesInstructions;
        capabilities.supports_server_managed_continuation = true;
        capabilities.supports_provider_compaction = true;

        let neutral = neutralize_namespace_capabilities(capabilities);

        assert_eq!(
            neutral.instruction_role,
            crate::llm::InstructionRole::System
        );
        assert!(!neutral.supports_server_managed_continuation);
        assert!(!neutral.supports_provider_compaction);
    }
}
