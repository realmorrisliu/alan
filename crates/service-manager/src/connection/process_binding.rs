//! Process-scoped callable authority; profile resolution stays in Connection Service.
use super::*;
use alan_agent_engine::runtime::model_binding::{
    CallableIdentity, CapturedCallable, ConnectionAuthority,
};

pub(crate) struct ProcessConnection {
    pub service: Arc<ConnectionService>,
    pub profile: String,
    pub namespace: alan_kernel::LiveNamespace,
    pub evaluator: Option<CapturedCallable>,
    pub guardian_profile: Option<String>,
}

#[async_trait::async_trait]
impl ConnectionAuthority for ProcessConnection {
    async fn capture_initial(&self) -> Result<Option<CapturedCallable>> {
        self.service.refresh().await?;
        let callables = self.service.callables.lock().await;
        let registry = callables
            .as_ref()
            .context("callable registry unavailable")?;
        let connections = self.service.metadata();
        let target_id = resolve_target_profile(&self.profile, &connections, registry);
        let available = target_id.as_ref().is_some_and(|id| {
            registry.published_profiles.contains_key(id)
                || registry.published_fallbacks.contains(id)
        });
        drop(callables);
        if !available {
            // Existing unavailable-provider boot mode grants no capture/restore authority.
            return Ok(None);
        }
        self.capture(None).await.map(Some)
    }

    async fn capture(&self, model: Option<&str>) -> Result<CapturedCallable> {
        let target_id = self.service.resolve_process_profile(&self.profile).await;
        self.service
            .capture_model(&target_id, model, &self.namespace)
            .await
    }

    async fn restore(&self, identity: &CallableIdentity) -> Result<CapturedCallable> {
        let target_id = self.service.resolve_process_profile(&self.profile).await;
        ensure!(
            identity.profile == self.profile || identity.profile == target_id,
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

    async fn capture_evaluation(&self, profile: &str) -> Result<CapturedCallable> {
        let captured = self
            .evaluator
            .as_ref()
            .context("Process has no explicitly granted evaluator")?;
        ensure!(
            captured.identity.profile == profile,
            "evaluator profile is outside Process authority"
        );
        Ok(captured.clone())
    }

    async fn capture_guardian(&self, profile: &str) -> Result<CapturedCallable> {
        ensure!(
            self.guardian_profile.as_deref() == Some(profile),
            "guardian profile is outside Process authority"
        );
        self.service
            .capture_model(profile, None, &self.namespace)
            .await
    }

    async fn catalog(&self) -> Result<serde_json::Value> {
        // Observation reads the Connection owner's published authority. Disk refresh
        // and callable construction belong to capture/restore and Connection operations,
        // not the Process's frequent model-status observation loop.
        let callables = self.service.callables.lock().await;
        let registry = callables
            .as_ref()
            .context("callable registry unavailable")?;
        let connections = self.service.metadata();
        let target_id = resolve_target_profile(&self.profile, &connections, registry)
            .unwrap_or_else(|| self.profile.clone());
        let injected = registry.published_fallbacks.contains(&target_id)
            && !connections.profiles.contains_key(&target_id);
        let mut config = registry.base_config.clone();
        let published_catalog = registry
            .published_catalogs
            .get(&target_id)
            .and_then(|publication| publication.get())
            .and_then(Option::as_ref);
        if let Some(catalog) = published_catalog.and_then(|cat| cat.engine_catalog.as_ref()) {
            config.set_model_catalog(catalog.clone());
        }
        if !injected {
            let profile = connections
                .profiles
                .get(&target_id)
                .context("Process profile unavailable")?;
            ensure!(
                registry.published_profiles.get(&target_id) == Some(profile),
                "Process profile is not callable"
            );
            let packages = self.service.package_service();
            let hydrated = provider_packages::hydrate_connections_with_package_defaults(
                packages.as_deref(),
                &connections,
                &target_id,
            );
            hydrated.apply_profile_metadata_to_config(Some(&target_id), &mut config)?;
        }
        if injected {
            let info = config.effective_model_info();
            return Ok(serde_json::json!({"profile":self.profile,"models":[{
                "model":config.effective_model(),
                "supported_reasoning_efforts":info.as_ref().map(|info| &info.supported_reasoning_efforts).cloned().unwrap_or_default(),
                "default_reasoning_effort":info.as_ref().and_then(|info| info.default_reasoning_effort)
            }]}));
        }
        let entries =
            crate::connection::catalog::catalog_entries_for_config(&config, published_catalog)?;
        Ok(serde_json::json!({"profile":self.profile,"models":entries}))
    }
}

impl ConnectionService {
    /// Capture an explicitly selected evaluator without touching generation selection.
    pub(crate) async fn capture_evaluation(
        &self,
        profile_id: &str,
        process_namespace: &alan_kernel::LiveNamespace,
    ) -> Result<CapturedCallable> {
        self.refresh().await?;
        let callables = self.callables.lock().await;
        let registry = callables
            .as_ref()
            .context("callable registry unavailable")?;
        let connections = self.metadata();
        let packages = self.package_service();
        let connections = provider_packages::hydrate_connections_with_package_defaults(
            packages.as_deref(),
            &connections,
            profile_id,
        );
        let profile = connections
            .profiles
            .get(profile_id)
            .context("evaluation profile unavailable")?;
        provider_packages::validate_provider_package_for_profile(packages.as_deref(), profile)?;
        ensure!(
            registry.published_profiles.get(profile_id) == Some(profile),
            "evaluation profile is not callable"
        );
        let llmfs = registry.llmfs.connection_snapshot(profile_id);
        ensure!(
            llmfs.supports_choice_evaluation(profile_id),
            "Connection does not support finite-choice evaluation"
        );
        let mut config = registry.base_config.clone();
        connections.apply_profile_metadata_to_config(Some(profile_id), &mut config)?;
        Ok(CapturedCallable {
            identity: CallableIdentity {
                profile: profile_id.into(),
                provider: config.llm_provider.as_str().into(),
                model: config.effective_model().into(),
                credential_ref: profile.credential_id.clone(),
                revision: serde_json::to_string(profile)?,
            },
            root: captured_namespace(process_namespace, llmfs),
            connection: profile_id.into(),
            config,
        })
    }

    async fn resolve_process_profile(&self, requested_profile: &str) -> String {
        let callables = self.callables.lock().await;
        let Some(registry) = callables.as_ref() else {
            return requested_profile.to_string();
        };
        let connections = self.metadata();
        resolve_target_profile(requested_profile, &connections, registry)
            .unwrap_or_else(|| requested_profile.to_string())
    }

    async fn capture_model(
        &self,
        profile_id: &str,
        model: Option<&str>,
        process_namespace: &alan_kernel::LiveNamespace,
    ) -> Result<CapturedCallable> {
        let profile_id = self.resolve_process_profile(profile_id).await;
        let profile_id = profile_id.as_str();
        if model.is_some() {
            let mut callables = self.callables.lock().await;
            if let Some(registry) = callables.as_mut()
                && matches!(
                    registry
                        .published_catalogs
                        .get(profile_id)
                        .and_then(|p| p.get()),
                    Some(None)
                )
            {
                registry.published_catalogs.remove(profile_id);
            }
        }
        self.refresh().await?;
        let callables = self.callables.lock().await;
        let registry = callables
            .as_ref()
            .context("callable registry unavailable")?;
        let raw_connections = self.metadata();
        let packages = self.package_service();
        // Only an explicitly published injected callable is a fallback. Missing
        // managed metadata alone never grants callable authority.
        if registry.published_fallbacks.contains(profile_id)
            && !raw_connections.profiles.contains_key(profile_id)
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
        let original = raw_connections
            .profiles
            .get(profile_id)
            .context("Process profile unavailable")?
            .clone();
        provider_packages::validate_provider_package_for_profile(packages.as_deref(), &original)?;
        ensure!(
            original.provider.supports_generation(),
            "evaluation-only profile cannot be captured for generation"
        );
        ensure!(
            registry.published_profiles.get(profile_id) == Some(&original),
            "Process profile is not callable"
        );
        let mut connections = provider_packages::hydrate_connections_with_package_defaults(
            packages.as_deref(),
            &raw_connections,
            profile_id,
        );
        let mut config = registry.base_config.clone();
        let published_catalog = registry
            .published_catalogs
            .get(profile_id)
            .and_then(|publication| publication.get())
            .and_then(Option::as_ref);
        if let Some(catalog) = published_catalog.and_then(|cat| cat.engine_catalog.as_ref()) {
            config.set_model_catalog(catalog.clone());
        }
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
                crate::connection::catalog::is_model_authorized(&config, model, published_catalog,),
                "model is unavailable in Connection catalog"
            );
        }
        let llmfs = if model.is_none()
            || model == Some(config.effective_model())
                && original.settings.get("model").map(String::as_str) == model
                && !registry.published_accounts.contains_key(profile_id)
        {
            registry.llmfs.connection_snapshot(profile_id)
        } else {
            let client =
                registry
                    .factory
                    .create(&registry.base_config, Some(profile_id), &connections)?;
            ensure!(
                client.account_identity()
                    == registry
                        .published_accounts
                        .get(profile_id)
                        .map(String::as_str),
                "provider account changed since callable publication"
            );
            let llmfs = alan_llmfs::LlmFs::new();
            llmfs.register_connection(profile_id, Box::new(ConnectionLlmProvider { client }));
            llmfs.connection_snapshot(profile_id)
        };
        // Only non-secret metadata names the exact restore revision.
        let revision = if let Some(account) = registry.published_accounts.get(profile_id) {
            config.chatgpt_account_id = Some(account.clone());
            if original.settings.get("account_id") == Some(account) {
                // Explicitly bound profiles already carry account identity in their
                // durable revision; preserve compatibility with earlier captures.
                serde_json::to_string(&original)?
            } else {
                serde_json::to_string(&(original.clone(), account))?
            }
        } else {
            serde_json::to_string(&original)?
        };
        Ok(CapturedCallable {
            identity: CallableIdentity {
                profile: profile_id.into(),
                provider: config.llm_provider.as_str().into(),
                model: config.effective_model().into(),
                credential_ref: original.credential_id,
                revision,
            },
            root: captured_namespace(process_namespace, llmfs),
            connection: profile_id.into(),
            config,
        })
    }
}

fn resolve_target_profile(
    requested_profile: &str,
    connections: &ConnectionsFile,
    registry: &CallableRegistry,
) -> Option<String> {
    if registry.live_default_redirect
        && requested_profile == "default"
        && let Some(default_profile) = &connections.default_profile
        && connections.profiles.contains_key(default_profile)
    {
        return Some(default_profile.clone());
    }
    if connections.profiles.contains_key(requested_profile)
        || registry.published_fallbacks.contains(requested_profile)
    {
        return Some(requested_profile.to_string());
    }
    None
}

fn captured_namespace(
    process_namespace: &alan_kernel::LiveNamespace,
    llmfs: alan_llmfs::LlmFs,
) -> alan_ap::InProcessTransport {
    let mut namespace = process_namespace.snapshot();
    namespace.unmount("/mnt/llm");
    namespace.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)))
}
