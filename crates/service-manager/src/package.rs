//! Package Service (Quartermaster): installed package ownership and lifecycle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use alan_agent_engine::skills::{
    SkillCompatibility, SkillScope, SkillTypedDependency, name_to_id, parse_skill_metadata,
    validate_skill_compatibility,
};
use alan_ap::{ErrorCode, FileServer};
use alan_hostfs::{HostDirAccess, HostDirFs};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::flat_fs::{FlatFileService, FlatServiceFs};

const FILES: &[(&str, bool)] = &[
    ("catalog", false),
    ("status", false),
    ("ctl", true),
    ("result", false),
];
const MATERIALIZER_VERSION: &str = "alan-skill-v1";
const MAX_COMMAND_BYTES: usize = 64 * 1024 * 1024;
const MAX_SOURCE_FILES: usize = 4_096;
const MAX_SOURCE_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 12 * 1024 * 1024;
const MAX_PACKAGES: usize = 4_096;
const MAX_RESULTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Preinstalled,
    Installed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageState {
    Installed,
    Retiring,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageExport {
    pub skill_id: String,
    pub root: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<SkillTypedDependency>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRecord {
    pub id: String,
    pub revision: String,
    pub kind: PackageKind,
    pub state: PackageState,
    pub materializer_version: String,
    pub materialized_fingerprint: String,
    pub exports: Vec<PackageExport>,
    pub reference_count: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCatalog {
    pub generation: u64,
    pub packages: BTreeMap<String, PackageRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSnapshotEntry {
    pub path: String,
    pub bytes: Vec<u8>,
    #[serde(default)]
    pub executable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSnapshot {
    /// Source leaf used only for portable root Skill identity derivation.
    pub source_name: String,
    pub entries: Vec<PackageSnapshotEntry>,
}

impl PackageSnapshot {
    /// Snapshot a trusted embedded/preinstalled package tree.
    pub fn from_directory(root: &Path) -> Result<Self> {
        ensure!(root.is_dir(), "package source is not a directory");
        let source_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .context("package source leaf is not UTF-8")?
            .to_string();
        Self::from_directory_named(root, source_name)
    }

    fn from_directory_named(root: &Path, source_name: String) -> Result<Self> {
        ensure_owned_directory(root, "package source is not an owned directory")?;
        let canonical_root = fs::canonicalize(root)?;
        let mut entries = Vec::new();
        let mut total_bytes = 0usize;
        let mut pending = vec![canonical_root.clone()];
        while let Some(directory) = pending.pop() {
            let mut children = fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                let path = child.path();
                let metadata = fs::symlink_metadata(&path)?;
                ensure!(
                    !metadata.file_type().is_symlink(),
                    "package source contains a symlink"
                );
                if metadata.is_dir() {
                    if child.file_name() != ".git" {
                        pending.push(path);
                    }
                    continue;
                }
                ensure!(metadata.is_file(), "package source contains a special file");
                let relative = path.strip_prefix(&canonical_root)?.to_path_buf();
                if has_vcs_component(&relative) {
                    continue;
                }
                let file_len = usize::try_from(metadata.len())
                    .context("package source file size does not fit in memory")?;
                ensure!(
                    file_len <= MAX_SOURCE_FILE_BYTES,
                    "package source file is too large"
                );
                ensure!(
                    file_len <= MAX_SOURCE_BYTES.saturating_sub(total_bytes),
                    "package source is too large"
                );
                let mut bytes = Vec::with_capacity(file_len);
                File::open(&path)?
                    .take(file_len as u64 + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() == file_len,
                    "package source changed while it was being snapshotted"
                );
                total_bytes += bytes.len();
                entries.push(PackageSnapshotEntry {
                    path: slash_path(&relative)?,
                    bytes,
                    executable: executable_bit(&metadata),
                });
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let snapshot = Self {
            source_name,
            entries,
        };
        validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageCommand {
    Install {
        request_id: String,
        package_id: String,
        snapshot: PackageSnapshot,
    },
    List {
        request_id: String,
    },
    Upgrade {
        request_id: String,
        package_id: String,
        snapshot: PackageSnapshot,
    },
    Uninstall {
        request_id: String,
        package_id: String,
    },
}

impl PackageCommand {
    fn request_id(&self) -> &str {
        match self {
            Self::Install { request_id, .. }
            | Self::List { request_id }
            | Self::Upgrade { request_id, .. }
            | Self::Uninstall { request_id, .. } => request_id,
        }
    }

    fn action(&self) -> &'static str {
        match self {
            Self::Install { .. } => "install",
            Self::List { .. } => "list",
            Self::Upgrade { .. } => "upgrade",
            Self::Uninstall { .. } => "uninstall",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCommandResult {
    pub request_id: String,
    pub success: bool,
    pub action: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<PackageCatalog>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterializationManifest {
    exports: Vec<PackageExport>,
    files: Vec<MaterializedFileRecord>,
    tree_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterializedFileRecord {
    path: String,
    generated: bool,
}

struct State {
    catalog: PackageCatalog,
    results: BTreeMap<String, PackageCommandResult>,
    result_order: Vec<String>,
    status: String,
    leases: BTreeMap<u64, (String, String)>,
    next_lease_id: u64,
}

/// Channel-scoped installed-package authority.
pub struct PackageService {
    channel_id: String,
    store_root: PathBuf,
    _store_lock: PackageStoreLock,
    _temporary_store: Option<tempfile::TempDir>,
    state: Mutex<State>,
    operation: Mutex<()>,
}

struct PackageStoreLock {
    file: File,
}

impl PackageStoreLock {
    fn acquire(root: &Path) -> Result<Self> {
        let path = root.join("store.lock");
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options
            .open(&path)
            .with_context(|| format!("open Package Store lock {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: file owns a valid descriptor for the lifetime of the lock.
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error
                    .raw_os_error()
                    .is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
                {
                    bail!("Package Store is already owned by another Package Service")
                }
                return Err(error).context("acquire Package Store lock");
            }
        }
        Ok(Self { file })
    }
}

impl Drop for PackageStoreLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: file remains valid until this Drop implementation returns.
            unsafe {
                libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
            }
        }
    }
}

/// An immutable Package Service revision reference retained by a Process context.
pub struct PackageReferenceLease {
    service: Weak<PackageService>,
    token: u64,
    record: PackageRecord,
    content_root: PathBuf,
}

impl std::fmt::Debug for PackageReferenceLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PackageReferenceLease")
            .field("token", &self.token)
            .field("record", &self.record)
            .finish_non_exhaustive()
    }
}

impl PackageReferenceLease {
    pub fn record(&self) -> &PackageRecord {
        &self.record
    }

    pub fn file_server(&self) -> Result<Arc<dyn FileServer>> {
        Ok(Arc::new(HostDirFs::new(
            &self.content_root,
            HostDirAccess::ReadOnly,
        )?))
    }

    #[cfg(test)]
    pub(crate) fn content_root(&self) -> &Path {
        &self.content_root
    }

    pub(crate) fn skill_descriptor(
        &self,
        relative_root: &str,
    ) -> Result<alan_agent_engine::ProcessFileTree> {
        let root = self.content_root.join(relative_root);
        let canonical_content = fs::canonicalize(&self.content_root)?;
        let canonical_root = fs::canonicalize(&root)?;
        ensure!(
            canonical_root.starts_with(&canonical_content),
            "package Skill descriptor escapes its immutable revision"
        );
        let source_leaf = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .context("package Skill descriptor root has no UTF-8 leaf")?;
        let snapshot =
            PackageSnapshot::from_directory_named(&canonical_root, source_leaf.to_string())?;
        alan_agent_engine::ProcessFileTree::new(
            snapshot
                .entries
                .into_iter()
                .map(|entry| (entry.path, entry.bytes))
                .collect(),
        )
    }
}

impl Drop for PackageReferenceLease {
    fn drop(&mut self) {
        if let Some(service) = self.service.upgrade()
            && let Err(error) = service.release(self.token)
        {
            tracing::error!(token = self.token, %error, "failed to release package reference");
        }
    }
}

impl std::fmt::Debug for PackageService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PackageService")
            .field("channel_id", &self.channel_id)
            .finish_non_exhaustive()
    }
}

impl PackageService {
    pub fn open(channel_id: impl Into<String>, store_root: PathBuf) -> Result<Arc<Self>> {
        Self::open_inner(channel_id.into(), store_root, None)
    }

    pub fn ephemeral(channel_id: impl Into<String>) -> Result<Arc<Self>> {
        let temporary = tempfile::Builder::new()
            .prefix("alan-package-service-")
            .tempdir()?;
        let root = temporary.path().to_path_buf();
        Self::open_inner(channel_id.into(), root, Some(temporary))
    }

    fn open_inner(
        channel_id: String,
        store_root: PathBuf,
        temporary_store: Option<tempfile::TempDir>,
    ) -> Result<Arc<Self>> {
        ensure!(
            matches!(channel_id.as_str(), "stable" | "dev" | "test"),
            "invalid Package Service channel"
        );
        ensure_package_store_channel_chain(&store_root)?;
        match fs::symlink_metadata(&store_root) {
            Ok(_) => {
                ensure_owned_directory(&store_root, "Package Store root is not an owned directory")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(&store_root)?;
            }
            Err(error) => return Err(error).context("inspect Package Store root"),
        }
        ensure_package_store_channel_chain(&store_root)?;
        fs::create_dir_all(store_root.join("revisions"))?;
        fs::create_dir_all(store_root.join("staging"))?;
        ensure_owned_directory(
            &store_root.join("revisions"),
            "package revisions store is not an owned directory",
        )?;
        ensure_owned_directory(
            &store_root.join("staging"),
            "package staging path is not an owned directory",
        )?;
        let store_lock = PackageStoreLock::acquire(&store_root)?;
        let mut catalog = load_catalog(&store_root)?;
        validate_catalog_structure(&catalog)?;
        recover_staging(&store_root, &catalog)?;
        let mut recovered = false;
        let retiring = catalog
            .packages
            .values()
            .filter(|record| record.state == PackageState::Retiring)
            .map(|record| record.id.clone())
            .collect::<Vec<_>>();
        for package_id in retiring {
            catalog.packages.remove(&package_id);
            remove_package_revisions(&store_root, &package_id)?;
            recovered = true;
        }
        for record in catalog.packages.values_mut() {
            if record.reference_count != 0 {
                record.reference_count = 0;
                recovered = true;
            }
        }
        verify_catalog(&store_root, &catalog)?;
        gc_unreferenced_store_revisions(&store_root, &catalog, &BTreeMap::new())?;
        if recovered {
            catalog.generation = catalog.generation.saturating_add(1);
            persist_catalog(&store_root, &catalog)?;
        }
        Ok(Arc::new(Self {
            channel_id,
            store_root,
            _store_lock: store_lock,
            _temporary_store: temporary_store,
            state: Mutex::new(State {
                catalog,
                results: BTreeMap::new(),
                result_order: Vec::new(),
                status: "ready".to_string(),
                leases: BTreeMap::new(),
                next_lease_id: 1,
            }),
            operation: Mutex::new(()),
        }))
    }

    pub fn file_server(self: &Arc<Self>) -> Arc<dyn FileServer> {
        Arc::new(FlatServiceFs::new(self.clone()))
    }

    pub fn catalog(&self) -> PackageCatalog {
        self.state
            .lock()
            .expect("package state poisoned")
            .catalog
            .clone()
    }

    pub(crate) fn seed_preinstalled(
        &self,
        package_id: &str,
        snapshot: PackageSnapshot,
    ) -> Result<()> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        validate_package_id(package_id)?;
        validate_snapshot(&snapshot)?;
        let revision = fingerprint(&snapshot)?;
        let existing = self
            .state
            .lock()
            .expect("package state poisoned")
            .catalog
            .packages
            .get(package_id)
            .cloned();
        match existing.as_ref() {
            Some(record) if record.kind != PackageKind::Preinstalled => {
                bail!("package id `{package_id}` is occupied by an operator package")
            }
            Some(record) if record.revision == revision => return Ok(()),
            _ => {}
        }
        if existing.is_none() {
            ensure!(
                self.catalog().packages.len() < MAX_PACKAGES,
                "package catalog is full"
            );
        }
        let materialization = self.materialize(package_id, &revision, &snapshot)?;
        let reference_count = existing.as_ref().map_or(0, |record| record.reference_count);
        let now = chrono::Utc::now();
        self.publish_record(PackageRecord {
            id: package_id.to_string(),
            revision,
            kind: PackageKind::Preinstalled,
            state: PackageState::Installed,
            materializer_version: MATERIALIZER_VERSION.to_string(),
            materialized_fingerprint: materialization.tree_fingerprint,
            exports: materialization.exports,
            reference_count,
            created_at: existing.as_ref().map_or(now, |record| record.created_at),
            updated_at: now,
        })?;
        self.gc_package_revisions_after_commit(package_id, "seed preinstalled package");
        Ok(())
    }

    pub fn resolve(&self, package_id: &str) -> Result<PackageRecord> {
        let state = self.state.lock().expect("package state poisoned");
        let record = state
            .catalog
            .packages
            .get(package_id)
            .filter(|record| record.state == PackageState::Installed)
            .cloned()
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        verify_revision(&self.store_root, &record)?;
        Ok(record)
    }

    pub fn acquire(self: &Arc<Self>, package_id: &str) -> Result<Arc<PackageReferenceLease>> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let mut state = self.state.lock().expect("package state poisoned");
        let mut record = state
            .catalog
            .packages
            .get(package_id)
            .filter(|record| record.state == PackageState::Installed)
            .cloned()
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        verify_revision(&self.store_root, &record)?;
        let token = state.next_lease_id;
        record.reference_count = record
            .reference_count
            .checked_add(1)
            .context("package reference count overflow")?;
        let mut next = state.catalog.clone();
        next.generation = next.generation.saturating_add(1);
        next.packages.insert(package_id.to_string(), record.clone());
        persist_catalog(&self.store_root, &next)?;
        state.catalog = next;
        state.next_lease_id = state.next_lease_id.saturating_add(1);
        state
            .leases
            .insert(token, (package_id.to_string(), record.revision.clone()));
        Ok(Arc::new(PackageReferenceLease {
            service: Arc::downgrade(self),
            token,
            content_root: revision_root(&self.store_root, package_id, &record.revision)
                .join("content"),
            record,
        }))
    }

    pub(crate) fn execute(&self, command: PackageCommand) -> Result<PackageCommandResult> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        validate_command(&command)?;
        let request_id = command.request_id().to_string();
        {
            let state = self.state.lock().expect("package state poisoned");
            ensure!(
                !state.results.contains_key(&request_id),
                "duplicate package request id"
            );
        }

        let action = command.action().to_string();
        let attempted = match command {
            PackageCommand::Install {
                request_id,
                package_id,
                snapshot,
            } => self.install(request_id, package_id, snapshot),
            PackageCommand::List { request_id } => Ok(PackageCommandResult {
                request_id,
                success: true,
                action: "list".to_string(),
                message: "package catalog".to_string(),
                package: None,
                catalog: Some(self.catalog()),
            }),
            PackageCommand::Upgrade {
                request_id,
                package_id,
                snapshot,
            } => self.upgrade(request_id, package_id, snapshot),
            PackageCommand::Uninstall {
                request_id,
                package_id,
            } => self.uninstall(request_id, package_id),
        };
        let result = attempted.unwrap_or_else(|error| PackageCommandResult {
            request_id: request_id.clone(),
            success: false,
            action,
            message: bounded_error_message(&error),
            package: None,
            catalog: None,
        });
        self.record_result(result.clone());
        Ok(result)
    }

    fn record_result(&self, result: PackageCommandResult) {
        let request_id = result.request_id.clone();
        let mut state = self.state.lock().expect("package state poisoned");
        state.status = format!("{}:{}", result.action, result.message);
        state.result_order.push(request_id.clone());
        state.results.insert(request_id, result);
        while state.result_order.len() > MAX_RESULTS {
            let oldest = state.result_order.remove(0);
            state.results.remove(&oldest);
        }
    }

    fn install(
        &self,
        request_id: String,
        package_id: String,
        snapshot: PackageSnapshot,
    ) -> Result<PackageCommandResult> {
        let revision = fingerprint(&snapshot)?;
        if let Some(existing) = self
            .state
            .lock()
            .expect("package state poisoned")
            .catalog
            .packages
            .get(&package_id)
            .cloned()
        {
            if existing.revision == revision
                && existing.kind == PackageKind::Installed
                && existing.state == PackageState::Installed
            {
                return Ok(PackageCommandResult {
                    request_id,
                    success: true,
                    action: "install".to_string(),
                    message: "already installed".to_string(),
                    package: Some(existing),
                    catalog: None,
                });
            }
            bail!("package id `{package_id}` is already occupied; choose another explicit --name");
        }
        ensure!(
            self.catalog().packages.len() < MAX_PACKAGES,
            "package catalog is full"
        );
        let materialization = self.materialize(&package_id, &revision, &snapshot)?;
        let now = chrono::Utc::now();
        let record = PackageRecord {
            id: package_id.clone(),
            revision,
            kind: PackageKind::Installed,
            state: PackageState::Installed,
            materializer_version: MATERIALIZER_VERSION.to_string(),
            materialized_fingerprint: materialization.tree_fingerprint,
            exports: materialization.exports,
            reference_count: 0,
            created_at: now,
            updated_at: now,
        };
        self.publish_record(record.clone())?;
        self.gc_package_revisions_after_commit(&package_id, "install package");
        Ok(PackageCommandResult {
            request_id,
            success: true,
            action: "install".to_string(),
            message: "installed".to_string(),
            package: Some(record),
            catalog: None,
        })
    }

    fn upgrade(
        &self,
        request_id: String,
        package_id: String,
        snapshot: PackageSnapshot,
    ) -> Result<PackageCommandResult> {
        let existing = self
            .state
            .lock()
            .expect("package state poisoned")
            .catalog
            .packages
            .get(&package_id)
            .cloned()
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        ensure!(
            existing.kind == PackageKind::Installed,
            "preinstalled package cannot be upgraded by an operator"
        );
        ensure!(
            existing.state == PackageState::Installed,
            "retiring package cannot be upgraded"
        );
        let revision = fingerprint(&snapshot)?;
        if revision == existing.revision {
            return Ok(PackageCommandResult {
                request_id,
                success: true,
                action: "upgrade".to_string(),
                message: "already current".to_string(),
                package: Some(existing),
                catalog: None,
            });
        }
        let materialization = self.materialize(&package_id, &revision, &snapshot)?;
        let record = PackageRecord {
            revision,
            materialized_fingerprint: materialization.tree_fingerprint,
            exports: materialization.exports,
            updated_at: chrono::Utc::now(),
            ..existing
        };
        self.publish_record(record.clone())?;
        self.gc_package_revisions_after_commit(&package_id, "upgrade package");
        Ok(PackageCommandResult {
            request_id,
            success: true,
            action: "upgrade".to_string(),
            message: "upgraded".to_string(),
            package: Some(record),
            catalog: None,
        })
    }

    fn uninstall(&self, request_id: String, package_id: String) -> Result<PackageCommandResult> {
        let mut next = self.catalog();
        let existing = next
            .packages
            .get(&package_id)
            .cloned()
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        ensure!(
            existing.kind == PackageKind::Installed,
            "preinstalled package cannot be uninstalled"
        );
        let retained = existing.reference_count > 0;
        let retained_record = if retained {
            let mut record = existing;
            record.state = PackageState::Retiring;
            next.packages.insert(package_id.clone(), record.clone());
            Some(record)
        } else {
            next.packages.remove(&package_id);
            None
        };
        next.generation = next.generation.saturating_add(1);
        let staged_revisions = if retained {
            None
        } else {
            stage_package_revisions(&self.store_root, &package_id)?
        };
        if let Err(error) = persist_catalog(&self.store_root, &next) {
            rollback_staged_package_revisions(staged_revisions.as_ref(), error)?;
        }
        self.state.lock().expect("package state poisoned").catalog = next;
        discard_staged_package_revisions(staged_revisions);
        Ok(PackageCommandResult {
            request_id,
            success: true,
            action: "uninstall".to_string(),
            message: if retained {
                "retiring".to_string()
            } else {
                "uninstalled".to_string()
            },
            package: retained_record,
            catalog: None,
        })
    }

    fn release(&self, token: u64) -> Result<()> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let mut state = self.state.lock().expect("package state poisoned");
        let Some((package_id, _revision)) = state.leases.get(&token).cloned() else {
            return Ok(());
        };
        let Some(mut record) = state.catalog.packages.get(&package_id).cloned() else {
            return Ok(());
        };
        record.reference_count = record.reference_count.saturating_sub(1);
        let mut next = state.catalog.clone();
        next.generation = next.generation.saturating_add(1);
        let remove_package = record.state == PackageState::Retiring && record.reference_count == 0;
        if remove_package {
            next.packages.remove(&package_id);
        } else {
            next.packages.insert(package_id.clone(), record);
        }
        let staged_revisions = if remove_package {
            stage_package_revisions(&self.store_root, &package_id)?
        } else {
            None
        };
        if let Err(error) = persist_catalog(&self.store_root, &next) {
            rollback_staged_package_revisions(staged_revisions.as_ref(), error)?;
        }
        state.catalog = next;
        state.leases.remove(&token);
        let catalog = state.catalog.clone();
        let leases = state.leases.clone();
        drop(state);
        if remove_package {
            discard_staged_package_revisions(staged_revisions);
        } else {
            gc_unreferenced_store_revisions(&self.store_root, &catalog, &leases)?;
        }
        Ok(())
    }

    fn gc_package_revisions(&self, package_id: &str) -> Result<()> {
        let state = self.state.lock().expect("package state poisoned");
        let catalog = state.catalog.clone();
        let leases = state.leases.clone();
        drop(state);
        gc_one_package_revisions(&self.store_root, package_id, &catalog, &leases)
    }

    fn gc_package_revisions_after_commit(&self, package_id: &str, action: &str) {
        if let Err(error) = self.gc_package_revisions(package_id) {
            tracing::warn!(
                package_id,
                action,
                error = %error,
                "Package revision cleanup failed after catalog commit; leaving stale revisions for later recovery"
            );
        }
    }

    fn materialize(
        &self,
        package_id: &str,
        revision: &str,
        snapshot: &PackageSnapshot,
    ) -> Result<MaterializationManifest> {
        let final_root = revision_root(&self.store_root, package_id, revision);
        match fs::symlink_metadata(&final_root) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_dir(),
                    "package revision path exists but is not an owned directory"
                );
                return verify_materialized_revision(&final_root, revision);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect package revision path"),
        }

        let stage = self.store_root.join("staging").join(format!(
            "{}-{}",
            package_id,
            uuid::Uuid::new_v4().simple()
        ));
        let source_root = stage.join("source");
        let content_root = stage.join("content");
        let skills_root = content_root.join("skills");
        fs::create_dir_all(&source_root)?;
        fs::create_dir_all(&skills_root)?;
        let materialized = (|| -> Result<MaterializationManifest> {
            for entry in &snapshot.entries {
                let target = source_root.join(&entry.path);
                ensure!(
                    target.starts_with(&source_root),
                    "snapshot path escaped staging"
                );
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&target, &entry.bytes)?;
                set_executable(&target, entry.executable)?;
            }

            fs::write(
                stage.join("source-name.json"),
                serde_json::to_vec(&snapshot.source_name)?,
            )?;
            copy_tree(&source_root, &content_root.join("source"))?;

            let mut exports = Vec::new();
            let mut skill_ids = BTreeSet::new();
            let mut generated_files = BTreeSet::new();
            let native_roots = native_skill_roots(snapshot)?;
            for native_root in native_roots {
                let skill_document = fs::read(source_root.join(&native_root).join("SKILL.md"))?;
                let skill_id = skill_id_for_native_root(snapshot, &native_root)?;
                let declared_dependencies = parse_skill_dependencies(&skill_id, &skill_document)?;
                let dependencies = merge_dependencies(
                    declared_dependencies,
                    detect_availability_dependencies(&skill_document),
                );
                ensure!(
                    skill_ids.insert(skill_id.clone()),
                    "duplicate Skill id `{skill_id}`"
                );
                let target = skills_root.join(&skill_id);
                copy_tree(&source_root.join(&native_root), &target)?;
                exports.push(PackageExport {
                    skill_id: skill_id.clone(),
                    root: format!("skills/{skill_id}"),
                    dependencies,
                });
            }

            if !snapshot
                .entries
                .iter()
                .any(|entry| entry.path == "SKILL.md")
            {
                for entry in &snapshot.entries {
                    let path = Path::new(&entry.path);
                    if path.components().count() != 2
                        || path.parent() != Some(Path::new("skills"))
                        || path.extension().and_then(|value| value.to_str()) != Some("md")
                    {
                        continue;
                    }
                    let skill_name = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .context("command Skill name is not UTF-8")?;
                    let skill_id = name_to_id(skill_name);
                    validate_package_id(&skill_id)?;
                    ensure!(
                        skill_ids.insert(skill_id.clone()),
                        "duplicate Skill id `{skill_id}`"
                    );
                    let body = std::str::from_utf8(&entry.bytes)
                        .context("command-style Skill is not UTF-8")?;
                    let dependencies = detect_availability_dependencies(&entry.bytes);
                    let document = command_skill_document(&skill_id, body, &dependencies);
                    ensure!(
                        document.len() <= MAX_SOURCE_FILE_BYTES,
                        "generated command-style Skill exceeds descriptor file size limit"
                    );
                    let target = skills_root.join(&skill_id);
                    fs::create_dir_all(&target)?;
                    fs::write(target.join("SKILL.md"), document)?;
                    generated_files.insert(format!("{skill_id}/SKILL.md"));
                    exports.push(PackageExport {
                        skill_id: skill_id.clone(),
                        root: format!("skills/{skill_id}"),
                        dependencies,
                    });
                }
            }
            exports.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
            let generated_files = generated_files
                .into_iter()
                .map(|path| format!("skills/{path}"))
                .collect::<BTreeSet<_>>();
            let files = materialized_file_records(&content_root, &generated_files)?;
            let manifest = MaterializationManifest {
                tree_fingerprint: fingerprint_directory(&content_root)?,
                exports,
                files,
            };
            fs::write(
                stage.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            sync_tree(&stage)?;
            Ok(manifest)
        })();

        match materialized {
            Ok(manifest) => {
                let parent = final_root.parent().context("revision root has no parent")?;
                match fs::symlink_metadata(parent) {
                    Ok(_) => ensure_owned_directory(
                        parent,
                        "package revisions path is not an owned directory",
                    )?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        fs::create_dir(parent)?;
                    }
                    Err(error) => return Err(error).context("inspect package revisions path"),
                }
                if fs::symlink_metadata(&final_root).is_ok() {
                    fs::remove_dir_all(&stage)?;
                    bail!("package revision path appeared during materialization");
                }
                fs::rename(&stage, &final_root)?;
                File::open(parent)?.sync_all()?;
                Ok(manifest)
            }
            Err(error) => {
                let _ = fs::remove_dir_all(&stage);
                Err(error)
            }
        }
    }

    fn publish_record(&self, record: PackageRecord) -> Result<()> {
        let mut next = self.catalog();
        ensure!(
            next.packages.contains_key(&record.id) || next.packages.len() < MAX_PACKAGES,
            "package catalog is full"
        );
        next.generation = next.generation.saturating_add(1);
        next.packages.insert(record.id.clone(), record);
        persist_catalog(&self.store_root, &next)?;
        self.state.lock().expect("package state poisoned").catalog = next;
        Ok(())
    }
}

#[async_trait::async_trait]
impl FlatFileService for PackageService {
    fn files(&self) -> &'static [(&'static str, bool)] {
        FILES
    }

    fn read(&self, name: &str) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().map_err(|_| ErrorCode::Io)?;
        match name {
            "catalog" => serde_json::to_vec(&state.catalog).map_err(|_| ErrorCode::Io),
            "status" => Ok(format!("{}\n", state.status).into_bytes()),
            "ctl" => Ok(b"write one Package Service command\n".to_vec()),
            "result" => serde_json::to_vec(&state.results).map_err(|_| ErrorCode::Io),
            _ => Err(ErrorCode::NotFound),
        }
    }

    async fn commit(&self, name: &str, bytes: &[u8]) -> Result<(), ErrorCode> {
        if name != "ctl" {
            return Err(ErrorCode::NoAccess);
        }
        let command: PackageCommand =
            serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
        self.execute(command).map_err(|_| ErrorCode::BadRequest)?;
        Ok(())
    }

    fn max_write_bytes(&self) -> usize {
        MAX_COMMAND_BYTES
    }
}

pub(crate) fn validate_package_id(value: &str) -> Result<()> {
    ensure!(!value.is_empty() && value.len() <= 64, "invalid package id");
    ensure!(
        value.split('-').all(|part| !part.is_empty()
            && part
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())),
        "invalid package id `{value}`"
    );
    Ok(())
}

fn validate_request_id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value.chars().all(
                |character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            ),
        "invalid package request id"
    );
    Ok(())
}

fn validate_command(command: &PackageCommand) -> Result<()> {
    validate_request_id(command.request_id())?;
    match command {
        PackageCommand::Install {
            package_id,
            snapshot,
            ..
        }
        | PackageCommand::Upgrade {
            package_id,
            snapshot,
            ..
        } => {
            validate_package_id(package_id)?;
            validate_snapshot(snapshot)
        }
        PackageCommand::Uninstall { package_id, .. } => validate_package_id(package_id),
        PackageCommand::List { .. } => Ok(()),
    }
}

fn validate_snapshot(snapshot: &PackageSnapshot) -> Result<()> {
    ensure!(
        !snapshot.source_name.is_empty()
            && snapshot.source_name.len() <= 255
            && !snapshot.source_name.contains('/')
            && !snapshot.source_name.contains('\0')
            && snapshot.source_name != "."
            && snapshot.source_name != "..",
        "invalid package source leaf"
    );
    ensure!(
        !snapshot.entries.is_empty() && snapshot.entries.len() <= MAX_SOURCE_FILES,
        "package snapshot file count is outside the supported range"
    );
    let mut paths = BTreeSet::new();
    let mut store_paths = BTreeSet::new();
    let mut total = 0usize;
    for entry in &snapshot.entries {
        ensure!(
            entry.bytes.len() <= MAX_SOURCE_FILE_BYTES,
            "package source file is too large"
        );
        total = total
            .checked_add(entry.bytes.len())
            .context("package snapshot size overflow")?;
        ensure!(total <= MAX_SOURCE_BYTES, "package snapshot is too large");
        let path = Path::new(&entry.path);
        ensure!(
            !path.is_absolute() && !entry.path.is_empty(),
            "invalid snapshot path"
        );
        ensure!(
            path.components()
                .all(|component| matches!(component, Component::Normal(_))),
            "snapshot path contains traversal"
        );
        ensure!(
            !has_vcs_component(path),
            "VCS control metadata is not package content"
        );
        let normalized = path.components().collect::<PathBuf>();
        ensure!(
            normalized.to_str() == Some(entry.path.as_str()),
            "snapshot path is not canonical"
        );
        ensure!(paths.insert(normalized), "duplicate snapshot path");
        ensure!(
            store_paths.insert(entry.path.to_lowercase()),
            "snapshot paths collide on a case-insensitive Package Store"
        );
    }
    Ok(())
}

fn fingerprint(snapshot: &PackageSnapshot) -> Result<String> {
    validate_snapshot(snapshot)?;
    let mut entries = snapshot.entries.clone();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let mut digest = Sha256::new();
    digest.update(MATERIALIZER_VERSION.as_bytes());
    digest.update((snapshot.source_name.len() as u64).to_be_bytes());
    digest.update(snapshot.source_name.as_bytes());
    for entry in entries {
        digest.update((entry.path.len() as u64).to_be_bytes());
        digest.update(entry.path.as_bytes());
        digest.update([u8::from(entry.executable)]);
        digest.update((entry.bytes.len() as u64).to_be_bytes());
        digest.update(&entry.bytes);
    }
    Ok(hex::encode(digest.finalize()))
}

fn parse_skill_dependencies(skill_id: &str, document: &[u8]) -> Result<Vec<SkillTypedDependency>> {
    let text = std::str::from_utf8(document).context("SKILL.md is not UTF-8")?;
    let virtual_path = Path::new(skill_id).join("SKILL.md");
    let metadata = parse_skill_metadata(text, &virtual_path, SkillScope::Installed)
        .map_err(anyhow::Error::from)
        .context("validate native SKILL.md")?;
    ensure!(
        metadata.id == skill_id,
        "native Skill runtime identity is not deterministic"
    );
    Ok(alan_agent_engine::skills::skill_declared_dependencies(
        &metadata,
    ))
}

fn detect_availability_dependencies(document: &[u8]) -> Vec<SkillTypedDependency> {
    let text = String::from_utf8_lossy(document);
    let mut dependencies = BTreeMap::new();
    for (needle, name) in [
        ("WebSearch", "web-search"),
        ("WebFetch", "web-fetch"),
        ("TeamCreate", "team-orchestration"),
        ("TaskCreate", "team-orchestration"),
    ] {
        if text.contains(needle) {
            dependencies.insert(
                format!("runtime_capability:{name}"),
                SkillTypedDependency::RuntimeCapability {
                    name: name.to_string(),
                    description: Some("Required by imported command vocabulary.".to_string()),
                },
            );
        }
    }
    dependencies.into_values().collect()
}

fn merge_dependencies(
    left: Vec<SkillTypedDependency>,
    right: Vec<SkillTypedDependency>,
) -> Vec<SkillTypedDependency> {
    let mut dependencies = BTreeMap::new();
    for dependency in left.into_iter().chain(right) {
        dependencies
            .entry(dependency.identity_key())
            .or_insert(dependency);
    }
    dependencies.into_values().collect()
}

fn command_skill_document(
    skill_id: &str,
    body: &str,
    dependencies: &[SkillTypedDependency],
) -> String {
    let yaml_skill_id = serde_yaml::to_string(skill_id)
        .expect("validated Skill id serialization must succeed")
        .trim_start_matches("---\n")
        .trim_end()
        .to_string();
    let mut document = format!(
        "---\nname: {yaml_skill_id}\ndescription: Imported command-style Skill `{skill_id}`.\n"
    );
    if !dependencies.is_empty() {
        document.push_str("compatibility:\n  dependencies:\n");
        for dependency in dependencies {
            let yaml = serde_yaml::to_string(dependency)
                .expect("typed dependency serialization must succeed");
            let yaml = yaml.trim_start_matches("---\n");
            let mut lines = yaml.lines();
            if let Some(first) = lines.next() {
                document.push_str(&format!("    - {first}\n"));
            }
            for line in lines {
                document.push_str(&format!("      {line}\n"));
            }
        }
    }
    document.push_str("---\n\n# Alan adapter\n\nThis Skill was imported by Package Service materializer `alan-skill-v1`. Treat foreign command placeholders as instruction context; unavailable capabilities remain explicit dependencies.\n\n");
    document.push_str(body);
    document
}

fn native_skill_roots(snapshot: &PackageSnapshot) -> Result<Vec<PathBuf>> {
    if snapshot
        .entries
        .iter()
        .any(|entry| entry.path == "SKILL.md")
    {
        return Ok(vec![PathBuf::new()]);
    }
    let mut roots = snapshot
        .entries
        .iter()
        .filter(|entry| entry.path.ends_with("/SKILL.md"))
        .map(|entry| {
            Path::new(&entry.path)
                .parent()
                .expect("nested SKILL.md has a parent")
                .to_path_buf()
        })
        .filter(|root| !has_non_export_component(root))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    for (index, root) in roots.iter().enumerate() {
        ensure!(
            roots
                .iter()
                .skip(index + 1)
                .all(|other| !other.starts_with(root)),
            "overlapping native Skill roots are ambiguous"
        );
    }
    Ok(roots)
}

fn skill_id_for_native_root(snapshot: &PackageSnapshot, root: &Path) -> Result<String> {
    let source = if root.as_os_str().is_empty() {
        snapshot.source_name.as_str()
    } else {
        root.file_name()
            .and_then(|name| name.to_str())
            .context("native Skill directory is not UTF-8")?
    };
    let skill_id = name_to_id(source);
    validate_package_id(&skill_id).context("native Skill directory has no canonical identity")?;
    Ok(skill_id)
}

fn bounded_error_message(error: &anyhow::Error) -> String {
    const LIMIT: usize = 1_024;
    let message = format!("{error:#}");
    if message.len() <= LIMIT {
        return message;
    }
    let mut end = LIMIT;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &message[..end])
}

fn recover_staging(root: &Path, catalog: &PackageCatalog) -> Result<()> {
    let staging = root.join("staging");
    ensure_owned_directory(&staging, "package staging path is not an owned directory")?;
    for entry in fs::read_dir(&staging)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("package staging entry is not UTF-8"))?;
        let interrupted_removal = name.strip_prefix("remove-").and_then(|rest| {
            let (package_id, nonce) = rest.rsplit_once('-')?;
            uuid::Uuid::parse_str(nonce).ok()?;
            Some(package_id)
        });
        if let Some(package_id) = interrupted_removal
            && catalog.packages.contains_key(package_id)
        {
            validate_package_id(package_id)?;
            ensure_owned_directory(
                &entry.path(),
                "staged package revisions are not an owned directory",
            )?;
            let active = root.join("revisions").join(package_id);
            match fs::symlink_metadata(&active) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => bail!("both active and staged package revisions exist"),
                Err(error) => return Err(error.into()),
            }
            fs::rename(entry.path(), active)?;
        } else {
            remove_path_without_following(&entry.path())?;
        }
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("catalog-")
            && name.to_string_lossy().ends_with(".tmp")
        {
            remove_path_without_following(&entry.path())?;
        }
    }
    Ok(())
}

fn remove_path_without_following(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn remove_package_revisions(root: &Path, package_id: &str) -> Result<()> {
    let path = root.join("revisions").join(package_id);
    if path.exists() {
        remove_path_without_following(&path)?;
    }
    Ok(())
}

struct StagedPackageRevisions {
    active: PathBuf,
    staged: PathBuf,
}

fn stage_package_revisions(
    root: &Path,
    package_id: &str,
) -> Result<Option<StagedPackageRevisions>> {
    let active = root.join("revisions").join(package_id);
    match fs::symlink_metadata(&active) {
        Ok(metadata) => ensure!(
            metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
            "package revisions path is not an owned directory"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    ensure_owned_directory(
        &root.join("staging"),
        "package staging path is not an owned directory",
    )?;
    let staged = root.join("staging").join(format!(
        "remove-{package_id}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::rename(&active, &staged).context("stage package revisions for removal")?;
    Ok(Some(StagedPackageRevisions { active, staged }))
}

fn rollback_staged_package_revisions(
    staged: Option<&StagedPackageRevisions>,
    catalog_error: anyhow::Error,
) -> Result<()> {
    if let Some(staged) = staged
        && let Err(rollback_error) = fs::rename(&staged.staged, &staged.active)
    {
        return Err(catalog_error.context(format!(
            "rollback staged package revisions failed: {rollback_error}"
        )));
    }
    Err(catalog_error)
}

fn discard_staged_package_revisions(staged: Option<StagedPackageRevisions>) {
    if let Some(staged) = staged
        && let Err(error) = remove_path_without_following(&staged.staged)
    {
        tracing::warn!(
            path = %staged.staged.display(),
            %error,
            "staged package revisions remain for startup recovery"
        );
    }
}

fn gc_unreferenced_store_revisions(
    root: &Path,
    catalog: &PackageCatalog,
    leases: &BTreeMap<u64, (String, String)>,
) -> Result<()> {
    let revisions = root.join("revisions");
    for entry in fs::read_dir(&revisions)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
            "package revisions store contains an unsupported entry"
        );
        let package_id = entry
            .file_name()
            .to_str()
            .context("package revision directory is not UTF-8")?
            .to_string();
        if catalog.packages.contains_key(&package_id) {
            gc_one_package_revisions(root, &package_id, catalog, leases)?;
        } else {
            remove_path_without_following(&entry.path())?;
        }
    }
    Ok(())
}

fn gc_one_package_revisions(
    root: &Path,
    package_id: &str,
    catalog: &PackageCatalog,
    leases: &BTreeMap<u64, (String, String)>,
) -> Result<()> {
    let Some(record) = catalog.packages.get(package_id) else {
        return remove_package_revisions(root, package_id);
    };
    let package_root = root.join("revisions").join(package_id);
    if !package_root.exists() {
        return Ok(());
    }
    let retained = leases
        .values()
        .filter(|(id, _)| id == package_id)
        .map(|(_, revision)| revision.as_str())
        .chain(std::iter::once(record.revision.as_str()))
        .collect::<BTreeSet<_>>();
    for entry in fs::read_dir(&package_root)? {
        let entry = entry?;
        let revision = entry.file_name();
        let revision = revision
            .to_str()
            .context("package revision name is not UTF-8")?;
        if !retained.contains(revision) {
            remove_path_without_following(&entry.path())?;
        }
    }
    Ok(())
}

fn verify_materialized_revision(root: &Path, revision: &str) -> Result<MaterializationManifest> {
    ensure_owned_directory(root, "package revision root is not an owned directory")?;
    let source_name_path = root.join("source-name.json");
    ensure_owned_file(
        &source_name_path,
        "package source-name record is not an owned file",
    )?;
    let source_name: String = serde_json::from_slice(&fs::read(source_name_path)?)
        .context("decode package source leaf")?;
    let source_root = root.join("source");
    ensure_owned_directory(
        &source_root,
        "package source root is not an owned directory",
    )?;
    let snapshot = PackageSnapshot::from_directory_named(&source_root, source_name)?;
    ensure!(
        fingerprint(&snapshot)? == revision,
        "package source fingerprint does not match its immutable revision"
    );
    let manifest_path = root.join("manifest.json");
    ensure_owned_file(&manifest_path, "package manifest is not an owned file")?;
    let manifest: MaterializationManifest = serde_json::from_slice(&fs::read(manifest_path)?)
        .context("decode materialization manifest")?;
    let content_root = root.join("content");
    ensure_owned_directory(
        &content_root,
        "materialized package root is not an owned directory",
    )?;
    ensure!(
        fingerprint_directory(&content_root)? == manifest.tree_fingerprint,
        "materialized package tree failed integrity validation"
    );
    let recorded_paths = manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    let actual_paths = list_relative_files(&content_root)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    ensure!(
        recorded_paths.len() == manifest.files.len() && recorded_paths == actual_paths,
        "materialization manifest does not enumerate its complete file tree"
    );
    for export in &manifest.exports {
        validate_package_id(&export.skill_id)?;
        let export_root = root.join("content").join(&export.root);
        ensure!(
            export.root == format!("skills/{}", export.skill_id)
                && export_root.join("SKILL.md").is_file(),
            "materialization manifest contains an invalid Skill export"
        );
    }
    Ok(manifest)
}

fn fingerprint_directory(root: &Path) -> Result<String> {
    ensure!(root.is_dir(), "materialized package tree is missing");
    let mut digest = Sha256::new();
    for relative in list_relative_files(root)? {
        let path = root.join(&relative);
        let mut file = File::open(&path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.file_type().is_file(),
            "materialized tree contains a special file"
        );
        digest.update((relative.len() as u64).to_be_bytes());
        digest.update(relative.as_bytes());
        digest.update([u8::from(executable_bit(&metadata))]);
        digest.update(metadata.len().to_be_bytes());

        let mut remaining = metadata.len();
        let mut buffer = [0u8; 8 * 1024];
        while remaining > 0 {
            let wanted = usize::try_from(remaining.min(buffer.len() as u64))?;
            let read = file.read(&mut buffer[..wanted])?;
            ensure!(
                read > 0,
                "materialized file changed while being fingerprinted"
            );
            digest.update(&buffer[..read]);
            remaining -= read as u64;
        }
        ensure!(
            file.read(&mut buffer[..1])? == 0,
            "materialized file changed while being fingerprinted"
        );
    }
    Ok(hex::encode(digest.finalize()))
}

fn materialized_file_records(
    root: &Path,
    generated: &BTreeSet<String>,
) -> Result<Vec<MaterializedFileRecord>> {
    Ok(list_relative_files(root)?
        .into_iter()
        .map(|path| MaterializedFileRecord {
            generated: generated.contains(&path),
            path,
        })
        .collect())
}

fn list_relative_files(root: &Path) -> Result<Vec<String>> {
    ensure_owned_directory(root, "materialized tree root is not an owned directory")?;
    let canonical_root = fs::canonicalize(root)?;
    let mut pending = vec![canonical_root.clone()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "materialized tree contains a symlink"
            );
            if metadata.file_type().is_dir() {
                pending.push(path);
            } else {
                ensure!(
                    metadata.file_type().is_file(),
                    "materialized tree contains a special file"
                );
                files.push(slash_path(path.strip_prefix(&canonical_root)?)?);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn ensure_owned_directory(path: &Path, message: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| format!("inspect {path:?}"))?;
    ensure!(
        metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
        "{message}"
    );
    Ok(())
}

fn ensure_package_store_channel_chain(store_root: &Path) -> Result<()> {
    let services_root = store_root
        .parent()
        .context("Package Store root has no services parent")?;
    let channel_root = services_root
        .parent()
        .context("Package Store root has no channel parent")?;
    for (path, message) in [
        (
            channel_root,
            "Package Store channel path contains an unsupported ancestor",
        ),
        (
            services_root,
            "Package Store channel path contains an unsupported ancestor",
        ),
        (store_root, "Package Store root is not an owned directory"),
    ] {
        match fs::symlink_metadata(path) {
            Ok(metadata) => ensure!(
                metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
                "{message}"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect Package Store channel path"),
        }
    }
    Ok(())
}

fn ensure_owned_file(path: &Path, message: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| format!("inspect {path:?}"))?;
    ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "{message}"
    );
    Ok(())
}

fn sync_tree(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(directory) = pending.pop() {
        directories.push(directory.clone());
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                pending.push(path);
            } else if metadata.file_type().is_file() {
                File::open(path)?.sync_all()?;
            } else {
                bail!("package staging contains an unsupported entry");
            }
        }
    }
    for directory in directories.into_iter().rev() {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}

fn load_catalog(root: &Path) -> Result<PackageCatalog> {
    let path = root.join("catalog.json");
    if !path.exists() {
        return Ok(PackageCatalog::default());
    }
    let bytes = fs::read(&path)?;
    serde_json::from_slice(&bytes).context("decode Package Service catalog")
}

fn persist_catalog(root: &Path, catalog: &PackageCatalog) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(catalog)?;
    let temporary = root.join(format!("catalog-{}.tmp", uuid::Uuid::new_v4().simple()));
    let mut file = File::create(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, root.join("catalog.json"))?;
    File::open(root)?.sync_all()?;
    Ok(())
}

fn verify_catalog(root: &Path, catalog: &PackageCatalog) -> Result<()> {
    validate_catalog_structure(catalog)?;
    for record in catalog.packages.values() {
        ensure!(
            record.materializer_version == MATERIALIZER_VERSION,
            "catalog references an unsupported materializer version"
        );
        verify_revision(root, record)?;
    }
    Ok(())
}

fn validate_catalog_structure(catalog: &PackageCatalog) -> Result<()> {
    ensure!(
        catalog.packages.len() <= MAX_PACKAGES,
        "package catalog exceeds its supported size"
    );
    for (package_id, record) in &catalog.packages {
        ensure!(
            package_id == &record.id,
            "package catalog key does not match its record id"
        );
        validate_package_id(&record.id)?;
    }
    Ok(())
}

fn verify_revision(root: &Path, record: &PackageRecord) -> Result<()> {
    ensure!(
        record.revision.len() == 64
            && record
                .revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "catalog contains an invalid package revision"
    );
    let revision_root = revision_root(root, &record.id, &record.revision);
    ensure!(
        revision_root.is_dir(),
        "catalog references missing package revision"
    );
    let manifest = verify_materialized_revision(&revision_root, &record.revision)?;
    ensure!(
        manifest.exports == record.exports
            && manifest.tree_fingerprint == record.materialized_fingerprint,
        "catalog and materialization manifest disagree"
    );
    ensure!(
        record.state == PackageState::Installed || record.reference_count > 0,
        "retiring package has an invalid reference state"
    );
    for export in &record.exports {
        validate_skill_compatibility(&SkillCompatibility {
            dependencies: export.dependencies.clone(),
            ..SkillCompatibility::default()
        })
        .map_err(anyhow::Error::from)
        .context("catalog contains an invalid typed package dependency")?;
    }
    Ok(())
}

fn revision_root(root: &Path, package_id: &str, revision: &str) -> PathBuf {
    root.join("revisions").join(package_id).join(revision)
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        let mut entries = fs::read_dir(&from)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let source_path = entry.path();
            let target_path = to.join(entry.file_name());
            let metadata = fs::symlink_metadata(&source_path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "materialized Skill contains a symlink"
            );
            if metadata.is_dir() {
                fs::create_dir_all(&target_path)?;
                pending.push((source_path, target_path));
            } else {
                fs::copy(&source_path, &target_path)?;
                set_executable(&target_path, executable_bit(&metadata))?;
            }
        }
    }
    Ok(())
}

fn slash_path(path: &Path) -> Result<String> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_string)
                .context("package path is not UTF-8"),
            _ => bail!("package path is not relative and normalized"),
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}

fn has_vcs_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::Normal(value) if value == ".git"))
}

fn has_non_export_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::Normal(value)
                if matches!(
                    value.to_str(),
                    Some(
                        "agents"
                            | "assets"
                            | "bin"
                            | "evals"
                            | "eval-viewer"
                            | "references"
                            | "scripts"
                    )
                )
        )
    })
}

#[cfg(unix)]
fn executable_bit(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable_bit(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(if executable { 0o555 } else { 0o444 });
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests;
