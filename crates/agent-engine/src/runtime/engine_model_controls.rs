//! Model selection and capture at the Process loop's serialized intake boundary.
use super::*;
use crate::runtime::model_binding::{CallableIdentity, CapturedCallable};

pub(super) const NO_CONFIRMED_CALLABLE: &str =
    "No confirmed callable binding; generation input not admitted.";

#[derive(Debug)]
pub(super) struct NoConfirmedCallable;
impl std::fmt::Display for NoConfirmedCallable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(NO_CONFIRMED_CALLABLE)
    }
}
impl std::error::Error for NoConfirmedCallable {}

fn is_direct_command(input: &Submission) -> bool {
    input.intent == alan_agent_protocol::InputIntent::Command
        && matches!(input.op, alan_agent_protocol::Op::Input { .. })
}

impl RuntimeSubmissionQueues {
    pub(super) async fn initialize_bindings(
        &self,
        config: &crate::Config,
        intent: crate::RequestControlIntent,
    ) -> Result<()> {
        let environment = self.environment.as_ref().context("missing environment")?;
        let mut bindings = environment.model_bindings.lock().await;
        bindings.runtime_intent = intent;
        let recovered = self
            .outer_queue
            .lock()
            .expect("input queue")
            .confirmed_binding
            .clone();
        if recovered.is_none()
            && let Some(authority) = &bindings.authority
        {
            bindings.confirmed = authority.capture_initial().await?;
            return Ok(());
        }
        bindings.confirmed = Some(if let Some(binding) = recovered {
            let authority = bindings
                .authority
                .as_ref()
                .context("confirmed selection restore authority unavailable")?;
            authority.restore(&binding.callable_binding).await?
        } else if let Some(authority) = &bindings.authority {
            authority.capture(None).await?
        } else {
            CapturedCallable {
                identity: CallableIdentity {
                    profile: environment.llm_connection_name().to_owned(),
                    provider: config.llm_provider.as_str().into(),
                    model: config.effective_model().into(),
                    credential_ref: None,
                    revision: "namespace-launch".into(),
                },
                root: environment.root_transport(),
                connection: environment.llm_connection_name().to_owned(),
                config: config.clone(),
            }
        });
        Ok(())
    }

    pub(super) async fn capture_input(&self, input: &Submission) -> Result<()> {
        // Deterministic commands are routed by the existing command handler, not generation.
        if is_direct_command(input) {
            return Ok(());
        }
        if !matches!(
            input.op,
            alan_agent_protocol::Op::Turn { .. } | alan_agent_protocol::Op::Input { .. }
        ) {
            return Ok(());
        }
        if self
            .outer_queue
            .lock()
            .expect("input queue")
            .admitted_ids
            .contains(&input.id)
        {
            return Ok(());
        }
        let Some(environment) = &self.environment else {
            return Ok(());
        };
        let mut bindings = environment.model_bindings.lock().await;
        let callable = bindings.confirmed.clone().ok_or(NoConfirmedCallable)?;
        let binding = bindings.resolve(&callable, input)?;
        bindings.captured.insert(input.id.clone(), callable);
        self.outer_queue
            .lock()
            .expect("input queue")
            .bindings
            .insert(input.id.clone(), binding);
        Ok(())
    }

    pub(super) async fn model_control(&mut self, input: &Submission) -> bool {
        let alan_agent_protocol::Op::SelectModel { model } = &input.op else {
            return false;
        };
        let result: Result<()> = async {
            anyhow::ensure!(input.intent == alan_agent_protocol::InputIntent::Agent, "model selection requires Agent intent");
            let environment = self.environment.as_ref().context("missing Connection authority")?;
            let mut bindings = environment.model_bindings.lock().await;
            let authority = bindings.authority.as_ref().context("Connection catalog unavailable")?;
            let callable = authority.capture(Some(model)).await?;
            let controls = crate::resolve_runtime_request_controls(&callable.config, crate::provider_capabilities_for_config(&callable.config), bindings.runtime_intent)?;
            // Publication must be durable before confirming. Queues and active work are untouched.
            crate::agent_machine::input_queue::persist_input_event(self.recorder.as_ref(), "machine_model_selected_v1", serde_json::json!({"submission_id": input.id, "callable_binding": callable.identity, "request_controls":controls})).await?;
            bindings.confirmed = Some(callable);
            Ok(())
        }.await;
        if let Err(error) = &result {
            warn!(%error, submission_id=%input.id, "Model selection failed at Connection authority");
        }
        self.observe_models().await;
        if let Some(environment) = &self.environment {
            let event = alan_agent_protocol::UiEvent::InputCompleted {
                submission_ids: vec![input.id.clone()],
                status: if result.is_ok() { alan_agent_protocol::UiInputStatus::Completed } else { alan_agent_protocol::UiInputStatus::Failed },
                error: result.err().map(|_| "Model selection not installed; Connection validation or persistence failed.".to_owned()),
            };
            if let Err(error) = environment.agent_files().append_ui_event(&event).await {
                warn!(%error, "selection settlement publication failed");
            }
        }
        true
    }

    pub(super) async fn activate_binding(&self, input: &Submission) -> Result<()> {
        let Some(environment) = &self.environment else {
            return Ok(());
        };
        if is_direct_command(input) {
            // Also ignore legacy captured bindings: a command must not inherit generation authority.
            *environment
                .active_binding
                .write()
                .expect("active binding snapshot") = None;
            return Ok(());
        }
        let binding = self
            .outer_queue
            .lock()
            .expect("input queue")
            .bindings
            .get(&input.id)
            .cloned();
        let bindings = environment.model_bindings.lock().await;
        let Some(binding) = binding else {
            anyhow::ensure!(
                bindings.authority.is_none()
                    || !matches!(
                        input.op,
                        alan_agent_protocol::Op::Input { .. }
                            | alan_agent_protocol::Op::Turn { .. }
                    ),
                "legacy input has no captured callable binding; cannot restore safely"
            );
            *environment
                .active_binding
                .write()
                .expect("active binding snapshot") = None;
            return Ok(());
        };
        let callable = if let Some(callable) = bindings.captured.get(&input.id) {
            callable.clone()
        } else if let Some(authority) = &bindings.authority {
            authority.restore(&binding.callable_binding).await?
        } else {
            anyhow::bail!(
                "captured callable unavailable: {}",
                binding.callable_binding.model
            );
        };
        anyhow::ensure!(
            callable.identity == binding.callable_binding,
            "restored callable identity mismatch"
        );
        *environment
            .active_binding
            .write()
            .expect("active binding snapshot") = Some((binding, callable));
        Ok(())
    }

    pub(super) async fn reject_incompatible_steer(&mut self, input: &Submission) -> Result<bool> {
        if !matches!(
            input.op,
            alan_agent_protocol::Op::Input {
                mode: alan_agent_protocol::InputMode::Steer,
                ..
            }
        ) {
            return Ok(false);
        }
        let Some(environment) = &self.environment else {
            return Ok(false);
        };
        let admitted = self
            .outer_queue
            .lock()
            .expect("input queue")
            .bindings
            .get(&input.id)
            .cloned();
        let active = environment
            .active_binding
            .read()
            .expect("active binding snapshot")
            .as_ref()
            .map(|(binding, _)| binding.clone());
        if admitted.is_none() || admitted == active {
            return Ok(false);
        }
        self.fail_accepted_input(
            input,
            &anyhow::anyhow!("incompatible steering binding"),
            "Steering callable or resolved controls differ from active work",
        )
        .await;
        Ok(true)
    }
}
