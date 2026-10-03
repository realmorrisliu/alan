//! Safe observation of canonical Process bindings; never serialize restore identities.
use super::*;
use crate::runtime::model_binding::InputBinding;
use alan_agent_protocol::{
    UiAdmittedModel, UiModelBinding, UiModelCatalog, UiModelControlSource, UiModelSnapshot,
};

fn safe(binding: &InputBinding) -> UiModelBinding {
    use crate::RequestControlSource as Source;
    let identity = &binding.callable_binding;
    UiModelBinding {
        profile: identity.profile.clone(),
        provider: identity.provider.clone(),
        model: identity.model.clone(),
        reasoning: binding.request_controls.reasoning,
        control_source: match binding.request_controls.source {
            Source::TurnOverride => UiModelControlSource::TurnOverride,
            Source::AgentMachineOverride => UiModelControlSource::AgentMachineOverride,
            Source::AgentConfig => UiModelControlSource::AgentConfig,
            Source::ModelDefault => UiModelControlSource::ModelDefault,
            Source::ProviderDefault => UiModelControlSource::ProviderDefault,
        },
    }
}
impl RuntimeSubmissionQueues {
    // Called only around actual deferred execution, never because a callable is retained.
    pub(super) async fn deferred_model_started(&mut self) {
        self.deferred_model = self.environment.as_ref().and_then(|environment| {
            environment
                .active_binding
                .read()
                .expect("active binding snapshot")
                .as_ref()
                .map(|(binding, _)| {
                    let mut binding = safe(binding);
                    // TurnMemoryPromotion builds its own default request controls.
                    binding.reasoning = Default::default();
                    binding.control_source = UiModelControlSource::ProviderDefault;
                    binding
                })
        });
        self.observe_models().await;
    }

    pub(super) async fn initialize_process_observations(
        &self,
        state: &RuntimeLoopState,
    ) -> Result<()> {
        self.initialize_bindings(
            &state.core_config,
            state.runtime_config.request_control_intent,
        )
        .await
        .map_err(|error| {
            error!(%error, "Connection capture failed");
            anyhow::anyhow!(
                "Connection binding initialization failed; capture or exact restore unavailable."
            )
        })?;
        super::super::queue_publication::initialize(&self.outer_queue, state.agent_files())
            .await
            .context("initialize accepted queue")?;
        self.publish_models()
            .await
            .context("initialize model status")
    }
    pub(super) async fn publish_models(&self) -> Result<()> {
        let Some(environment) = &self.environment else {
            return Ok(());
        };
        let mut last = self.model_status.lock().await;
        let bindings = environment.model_bindings.lock().await;
        let catalog: Option<UiModelCatalog> = match &bindings.authority {
            Some(authority) => authority
                .catalog()
                .await
                .ok()
                .and_then(|value| serde_json::from_value(value).ok()),
            None => bindings.confirmed.as_ref().map(|callable| {
                let info = callable.config.effective_model_info();
                UiModelCatalog {
                    profile: callable.identity.profile.clone(),
                    models: vec![alan_agent_protocol::UiModelChoice {
                        model: callable.identity.model.clone(),
                        supported_reasoning_efforts: info
                            .as_ref()
                            .map(|info| info.supported_reasoning_efforts.clone())
                            .unwrap_or_default(),
                        default_reasoning_effort: info
                            .as_ref()
                            .and_then(|info| info.default_reasoning_effort),
                    }],
                }
            }),
        };
        let selected_next = bindings
            .confirmed
            .as_ref()
            .map(|callable| {
                crate::resolve_runtime_request_controls(
                    &callable.config,
                    crate::provider_capabilities_for_config(&callable.config),
                    bindings.runtime_intent,
                )
                .map(|request_controls| {
                    safe(&InputBinding {
                        callable_binding: callable.identity.clone(),
                        request_controls,
                    })
                })
            })
            .transpose()?;
        let (active, admitted) = {
            let queue = self.outer_queue.lock().expect("input queue");
            let active = if let Some(binding) = &self.deferred_model {
                Some(binding.clone())
            } else if queue.active_submission_ids.is_empty() {
                None
            } else {
                environment
                    .active_binding
                    .read()
                    .expect("active binding snapshot")
                    .as_ref()
                    .map(|(binding, _)| safe(binding))
            };
            let mut ids: Vec<_> = queue
                .admitted_ids
                .iter()
                .filter(|id| !queue.settled_ids.contains(*id))
                .collect();
            ids.sort();
            let admitted = ids
                .into_iter()
                .map(|id| UiAdmittedModel {
                    submission_id: id.clone(),
                    binding: queue.bindings.get(id).map(safe),
                })
                .collect();
            (active, admitted)
        };
        drop(bindings);
        let mut next = UiModelSnapshot {
            known: true,
            process_path: self.model_process_path.clone(),
            catalog,
            selected_next,
            active,
            admitted,
            publication_version: last.publication_version,
            ..Default::default()
        };
        if next == *last && last.publication_version > 0 {
            return Ok(());
        }
        next.publication_version = last
            .publication_version
            .checked_add(1)
            .context("model publication version exhausted")?;
        environment
            .agent_files()
            .write_ui_model_snapshot(&next)
            .await?;
        *last = next;
        Ok(())
    }
    pub(super) async fn observe_models(&self) {
        if let Err(error) = self.publish_models().await {
            warn!(%error, "Model status observation failed");
        }
    }
}
