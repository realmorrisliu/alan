use anyhow::Result;
use tokio::sync::mpsc;

use crate::{GenerationRequest, GenerationResponse, StreamChunk};

/// Account-authorized model metadata supplied by a provider adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderModel {
    pub slug: String,
    pub context_window_tokens: u32,
    pub supported_reasoning_efforts: Vec<crate::ReasoningEffort>,
    pub default_reasoning_effort: Option<crate::ReasoningEffort>,
}

/// Unified trait for LLM providers.
///
/// This trait abstracts over different LLM backends and API surfaces.
/// providing a consistent interface for generation, streaming, and simple chat.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a response with tool calling support
    ///
    /// # Arguments
    /// * `request` - The generation request containing messages, tools, and configuration
    ///
    /// # Returns
    /// * `Result<GenerationResponse>` - The generated response or an error
    async fn generate(&mut self, _request: GenerationRequest) -> Result<GenerationResponse> {
        anyhow::bail!("generation unavailable")
    }

    /// Simple chat without tool calling
    ///
    /// This is a convenience method for simple one-turn conversations.
    ///
    /// # Arguments
    /// * `system` - Optional system prompt
    /// * `user` - The user message
    ///
    /// # Returns
    /// * `Result<String>` - The assistant's response text
    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> Result<String> {
        anyhow::bail!("generation unavailable")
    }

    /// Generate with streaming support
    ///
    /// Returns a receiver channel that yields text chunks as they arrive.
    /// Each chunk can be a character, word, or sentence fragment.
    ///
    /// # Arguments
    /// * `request` - The generation request
    ///
    /// # Returns
    /// * `Result<mpsc::Receiver<StreamChunk>>` - Channel receiving stream chunks
    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        anyhow::bail!("generation unavailable")
    }

    /// Whether this callable implements generation (evaluation-only adapters do not).
    fn supports_generation(&self) -> bool {
        true
    }

    /// Whether this actual callable supports finite-choice evaluation.
    /// Generation or tool-calling support alone does not imply this capability.
    /// Defaults to [`Self::supports_decisions`] so decision-capable providers
    /// automatically support `evaluate_choice` via [`crate::DecisionQuestion::Choice`].
    fn supports_choice_evaluation(&self) -> bool {
        self.supports_decisions()
    }

    /// Evaluate a finite set without producing assistant prose or dispatching Tools.
    ///
    /// When [`Self::supports_decisions`] is true, delegates through [`Self::decide`]
    /// using a projected [`crate::DecisionQuestion::Choice`] request without duplicating
    /// provider logic.
    async fn evaluate_choice(
        &mut self,
        request: crate::ChoiceEvaluationRequest,
    ) -> Result<crate::ChoiceEvaluationResponse> {
        if !self.supports_decisions() {
            anyhow::bail!("finite-choice evaluation unavailable");
        }
        let decision_request = request.to_decision_request()?;
        let response = self.decide(decision_request.clone()).await?;
        request.response_from_decision(&decision_request, response)
    }

    /// Whether this actual callable supports first-class typed Decisions (`Predicate`,
    /// `Choice`, `Score`). Generation or tool-calling support alone does not imply this.
    fn supports_decisions(&self) -> bool {
        false
    }

    /// Baseline calibration advertised by this callable when [`Self::supports_decisions`]
    /// is true (`Some(Calibrated)` for native calibrated Decisions endpoints,
    /// `Some(Uncalibrated)` for generation-based fallbacks, `None` when unsupported).
    fn decision_calibration(&self) -> Option<crate::CalibrationKind> {
        None
    }

    /// Evaluate one or more typed decision questions (`Predicate`, `Choice`, `Score`)
    /// over untrusted JSON `state` without assistant prose or Tool dispatch.
    async fn decide(
        &mut self,
        _request: crate::DecisionRequest,
    ) -> Result<crate::DecisionResponse> {
        anyhow::bail!("decision evaluation unavailable")
    }

    /// Non-secret account identity fixed when this callable was constructed.
    fn account_identity(&self) -> Option<&str> {
        None
    }

    /// Fetch account-scoped metadata when supported; never called by status observation.
    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        Ok(None)
    }

    /// Get the provider name (for logging/debugging)
    fn provider_name(&self) -> &'static str;
}

/// Thread-safe, cloneable handle to an [`LlmProvider`] whose static capabilities are cached.
///
/// Useful when multiple embedded sessions, child runs, or retries share one in-memory
/// provider instance without wrapping a fresh client for every spawn.
#[derive(Clone)]
pub struct SharedLlmProvider {
    inner: std::sync::Arc<tokio::sync::Mutex<Box<dyn LlmProvider>>>,
    provider_name: &'static str,
    supports_generation: bool,
    supports_choice_evaluation: bool,
    supports_decisions: bool,
    decision_calibration: Option<crate::CalibrationKind>,
    account_identity: Option<String>,
}

impl std::fmt::Debug for SharedLlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedLlmProvider")
            .field("provider_name", &self.provider_name)
            .field("supports_generation", &self.supports_generation)
            .field(
                "supports_choice_evaluation",
                &self.supports_choice_evaluation,
            )
            .field("supports_decisions", &self.supports_decisions)
            .field("decision_calibration", &self.decision_calibration)
            .finish()
    }
}

impl SharedLlmProvider {
    /// Wrap any concrete [`LlmProvider`] for thread-safe reuse across clones.
    pub fn new<P: LlmProvider + 'static>(provider: P) -> Self {
        Self::from_boxed(Box::new(provider))
    }

    /// Wrap a boxed [`LlmProvider`] for thread-safe reuse across clones.
    pub fn from_boxed(provider: Box<dyn LlmProvider>) -> Self {
        Self {
            provider_name: provider.provider_name(),
            supports_generation: provider.supports_generation(),
            supports_choice_evaluation: provider.supports_choice_evaluation(),
            supports_decisions: provider.supports_decisions(),
            decision_calibration: provider.decision_calibration(),
            account_identity: provider.account_identity().map(str::to_owned),
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(provider)),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for SharedLlmProvider {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.inner.lock().await.generate(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.inner.lock().await.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        self.inner.lock().await.generate_stream(request).await
    }

    fn supports_generation(&self) -> bool {
        self.supports_generation
    }

    fn supports_choice_evaluation(&self) -> bool {
        self.supports_choice_evaluation
    }

    async fn evaluate_choice(
        &mut self,
        request: crate::ChoiceEvaluationRequest,
    ) -> Result<crate::ChoiceEvaluationResponse> {
        self.inner.lock().await.evaluate_choice(request).await
    }

    fn supports_decisions(&self) -> bool {
        self.supports_decisions
    }

    fn decision_calibration(&self) -> Option<crate::CalibrationKind> {
        self.decision_calibration
    }

    async fn decide(&mut self, request: crate::DecisionRequest) -> Result<crate::DecisionResponse> {
        self.inner.lock().await.decide(request).await
    }

    fn account_identity(&self) -> Option<&str> {
        self.account_identity.as_deref()
    }

    async fn model_catalog(&self) -> Result<Option<Vec<ProviderModel>>> {
        self.inner.lock().await.model_catalog().await
    }

    fn provider_name(&self) -> &'static str {
        self.provider_name
    }
}
