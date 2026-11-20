use alan_agent_protocol::Event;
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::live_stream::LiveTurnStream;
use crate::runtime::transition::NamespaceGeneration;

/// File-native generation context for one logical turn.
///
/// The Agent Execution Engine consumes the mounted LLM Connection through files, always sending
/// the full neutral message list. The instruction projection is therefore neutralized at the
/// namespace boundary, while the Connection's own continuation and compaction capabilities are
/// reported as-is: the provider adapter behind llmfs owns provider-local state.
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

    pub(super) fn ensure_callable(&self) -> Result<()> {
        anyhow::ensure!(
            self.provider != "unconfigured",
            "No callable Connection is available. Configure and select a Connection profile before submitting tasks."
        );
        Ok(())
    }

    pub(super) fn capabilities(&self) -> crate::llm::ProviderCapabilities {
        self.capabilities
    }

    /// Generate one response, publishing text and reasoning deltas through `live` as they arrive.
    ///
    /// A transient failure is retried only while nothing has reached `io/output`: streamed text
    /// cannot be retracted, so a failure after output started is reported instead.
    pub(super) async fn generate<E, F>(
        &self,
        generation: &NamespaceGeneration,
        request: crate::llm::GenerationRequest,
        timeout_secs: u64,
        cancel: &CancellationToken,
        live: &LiveTurnStream,
        emit: &mut E,
    ) -> Result<crate::llm::GenerationResponse>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        let max_retries = crate::retry::DEFAULT_MAX_RETRIES;
        let mut last_error = None;

        for attempt in 0..=max_retries {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("LLM request cancelled"));
            }

            let attempt_request = request.clone();
            let mut sink = |event: Event| {
                let mut plan = live.plan_event(event);
                let forwarded: Vec<F> = plan.take_forward().into_iter().map(&mut *emit).collect();
                async move {
                    live.apply(plan).await;
                    for forwarded in forwarded {
                        forwarded.await;
                    }
                }
            };
            let result = generation
                .generate_with_text_events_controlled(
                    &attempt_request,
                    &mut sink,
                    timeout_secs,
                    cancel,
                )
                .await;

            match result {
                Ok((response, _saw_text_events)) => return Ok(response),
                Err(error) => {
                    if live.text_visible()
                        || !crate::retry::is_retryable(&error)
                        || attempt >= max_retries
                    {
                        return Err(error);
                    }
                    last_error = Some(error);
                    live.apply(live.plan_retry()).await;
                    generation.wait_before_retry(attempt + 1, cancel).await?;
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
    fn namespace_boundary_neutralizes_instructions_but_reports_real_provider_capabilities() {
        let mut capabilities = neutral_namespace_generation_capabilities();
        capabilities.instruction_role = crate::llm::InstructionRole::ResponsesInstructions;
        capabilities.supports_server_managed_continuation = true;
        capabilities.supports_provider_compaction = true;

        let reported = neutralize_namespace_capabilities(capabilities);

        assert_eq!(
            reported.instruction_role,
            crate::llm::InstructionRole::System
        );
        assert!(reported.supports_server_managed_continuation);
        assert!(reported.supports_provider_compaction);

        let mut stateless = neutral_namespace_generation_capabilities();
        stateless.supports_server_managed_continuation = false;
        stateless.supports_provider_compaction = false;
        let reported = neutralize_namespace_capabilities(stateless);
        assert!(!reported.supports_server_managed_continuation);
        assert!(!reported.supports_provider_compaction);
    }

    #[test]
    fn unreadable_capabilities_fail_closed_without_continuation_or_compaction() {
        let fallback = neutral_namespace_generation_capabilities();

        assert!(!fallback.supports_server_managed_continuation);
        assert!(!fallback.supports_provider_compaction);
        assert_eq!(
            fallback.instruction_role,
            crate::llm::InstructionRole::System
        );
    }

    #[test]
    fn unconfigured_connection_has_a_clear_error_before_generation() {
        let generation = NamespaceTurnGeneration {
            provider: "unconfigured".to_string(),
            capabilities: neutral_namespace_generation_capabilities(),
        };

        let error = generation.ensure_callable().unwrap_err();

        assert_eq!(
            error.to_string(),
            "No callable Connection is available. Configure and select a Connection profile before submitting tasks."
        );
    }
}
