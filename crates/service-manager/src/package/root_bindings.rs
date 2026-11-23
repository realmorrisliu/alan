//! Durable System Store Root package bindings separating knowledge attachment (`SkillsOnly`)
//! from explicit runtime activation (`Active`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use super::fs_safety::{ensure_owned_directory, ensure_owned_file};
use super::{
    PackageCatalog, PackageExportKind, PackageKind, PackageRecord, PackageService, PackageState,
    validate_package_id,
};

const ROOT_BINDINGS_FILE: &str = "root-bindings.json";
const ROOT_BINDINGS_SCHEMA_VERSION: u16 = 1;
const MAX_ROOT_BINDINGS_BYTES: u64 = 1024 * 1024;

/// Root projection level for an installed Quartermaster package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootBindingMode {
    /// Mount `/lib/pkg/<id>` read-only and project Skills / Companion Skills without launching runtimes or `/bin` tools.
    SkillsOnly,
    /// Project Skills and enable runtime capabilities (packaged Tools, MCP servers, or confined FileServers).
    Active,
}

impl RootBindingMode {
    /// Stable lowercase wire label used in CLI output and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SkillsOnly => "skills_only",
            Self::Active => "active",
        }
    }
}

/// Mount access mode granted when activating a `fileserver` package.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileserverMountAccess {
    #[default]
    ReadOnly,
    ReadWrite,
}

impl FileserverMountAccess {
    /// Stable lowercase label used in CLI flags and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }

    /// Convert to Kernel mount access policy.
    pub const fn into_kernel_access(self) -> alan_kernel::Access {
        match self {
            Self::ReadOnly => alan_kernel::Access::ReadOnly,
            Self::ReadWrite => alan_kernel::Access::ReadWrite,
        }
    }
}

/// Explicit runtime activation parameters for a bound `fileserver` package.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootFileserverActivation {
    #[serde(default)]
    pub access: FileserverMountAccess,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_grant_ids: Vec<String>,
}

/// Durable binding record authorizing Root exposure for one installed Quartermaster package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootPackageBinding {
    pub package_id: String,
    pub mode: RootBindingMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fileserver: Option<RootFileserverActivation>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootBindingsFile {
    schema_version: u16,
    #[serde(default)]
    bindings: BTreeMap<String, RootPackageBinding>,
}

impl Default for RootBindingsFile {
    fn default() -> Self {
        Self {
            schema_version: ROOT_BINDINGS_SCHEMA_VERSION,
            bindings: BTreeMap::new(),
        }
    }
}

impl PackageService {
    /// Reads durable Root package bindings from `store_root/root-bindings.json` without modifying the store.
    pub fn load_root_bindings_from_store(store_root: &Path) -> Result<Vec<RootPackageBinding>> {
        if !store_root.exists() {
            return Ok(Vec::new());
        }
        ensure_owned_directory(store_root, "Package Store root is not an owned directory")?;
        let file = read_root_bindings_file(store_root)?;
        Ok(file.bindings.into_values().collect())
    }

    /// Lists all Root package bindings currently saved for this Package Service.
    pub fn list_root_bindings(&self) -> Result<Vec<RootPackageBinding>> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        let bindings = self.load_bindings_under_lock()?;
        Ok(bindings.into_values().collect())
    }

    /// Returns the active Root binding for `package_id`, if present.
    pub fn root_binding(&self, package_id: &str) -> Result<Option<RootPackageBinding>> {
        validate_package_id(package_id)?;
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        let bindings = self.load_bindings_under_lock()?;
        Ok(bindings.get(package_id).cloned())
    }

    /// Saves or updates a Root binding for an installed package after validating grants and Skill uniqueness.
    pub fn bind_for_root(
        &self,
        package_id: &str,
        mode: RootBindingMode,
        fileserver: Option<RootFileserverActivation>,
    ) -> Result<RootPackageBinding> {
        validate_package_id(package_id)?;
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        let catalog = self.cached_catalog();
        let record = catalog
            .packages
            .get(package_id)
            .filter(|record| record.state == PackageState::Installed)
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        validate_binding_options(record, mode, fileserver.as_ref())?;

        let mut bindings = self.load_bindings_under_lock()?;
        ensure_no_skill_collision(&catalog, &bindings, record)?;

        let binding = RootPackageBinding {
            package_id: package_id.to_string(),
            mode,
            fileserver,
            updated_at: chrono::Utc::now(),
        };
        bindings.insert(package_id.to_string(), binding.clone());
        self.save_bindings_under_lock(&bindings)?;
        Ok(binding)
    }

    /// Removes any Root binding for `package_id`, returning whether a binding existed.
    pub fn unbind_for_root(&self, package_id: &str) -> Result<bool> {
        validate_package_id(package_id)?;
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        self.unbind_after_uninstall_locked(package_id)
    }

    pub(super) fn unbind_after_uninstall_locked(&self, package_id: &str) -> Result<bool> {
        let mut bindings = self.load_bindings_under_lock()?;
        let existed = bindings.remove(package_id).is_some();
        if existed {
            self.save_bindings_under_lock(&bindings)?;
        }
        Ok(existed)
    }

    fn load_bindings_under_lock(&self) -> Result<BTreeMap<String, RootPackageBinding>> {
        if let Some(root) = self.store.disk_root() {
            let file = read_root_bindings_file(root)?;
            self.state
                .lock()
                .expect("package state poisoned")
                .root_bindings = file.bindings.clone();
            Ok(file.bindings)
        } else {
            Ok(self
                .state
                .lock()
                .expect("package state poisoned")
                .root_bindings
                .clone())
        }
    }

    fn save_bindings_under_lock(
        &self,
        bindings: &BTreeMap<String, RootPackageBinding>,
    ) -> Result<()> {
        if let Some(root) = self.store.disk_root() {
            let file = RootBindingsFile {
                schema_version: ROOT_BINDINGS_SCHEMA_VERSION,
                bindings: bindings.clone(),
            };
            persist_root_bindings_file(root, &file)?;
        }
        self.state
            .lock()
            .expect("package state poisoned")
            .root_bindings = bindings.clone();
        Ok(())
    }
}

fn validate_binding_options(
    record: &PackageRecord,
    mode: RootBindingMode,
    fileserver: Option<&RootFileserverActivation>,
) -> Result<()> {
    match (mode, record.export_kind) {
        (RootBindingMode::SkillsOnly, _) => {
            ensure!(
                fileserver.is_none(),
                "skills_only binding must not specify fileserver activation options"
            );
        }
        (RootBindingMode::Active, PackageExportKind::Fileserver) => {
            let activation = fileserver.with_context(|| {
                format!(
                    "active fileserver binding for `{}` requires mount access configuration",
                    record.id
                )
            })?;
            let export = record.fileserver_export.as_ref().with_context(|| {
                format!(
                    "package `{}` lacks a fileserver export descriptor",
                    record.id
                )
            })?;
            ensure!(
                !activation.allow_network || export.allow_network,
                "fileserver package `{}` does not declare `allow_network = true`",
                record.id
            );
            for grant_id in &activation.host_grant_ids {
                ensure!(
                    !grant_id.is_empty()
                        && grant_id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
                    "invalid host_grant_id `{grant_id}`"
                );
            }
        }
        (RootBindingMode::Active, _) => {
            ensure!(
                fileserver.is_none(),
                "fileserver activation options are only valid for `fileserver` packages"
            );
        }
    }
    Ok(())
}

fn ensure_no_skill_collision(
    catalog: &PackageCatalog,
    bindings: &BTreeMap<String, RootPackageBinding>,
    candidate: &PackageRecord,
) -> Result<()> {
    let candidate_skills: BTreeSet<&str> = candidate
        .exports
        .iter()
        .map(|export| export.skill_id.as_str())
        .collect();
    if candidate_skills.is_empty() {
        return Ok(());
    }

    for record in catalog.packages.values() {
        if record.id == candidate.id || record.state != PackageState::Installed {
            continue;
        }
        let is_active_in_root =
            record.kind == PackageKind::Preinstalled || bindings.contains_key(&record.id);
        if !is_active_in_root {
            continue;
        }
        for export in &record.exports {
            if candidate_skills.contains(export.skill_id.as_str()) {
                bail!(
                    "package `{}` exports skill `{}` which collides with already bound package `{}`",
                    candidate.id,
                    export.skill_id,
                    record.id
                );
            }
        }
    }
    Ok(())
}

fn bindings_path(store_root: &Path) -> PathBuf {
    store_root.join(ROOT_BINDINGS_FILE)
}

fn read_root_bindings_file(store_root: &Path) -> Result<RootBindingsFile> {
    let path = bindings_path(store_root);
    match fs::symlink_metadata(&path) {
        Ok(meta) => {
            ensure!(
                meta.file_type().is_file() && !meta.file_type().is_symlink(),
                "root bindings path is not an owned regular file"
            );
            ensure!(
                meta.len() <= MAX_ROOT_BINDINGS_BYTES,
                "root bindings file exceeds maximum size"
            );
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RootBindingsFile::default());
        }
        Err(err) => return Err(err).context("inspect root bindings file"),
    }
    ensure_owned_file(&path, "root bindings file is not owned cleanly")?;
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let parsed: RootBindingsFile =
        serde_json::from_slice(&bytes).context("parse root-bindings.json")?;
    ensure!(
        parsed.schema_version == ROOT_BINDINGS_SCHEMA_VERSION,
        "unsupported root-bindings schema_version {}",
        parsed.schema_version
    );
    for (key, binding) in &parsed.bindings {
        validate_package_id(key)?;
        ensure!(
            key == &binding.package_id,
            "mismatched package_id key `{key}` in root-bindings.json"
        );
    }
    Ok(parsed)
}

fn persist_root_bindings_file(store_root: &Path, file: &RootBindingsFile) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(file)?;
    let temp = store_root.join(format!(
        "{ROOT_BINDINGS_FILE}.tmp.{}",
        uuid::Uuid::new_v4().simple()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut handle = options
        .open(&temp)
        .with_context(|| format!("open temporary root bindings file {}", temp.display()))?;
    handle.write_all(&bytes)?;
    handle.sync_all()?;
    drop(handle);

    let dest = bindings_path(store_root);
    fs::rename(&temp, &dest)
        .with_context(|| format!("replace root bindings file {}", dest.display()))?;
    File::open(store_root)?.sync_all()?;
    Ok(())
}
