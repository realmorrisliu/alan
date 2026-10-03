//! Process-scoped callable authority; profile resolution stays in Connection Service.
use super::*;
use alan_agent_engine::runtime::model_binding::{
    CallableIdentity, CapturedCallable, ConnectionAuthority,
};

pub(crate) struct ProcessConnection {
    pub service: Arc<ConnectionService>,
    pub profile: String,
    pub namespace: alan_kernel::LiveNamespace,
}

#[async_trait::async_trait]
impl ConnectionAuthority for ProcessConnection {
    async fn capture_initial(&self) -> Result<Option<CapturedCallable>> {
        self.service.refresh().await?;
        let callables = self.service.callables.lock().await;
        let registry = callables
            .as_ref()
            .context("callable registry unavailable")?;
        let available = registry.published_profiles.contains_key(&self.profile)
            || registry.published_fallbacks.contains(&self.profile);
        drop(callables);
        if !available {
            // Existing unavailable-provider boot mode grants no capture/restore authority.
            return Ok(None);
        }
        self.capture(None).await.map(Some)
    }

    async fn capture(&self, model: Option<&str>) -> Result<CapturedCallable> {
        self.service
            .capture_model(&self.profile, model, &self.namespace)
            .await
    }

    async fn restore(&self, identity: &CallableIdentity) -> Result<CapturedCallable> {
        ensure!(
            identity.profile == self.profile,
            "restore profile is outside Process authority"
        );
        // Exact recovery of the profile's original callable is not a new model
        // selection. Changed models still cross the existing catalog boundary.
        let original = self.capture(None).await?;
        let captured = if original.identity.model == identity.model {
            original
        } else {
            self.capture(Some(&identity.model)).await?
        };
        ensure!(
            &captured.identity == identity,
            "captured connection revision unavailable"
        );
        Ok(captured)
    }

    async fn catalog(&self) -> Result<serde_json::Value> {
        let captured = self.capture(None).await?;
        // Published injection grants exactly one callable, not the provider's static catalog.
        // Consult authority state directly; Connection revision is opaque restore evidence.
        let injected = {
            let callables = self.service.callables.lock().await;
            let registry = callables
                .as_ref()
                .context("callable registry unavailable")?;
            registry.published_fallbacks.contains(&self.profile)
                && !self.service.metadata().profiles.contains_key(&self.profile)
        };
        if injected {
            let info = captured.config.effective_model_info();
            return Ok(serde_json::json!({"profile":self.profile,"models":[{
                "model":captured.identity.model,
                "supported_reasoning_efforts":info.as_ref().map(|info| &info.supported_reasoning_efforts).cloned().unwrap_or_default(),
                "default_reasoning_effort":info.as_ref().and_then(|info| info.default_reasoning_effort)
            }]}));
        }
        let info = captured
            .config
            .effective_model_info()
            .context("model catalog unavailable")?;
        let catalog = captured
            .config
            .model_catalog
            .as_ref()
            .context("model catalog unavailable")?;
        let entries: Vec<_> = catalog
            .supported_model_slugs(info.provider)
            .into_iter()
            .filter_map(|slug| {
                catalog.find_model_info(info.provider, slug).map(|entry| serde_json::json!({
                "model":entry.slug, "supported_reasoning_efforts":entry.supported_reasoning_efforts,
                "default_reasoning_effort":entry.default_reasoning_effort
            }))
            })
            .collect();
        Ok(serde_json::json!({"profile":self.profile,"models":entries}))
    }
}

impl ConnectionService {
    async fn capture_model(
        &self,
        profile_id: &str,
        model: Option<&str>,
        process_namespace: &alan_kernel::LiveNamespace,
    ) -> Result<CapturedCallable> {
        self.refresh().await?;
        let callables = self.callables.lock().await;
        let registry = callables
            .as_ref()
            .context("callable registry unavailable")?;
        let mut connections = self.metadata();
        // Only an explicitly published injected callable is a fallback. Missing
        // managed metadata alone never grants callable authority.
        if registry.published_fallbacks.contains(profile_id)
            && !connections.profiles.contains_key(profile_id)
        {
            let config = registry.base_config.clone();
            ensure!(
                model.is_none() || model == Some(config.effective_model()),
                "injected callable does not authorize model selection"
            );
            return Ok(CapturedCallable {
                identity: CallableIdentity {
                    profile: profile_id.into(),
                    provider: config.llm_provider.as_str().into(),
                    model: config.effective_model().into(),
                    credential_ref: None,
                    revision: "injected-namespace-callable".into(),
                },
                root: alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
                    process_namespace.snapshot(),
                ))),
                connection: profile_id.into(),
                config,
            });
        }
        let original = connections
            .profiles
            .get(profile_id)
            .context("Process profile unavailable")?
            .clone();
        ensure!(
            registry.published_profiles.get(profile_id) == Some(&original),
            "Process profile is not callable"
        );
        let mut config = registry.base_config.clone();
        connections.apply_profile_metadata_to_config(Some(profile_id), &mut config)?;
        if let Some(model) = model {
            ensure!(!model.trim().is_empty(), "model selection is empty");
            connections
                .profiles
                .get_mut(profile_id)
                .expect("profile exists")
                .settings
                .insert("model".into(), model.into());
            connections.apply_profile_metadata_to_config(Some(profile_id), &mut config)?;
            ensure!(
                config.effective_model_info().is_some(),
                "model is unavailable in Connection catalog"
            );
        }
        let llmfs = if model.is_none()
            || model == Some(config.effective_model())
                && original.settings.get("model").map(String::as_str) == model
        {
            registry.llmfs.connection_snapshot(profile_id)
        } else {
            let client =
                registry
                    .factory
                    .create(&registry.base_config, Some(profile_id), &connections)?;
            let llmfs = alan_llmfs::LlmFs::new();
            llmfs.register_connection(profile_id, Box::new(ConnectionLlmProvider { client }));
            llmfs.connection_snapshot(profile_id)
        };
        // Only non-secret metadata names the exact restore revision.
        let revision = serde_json::to_string(&original)?;
        let mut namespace = process_namespace.snapshot();
        namespace.unmount("/mnt/llm");
        namespace.mount(
            "/mnt/llm",
            alan_ap::InProcessTransport::new(Arc::new(llmfs)),
            alan_kernel::Access::ReadWrite,
        );
        Ok(CapturedCallable {
            identity: CallableIdentity {
                profile: profile_id.into(),
                provider: config.llm_provider.as_str().into(),
                model: config.effective_model().into(),
                credential_ref: original.credential_id,
                revision,
            },
            root: alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace))),
            connection: profile_id.into(),
            config,
        })
    }
}
