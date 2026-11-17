//! Embeddable in-process Alan runtime facade for Rust host applications.
//!
//! Each [`EmbeddedAlan`] instance owns an isolated [`ServiceManager`],
//! [`alan_kernel::ProcessTable`], `/agent/root`, and optional app-scoped
//! durable stores. It never reads global `Alan` product directories,
//! `ALAN_CONFIG_PATH`, CLI installation metadata, or global host locks.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alan_agent_engine::{
    AgentProcessConfig, InputIntent, InputMode, LlmClient, ProcessDescriptor, ProcessFileTree,
    ToolRegistry, UserInputRecord,
};
use alan_ap::{FileServer, InProcessTransport};
use alan_kernel::{Access, Credentials, Namespace, Pid};
use alan_service_manager::{
    ConnectionsFile, HostMountExportAdapter, LlmClientFactory, ProcessLaunchContext,
    ServiceManager, ServiceManagerConfig, UnavailableHostMountExportAdapter,
};
use alan_shell::{Shell, Tail};
use anyhow::{Context, Result, ensure};
use uuid::Uuid;

use crate::boot::{ProductLlmClientFactory, prepare_system_store_launch};
use crate::paths::{HostStorePaths, SystemStorePaths};

/// Storage isolation mode for an embedded Alan instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageMode {
    /// Purely in-memory ephemeral execution with no disk persistence.
    Ephemeral,
    /// Durable state isolated strictly under app-supplied root directories.
    Isolated {
        /// Root directory for this instance's System Store (rollouts, checkpoints, memory, etc.).
        system_store_root: PathBuf,
        /// Optional root directory for app-scoped credentials and managed auth.
        host_store_root: Option<PathBuf>,
    },
}

/// Model/credential source for an embedded Alan instance.
#[derive(Debug)]
pub enum LlmSource {
    /// Host-supplied factory controlling model resolution and credentials.
    Factory(Arc<dyn LlmClientFactory>),
    /// Single pre-built [`LlmClient`] wrapped for one Root Agent launch.
    Client(LlmClient),
    /// Use Alan's `ConnectionService` + `HostStore` scoped to [`StorageMode::Isolated::host_store_root`].
    IsolatedHostStore,
}

#[derive(Debug)]
struct SingleLlmClientFactory(Mutex<Option<LlmClient>>);

impl LlmClientFactory for SingleLlmClientFactory {
    fn create(
        &self,
        _base_config: &alan_agent_engine::Config,
        _selected_profile: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        self.0
            .lock()
            .map_err(|_| anyhow::anyhow!("embedded LLM client lock poisoned"))?
            .take()
            .context("embedded single LLM client was already consumed")
    }
}

/// Configuration for booting an isolated [`EmbeddedAlan`] instance.
pub struct EmbeddedConfig {
    pub storage: StorageMode,
    pub process: AgentProcessConfig,
    pub llm: LlmSource,
    pub tools: ToolRegistry,
    pub host_mount_adapter: Arc<dyn HostMountExportAdapter>,
    pub namespace_mounts: Vec<(String, InProcessTransport, Access)>,
    pub definition: Option<ProcessFileTree>,
    pub resume_root: bool,
}

impl EmbeddedConfig {
    /// Create an ephemeral, zero-host-access embedded configuration.
    pub fn ephemeral(llm: LlmSource) -> Self {
        Self {
            storage: StorageMode::Ephemeral,
            process: AgentProcessConfig::default(),
            llm,
            tools: ToolRegistry::new(),
            host_mount_adapter: Arc::new(UnavailableHostMountExportAdapter),
            namespace_mounts: Vec::new(),
            definition: None,
            resume_root: false,
        }
    }

    /// Create an isolated-disk, zero-host-access embedded configuration.
    pub fn isolated(system_store_root: impl Into<PathBuf>, llm: LlmSource) -> Self {
        Self {
            storage: StorageMode::Isolated {
                system_store_root: system_store_root.into(),
                host_store_root: None,
            },
            process: AgentProcessConfig::default(),
            llm,
            tools: ToolRegistry::new(),
            host_mount_adapter: Arc::new(UnavailableHostMountExportAdapter),
            namespace_mounts: Vec::new(),
            definition: None,
            resume_root: false,
        }
    }

    /// Set an isolated Host Store root when using [`StorageMode::Isolated`].
    pub fn with_host_store(mut self, host_store_root: impl Into<PathBuf>) -> Result<Self> {
        match &mut self.storage {
            StorageMode::Isolated {
                host_store_root: slot,
                ..
            } => {
                *slot = Some(host_store_root.into());
                Ok(self)
            }
            StorageMode::Ephemeral => {
                anyhow::bail!("ephemeral storage mode cannot attach a Host Store directory")
            }
        }
    }

    /// Override the default [`AgentProcessConfig`].
    pub fn with_process_config(mut self, process: AgentProcessConfig) -> Self {
        self.process = process;
        self
    }

    /// Supply the explicit [`ToolRegistry`] available to Agent Processes.
    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = tools;
        self
    }

    /// Supply an explicit [`HostMountExportAdapter`] if the host allows host directory mounts.
    pub fn with_host_mount_adapter(mut self, adapter: Arc<dyn HostMountExportAdapter>) -> Self {
        self.host_mount_adapter = adapter;
        self
    }

    /// Mount an app-owned [`FileServer`] into the root namespace (e.g. `/mnt/my-app`).
    pub fn with_mount(
        mut self,
        path: impl Into<String>,
        server: Arc<dyn FileServer>,
        access: Access,
    ) -> Self {
        self.namespace_mounts
            .push((path.into(), InProcessTransport::new(server), access));
        self
    }

    /// Supply an explicit in-memory Agent Definition tree for `/lib/agents/root`.
    pub fn with_definition(mut self, definition: ProcessFileTree) -> Self {
        self.definition = Some(definition);
        self
    }

    /// Enable Root Agent rollout resumption (requires [`StorageMode::Isolated`]).
    pub fn with_root_resume(mut self, resume_root: bool) -> Self {
        self.resume_root = resume_root;
        self
    }

    fn into_service_manager_config(mut self) -> Result<ServiceManagerConfig> {
        let mut namespace = Namespace::new();
        for (path, transport, access) in self.namespace_mounts {
            namespace.mount(&path, transport, access);
        }

        let (launch_context, connection_store, package_store, host_store_paths, in_memory_stores) =
            match &self.storage {
                StorageMode::Ephemeral => {
                    ensure!(
                        !self.resume_root,
                        "Root Agent resume requires StorageMode::Isolated"
                    );
                    if namespace.resolve("/memory").is_err() {
                        namespace.mount(
                            "/memory",
                            InProcessTransport::new(
                                Arc::new(alan_ap::reference::MemFs::writable()),
                            ),
                            Access::ReadWrite,
                        );
                    }
                    if namespace.resolve("/lib/agents/root").is_err() {
                        namespace.mount(
                            "/lib/agents/root",
                            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                            Access::ReadOnly,
                        );
                    }
                    let mut ctx = ProcessLaunchContext::new(namespace, Credentials::system(), "/")?
                        .with_descriptor(
                            alan_agent_engine::MEMORY_STORE_DESCRIPTOR,
                            ProcessDescriptor::new("/memory")?,
                        );
                    if let Some(tree) = self.definition {
                        ctx = ctx.with_descriptor(
                            alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
                            ProcessDescriptor::with_file_tree("/lib/agents/root", tree)?,
                        );
                    }
                    (ctx, None, None, None, true)
                }
                StorageMode::Isolated {
                    system_store_root,
                    host_store_root,
                } => {
                    let system_store = SystemStorePaths::from_root(system_store_root)?;
                    let host_store = host_store_root
                        .as_deref()
                        .map(HostStorePaths::from_root)
                        .transpose()?;
                    let ctx = prepare_system_store_launch(
                        &system_store,
                        namespace,
                        &mut self.process,
                        self.definition,
                    )?;
                    let connection_store = Some(system_store.connection_bindings()?);
                    let package_store = Some(system_store.packages()?);
                    (ctx, connection_store, package_store, host_store, false)
                }
            };

        let llm_factory: Arc<dyn LlmClientFactory> = match self.llm {
            LlmSource::Factory(factory) => factory,
            LlmSource::Client(client) => Arc::new(SingleLlmClientFactory(Mutex::new(Some(client)))),
            LlmSource::IsolatedHostStore => {
                let host_store = host_store_paths.context(
                    "LlmSource::IsolatedHostStore requires StorageMode::Isolated with host_store_root",
                )?;
                Arc::new(ProductLlmClientFactory::new(
                    host_store.credentials,
                    Some(host_store.managed_auth),
                ))
            }
        };

        Ok(ServiceManagerConfig {
            process: self.process,
            resume_root: self.resume_root,
            input_shadow: None,
            launch_context,
            connection_store,
            package_store,
            in_memory_stores,
            llm_factory,
            host_mount_adapter: self.host_mount_adapter,
            tools: self.tools,
        })
    }
}

/// One running, self-contained embedded Alan runtime instance.
pub struct EmbeddedAlan {
    manager: ServiceManager,
    root_transport: InProcessTransport,
}

impl EmbeddedAlan {
    /// Boot an isolated embedded Alan instance in-process.
    pub async fn boot(config: EmbeddedConfig) -> Result<Self> {
        let sm_config = config.into_service_manager_config()?;
        let manager = ServiceManager::boot(sm_config).await?;
        let root_transport =
            InProcessTransport::new(manager.local_entry().namespace_for_local_client());
        Ok(Self {
            manager,
            root_transport,
        })
    }

    /// Unique boot identifier for this embedded instance.
    pub fn boot_id(&self) -> Uuid {
        self.manager.boot_id()
    }

    /// Process ID of the instance's Root Agent Process.
    pub fn root_pid(&self) -> Pid {
        self.manager.root_pid()
    }

    /// Raw `aP` transport bound to the root namespace of this instance.
    pub fn root_transport(&self) -> InProcessTransport {
        self.root_transport.clone()
    }

    /// Create an `aP`-only [`Shell`] client over this instance's namespace.
    pub fn shell(&self) -> Shell {
        Shell::new(self.root_transport.clone())
    }

    /// Return an ergonomic client bound to `/agent/<root_pid>`.
    pub fn root_agent(&self) -> RootAgentClient {
        RootAgentClient::new(self.shell(), format!("/agent/{}", self.root_pid().0))
    }

    /// Gracefully shut down the Root Agent and internal [`ServiceManager`].
    pub async fn shutdown(self) -> Result<()> {
        self.manager.shutdown().await
    }
}

/// Ergonomic `aP` client helpers over an Agent Process directory (`/agent/<pid>`).
#[derive(Clone)]
pub struct RootAgentClient {
    shell: Shell,
    agent_path: String,
}

impl RootAgentClient {
    /// Bind a client to a specific `/agent/<pid>` path.
    pub fn new(shell: Shell, agent_path: impl Into<String>) -> Self {
        Self {
            shell,
            agent_path: agent_path.into(),
        }
    }

    /// Underlying `aP`-only [`Shell`] client.
    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    /// Target agent path (e.g. `/agent/8`).
    pub fn agent_path(&self) -> &str {
        &self.agent_path
    }

    /// Submit a plain text turn to `io/input` and return its `submission_id`.
    pub async fn submit_text(&self, text: impl Into<String>) -> Result<String> {
        let record = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, text);
        let submission_id = record.submission_id.clone();
        self.submit_record(&record).await?;
        Ok(submission_id)
    }

    /// Submit an encoded [`UserInputRecord`] to `io/input`.
    pub async fn submit_record(&self, record: &UserInputRecord) -> Result<()> {
        let path = format!("{}/io/input", self.agent_path);
        self.shell
            .write(&path, &record.encode_payload()?)
            .await
            .map_err(|err| anyhow::anyhow!("write embedded agent input failed: {err:?}"))
    }

    /// Open a streaming tail on `io/output` for assistant text.
    pub async fn tail_output(&self) -> Result<Tail> {
        let path = format!("{}/io/output", self.agent_path);
        self.shell
            .tail(&path)
            .await
            .map_err(|err| anyhow::anyhow!("tail embedded agent output failed: {err:?}"))
    }

    /// Open a streaming tail on `machine/ui/events` for structured UI events.
    pub async fn tail_ui_events(&self) -> Result<Tail> {
        let path = format!("{}/machine/ui/events", self.agent_path);
        self.shell
            .tail(&path)
            .await
            .map_err(|err| anyhow::anyhow!("tail embedded ui events failed: {err:?}"))
    }

    /// Open a streaming tail on `requests/events` for pending yield notifications.
    pub async fn tail_requests(&self) -> Result<Tail> {
        let path = format!("{}/requests/events", self.agent_path);
        self.shell
            .tail(&path)
            .await
            .map_err(|err| anyhow::anyhow!("tail embedded requests failed: {err:?}"))
    }

    /// Write a response to a pending yield request under `requests/<request_id>/response`.
    pub async fn respond_request(&self, request_id: &str, response: &str) -> Result<()> {
        let path = format!("{}/requests/{request_id}/response", self.agent_path);
        self.shell
            .write(&path, response.as_bytes())
            .await
            .map_err(|err| anyhow::anyhow!("write embedded request response failed: {err:?}"))
    }

    /// Issue an interrupt command to `machine/ctl`.
    pub async fn interrupt(&self, submission_id: Option<&str>) -> Result<()> {
        let path = format!("{}/machine/ctl", self.agent_path);
        let cmd = match submission_id {
            Some(id) => format!("queue-v1 interrupt {id}"),
            None => "interrupt".to_string(),
        };
        self.shell
            .write(&path, cmd.as_bytes())
            .await
            .map_err(|err| anyhow::anyhow!("interrupt embedded agent failed: {err:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::LlmClient;
    use alan_llm::{GenerationResponse, MockLlmProvider};

    fn mock_response(text: &str) -> GenerationResponse {
        GenerationResponse {
            content: text.to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        }
    }

    async fn wait_for_tail_contains(tail: &mut Tail, expected: &str) -> String {
        let mut collected = String::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while !collected.contains(expected) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for `{expected}`, got `{collected}`"
            );
            let chunk = tokio::time::timeout(remaining, tail.read(4096))
                .await
                .expect("timed out waiting for tail chunk")
                .expect("tail read failed");
            if chunk.is_empty() {
                break;
            }
            collected.push_str(&String::from_utf8_lossy(&chunk));
        }
        collected
    }

    #[tokio::test]
    async fn ephemeral_embedded_instance_executes_turn_and_has_no_host_tools() {
        let provider =
            MockLlmProvider::new().with_response(mock_response("hello from embedded alan"));
        let config = EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(provider)));
        let app = EmbeddedAlan::boot(config).await.unwrap();

        // Default embedded instance mounts no host directories under /mnt
        let mnt_entries = app.shell().ls("/mnt").await.unwrap();
        assert!(!mnt_entries.iter().any(|entry| entry == "host"));

        let client = app.root_agent();
        let mut output = client.tail_output().await.unwrap();
        let _submission_id = client.submit_text("say hi").await.unwrap();

        let text = wait_for_tail_contains(&mut output, "hello from embedded alan").await;
        assert!(text.contains("hello from embedded alan"), "got: {text}");

        app.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn ephemeral_embedded_instance_has_writable_memory_and_bound_descriptor() {
        let provider = MockLlmProvider::new().with_response(mock_response("ok"));
        let config = EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(provider)));
        let sm_config = config.into_service_manager_config().unwrap();
        assert!(sm_config.in_memory_stores);
        assert!(
            sm_config
                .launch_context
                .descriptor(alan_agent_engine::MEMORY_STORE_DESCRIPTOR)
                .is_some()
        );

        use alan_ap::{Fid, Request};
        let resolved = sm_config
            .launch_context
            .namespace
            .resolve("/memory")
            .expect("/memory must be mounted in ephemeral mode");
        assert_eq!(resolved.access, Access::ReadWrite);
        let ns_transport = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
            sm_config.launch_context.namespace,
        )));
        ns_transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["memory".to_string()],
            })
            .await
            .unwrap();
        ns_transport
            .call(Request::Create {
                fid: Fid(1),
                newfid: Fid(2),
                name: "note.txt".to_string(),
                kind: alan_ap::FileKind::File,
            })
            .await
            .unwrap();
        ns_transport
            .call(Request::Open {
                fid: Fid(2),
                mode: alan_ap::OpenMode::Write,
            })
            .await
            .unwrap();
        ns_transport
            .call(Request::Write {
                fid: Fid(2),
                offset: 0,
                data: b"ephemeral-memory".to_vec(),
            })
            .await
            .unwrap();
        ns_transport
            .call(Request::Clunk { fid: Fid(2) })
            .await
            .unwrap();
        ns_transport
            .call(Request::Clunk { fid: Fid(1) })
            .await
            .unwrap();

        let shell = Shell::new(ns_transport);
        let stored = shell.cat("/memory/note.txt").await.unwrap();
        assert_eq!(stored, b"ephemeral-memory");

        let provider = MockLlmProvider::new().with_response(mock_response("ok"));
        let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
            LlmClient::new(provider),
        )))
        .await
        .unwrap();
        app.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn isolated_instances_do_not_interfere_and_support_custom_mounts() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();

        let fs_a = Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "note.txt",
            b"only-a".to_vec(),
        ));
        let fs_b = Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "note.txt",
            b"only-b".to_vec(),
        ));

        let provider_a = MockLlmProvider::new().with_response(mock_response("reply A"));
        let provider_b = MockLlmProvider::new().with_response(mock_response("reply B"));

        let app_a = EmbeddedAlan::boot(
            EmbeddedConfig::isolated(dir_a.path(), LlmSource::Client(LlmClient::new(provider_a)))
                .with_mount("/mnt/app-a", fs_a, Access::ReadOnly),
        )
        .await
        .unwrap();

        let app_b = EmbeddedAlan::boot(
            EmbeddedConfig::isolated(dir_b.path(), LlmSource::Client(LlmClient::new(provider_b)))
                .with_mount("/mnt/app-b", fs_b, Access::ReadOnly),
        )
        .await
        .unwrap();

        assert_ne!(app_a.boot_id(), app_b.boot_id());

        // Each instance sees only its own app mount
        assert_eq!(
            app_a.shell().cat("/mnt/app-a/note.txt").await.unwrap(),
            b"only-a"
        );
        assert!(app_b.shell().ls("/mnt/app-a").await.is_err());

        // Turn execution works independently on both instances
        let mut out_a = app_a.root_agent().tail_output().await.unwrap();
        let mut out_b = app_b.root_agent().tail_output().await.unwrap();
        app_a.root_agent().submit_text("hi a").await.unwrap();
        app_b.root_agent().submit_text("hi b").await.unwrap();

        wait_for_tail_contains(&mut out_a, "reply A").await;
        wait_for_tail_contains(&mut out_b, "reply B").await;

        // Rollouts are isolated inside each app's own root directory
        assert!(
            dir_a
                .path()
                .join("services/agent-runtime/rollouts")
                .exists()
        );
        assert!(
            dir_b
                .path()
                .join("services/agent-runtime/rollouts")
                .exists()
        );

        // Shutting down A does not affect B
        app_a.shutdown().await.unwrap();
        assert!(app_b.shell().ls("/agent/root").await.is_ok());
        app_b.shutdown().await.unwrap();
    }
}
