//! Stateless one-shot host LLM completion helper with bounded timeout.

use std::time::Duration;

use alan_agent_engine::{Config, LlmClient};
use alan_service_manager::ConnectionsFile;
use anyhow::{Context, Result, bail};

use crate::boot::ProductLlmClientFactory;
use crate::paths::{HostStorePaths, SystemStorePaths};

/// Execute a single stateless host-side LLM prompt using the active Connection Service profile.
///
/// No Tape, Tool Registry, or Agent Process state is touched.
pub async fn complete_one_shot_prompt(
    profile: Option<&str>,
    system_prompt: &str,
    user_prompt: &str,
    timeout: Duration,
) -> Result<String> {
    let system_store = SystemStorePaths::detect()?;
    let host_store = HostStorePaths::detect()?;
    let bindings = system_store.connection_bindings()?;
    let (connections, _) = ConnectionsFile::load_from_path(&bindings.metadata_path)?;
    let base_config = Config::load()?;

    let effective_profile = profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            base_config
                .connection_profile
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| connections.default_profile.clone());

    let factory =
        ProductLlmClientFactory::new(host_store.credentials, Some(host_store.managed_auth));
    let mut client = factory.resolve(&base_config, effective_profile.as_deref(), &connections)?;
    complete_one_shot_with_client(&mut client, system_prompt, user_prompt, timeout).await
}

pub(crate) async fn complete_one_shot_with_client(
    client: &mut LlmClient,
    system_prompt: &str,
    user_prompt: &str,
    timeout: Duration,
) -> Result<String> {
    if timeout.is_zero() {
        bail!("one-shot LLM request timed out");
    }
    let system = (!system_prompt.trim().is_empty()).then_some(system_prompt);
    match tokio::time::timeout(timeout, client.chat(system, user_prompt)).await {
        Ok(result) => result.context("one-shot LLM request failed"),
        Err(_) => bail!(
            "one-shot LLM request timed out after {}ms",
            timeout.as_millis()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};

    struct StubOneShotProvider {
        delay: Duration,
        reply: Result<String, String>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for StubOneShotProvider {
        async fn generate(&mut self, _request: GenerationRequest) -> Result<GenerationResponse> {
            bail!("not used in chat-only test")
        }

        async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            match &self.reply {
                Ok(text) => Ok(format!("{}|{user}|{text}", system.unwrap_or(""))),
                Err(msg) => bail!("{msg}"),
            }
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            bail!("not used in chat-only test")
        }

        fn provider_name(&self) -> &'static str {
            "stub"
        }
    }

    #[tokio::test]
    async fn one_shot_returns_completion_and_passes_system_and_user_prompts() {
        let mut client = LlmClient::new(StubOneShotProvider {
            delay: Duration::ZERO,
            reply: Ok("{\"title\":\"t\"}".into()),
        });
        let out = complete_one_shot_with_client(&mut client, "sys", "usr", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(out, "sys|usr|{\"title\":\"t\"}");
    }

    #[tokio::test]
    async fn one_shot_propagates_provider_error() {
        let mut client = LlmClient::new(StubOneShotProvider {
            delay: Duration::ZERO,
            reply: Err("http 503".into()),
        });
        let err = complete_one_shot_with_client(&mut client, "sys", "usr", Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("http 503"), "{err:#}");
    }

    #[tokio::test]
    async fn one_shot_enforces_timeout() {
        let mut client = LlmClient::new(StubOneShotProvider {
            delay: Duration::from_millis(250),
            reply: Ok("late".into()),
        });
        let err =
            complete_one_shot_with_client(&mut client, "sys", "usr", Duration::from_millis(20))
                .await
                .unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err:#}");
    }
}
