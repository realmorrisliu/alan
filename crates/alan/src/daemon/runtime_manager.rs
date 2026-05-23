//! Runtime Manager - directly manages `RuntimeController` instances.
//!
//! Replaces the legacy `WorkspaceManager` by removing the `WorkspaceInstance`
//! middle layer and managing the `session -> runtime` mapping directly.

use alan_runtime::runtime::{
    ChildRunRecord, ChildRunRegistryError, ChildRunTerminationMode, RuntimeController,
    RuntimeHandle, RuntimeStartupMetadata, WorkspaceRuntimeConfig, global_child_run_registry,
    spawn_with_tool_registry,
};
use alan_runtime::{AlanHomePaths, ModelCatalog};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::registry::generate_workspace_id;

/// Runtime entry for an active session
struct RuntimeEntry {
    /// Associated session ID
    #[allow(dead_code)]
    session_id: String,
    /// Workspace root path (tool working directory)
    #[allow(dead_code)]
    workspace_root_path: PathBuf,
    /// Workspace state directory (`.alan`)
    #[allow(dead_code)]
    workspace_alan_dir: PathBuf,
    /// Runtime controller
    controller: RuntimeController,
    /// Startup metadata describing durability/warnings for this runtime.
    startup: RuntimeStartupMetadata,
    /// Resolved connection profile id reported at startup.
    resolved_profile_id: Option<String>,
    /// Resolved provider reported at startup.
    resolved_provider: Option<alan_runtime::LlmProvider>,
    /// Resolved model reported at startup.
    resolved_model: String,
    /// Resolved reasoning effort reported at startup.
    effective_reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    /// Creation timestamp
    #[allow(dead_code)]
    created_at: Instant,
    /// Last activity timestamp
    #[allow(dead_code)]
    last_activity: Instant,
}

/// Runtime manager configuration
#[derive(Debug, Clone)]
pub struct RuntimeManagerConfig {
    /// Maximum number of concurrent runtimes
    pub max_concurrent_runtimes: usize,
    /// Global runtime config template
    pub runtime_config_template: WorkspaceRuntimeConfig,
}

impl Default for RuntimeManagerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_runtimes: 10,
            runtime_config_template: WorkspaceRuntimeConfig::default(),
        }
    }
}

/// Per-session runtime policy overrides.
#[derive(Debug, Clone, Default)]
pub struct RuntimeSessionPolicy {
    pub governance: alan_protocol::GovernanceConfig,
    pub agent_name: Option<String>,
    pub connection_profile: Option<String>,
    pub reasoning_effort: Option<alan_protocol::ReasoningEffort>,
    pub streaming_mode: Option<alan_runtime::StreamingMode>,
    pub partial_stream_recovery_mode: Option<alan_runtime::PartialStreamRecoveryMode>,
    pub durability_required: bool,
}

/// Runtime manager
///
/// Manages the `Session -> RuntimeController` mapping directly without a
/// `WorkspaceInstance` middle layer.
pub struct RuntimeManager {
    config: RuntimeManagerConfig,
    /// Serializes runtime start flow to avoid duplicate start races.
    start_lock: Mutex<()>,
    /// session_id -> RuntimeEntry
    runtimes: RwLock<HashMap<String, RuntimeEntry>>,
}

#[derive(Clone)]
pub struct RuntimeStartResult {
    pub handle: RuntimeHandle,
    pub startup: RuntimeStartupMetadata,
    pub resolved_profile_id: Option<String>,
    pub resolved_provider: Option<alan_runtime::LlmProvider>,
    pub resolved_model: String,
    pub effective_reasoning_effort: Option<alan_protocol::ReasoningEffort>,
}

impl RuntimeManager {
    /// Create a new `RuntimeManager`
    pub fn new(config: RuntimeManagerConfig) -> Self {
        Self {
            config,
            start_lock: Mutex::new(()),
            runtimes: RwLock::new(HashMap::new()),
        }
    }

    /// Create with default configuration
    #[allow(dead_code, clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(RuntimeManagerConfig::default())
    }

    /// Create with a runtime config template
    pub fn with_template(template: WorkspaceRuntimeConfig) -> Self {
        let config = RuntimeManagerConfig {
            runtime_config_template: template,
            ..Default::default()
        };
        Self::new(config)
    }

    pub fn runtime_config_template(&self) -> WorkspaceRuntimeConfig {
        self.config.runtime_config_template.clone()
    }

    /// Start a new runtime
    ///
    /// # Arguments
    /// * `session_id` - Session ID
    /// * `workspace_root_path` - Workspace root path (used as tool cwd)
    /// * `workspace_alan_dir` - Workspace `.alan` dir (agent overlays and generated runtime root)
    /// * `resume_rollout_path` - Optional rollout recovery path
    ///
    /// # Returns
    /// * `Ok(RuntimeHandle)` - Runtime started successfully
    /// * `Err(...)` - Startup failed or concurrency limit exceeded
    pub async fn start_runtime(
        &self,
        session_id: String,
        workspace_root_path: PathBuf,
        workspace_alan_dir: PathBuf,
        resume_rollout_path: Option<PathBuf>,
        session_policy: RuntimeSessionPolicy,
    ) -> anyhow::Result<RuntimeStartResult> {
        let _start_guard = self.start_lock.lock().await;
        self.cleanup_finished().await;

        // Check whether it already exists.
        {
            let runtimes = self.runtimes.read().await;
            if let Some(entry) = runtimes.get(&session_id)
                && !entry.controller.is_finished()
            {
                debug!(%session_id, "Runtime already exists and running");
                return Ok(RuntimeStartResult {
                    handle: entry.controller.handle.clone(),
                    startup: entry.startup.clone(),
                    resolved_profile_id: entry.resolved_profile_id.clone(),
                    resolved_provider: entry.resolved_provider,
                    resolved_model: entry.resolved_model.clone(),
                    effective_reasoning_effort: entry.effective_reasoning_effort,
                });
            }
        }

        let normalized_workspace = normalize_workspace_path(&workspace_root_path);
        {
            let runtimes = self.runtimes.read().await;
            if let Some((existing_session, _)) =
                runtimes.iter().find(|(existing_session, entry)| {
                    *existing_session != &session_id
                        && !entry.controller.is_finished()
                        && normalize_workspace_path(&entry.workspace_root_path)
                            == normalized_workspace
                })
            {
                anyhow::bail!(
                    "Workspace already has an active session runtime: {}",
                    existing_session
                );
            }
        }

        // Check concurrency limit.
        let current_count = self.runtime_count().await;
        if current_count >= self.config.max_concurrent_runtimes {
            anyhow::bail!(
                "Maximum number of concurrent runtimes ({}) reached",
                self.config.max_concurrent_runtimes
            );
        }

        info!(
            %session_id,
            workspace_root = %workspace_root_path.display(),
            workspace_alan = %workspace_alan_dir.display(),
            "Starting runtime"
        );

        // Build runtime configuration.
        let mut runtime_config = self.config.runtime_config_template.clone();
        runtime_config.session_id = Some(session_id.clone());
        runtime_config.workspace_id = generate_workspace_id(&normalized_workspace);
        runtime_config.agent_name = session_policy.agent_name.clone();
        if session_policy.connection_profile.is_some() {
            runtime_config.agent_config.core_config.connection_profile =
                session_policy.connection_profile.clone();
        }
        if let Some(reasoning_effort) = session_policy.reasoning_effort {
            runtime_config
                .agent_config
                .set_model_reasoning_effort_override(Some(reasoning_effort));
        }
        runtime_config.workspace_root_dir = Some(workspace_root_path.clone());
        runtime_config.workspace_alan_dir = Some(workspace_alan_dir.clone());
        runtime_config.resume_rollout_path = resume_rollout_path;
        runtime_config.agent_config.runtime_config.governance = session_policy.governance;
        runtime_config
            .agent_config
            .set_durability_required_override(session_policy.durability_required);
        if let Some(streaming_mode) = session_policy.streaming_mode {
            runtime_config
                .agent_config
                .set_streaming_mode_override(streaming_mode);
        }
        if let Some(partial_stream_recovery_mode) = session_policy.partial_stream_recovery_mode {
            runtime_config
                .agent_config
                .set_partial_stream_recovery_mode_override(partial_stream_recovery_mode);
        }
        let model_catalog = Arc::new(ModelCatalog::load_with_overlays(Some(
            &workspace_root_path,
        ))?);
        runtime_config
            .agent_config
            .core_config
            .set_model_catalog(model_catalog);
        runtime_config.agent_config.refresh_runtime_derived_fields();

        let resolved_core_config =
            alan_runtime::runtime::effective_core_config_for_runtime(&runtime_config)?;
        let resolved_model = resolved_core_config.effective_model().to_string();
        let resolved_connection = if let Some(home_paths) = runtime_config
            .agent_home_paths
            .clone()
            .or_else(AlanHomePaths::detect)
        {
            let mut resolved_for_metadata = resolved_core_config.clone();
            resolved_for_metadata
                .resolve_connection_profile(Some(&home_paths))
                .ok()
        } else {
            None
        };

        let mut tools = alan_runtime::tools::ToolRegistry::with_config(Arc::new(
            runtime_config.agent_config.core_config.clone(),
        ));
        alan_tools::register_builtin_tool_catalog(&mut tools);
        for tool in alan_tools::create_core_tools() {
            tools.register_boxed(tool);
        }

        // Start runtime.
        let mut controller = spawn_with_tool_registry(runtime_config, tools)?;

        // Wait until startup completes.
        let startup = match controller.wait_until_ready().await {
            Ok(startup) => startup,
            Err(err) => {
                controller.abort().await;
                return Err(err);
            }
        };

        let handle = controller.handle.clone();
        let effective_reasoning_effort = startup.request_controls.reasoning_effort();

        // Store runtime entry.
        let resolved_profile_id = resolved_connection
            .as_ref()
            .map(|resolved| resolved.profile_id.clone());
        let resolved_provider = resolved_connection
            .as_ref()
            .map(|resolved| resolved.provider);

        let entry = RuntimeEntry {
            session_id: session_id.clone(),
            workspace_root_path,
            workspace_alan_dir,
            controller,
            startup: startup.clone(),
            resolved_profile_id: resolved_profile_id.clone(),
            resolved_provider,
            resolved_model: resolved_model.clone(),
            effective_reasoning_effort,
            created_at: Instant::now(),
            last_activity: Instant::now(),
        };

        let mut runtimes = self.runtimes.write().await;
        runtimes.insert(session_id.clone(), entry);

        info!(%session_id, "Runtime started successfully");
        Ok(RuntimeStartResult {
            handle,
            startup,
            resolved_profile_id,
            resolved_provider,
            resolved_model,
            effective_reasoning_effort,
        })
    }

    /// Get the runtime handle
    ///
    /// Returns an error if the runtime does not exist or has stopped
    pub async fn get_handle(&self, session_id: &str) -> anyhow::Result<RuntimeHandle> {
        let runtimes = self.runtimes.read().await;
        match runtimes.get(session_id) {
            Some(entry) => {
                if entry.controller.is_finished() {
                    anyhow::bail!("Runtime for session {} has stopped", session_id);
                }
                Ok(entry.controller.handle.clone())
            }
            None => anyhow::bail!("Runtime for session {} not found", session_id),
        }
    }

    /// Stop runtime
    ///
    /// Stop a specific runtime
    #[allow(dead_code)]
    pub async fn stop_runtime(&self, session_id: &str) -> anyhow::Result<()> {
        let controller = {
            let mut runtimes = self.runtimes.write().await;
            match runtimes.remove(session_id) {
                Some(entry) => {
                    info!(%session_id, "Stopping runtime");
                    entry.controller
                }
                None => {
                    debug!(%session_id, "Runtime not found, already stopped?");
                    return Ok(());
                }
            }
        };

        // Execute shutdown outside the lock.
        match controller.shutdown().await {
            Ok(()) => {
                info!(%session_id, "Runtime stopped gracefully");
                Ok(())
            }
            Err(err) => {
                error!(%session_id, error = %err, "Runtime shutdown failed");
                Err(err)
            }
        }
    }

    /// Abort runtime
    ///
    /// Abort immediately without waiting for graceful shutdown
    #[allow(dead_code)]
    pub async fn abort_runtime(&self, session_id: &str) {
        let controller = {
            let mut runtimes = self.runtimes.write().await;
            runtimes.remove(session_id)
        };

        if let Some(entry) = controller {
            info!(%session_id, "Aborting runtime");
            entry.controller.abort().await;
        }
    }

    /// Check whether a runtime is running
    #[allow(dead_code)]
    pub async fn is_running(&self, session_id: &str) -> bool {
        let runtimes = self.runtimes.read().await;
        match runtimes.get(session_id) {
            Some(entry) => !entry.controller.is_finished(),
            None => false,
        }
    }

    /// Get current runtime count
    pub async fn runtime_count(&self) -> usize {
        // Clean up finished runtimes.
        self.cleanup_finished().await;
        self.runtimes.read().await.len()
    }

    /// Stop all runtimes
    #[allow(dead_code)]
    pub async fn stop_all(&self) {
        let controllers: Vec<_> = {
            let mut runtimes = self.runtimes.write().await;
            runtimes
                .drain()
                .map(|(_, entry)| entry.controller)
                .collect()
        };

        info!(count = controllers.len(), "Stopping all runtimes");

        for controller in controllers {
            // Shut down all runtimes.
            let _ = controller.shutdown().await;
        }
    }

    /// Get the runtime workspace root path
    #[allow(dead_code)]
    pub async fn get_workspace_path(&self, session_id: &str) -> Option<PathBuf> {
        let runtimes = self.runtimes.read().await;
        runtimes
            .get(session_id)
            .map(|e| e.workspace_root_path.clone())
    }

    /// Get the runtime workspace state directory (`.alan`)
    #[allow(dead_code)]
    pub async fn get_workspace_alan_dir(&self, session_id: &str) -> Option<PathBuf> {
        let runtimes = self.runtimes.read().await;
        runtimes
            .get(session_id)
            .map(|e| e.workspace_alan_dir.clone())
    }

    /// Update last activity timestamp
    #[allow(dead_code)]
    pub async fn touch(&self, session_id: &str) {
        let mut runtimes = self.runtimes.write().await;
        if let Some(entry) = runtimes.get_mut(session_id) {
            entry.last_activity = Instant::now();
        }
    }

    /// Get runtime info
    #[allow(dead_code)]
    pub async fn get_runtime_info(&self, session_id: &str) -> Option<RuntimeInfo> {
        let runtimes = self.runtimes.read().await;
        runtimes.get(session_id).map(|e| RuntimeInfo {
            session_id: e.session_id.clone(),
            workspace_path: e.workspace_root_path.clone(),
            workspace_alan_dir: e.workspace_alan_dir.clone(),
            created_at: e.created_at,
            last_activity: e.last_activity,
            is_running: !e.controller.is_finished(),
        })
    }

    /// List known child runs for a parent session.
    pub async fn list_child_runs(&self, session_id: &str) -> Vec<ChildRunRecord> {
        global_child_run_registry().list_for_parent(session_id)
    }

    /// Read a known child run for a parent session.
    pub async fn get_child_run(
        &self,
        session_id: &str,
        child_run_id: &str,
    ) -> Option<ChildRunRecord> {
        global_child_run_registry().get_for_parent(session_id, child_run_id)
    }

    /// Request child-run termination through the shared registry transition.
    pub async fn terminate_child_run(
        &self,
        session_id: &str,
        child_run_id: &str,
        actor: impl Into<String>,
        mode: ChildRunTerminationMode,
        reason: impl Into<String>,
    ) -> Result<ChildRunRecord, ChildRunRegistryError> {
        global_child_run_registry().request_termination(
            session_id,
            child_run_id,
            actor,
            mode,
            reason,
        )
    }

    /// List all running runtimes
    #[allow(dead_code)]
    pub async fn list_runtimes(&self) -> Vec<RuntimeInfo> {
        self.cleanup_finished().await;
        let runtimes = self.runtimes.read().await;
        runtimes
            .values()
            .map(|e| RuntimeInfo {
                session_id: e.session_id.clone(),
                workspace_path: e.workspace_root_path.clone(),
                workspace_alan_dir: e.workspace_alan_dir.clone(),
                created_at: e.created_at,
                last_activity: e.last_activity,
                is_running: !e.controller.is_finished(),
            })
            .collect()
    }

    /// Remove finished runtimes from the registry
    async fn cleanup_finished(&self) {
        let to_remove: Vec<String> = {
            let runtimes = self.runtimes.read().await;
            runtimes
                .iter()
                .filter(|(_, entry)| entry.controller.is_finished())
                .map(|(id, _)| id.clone())
                .collect()
        };

        if !to_remove.is_empty() {
            let mut runtimes = self.runtimes.write().await;
            for id in to_remove {
                warn!(session_id = %id, "Removing finished runtime from registry");
                runtimes.remove(&id);
            }
        }
    }
}

fn normalize_workspace_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Runtime info summary
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeInfo {
    pub session_id: String,
    pub workspace_path: PathBuf,
    pub workspace_alan_dir: PathBuf,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub is_running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_runtime::Config;
    use tempfile::TempDir;

    fn test_runtime_config() -> Config {
        Config::for_openai_responses("sk-test", None, Some("gpt-5.4"))
    }

    fn manager_with_isolated_agent_overlays(
        mut template: WorkspaceRuntimeConfig,
    ) -> RuntimeManager {
        template.core_config_source = alan_runtime::ConfigSourceKind::EnvOverride;
        RuntimeManager::with_template(template)
    }

    fn recorder_blocked_workspace(
        temp: &TempDir,
    ) -> (PathBuf, PathBuf, SessionsDirPermissionGuard) {
        let workspace_root = temp.path().join("workspace");
        let alan_dir = workspace_root.join(".alan");
        let sessions_dir = alan_runtime::workspace_runtime_sessions_dir_from_alan_dir(
            &alan_dir,
            alan_runtime::InstallChannel::Stable,
        );
        let memory_dir = alan_runtime::workspace_runtime_memory_dir_from_alan_dir(
            &alan_dir,
            alan_runtime::InstallChannel::Stable,
        );
        std::fs::create_dir_all(&sessions_dir).unwrap();
        std::fs::create_dir_all(memory_dir).unwrap();
        std::fs::create_dir_all(alan_dir.join("agents/default/persona")).unwrap();
        let guard = SessionsDirPermissionGuard::new(sessions_dir);
        (workspace_root, alan_dir, guard)
    }

    struct SessionsDirPermissionGuard {
        path: PathBuf,
    }

    impl SessionsDirPermissionGuard {
        fn new(path: PathBuf) -> Self {
            set_directory_writable(&path, false);
            Self { path }
        }
    }

    impl Drop for SessionsDirPermissionGuard {
        fn drop(&mut self) {
            set_directory_writable(&self.path, true);
        }
    }

    fn set_directory_writable(path: &Path, writable: bool) {
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            permissions.set_mode(if writable { 0o755 } else { 0o555 });
        }
        #[cfg(not(unix))]
        {
            permissions.set_readonly(!writable);
        }
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn test_runtime_manager_new() {
        let manager = RuntimeManager::default();
        assert_eq!(manager.config.max_concurrent_runtimes, 10);
    }

    #[test]
    fn test_runtime_manager_with_config() {
        let config = RuntimeManagerConfig {
            max_concurrent_runtimes: 5,
            ..Default::default()
        };
        let manager = RuntimeManager::new(config);
        assert_eq!(manager.config.max_concurrent_runtimes, 5);
    }

    #[tokio::test]
    async fn test_runtime_count_empty() {
        let manager = RuntimeManager::default();
        assert_eq!(manager.runtime_count().await, 0);
    }

    #[tokio::test]
    async fn test_is_running_nonexistent() {
        let manager = RuntimeManager::default();
        assert!(!manager.is_running("nonexistent").await);
    }

    #[tokio::test]
    async fn test_get_handle_nonexistent() {
        let manager = RuntimeManager::default();
        let result = manager.get_handle("nonexistent").await;
        match result {
            Err(e) => {
                let err_msg = format!("{}", e);
                assert!(err_msg.contains("not found"));
            }
            Ok(_) => panic!("Expected error for nonexistent session"),
        }
    }

    #[tokio::test]
    async fn test_stop_runtime_nonexistent() {
        let manager = RuntimeManager::default();
        // Should succeed silently (idempotent).
        let result = manager.stop_runtime("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_workspace_path() {
        let _temp = TempDir::new().unwrap();
        let manager = RuntimeManager::default();

        // Returns `None` when not started.
        assert!(manager.get_workspace_path("test-session").await.is_none());
    }

    #[tokio::test]
    async fn test_touch_nonexistent() {
        let manager = RuntimeManager::default();
        // Should succeed silently.
        manager.touch("nonexistent").await;
    }

    #[tokio::test]
    async fn test_list_runtimes_empty() {
        let manager = RuntimeManager::default();
        let list = manager.list_runtimes().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_runtime_info_nonexistent() {
        let manager = RuntimeManager::default();
        assert!(manager.get_runtime_info("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_limit() {
        let config = RuntimeManagerConfig {
            max_concurrent_runtimes: 0, // Set to 0 to test the limit.
            ..Default::default()
        };
        let manager = RuntimeManager::new(config);

        let temp = TempDir::new().unwrap();
        let result = manager
            .start_runtime(
                "test-session".to_string(),
                temp.path().to_path_buf(),
                temp.path().join(".alan"),
                None,
                RuntimeSessionPolicy::default(),
            )
            .await;

        match result {
            Err(e) => {
                let err_msg = format!("{}", e);
                assert!(err_msg.contains("Maximum number of concurrent runtimes"));
            }
            Ok(_) => panic!("Expected error for concurrent limit"),
        }
    }

    #[tokio::test]
    async fn test_start_runtime_loads_workspace_model_catalog_overlay() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let alan_dir = workspace_root.join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();
        std::fs::create_dir_all(alan_runtime::workspace_runtime_sessions_dir_from_alan_dir(
            &alan_dir,
            alan_runtime::InstallChannel::Stable,
        ))
        .unwrap();
        std::fs::create_dir_all(alan_runtime::workspace_runtime_memory_dir_from_alan_dir(
            &alan_dir,
            alan_runtime::InstallChannel::Stable,
        ))
        .unwrap();
        std::fs::create_dir_all(alan_dir.join("agents/default/persona")).unwrap();
        std::fs::write(
            alan_dir.join("models.toml"),
            r#"
[openai_chat_completions_compatible]
[[openai_chat_completions_compatible.models]]
slug = "custom-kimi"
family = "custom"
context_window_tokens = 654321
supports_reasoning = true
"#,
        )
        .unwrap();

        let config =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("custom-kimi"));
        let manager = manager_with_isolated_agent_overlays(WorkspaceRuntimeConfig::from(config));

        let result = manager
            .start_runtime(
                "test-session".to_string(),
                workspace_root,
                alan_dir,
                None,
                RuntimeSessionPolicy::default(),
            )
            .await;

        assert!(
            result.is_ok(),
            "expected runtime startup with overlay model to succeed: {:?}",
            result.as_ref().err()
        );
        manager.stop_runtime("test-session").await.unwrap();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_start_runtime_reports_best_effort_non_durable_startup() {
        let temp = TempDir::new().unwrap();
        let manager = manager_with_isolated_agent_overlays(WorkspaceRuntimeConfig::from(
            test_runtime_config(),
        ));
        let (workspace_root, alan_dir, _guard) = recorder_blocked_workspace(&temp);

        let result = manager
            .start_runtime(
                "test-session".to_string(),
                workspace_root,
                alan_dir,
                None,
                RuntimeSessionPolicy::default(),
            )
            .await
            .unwrap();

        assert!(!result.startup.durability.required);
        assert!(!result.startup.durability.durable);
        assert!(
            result
                .startup
                .warnings
                .iter()
                .any(|warning| warning.contains("in-memory mode"))
        );

        manager.stop_runtime("test-session").await.unwrap();
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_start_runtime_fails_when_strict_durability_is_required() {
        let mut config = test_runtime_config();
        config.durability.required = true;
        let temp = TempDir::new().unwrap();
        let manager = manager_with_isolated_agent_overlays(WorkspaceRuntimeConfig::from(config));
        let (workspace_root, alan_dir, _guard) = recorder_blocked_workspace(&temp);

        let err = match manager
            .start_runtime(
                "test-session".to_string(),
                workspace_root,
                alan_dir,
                None,
                RuntimeSessionPolicy {
                    durability_required: true,
                    ..RuntimeSessionPolicy::default()
                },
            )
            .await
        {
            Ok(_) => panic!("expected strict durability startup to fail"),
            Err(err) => err,
        };

        assert!(format!("{err:#}").contains("Strict durability required"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_start_runtime_preserves_explicit_best_effort_durability_over_workspace_overlay() {
        let temp = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let (workspace_root, alan_dir, _guard) = recorder_blocked_workspace(&temp);
        std::fs::write(
            alan_dir.join("agents/default/agent.toml"),
            r#"
[durability]
required = true
"#,
        )
        .unwrap();

        let mut template = WorkspaceRuntimeConfig::from(test_runtime_config());
        template.core_config_source = alan_runtime::ConfigSourceKind::GlobalAgentHome;
        template.agent_home_paths = Some(alan_runtime::AlanHomePaths::from_home_dir(home.path()));

        let manager = RuntimeManager::with_template(template);
        let result = manager
            .start_runtime(
                "test-session".to_string(),
                workspace_root,
                alan_dir,
                None,
                RuntimeSessionPolicy {
                    durability_required: false,
                    ..RuntimeSessionPolicy::default()
                },
            )
            .await
            .unwrap();

        assert!(!result.startup.durability.required);
        assert!(!result.startup.durability.durable);
        assert!(
            result
                .startup
                .warnings
                .iter()
                .any(|warning| warning.contains("in-memory mode"))
        );

        manager.stop_runtime("test-session").await.unwrap();
    }

    #[tokio::test]
    async fn test_stop_all_empty() {
        let manager = RuntimeManager::default();
        // Should succeed silently.
        manager.stop_all().await;
    }
}
