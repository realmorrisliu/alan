//! Actual managed-unavailable startup must preserve deterministic command dispatch.
use super::*;
use crate::host_mount::{HostMountExport, HostMountToolProjection};
use alan_agent_engine::tools::{Sandbox, ToolExecutionAdapter};
use alan_agent_protocol::{UiEvent, UiInputStatus};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct UnavailableDefaultFactory(MockLlmProvider);
impl LlmClientFactory for UnavailableDefaultFactory {
    fn create(
        &self,
        _: &alan_agent_engine::Config,
        profile: Option<&str>,
        _: &ConnectionsFile,
    ) -> Result<LlmClient> {
        ensure!(
            profile == Some("other-profile"),
            "selected managed profile unavailable"
        );
        Ok(LlmClient::new(self.0.clone()))
    }
}

struct CommandExport {
    tree: Arc<alan_hostfs::HostDirFs>,
    _directory: tempfile::TempDir,
}
impl std::fmt::Debug for CommandExport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CommandExport")
    }
}
impl HostMountExport for CommandExport {
    fn file_tree(&self) -> InProcessTransport {
        InProcessTransport::new(self.tree.clone())
    }
    fn revoke(&self) {
        self.tree.revoke();
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// Only deterministic namespace cd is implemented; no native shell or credentials.
#[derive(Debug)]
struct CommandAdapter(PathBuf);
impl ToolExecutionAdapter for CommandAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        self.0.clone()
    }
    fn cwd(&self) -> Result<PathBuf> {
        anyhow::bail!("no native execution")
    }
    fn resolve_path(&self, _: &Path, _: &Path) -> Result<PathBuf> {
        anyhow::bail!("no native execution")
    }
    fn resolve_directory(&self, _: &Path, path: &Path) -> Result<PathBuf> {
        ensure!(
            path.to_string_lossy().starts_with("/mnt/project-"),
            "outside test directory"
        );
        Ok(path.to_path_buf())
    }
    fn visible_path(&self, _: &Path) -> PathBuf {
        PathBuf::from("<unmapped>")
    }
    fn project_text(&self, text: &str) -> String {
        text.into()
    }
    fn sandbox(&self) -> Result<Sandbox> {
        anyhow::bail!("no native execution")
    }
}
impl HostMountExportAdapter for CommandAdapter {
    fn tool_execution_adapter(
        &self,
        _: &[HostMountToolProjection],
        cwd: &Path,
    ) -> Result<Arc<dyn ToolExecutionAdapter>> {
        Ok(Arc::new(CommandAdapter(cwd.to_path_buf())))
    }
}

async fn settlement(shell: &alan_shell::Shell, id: &str) -> (UiInputStatus, Option<String>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events =
                String::from_utf8(shell.cat("/agent/root/machine/ui/events").await.unwrap())
                    .unwrap();
            for line in events.lines() {
                if let UiEvent::InputCompleted {
                    submission_ids,
                    status,
                    error,
                } = serde_json::from_str(line).unwrap()
                    && submission_ids.iter().any(|entry| entry == id)
                {
                    return (status, error);
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("correlated input settlement")
}

#[tokio::test]
async fn unavailable_managed_default_dispatches_direct_command_without_generation() {
    let temp = tempfile::tempdir().unwrap();
    let metadata = temp.path().join("connections.toml");
    let now = chrono::Utc::now();
    ConnectionsFile {
        version: 1,
        default_profile: Some("default-profile".into()),
        credentials: [(
            "missing-secret".into(),
            ConnectionCredential {
                kind: CredentialKind::SecretString,
                provider_family: ConnectionProvider::OpenAiResponses,
                label: "Missing secret".into(),
                backend: "host_credential_store".into(),
            },
        )]
        .into_iter()
        .collect(),
        profiles: [
            (
                "other-profile".into(),
                ConnectionProfile {
                    provider: ConnectionProvider::OpenAiResponses,
                    label: None,
                    credential_id: Some("missing-secret".into()),
                    created_at: now,
                    updated_at: now,
                    source: "managed".into(),
                    settings: BTreeMap::new(),
                },
            ),
            (
                "default-profile".into(),
                ConnectionProfile {
                    provider: ConnectionProvider::OpenAiResponses,
                    label: None,
                    credential_id: Some("missing-secret".into()),
                    created_at: now,
                    updated_at: now,
                    source: "managed".into(),
                    settings: BTreeMap::new(),
                },
            ),
        ]
        .into_iter()
        .collect(),
    }
    .save_to_path(&metadata)
    .unwrap();
    // A published unrelated managed callable must never replace the unavailable default.
    let probe = MockLlmProvider::new();
    let mut config = ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(probe.clone()),
        ToolRegistry::new(),
    );
    config.connection_store = Some(ConnectionStoreBindings::new(metadata).unwrap());
    config.llm_factory = Arc::new(UnavailableDefaultFactory(probe.clone()));
    config.host_mount_adapter = Arc::new(CommandAdapter(PathBuf::from("/")));
    config.process.store_bindings = Some(alan_agent_engine::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("runtime-metadata"),
    });
    let manager = ServiceManager::boot(config).await.unwrap();
    let request = manager
        .host_mount()
        .request_project_mount(manager.root_pid(), crate::HostMountAccess::ReadOnly)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let grant = manager
        .host_mount()
        .approve_export(
            &request,
            Arc::new(CommandExport {
                tree: Arc::new(
                    alan_hostfs::HostDirFs::new(
                        directory.path(),
                        alan_hostfs::HostDirAccess::ReadOnly,
                    )
                    .unwrap(),
                ),
                _directory: directory,
            }),
            "test",
            "test",
        )
        .unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
    assert_eq!(
        shell.cat("/mnt/connections/validation").await.unwrap(),
        br#"{"default-profile":"unavailable","other-profile":"ready"}"#
    );
    assert_eq!(manager.root_model(), None);
    let models: alan_agent_protocol::UiModelSnapshot =
        serde_json::from_slice(&shell.cat("/agent/root/machine/ui/models").await.unwrap()).unwrap();
    assert!(models.is_valid());
    assert!(models.catalog.is_none());
    assert!(models.selected_next.is_none());
    assert!(models.active.is_none());
    assert!(models.admitted.is_empty());
    let command_id = uuid::Uuid::new_v4().to_string();
    let frame = serde_json::json!({"version":1,"submission_id":command_id,"intent":"command","mode":"follow_up","body":format!("cd {}", grant.namespace_path)});
    shell
        .write(
            "/agent/root/io/input",
            format!("alan-input-v1\n{frame}").as_bytes(),
        )
        .await
        .unwrap();
    let command = settlement(&shell, &command_id).await;
    let mut cd_result = None;
    for action in shell.ls("/agent/root/actions").await.unwrap() {
        if !action.starts_with('a') {
            continue;
        }
        let result: serde_json::Value = serde_json::from_slice(
            &shell
                .cat(&format!("/agent/root/actions/{action}/result"))
                .await
                .unwrap(),
        )
        .unwrap_or_default();
        if result["call_id"] == command_id {
            cd_result = Some(result);
        }
    }
    let agent_id = uuid::Uuid::new_v4().to_string();
    let frame = serde_json::json!({"version":1,"submission_id":agent_id,"intent":"agent","mode":"follow_up","body":"must not generate"});
    shell
        .write(
            "/agent/root/io/input",
            format!("alan-input-v1\n{frame}").as_bytes(),
        )
        .await
        .unwrap();
    let agent = settlement(&shell, &agent_id).await;
    manager.shutdown().await.unwrap();
    assert_eq!(
        command,
        (UiInputStatus::Completed, None),
        "direct command must reach its existing handler"
    );
    let result = cd_result.expect("existing cd handler must write a correlated Action");
    assert_eq!(result["exit_code"], 0, "{result}");
    assert_eq!(agent.0, UiInputStatus::Failed);
    assert!(agent.1.is_some_and(|error| error.len() <= 256));
    assert!(
        probe.recorded_requests().is_empty(),
        "unavailable managed profile must not invoke injection"
    );
}
