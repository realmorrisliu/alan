use std::path::{Component, Path, PathBuf};

use alan_agent_engine::{AgentRuntimeStoreBindings, MemoryDomain, resolve_domain_store_dir};
use alan_service_manager::ConnectionStoreBindings;
use anyhow::{Context, Result, ensure};

const PRODUCT_DIR: &str = "Alan";
const SYSTEM_STORE_DIR: &str = "System Store";
const HOST_STORE_DIR: &str = "Host Store";

/// Host-only backing paths for the Alan System Store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemStorePaths {
    pub root: PathBuf,
}

impl SystemStorePaths {
    pub fn detect() -> Result<Self> {
        let data_dir =
            dirs::data_dir().context("cannot determine platform application data directory")?;
        Self::from_data_dir(&data_dir)
    }

    pub fn from_data_dir(data_dir: &Path) -> Result<Self> {
        validate_absolute_path("platform application data directory", data_dir)?;
        Self::from_root(&data_dir.join(PRODUCT_DIR).join(SYSTEM_STORE_DIR))
    }

    /// Build System Store paths rooted directly under an explicit app-owned directory.
    pub fn from_root(root: &Path) -> Result<Self> {
        validate_absolute_path("System Store root", root)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn service(&self, service: &str) -> Result<PathBuf> {
        ensure!(
            !service.is_empty()
                && service
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character == '-'),
            "invalid System Store service id `{service}`"
        );
        Ok(self.root.join("services").join(service))
    }

    pub fn agent_runtime(&self) -> Result<AgentRuntimeStorePaths> {
        let root = self.service("agent-runtime")?;
        Ok(AgentRuntimeStorePaths {
            rollouts: root.join("rollouts"),
            checkpoints: root.join("checkpoints"),
            cache: root.join("cache"),
            tmp: root.join("tmp"),
            metadata: root.join("metadata"),
        })
    }

    pub fn agent_runtime_bindings(&self) -> Result<AgentRuntimeStoreBindings> {
        let runtime = self.agent_runtime()?;
        Ok(AgentRuntimeStoreBindings {
            rollouts: runtime.rollouts,
            checkpoints: runtime.checkpoints,
            cache: runtime.cache,
            tmp: runtime.tmp,
            metadata: runtime.metadata,
        })
    }

    pub fn memory_stores(&self) -> Result<PathBuf> {
        Ok(self.service("memory")?.join("stores"))
    }

    /// Resolve a domain-partitioned Memory Store directory under `services/memory/stores/`.
    pub fn memory_store_for_domain(&self, domain: &MemoryDomain) -> Result<PathBuf> {
        resolve_domain_store_dir(&self.memory_stores()?, domain)
    }

    pub fn agent_definitions(&self) -> Result<PathBuf> {
        Ok(self.service("agent-runtime")?.join("definitions"))
    }

    pub fn packages(&self) -> Result<PathBuf> {
        self.service("packages")
    }

    pub fn host_mount_grants(&self) -> Result<PathBuf> {
        Ok(self.service("host-mount")?.join("grants.json"))
    }

    pub fn projects(&self) -> Result<PathBuf> {
        self.service("projects")
    }

    /// Root directory for Host-private CoW workspace clones (`System Store/workspaces/`).
    pub fn workspaces(&self) -> Result<PathBuf> {
        Ok(self.root.join("workspaces"))
    }

    /// Instance-scoped CoW workspace staging directory (`System Store/workspaces/<instance_id>/`).
    pub fn instance_workspaces(&self, instance_id: &str) -> Result<PathBuf> {
        validate_store_segment("workspace instance id", instance_id)?;
        Ok(self.workspaces()?.join(instance_id))
    }

    /// System Store rollout evidence directory for unapplied winning CoW workspace patches.
    pub fn cow_workspace_evidence(&self) -> Result<PathBuf> {
        Ok(self.agent_runtime()?.rollouts.join("cow-evidence"))
    }

    pub fn connections_metadata(&self) -> Result<PathBuf> {
        Ok(self.service("connections")?.join("connections.toml"))
    }

    pub fn connection_bindings(&self) -> Result<ConnectionStoreBindings> {
        ConnectionStoreBindings::new(self.connections_metadata()?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRuntimeStorePaths {
    pub rollouts: PathBuf,
    pub checkpoints: PathBuf,
    pub cache: PathBuf,
    pub tmp: PathBuf,
    pub metadata: PathBuf,
}

/// Host-owned credential and managed-auth paths. These are not System Store data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostStorePaths {
    pub credentials: PathBuf,
    pub managed_auth: PathBuf,
}

impl HostStorePaths {
    pub fn from_data_dir(data_dir: &Path) -> Result<Self> {
        validate_absolute_path("platform application data directory", data_dir)?;
        Self::from_root(&data_dir.join(PRODUCT_DIR).join(HOST_STORE_DIR))
    }

    /// Build Host Store paths rooted directly under an explicit app-owned directory.
    pub fn from_root(root: &Path) -> Result<Self> {
        validate_absolute_path("Host Store root", root)?;
        Ok(Self {
            credentials: root.join("credentials"),
            managed_auth: root.join("auth.json"),
        })
    }

    pub fn detect() -> Result<Self> {
        let data_dir =
            dirs::data_dir().context("cannot determine platform application data directory")?;
        Self::from_data_dir(&data_dir)
    }
}

fn validate_absolute_path(label: &str, path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "{label} must be absolute: {}",
        path.display()
    );
    ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir)),
        "{label} must not contain relative components: {}",
        path.display()
    );
    Ok(())
}

pub(crate) fn validate_store_segment(label: &str, segment: &str) -> Result<()> {
    ensure!(
        !segment.is_empty()
            && segment.len() <= 128
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "invalid {label} `{segment}`; expected 1..=128 ASCII alphanumeric, `-`, or `_` characters"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_store_paths_have_no_channel_suffix() {
        let data = Path::new("/Users/test/Library/Application Support");
        let system = SystemStorePaths::from_data_dir(data).unwrap();
        let host = HostStorePaths::from_data_dir(data).unwrap();
        assert_eq!(system.root, data.join("Alan/System Store"));
        assert_eq!(host.credentials, data.join("Alan/Host Store/credentials"));
        assert_eq!(host.managed_auth, data.join("Alan/Host Store/auth.json"));
    }

    #[test]
    fn durable_owners_receive_separate_subtrees() {
        let store = SystemStorePaths::from_data_dir(Path::new("/data")).unwrap();
        let runtime = store.agent_runtime().unwrap();

        assert_eq!(
            runtime.rollouts,
            store.root.join("services/agent-runtime/rollouts")
        );
        assert_eq!(
            store.memory_stores().unwrap(),
            store.root.join("services/memory/stores")
        );
        assert_eq!(
            store.connections_metadata().unwrap(),
            store.root.join("services/connections/connections.toml")
        );
        assert_eq!(
            store.packages().unwrap(),
            store.root.join("services/packages")
        );
        assert_eq!(
            store.projects().unwrap(),
            store.root.join("services/projects")
        );
    }

    #[test]
    fn host_credentials_are_outside_system_store() {
        let data = Path::new("/data");
        let system = SystemStorePaths::from_data_dir(data).unwrap();
        let host = HostStorePaths::from_data_dir(data).unwrap();

        assert!(!host.credentials.starts_with(&system.root));
        assert!(!host.managed_auth.starts_with(&system.root));
    }

    #[test]
    fn from_root_uses_explicit_app_directory_without_alan_prefix() {
        let app_root = Path::new("/Users/test/AppData/MyApp");
        let system = SystemStorePaths::from_root(&app_root.join("system")).unwrap();
        let host = HostStorePaths::from_root(&app_root.join("host")).unwrap();

        assert_eq!(system.root, app_root.join("system"));
        assert_eq!(host.credentials, app_root.join("host/credentials"));
        assert_eq!(host.managed_auth, app_root.join("host/auth.json"));
    }

    #[test]
    fn memory_store_for_domain_partitions_stores_cleanly() {
        let temp = tempfile::TempDir::new().unwrap();
        let store = SystemStorePaths::from_root(&temp.path().join("System Store")).unwrap();
        assert_eq!(
            store
                .memory_store_for_domain(&MemoryDomain::Personal)
                .unwrap(),
            store.memory_stores().unwrap().join("personal")
        );
        assert_eq!(
            store
                .memory_store_for_domain(&MemoryDomain::Continuity)
                .unwrap(),
            store.memory_stores().unwrap().join("continuity")
        );
        assert_eq!(
            store
                .memory_store_for_domain(&MemoryDomain::App("slowpage".to_string()))
                .unwrap(),
            store.memory_stores().unwrap().join("app/slowpage")
        );
        assert_eq!(
            store
                .memory_store_for_domain(&MemoryDomain::Project("alan".to_string()))
                .unwrap(),
            store.memory_stores().unwrap().join("project/alan")
        );
    }
}
