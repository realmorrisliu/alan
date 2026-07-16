//! Package Service (Quartermaster): installed package ownership and lifecycle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use alan_agent_engine::skills::{
    SkillCompatibility, SkillTypedDependency, validate_skill_compatibility,
};
use alan_ap::{ErrorCode, FileServer};
use alan_hostfs::{HostDirAccess, HostDirFs};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::flat_fs::{FlatFileService, FlatServiceFs};

mod materializer;

use materializer::{
    PackageMaterializer, fingerprint, validate_snapshot, verify_materialized_revision,
};

const FILES: &[(&str, bool)] = &[
    ("catalog", false),
    ("status", false),
    ("ctl", true),
    ("result", false),
];
const MATERIALIZER_VERSION: &str = "alan-skill-v1";
const MAX_COMMAND_BYTES: usize = 64 * 1024 * 1024;
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
        let materialization = PackageMaterializer::new(&self.store_root)
            .materialize(package_id, &revision, &snapshot)?;
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
        let materialization = PackageMaterializer::new(&self.store_root).materialize(
            &package_id,
            &revision,
            &snapshot,
        )?;
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
        let materialization = PackageMaterializer::new(&self.store_root).materialize(
            &package_id,
            &revision,
            &snapshot,
        )?;
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

#[cfg(test)]
mod tests;
