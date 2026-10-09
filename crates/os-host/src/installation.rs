//! Product store layout and explicit historical installation inspection.

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
        Ok(Self {
            product: data_dir.join("Alan"),
        })
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

    pub fn inspect(&self) -> Result<InstallationInspection> {
        directory_or_absent(&self.product)?;
        let system = self.system_root();
        let host = self.host_root();
        let canonical = StorePairInspection {
            system_present: has_canonical_content(&system)?,
            host_present: has_canonical_content(&host)?,
            system_root: system.clone(),
            host_root: host.clone(),
        };
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
        let metadata = entry.file_type()?;
        ensure!(
            !metadata.is_symlink(),
            "store entry is a symlink: {}",
            entry.path().display()
        );
        if entry.file_name() != "stable" && entry.file_name() != "dev" {
            present = true;
        }
    }
    Ok(present)
}

#[cfg(test)]
mod tests;
