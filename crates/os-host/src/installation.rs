//! Product store layout and explicit historical installation inspection.

mod access;
mod extended_metadata;
pub use access::{
    InstallationAccess, InstallationMigrationAccess, MigrationComponent, MigrationJournal,
    MigrationPayload, MigrationState,
};
pub use extended_metadata::ExtendedMetadata;

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

/// Historical input to an explicit adoption operation, never a runtime identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LegacyInstallation {
    Stable,
    Dev,
}

impl LegacyInstallation {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Dev => "dev",
        }
    }
}

impl std::str::FromStr for LegacyInstallation {
    type Err = String;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "stable" => Ok(Self::Stable),
            "dev" => Ok(Self::Dev),
            _ => Err("historical source must be stable or dev".into()),
        }
    }
}

/// Reject retired selectors before any command opens product data; help/version parse first.
pub fn validate_current_invocation() -> Result<()> {
    ensure!(
        std::env::var_os("ALAN_INSTALL_CHANNEL").is_none(),
        "ALAN_INSTALL_CHANNEL is retired; unset it and use alan. Select old data with alan legacy-state migrate-installation --from stable|dev"
    );
    let argv = std::env::args_os().next();
    let name = argv
        .as_deref()
        .and_then(|arg| Path::new(arg).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    ensure!(
        !matches!(
            name.strip_suffix(".exe").unwrap_or(name),
            "alan-dev" | "alan-os-host-dev"
        ),
        "this executable name is retired; invoke alan and explicitly adopt old data with alan legacy-state migrate-installation"
    );
    Ok(())
}

/// Host-owned layout containing the canonical stores and any retained old inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallationPaths {
    pub product: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StorePairInspection {
    pub system_root: PathBuf,
    pub host_root: PathBuf,
    pub system_present: bool,
    pub host_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LegacyInstallationInspection {
    pub source: LegacyInstallation,
    #[serde(flatten)]
    pub stores: StorePairInspection,
}

/// Paths and presence only; inspection never opens credentials or provider data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstallationInspection {
    pub canonical: StorePairInspection,
    pub sources: Vec<LegacyInstallationInspection>,
    pub migration_journal_present: bool,
}

impl InstallationPaths {
    pub fn detect() -> Result<Self> {
        Self::from_data_dir(&dirs::data_dir().context("cannot determine platform data directory")?)
    }

    pub fn from_data_dir(data_dir: &Path) -> Result<Self> {
        ensure!(
            data_dir.is_absolute(),
            "platform data directory must be absolute"
        );
        ensure!(
            !data_dir
                .components()
                .any(|component| { matches!(component, Component::CurDir | Component::ParentDir) }),
            "platform data directory must not contain relative components"
        );
        // Resolve platform aliases such as macOS /var before checking product-owned paths.
        for ancestor in data_dir.ancestors() {
            match dunce::canonicalize(ancestor) {
                Ok(base) => {
                    return Ok(Self {
                        product: base.join(data_dir.strip_prefix(ancestor)?).join("Alan"),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("resolve platform data directory"),
            }
        }
        anyhow::bail!("platform data directory has no existing ancestor")
    }

    pub fn system_root(&self) -> PathBuf {
        self.product.join("System Store")
    }

    pub fn host_root(&self) -> PathBuf {
        self.product.join("Host Store")
    }

    pub fn journal(&self) -> PathBuf {
        self.product.join("installation-migration.json")
    }

    /// Inspect only the canonical pair; recovery must not depend on retained source contents.
    pub fn inspect_canonical(&self) -> Result<StorePairInspection> {
        directory_or_absent(&self.product)?;
        let system = self.system_root();
        let host = self.host_root();
        Ok(StorePairInspection {
            system_present: has_canonical_content(&system)?,
            host_present: has_canonical_content(&host)?,
            system_root: system,
            host_root: host,
        })
    }

    pub fn inspect(&self) -> Result<InstallationInspection> {
        let canonical = self.inspect_canonical()?;
        let system = &canonical.system_root;
        let host = &canonical.host_root;
        let sources = [LegacyInstallation::Stable, LegacyInstallation::Dev]
            .into_iter()
            .map(|source| {
                let system_root = system.join(source.id());
                let host_root = host.join(source.id());
                Ok(LegacyInstallationInspection {
                    source,
                    stores: StorePairInspection {
                        system_present: directory_or_absent(&system_root)?,
                        host_present: directory_or_absent(&host_root)?,
                        system_root,
                        host_root,
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let migration_journal_present = match fs::symlink_metadata(self.journal()) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file() && !metadata.file_type().is_symlink(),
                    "installation migration journal is not a regular file"
                );
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error).context("inspect installation migration journal"),
        };
        Ok(InstallationInspection {
            canonical,
            sources,
            migration_journal_present,
        })
    }
}

fn directory_or_absent(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "store root is not a real directory: {}",
                path.display()
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("inspect store root {}", path.display())),
    }
}

fn has_canonical_content(root: &Path) -> Result<bool> {
    if !directory_or_absent(root)? {
        return Ok(false);
    }
    let mut present = false;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == "stable" || entry.file_name() == "dev" {
            continue;
        }
        let metadata = entry.file_type()?;
        ensure!(
            !metadata.is_symlink(),
            "store entry is a symlink: {}",
            entry.path().display()
        );
        present = true;
    }
    Ok(present)
}

#[cfg(test)]
mod tests;
