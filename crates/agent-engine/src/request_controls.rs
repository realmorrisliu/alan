//! Runtime-owned provider request-control resolution.
//!
//! This module owns alan-level precedence and validation for model request
//! controls. Provider adapters remain responsible for wire projection only.

use crate::config::{Config, LlmProvider};
use crate::llm::{ProviderCapabilities, factory::ProviderType};
use alan_agent_protocol::{ReasoningControls, ReasoningEffort};
use serde::{Deserialize, Serialize};

/// Raw request-control intent from config, machine launch, or a single turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RequestControlIntent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl RequestControlIntent {
    pub fn from_config(config: &Config) -> Self {
        Self {
            reasoning_effort: config.model_reasoning_effort,
        }
    }

    pub fn reasoning_effort(reasoning_effort: Option<ReasoningEffort>) -> Self {
        Self { reasoning_effort }
    }

    pub fn is_empty(self) -> bool {
        self.reasoning_effort.is_none()
    }

    pub fn apply_to_config(self, config: &mut Config) {
        config.model_reasoning_effort = self.reasoning_effort;
    }
}

/// Source of the resolved request controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestControlSource {
    TurnOverride,
    AgentMachineOverride,
    AgentConfig,
    ModelDefault,
    ProviderDefault,
}

/// Diagnostic produced during request-control resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestControlDiagnostic {
    pub message: String,
}

/// Normalized request controls plus their source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRequestControls {
    pub reasoning: ReasoningControls,
    pub source: RequestControlSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<RequestControlDiagnostic>,
}

impl Default for ResolvedRequestControls {
    fn default() -> Self {
        Self {
            reasoning: ReasoningControls::default(),
            source: RequestControlSource::ProviderDefault,
            diagnostics: Vec::new(),
        }
    }
}

impl ResolvedRequestControls {
    pub fn reasoning_effort(&self) -> Option<ReasoningEffort> {
        self.reasoning.effort
    }
}

/// Inputs for resolving request controls.
#[derive(Debug, Clone, Copy)]
pub struct RequestControlResolutionInput<'a> {
    pub config: &'a Config,
    pub provider_capabilities: ProviderCapabilities,
    pub runtime_intent: RequestControlIntent,
    pub turn_intent: RequestControlIntent,
}

/// Resolve machine-scoped request controls.
pub fn resolve_runtime_request_controls(
    config: &Config,
    provider_capabilities: ProviderCapabilities,
    runtime_intent: RequestControlIntent,
) -> anyhow::Result<ResolvedRequestControls> {
    resolve_request_controls(RequestControlResolutionInput {
        config,
        provider_capabilities,
        runtime_intent,
        turn_intent: RequestControlIntent::default(),
    })
}

/// Resolve request controls for a single turn.
pub fn resolve_turn_request_controls(
    config: &Config,
    provider_capabilities: ProviderCapabilities,
    runtime_intent: RequestControlIntent,
    turn_intent: RequestControlIntent,
) -> anyhow::Result<ResolvedRequestControls> {
    resolve_request_controls(RequestControlResolutionInput {
        config,
        provider_capabilities,
        runtime_intent,
        turn_intent,
    })
}

/// Resolve effective request controls from turn/machine/config/model/provider inputs.
pub fn resolve_request_controls(
    input: RequestControlResolutionInput<'_>,
) -> anyhow::Result<ResolvedRequestControls> {
    if let Some(effort) = input.turn_intent.reasoning_effort {
        validate_reasoning_effort(input.config, input.provider_capabilities, effort)?;
        return Ok(effort_controls(effort, RequestControlSource::TurnOverride));
    }

    if let Some(effort) = input.runtime_intent.reasoning_effort {
        validate_reasoning_effort(input.config, input.provider_capabilities, effort)?;
        return Ok(effort_controls(
            effort,
            RequestControlSource::AgentMachineOverride,
        ));
    }

    let config_intent = RequestControlIntent::from_config(input.config);
    if let Some(effort) = config_intent.reasoning_effort {
        validate_reasoning_effort(input.config, input.provider_capabilities, effort)?;
        return Ok(effort_controls(effort, RequestControlSource::AgentConfig));
    }

    if let Some(effort) = input
        .config
        .effective_model_info()
        .and_then(|model_info| model_info.default_reasoning_effort)
    {
        validate_reasoning_effort(input.config, input.provider_capabilities, effort)?;
        return Ok(effort_controls(effort, RequestControlSource::ModelDefault));
    }

    Ok(ResolvedRequestControls::default())
}

/// Return the static capability matrix for the provider selected by config.
pub fn provider_capabilities_for_config(config: &Config) -> ProviderCapabilities {
    provider_type_for_llm_provider(config.llm_provider).capabilities()
}

fn effort_controls(
    effort: ReasoningEffort,
    source: RequestControlSource,
) -> ResolvedRequestControls {
    ResolvedRequestControls {
        reasoning: ReasoningControls {
            effort: Some(effort),
        },
        source,
        diagnostics: Vec::new(),
    }
}

fn validate_reasoning_effort(
    config: &Config,
    provider_capabilities: ProviderCapabilities,
    effort: ReasoningEffort,
) -> anyhow::Result<()> {
    if !provider_capabilities.supports_reasoning_effort_control {
        anyhow::bail!(
            "provider `{}` does not support reasoning effort controls",
            config.llm_provider.as_str()
        );
    }

    let Some(model_info) = config.effective_model_info() else {
        return Ok(());
    };

    if model_info.supported_reasoning_efforts.contains(&effort) {
        return Ok(());
    }

    let supported = if model_info.supported_reasoning_efforts.is_empty() {
        "none declared".to_string()
    } else {
        model_info
            .supported_reasoning_efforts
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    anyhow::bail!(
        "model `{}` does not support reasoning effort `{}`; supported efforts: {}",
        model_info.slug,
        effort,
        supported
    );
}

fn provider_type_for_llm_provider(provider: LlmProvider) -> ProviderType {
    match provider {
        LlmProvider::GoogleGeminiGenerateContent => ProviderType::GoogleGeminiGenerateContent,
        LlmProvider::Chatgpt => ProviderType::ChatgptResponses,
        LlmProvider::OpenAiResponses => ProviderType::OpenAiResponses,
        LlmProvider::OpenAiChatCompletions => ProviderType::OpenAiChatCompletions,
        LlmProvider::OpenAiChatCompletionsCompatible => {
            ProviderType::OpenAiChatCompletionsCompatible
        }
        LlmProvider::OpenRouter => ProviderType::OpenRouter,
        LlmProvider::AnthropicMessages => ProviderType::AnthropicMessages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModelCatalogProvider;

    fn openai_config() -> Config {
        Config::default()
    }

    fn openai_caps() -> ProviderCapabilities {
        provider_capabilities_for_config(&openai_config())
    }

    #[test]
    fn turn_override_has_highest_precedence() {
        let mut config = openai_config();
        config.model_reasoning_effort = Some(ReasoningEffort::Medium);
        let resolved = resolve_turn_request_controls(
            &config,
            openai_caps(),
            RequestControlIntent::reasoning_effort(Some(ReasoningEffort::High)),
            RequestControlIntent::reasoning_effort(Some(ReasoningEffort::Low)),
        )
        .unwrap();

        assert_eq!(resolved.reasoning.effort, Some(ReasoningEffort::Low));
        assert_eq!(resolved.source, RequestControlSource::TurnOverride);
    }

    #[test]
    fn runtime_override_precedes_agent_config() {
        let mut config = openai_config();
        config.model_reasoning_effort = Some(ReasoningEffort::High);
        let resolved = resolve_runtime_request_controls(
            &config,
            openai_caps(),
            RequestControlIntent::reasoning_effort(Some(ReasoningEffort::Low)),
        )
        .unwrap();

        assert_eq!(resolved.reasoning.effort, Some(ReasoningEffort::Low));
        assert_eq!(resolved.source, RequestControlSource::AgentMachineOverride);
    }

    #[test]
    fn agent_config_effort_precedes_model_default() {
        let mut config = openai_config();
        config.model_reasoning_effort = Some(ReasoningEffort::High);
        let resolved = resolve_runtime_request_controls(
            &config,
            openai_caps(),
            RequestControlIntent::default(),
        )
        .unwrap();

        assert_eq!(resolved.reasoning.effort, Some(ReasoningEffort::High));
        assert_eq!(resolved.source, RequestControlSource::AgentConfig);
    }

    #[test]
    fn model_default_applies_when_no_explicit_intent() {
        let config = openai_config();
        let resolved = resolve_runtime_request_controls(
            &config,
            openai_caps(),
            RequestControlIntent::default(),
        )
        .unwrap();

        assert_eq!(resolved.reasoning.effort, Some(ReasoningEffort::Medium));
        assert_eq!(resolved.source, RequestControlSource::ModelDefault);
    }

    #[test]
    fn unknown_model_metadata_uses_provider_default() {
        let mut config = openai_config();
        config.llm_provider = LlmProvider::OpenRouter;
        let resolved = resolve_runtime_request_controls(
            &config,
            provider_capabilities_for_config(&config),
            RequestControlIntent::default(),
        )
        .unwrap();

        assert_eq!(resolved.reasoning, ReasoningControls::default());
        assert_eq!(resolved.source, RequestControlSource::ProviderDefault);
    }

    #[test]
    fn provider_without_effort_support_rejects_explicit_effort() {
        let config = openai_config();
        let mut caps = openai_caps();
        caps.supports_reasoning_effort_control = false;
        let err = resolve_runtime_request_controls(
            &config,
            caps,
            RequestControlIntent::reasoning_effort(Some(ReasoningEffort::High)),
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("does not support reasoning effort controls")
        );
    }

    #[test]
    fn model_catalog_defaults_are_exposed_as_model_default_source() {
        let config = openai_config();
        let model_info = config
            .effective_model_info()
            .expect("default OpenAI Responses model should be cataloged");
        assert_eq!(model_info.provider, ModelCatalogProvider::OpenAiResponses);

        let resolved = resolve_runtime_request_controls(
            &config,
            openai_caps(),
            RequestControlIntent::default(),
        )
        .unwrap();
        assert_eq!(resolved.source, RequestControlSource::ModelDefault);
    }

    #[test]
    fn layering_contract_keeps_effective_resolution_out_of_call_sites() {
        let turn_executor = include_str!("runtime/turn_executor.rs");
        for forbidden in [
            "effective_model_reasoning_effort",
            "validate_reasoning_effort_for_resolved_model",
            "active_turn_reasoning_effort",
            "runtime_config.model_reasoning_effort",
        ] {
            assert!(
                !turn_executor.contains(forbidden),
                "`turn_executor` must consume request-control resolver output, found `{forbidden}`"
            );
        }
    }
}
