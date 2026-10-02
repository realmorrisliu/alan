//! Connection authority injected by Agent Runtime Service; Engine never resolves profiles.
use alan_agent_protocol::Submission;
use alan_ap::InProcessTransport;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Non-secret restore identity owned by Connection Service.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallableIdentity {
    pub profile: String,
    pub provider: String,
    pub model: String,
    pub credential_ref: Option<String>,
    pub revision: String,
}

/// Durable input binding, including canonical normalized controls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputBinding {
    pub callable_binding: CallableIdentity,
    pub request_controls: crate::ResolvedRequestControls,
}

/// Immutable callable and private request configuration; never serialized.
#[derive(Clone)]
pub struct CapturedCallable {
    pub identity: CallableIdentity,
    pub root: InProcessTransport,
    pub connection: String,
    pub config: crate::Config,
}

/// Connection-owned authorized catalog, validation, publication and restoration.
#[async_trait::async_trait]
pub trait ConnectionAuthority: Send + Sync {
    /// Boot may explicitly have no callable; errors are not a no-callable signal.
    async fn capture_initial(&self) -> Result<Option<CapturedCallable>> {
        self.capture(None).await.map(Some)
    }
    async fn capture(&self, model: Option<&str>) -> Result<CapturedCallable>;
    async fn restore(&self, identity: &CallableIdentity) -> Result<CapturedCallable>;
    async fn catalog(&self) -> Result<serde_json::Value>;
}

#[derive(Default)]
pub(crate) struct ProcessBindings {
    pub authority: Option<Arc<dyn ConnectionAuthority>>,
    pub confirmed: Option<CapturedCallable>,
    pub captured: std::collections::HashMap<String, CapturedCallable>,
    pub runtime_intent: crate::RequestControlIntent,
}

impl ProcessBindings {
    pub(crate) fn resolve(
        &self,
        callable: &CapturedCallable,
        input: &Submission,
    ) -> Result<InputBinding> {
        let turn = match &input.op {
            alan_agent_protocol::Op::Turn { context, .. } => {
                crate::RequestControlIntent::reasoning_effort(
                    context.as_ref().and_then(|c| c.reasoning_effort),
                )
            }
            _ => Default::default(),
        };
        Ok(InputBinding {
            callable_binding: callable.identity.clone(),
            request_controls: crate::resolve_turn_request_controls(
                &callable.config,
                crate::provider_capabilities_for_config(&callable.config),
                self.runtime_intent,
                turn,
            )?,
        })
    }
}
