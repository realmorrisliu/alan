use std::path::{Component, Path, PathBuf};

use alan_agent_engine::{AgentRuntimeStoreBindings, ConnectionStoreBindings};
use anyhow::{Context, Result, ensure};

const PRODUCT_DIR: &str = "Alan";
const SYSTEM_STORE_DIR: &str = "System Store";
const HOST_STORE_DIR: &str = "Host Store";

/// Host-only backing paths for one install channel's Alan OS System Store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemStorePaths {
    pub channel_id: String,
    pub root: PathBuf,
}

impl SystemStorePaths {
    pub fn detect(channel_id: &str) -> Result<Self> {
        let data_dir =
            dirs::data_dir().context("cannot determine platform application data directory")?;
        Self::from_data_dir(&data_dir, channel_id)
    }

    pub fn from_data_dir(data_dir: &Path, channel_id: &str) -> Result<Self> {
        validate_absolute_path("platform application data directory", data_dir)?;
        validate_channel_id(channel_id)?;
        Ok(Self {
            channel_id: channel_id.to_string(),
            root: data_dir
                .join(PRODUCT_DIR)
                .join(SYSTEM_STORE_DIR)
                .join(channel_id),
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

    pub fn agent_definitions(&self) -> Result<PathBuf> {
        Ok(self.service("agent-runtime")?.join("definitions"))
    }

    pub fn packages(&self) -> Result<PathBuf> {
        self.service("packages")
    }

    pub fn connections_metadata(&self) -> Result<PathBuf> {
        Ok(self.service("connections")?.join("connections.toml"))
    }

    pub fn connection_bindings(&self, host: &HostStorePaths) -> Result<ConnectionStoreBindings> {
        ConnectionStoreBindings::new(self.connections_metadata()?, host.credentials.clone())
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
    pub fn from_data_dir(data_dir: &Path, channel_id: &str) -> Result<Self> {
        validate_absolute_path("platform application data directory", data_dir)?;
        validate_channel_id(channel_id)?;
        let root = data_dir
            .join(PRODUCT_DIR)
            .join(HOST_STORE_DIR)
            .join(channel_id);
        Ok(Self {
            credentials: root.join("credentials"),
            managed_auth: root.join("auth.json"),
        })
    }

    pub fn detect(channel_id: &str) -> Result<Self> {
        let data_dir =
            dirs::data_dir().context("cannot determine platform application data directory")?;
        Self::from_data_dir(&data_dir, channel_id)
    }
}

fn validate_channel_id(channel_id: &str) -> Result<()> {
    ensure!(
        matches!(channel_id, "stable" | "dev"),
        "invalid Alan install channel `{channel_id}`"
    );
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_dev_system_stores_are_isolated() {
        let data = Path::new("/Users/test/Library/Application Support");
        let stable = SystemStorePaths::from_data_dir(data, "stable").unwrap();
        let dev = SystemStorePaths::from_data_dir(data, "dev").unwrap();

        assert_eq!(stable.root, data.join("Alan/System Store/stable"));
        assert_eq!(dev.root, data.join("Alan/System Store/dev"));
        assert_ne!(stable.root, dev.root);
    }

    #[test]
    fn durable_owners_receive_separate_subtrees() {
        let store = SystemStorePaths::from_data_dir(Path::new("/data"), "stable").unwrap();
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
    }

    #[test]
    fn host_credentials_are_outside_system_store() {
        let data = Path::new("/data");
        let system = SystemStorePaths::from_data_dir(data, "dev").unwrap();
        let host = HostStorePaths::from_data_dir(data, "dev").unwrap();

        assert!(!host.credentials.starts_with(&system.root));
        assert!(!host.managed_auth.starts_with(&system.root));
    }
}
