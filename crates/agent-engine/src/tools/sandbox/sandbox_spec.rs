use std::path::{Path, PathBuf};

use super::super::reified_namespace::ReifiedMountAccess;

/// Default network posture for commands run inside a sandbox.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPosture {
    #[default]
    Deny,
    Allow,
}

impl NetworkPosture {
    pub(crate) const fn allows_network(self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Projected OS-sandbox confinement input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxSpec {
    pub host_mounts: Vec<SandboxHostMount>,
    pub readable_roots: Vec<PathBuf>,
    pub writable_roots: Vec<PathBuf>,
    pub read_denylist: Vec<PathBuf>,
    pub network: NetworkPosture,
}

/// Native mount input consumed only while a Host adapter constructs a
/// [`Sandbox`](crate::tools::Sandbox).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxHostMount {
    pub namespace_path: PathBuf,
    pub host_path: PathBuf,
    pub access: ReifiedMountAccess,
}

impl SandboxSpec {
    /// Build a single writable mount spec for isolated tests and adapters.
    pub fn seed(root: PathBuf) -> Self {
        let readable_roots = vec![root.clone()];
        let writable_roots = vec![root.clone()];
        let read_denylist = super::super::sandbox_backend::read_denylist_excluding_writable_roots(
            &Self::default_sensitive_read_denylist(),
            &writable_roots,
        );
        Self {
            host_mounts: vec![SandboxHostMount {
                namespace_path: PathBuf::from("/mnt/source"),
                host_path: root,
                access: ReifiedMountAccess::ReadWrite,
            }],
            readable_roots,
            writable_roots,
            read_denylist,
            network: NetworkPosture::Deny,
        }
    }

    /// Derive native write authority from the same Host Mount grants used by the namespace.
    pub fn from_host_mounts(grants: &[SandboxHostMount]) -> Self {
        let host_mounts = grants
            .iter()
            .map(|grant| SandboxHostMount {
                namespace_path: grant.namespace_path.clone(),
                host_path: dunce::canonicalize(&grant.host_path)
                    .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf()),
                access: grant.access,
            })
            .collect::<Vec<_>>();
        let readable_roots = host_mounts
            .iter()
            .map(|grant| grant.host_path.clone())
            .collect::<Vec<_>>();
        let writable_roots = grants
            .iter()
            .filter(|grant| grant.access == ReifiedMountAccess::ReadWrite)
            .map(|grant| {
                dunce::canonicalize(&grant.host_path)
                    .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf())
            })
            .collect::<Vec<_>>();
        let read_denylist = super::super::sandbox_backend::read_denylist_excluding_writable_roots(
            &Self::default_sensitive_read_denylist(),
            &readable_roots,
        );
        Self {
            host_mounts,
            readable_roots,
            writable_roots,
            read_denylist,
            network: NetworkPosture::Deny,
        }
    }

    /// Build the default sensitive-read denylist from the current user's home
    /// directory. If the host home cannot be detected, keep the list empty
    /// rather than guessing.
    pub fn default_sensitive_read_denylist() -> Vec<PathBuf> {
        dirs::home_dir()
            .map(|home| Self::sensitive_read_denylist_for_home(&home))
            .unwrap_or_default()
    }

    /// Derive sensitive read-deny paths from an explicit home directory.
    pub fn sensitive_read_denylist_for_home(home_dir: &Path) -> Vec<PathBuf> {
        let mut paths = [".alan", ".alan-dev"]
            .into_iter()
            .map(|name| home_dir.join(name))
            .collect::<Vec<_>>();

        paths.extend(
            [
                ".ssh",
                ".aws",
                ".azure",
                ".config/gcloud",
                ".config/gh",
                ".docker",
                ".gnupg",
                ".kube",
                ".netrc",
                ".npmrc",
                ".pypirc",
                "Library/Keychains",
                "Library/Safari",
                "Library/Application Support/Arc",
                "Library/Application Support/BraveSoftware",
                "Library/Application Support/Chromium",
                "Library/Application Support/Firefox",
                "Library/Application Support/Google/Chrome",
                "Library/Application Support/com.apple.Safari",
            ]
            .into_iter()
            .map(|relative| home_dir.join(relative)),
        );

        paths
    }

    pub(super) fn exclude_explicit_mounts_from_read_denylist(mut self) -> Self {
        self.read_denylist = super::super::sandbox_backend::read_denylist_excluding_writable_roots(
            &self.read_denylist,
            &self.readable_roots,
        );
        self
    }
}
