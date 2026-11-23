//! Package Service (Quartermaster): installed package ownership and lifecycle across
//! `skill`, `mcp`, `tool`, `provider`, and `fileserver` packages.

use std::collections::BTreeMap;
use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alan_ap::{ErrorCode, FileServer};
use alan_hostfs::{HostDirAccess, HostDirFs};
use anyhow::{Context, Result, bail, ensure};

use crate::flat_fs::{FlatFileService, FlatServiceFs};

mod backend;
mod bootstrap;
pub(crate) use bootstrap::PackageBootstrap;
mod fs_safety;
mod inspection;
pub use inspection::validate_package_store_for_migration;
mod manifest;
mod materializer;
mod materializer_exports;
mod root_bindings;
mod store;
mod sync;
mod types;

pub use root_bindings::{
    FileserverMountAccess, RootBindingMode, RootFileserverActivation, RootPackageBinding,
};
pub use types::{
    FileserverPackageExport, FileserverTransportSpec, McpPackageExport, McpTransportSpec,
    PackageCatalog, PackageCommand, PackageCommandResult, PackageExport, PackageExportKind,
    PackageKind, PackageRecord, PackageSnapshot, PackageSnapshotEntry, PackageState,
    PackageSyncEntry, ProviderAuthMode, ProviderPackageExport, ToolPackageExport,
    validate_package_id,
};

use backend::{PackageLeaseContent, PackageStoreBackend};
#[cfg(test)]
use materializer::PackageMaterializer;
use materializer::{fingerprint, validate_snapshot};
use store::PackageStore;
use sync::record_from_materialization;
pub(crate) use types::{LEGACY_SKILL_MATERIALIZER_VERSION, MATERIALIZER_VERSION};

const FILES: &[(&str, bool)] = &[
    ("catalog", false),
    ("status", false),
    ("ctl", true),
    ("result", false),
];
const MAX_COMMAND_BYTES: usize = 64 * 1024 * 1024;
const MAX_PACKAGES: usize = 4_096;
const MAX_RESULTS: usize = 128;

struct State {
    catalog: PackageCatalog,
    root_bindings: BTreeMap<String, RootPackageBinding>,
    results: BTreeMap<String, PackageCommandResult>,
    result_order: Vec<String>,
    status: String,
    leases: BTreeMap<u64, (String, String)>,
    next_lease_id: u64,
}

/// Installed-package authority bound to an explicit store or owned temporary directory.
pub struct PackageService {
    store: PackageStoreBackend,
    _temporary_store: Option<crate::temporary_store::TemporaryStore>,
    state: Mutex<State>,
    operation: Mutex<()>,
}

/// An immutable Package Service revision reference retained by a Process context.
pub struct PackageReferenceLease {
    service: Arc<PackageService>,
    lease: Option<store::PackageLease>,
    token: u64,
    record: PackageRecord,
    content: PackageLeaseContent,
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
    /// Returns the durable catalog record pinned by this immutable revision lease.
    pub fn record(&self) -> &PackageRecord {
        &self.record
    }

    /// Serves this immutable package `content/` directory over a read-only FileServer.
    pub fn file_server(&self) -> Result<Arc<dyn FileServer>> {
        match &self.content {
            PackageLeaseContent::Disk(content_root) => Ok(Arc::new(HostDirFs::new(
                content_root,
                HostDirAccess::ReadOnly,
            )?)),
            PackageLeaseContent::InMemory(files) => {
                let memfs = alan_ap::reference::MemFs::try_with_read_only_tree(
                    files
                        .iter()
                        .map(|(path, bytes)| (path.clone(), bytes.clone())),
                )
                .map_err(|error| anyhow::anyhow!("invalid in-memory package tree: {error:?}"))?;
                Ok(Arc::new(memfs))
            }
        }
    }

    /// Returns the canonical immutable disk `content/` directory for this package lease.
    pub(crate) fn disk_content_root(&self) -> Result<PathBuf> {
        let content_root = match &self.content {
            PackageLeaseContent::Disk(content_root) => content_root,
            PackageLeaseContent::InMemory(_) => {
                bail!("this package export requires a disk-backed Package Store revision");
            }
        };
        Ok(fs::canonicalize(content_root)?)
    }

    /// Resolves a content-relative file path strictly inside this immutable disk-backed revision.
    pub(crate) fn content_file_path(&self, relative_path: &str) -> Result<PathBuf> {
        let canonical_content = self.disk_content_root()?;
        let candidate = canonical_content.join(relative_path);
        let canonical_file = fs::canonicalize(&candidate).with_context(|| {
            format!(
                "missing materialized package file `{relative_path}` in `{}`",
                self.record.id
            )
        })?;
        ensure!(
            canonical_file.starts_with(&canonical_content),
            "package file `{relative_path}` escapes immutable revision"
        );
        ensure!(
            canonical_file.is_file(),
            "package file `{relative_path}` is not a regular file"
        );
        Ok(canonical_file)
    }

    #[cfg(test)]
    pub(crate) fn content_root(&self) -> &Path {
        match &self.content {
            PackageLeaseContent::Disk(content_root) => content_root,
            PackageLeaseContent::InMemory(_) => {
                panic!("in-memory package lease has no filesystem content_root")
            }
        }
    }

    pub(crate) fn skill_descriptor(
        &self,
        relative_root: &str,
    ) -> Result<alan_agent_engine::ProcessFileTree> {
        match &self.content {
            PackageLeaseContent::Disk(content_root) => {
                let root = content_root.join(relative_root);
                let canonical_content = fs::canonicalize(content_root)?;
                let canonical_root = fs::canonicalize(&root)?;
                ensure!(
                    canonical_root.starts_with(&canonical_content),
                    "package Skill descriptor escapes its immutable revision"
                );
                let source_leaf = canonical_root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("package Skill descriptor root has no UTF-8 leaf")?;
                let snapshot = PackageSnapshot::from_directory_named(
                    &canonical_root,
                    source_leaf.to_string(),
                )?;
                alan_agent_engine::ProcessFileTree::new(
                    snapshot
                        .entries
                        .into_iter()
                        .map(|entry| (entry.path, entry.bytes))
                        .collect(),
                )
            }
            PackageLeaseContent::InMemory(files) => {
                let normalized = relative_root.trim_matches('/');
                let prefix = if normalized.is_empty() {
                    String::new()
                } else {
                    format!("{normalized}/")
                };
                let entries: BTreeMap<String, Vec<u8>> = files
                    .iter()
                    .filter_map(|(path, bytes)| {
                        path.strip_prefix(&prefix)
                            .map(|rel| (rel.to_string(), bytes.clone()))
                    })
                    .collect();
                ensure!(
                    !entries.is_empty(),
                    "in-memory package Skill descriptor `{relative_root}` is missing"
                );
                alan_agent_engine::ProcessFileTree::new(entries)
            }
        }
    }
}

impl Drop for PackageReferenceLease {
    fn drop(&mut self) {
        if let Err(error) = self.service.release(self.token, &mut self.lease) {
            tracing::error!(token = self.token, %error, "failed to release package reference");
        }
    }
}

impl std::fmt::Debug for PackageService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PackageService")
            .finish_non_exhaustive()
    }
}

impl PackageService {
    /// Opens (or initializes) a durable Package Service backed by `store_root`.
    pub fn open(store_root: PathBuf) -> Result<Arc<Self>> {
        Self::open_inner(store_root, None, None)
    }

    /// Creates an ephemeral disk-backed Package Service in an owned temporary store.
    pub fn ephemeral() -> Result<Arc<Self>> {
        let temporary = crate::temporary_store::TemporaryStore::new("package")?;
        let root = temporary.path().to_path_buf();
        Self::open_inner(root, Some(temporary), None)
    }

    /// Creates a purely in-memory Package Service with no disk backing.
    pub fn in_memory() -> Arc<Self> {
        Arc::new(Self {
            store: PackageStoreBackend::InMemory(Mutex::new(BTreeMap::new())),
            _temporary_store: None,
            state: Mutex::new(State {
                catalog: PackageCatalog::default(),
                root_bindings: BTreeMap::new(),
                results: BTreeMap::new(),
                result_order: Vec::new(),
                status: "ready".to_string(),
                leases: BTreeMap::new(),
                next_lease_id: 1,
            }),
            operation: Mutex::new(()),
        })
    }

    fn open_inner(
        store_root: PathBuf,
        temporary_store: Option<crate::temporary_store::TemporaryStore>,
        bootstrap: Option<Arc<bootstrap::BootstrapWait>>,
    ) -> Result<Arc<Self>> {
        let initial_bindings = Self::load_root_bindings_from_store(&store_root)?
            .into_iter()
            .map(|binding| (binding.package_id.clone(), binding))
            .collect();
        let (store, catalog) = PackageStore::open(store_root, bootstrap)?;
        Ok(Arc::new(Self {
            store: PackageStoreBackend::Disk(store),
            _temporary_store: temporary_store,
            state: Mutex::new(State {
                catalog,
                root_bindings: initial_bindings,
                results: BTreeMap::new(),
                result_order: Vec::new(),
                status: "ready".to_string(),
                leases: BTreeMap::new(),
                next_lease_id: 1,
            }),
            operation: Mutex::new(()),
        }))
    }

    /// Returns a read-write/control FileServer serving `/mnt/package`.
    pub fn file_server(self: &Arc<Self>) -> Arc<dyn FileServer> {
        Arc::new(FlatServiceFs::new(self.clone()))
    }

    /// Reads the current shared catalog under its cross-process transaction lock.
    pub fn catalog(&self) -> Result<PackageCatalog> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        Ok(self.cached_catalog())
    }

    /// Lists all active installed extension provider packages.
    pub fn list_provider_packages(&self) -> Result<Vec<(PackageRecord, ProviderPackageExport)>> {
        let catalog = self.catalog()?;
        Ok(catalog
            .packages
            .into_values()
            .filter(|record| {
                record.state == PackageState::Installed
                    && record.export_kind == PackageExportKind::Provider
            })
            .filter_map(|record| {
                let provider = record.provider_export.clone()?;
                Some((record, provider))
            })
            .collect())
    }

    /// Resolves an installed extension `provider` package by `"pkg:<package-id>"` or `<provider-id>`.
    pub fn resolve_provider_package(
        &self,
        selector: &str,
    ) -> Result<(PackageRecord, ProviderPackageExport)> {
        let trimmed = selector.trim();
        let catalog = self.catalog()?;
        if let Some(package_id) = trimmed.strip_prefix("pkg:") {
            validate_package_id(package_id)?;
            let record = catalog
                .packages
                .get(package_id)
                .filter(|record| record.state == PackageState::Installed)
                .cloned()
                .with_context(|| format!("provider package `{package_id}` is not installed"))?;
            let provider = record
                .provider_export
                .clone()
                .with_context(|| format!("package `{package_id}` is not a provider package"))?;
            return Ok((record, provider));
        }

        let mut matches = catalog
            .packages
            .into_values()
            .filter(|record| record.state == PackageState::Installed)
            .filter_map(|record| {
                let provider = record.provider_export.clone()?;
                if provider.provider_id == trimmed || provider.protocol == trimmed {
                    Some((record, provider))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        ensure!(
            matches.len() <= 1,
            "multiple installed provider packages match `{trimmed}`; select one with `pkg:<package-id>`"
        );
        matches
            .pop()
            .with_context(|| format!("no installed provider package matches `{trimmed}`"))
    }

    fn refresh(&self) -> Result<Option<store::PackageStoreLock>> {
        let transaction = self.store.transaction()?;
        if let Some(catalog) = self.store.load()? {
            self.state.lock().expect("package state poisoned").catalog = catalog;
        }
        Ok(transaction)
    }

    fn cached_catalog(&self) -> PackageCatalog {
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
        let _transaction = self.refresh()?;
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
                self.cached_catalog().packages.len() < MAX_PACKAGES,
                "package catalog is full"
            );
        }
        let materialization = self.store.materialize(package_id, &revision, &snapshot)?;
        let reference_count = existing.as_ref().map_or(0, |record| record.reference_count);
        let now = chrono::Utc::now();
        self.publish_record(record_from_materialization(
            package_id.to_string(),
            revision,
            PackageKind::Preinstalled,
            reference_count,
            existing.as_ref().map_or(now, |record| record.created_at),
            now,
            materialization,
        ))?;
        self.gc_package_revisions_after_commit(package_id, "seed preinstalled package");
        Ok(())
    }

    /// Resolves and verifies one currently active installed/preinstalled package record.
    pub fn resolve(&self, package_id: &str) -> Result<PackageRecord> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        let state = self.state.lock().expect("package state poisoned");
        let record = state
            .catalog
            .packages
            .get(package_id)
            .filter(|record| record.state == PackageState::Installed)
            .cloned()
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        self.store.verify_revision(&record)?;
        Ok(record)
    }

    /// Acquires an immutable revision reference lease for `package_id`.
    pub fn acquire(self: &Arc<Self>, package_id: &str) -> Result<Arc<PackageReferenceLease>> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        let mut state = self.state.lock().expect("package state poisoned");
        let mut record = state
            .catalog
            .packages
            .get(package_id)
            .filter(|record| record.state == PackageState::Installed)
            .cloned()
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        self.store.verify_revision(&record)?;
        let lease = self.store.lease(package_id, &record.revision)?;
        let token = state.next_lease_id;
        record.reference_count = record
            .reference_count
            .checked_add(1)
            .context("package reference count overflow")?;
        let mut next = state.catalog.clone();
        next.generation = next.generation.saturating_add(1);
        next.packages.insert(package_id.to_string(), record.clone());
        self.store.persist_catalog(&next)?;
        state.catalog = next;
        state.next_lease_id = state.next_lease_id.saturating_add(1);
        state
            .leases
            .insert(token, (package_id.to_string(), record.revision.clone()));
        Ok(Arc::new(PackageReferenceLease {
            service: self.clone(),
            lease,
            token,
            content: self.store.lease_content(package_id, &record.revision)?,
            record,
        }))
    }

    /// Executes a validated Quartermaster command synchronously inside the operation lock.
    pub fn execute(&self, command: PackageCommand) -> Result<PackageCommandResult> {
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
        let attempted = self.refresh().and_then(|_transaction| match command {
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
                catalog: Some(self.cached_catalog()),
            }),
            PackageCommand::Upgrade {
                request_id,
                package_id,
                snapshot,
            } => self.upgrade(request_id, package_id, snapshot),
            PackageCommand::Uninstall {
                request_id,
                package_id,
            } => self.remove_package(request_id, package_id, PackageKind::Installed),
            PackageCommand::Sync {
                request_id,
                packages,
                prune,
            } => self.sync(request_id, packages, prune),
        });
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
            self.cached_catalog().packages.len() < MAX_PACKAGES,
            "package catalog is full"
        );
        let materialization = self.store.materialize(&package_id, &revision, &snapshot)?;
        let now = chrono::Utc::now();
        let record = record_from_materialization(
            package_id.clone(),
            revision,
            PackageKind::Installed,
            0,
            now,
            now,
            materialization,
        );
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
        let materialization = self.store.materialize(&package_id, &revision, &snapshot)?;
        let record = record_from_materialization(
            package_id.clone(),
            revision,
            PackageKind::Installed,
            existing.reference_count,
            existing.created_at,
            chrono::Utc::now(),
            materialization,
        );
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

    /// Retires an obsolete product-owned package without touching operator packages.
    pub(crate) fn retire_preinstalled(&self, package_id: &str) -> Result<()> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
        if self
            .cached_catalog()
            .packages
            .get(package_id)
            .is_some_and(|record| {
                record.kind == PackageKind::Preinstalled && record.state == PackageState::Installed
            })
        {
            self.remove_package(
                String::new(),
                package_id.to_string(),
                PackageKind::Preinstalled,
            )?;
        }
        Ok(())
    }

    fn remove_package(
        &self,
        request_id: String,
        package_id: String,
        expected_kind: PackageKind,
    ) -> Result<PackageCommandResult> {
        let mut next = self.cached_catalog();
        let existing = next
            .packages
            .get(&package_id)
            .cloned()
            .with_context(|| format!("package `{package_id}` is not installed"))?;
        ensure!(
            existing.kind == expected_kind,
            "package kind does not permit this removal"
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
            self.store.stage_package_revisions(&package_id)?
        };
        if let Err(error) = self.store.persist_catalog(&next) {
            self.store
                .rollback_staged_package_revisions(staged_revisions.as_ref(), error)?;
        }
        self.state.lock().expect("package state poisoned").catalog = next;
        self.store
            .discard_staged_package_revisions(staged_revisions);
        if !retained {
            self.store.remove_in_memory_revisions(&package_id);
        }
        if expected_kind == PackageKind::Installed {
            self.unbind_after_uninstall_locked(&package_id)?;
        }
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

    fn release(&self, token: u64, lease: &mut Option<store::PackageLease>) -> Result<()> {
        let _operation = self.operation.lock().expect("package operation poisoned");
        let _transaction = self.refresh()?;
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
            self.store.stage_package_revisions(&package_id)?
        } else {
            None
        };
        if let Err(error) = self.store.persist_catalog(&next) {
            self.store
                .rollback_staged_package_revisions(staged_revisions.as_ref(), error)?;
        }
        state.catalog = next;
        state.leases.remove(&token);
        lease.take();
        let catalog = state.catalog.clone();
        drop(state);
        if remove_package {
            self.store
                .discard_staged_package_revisions(staged_revisions);
            self.store.remove_in_memory_revisions(&package_id);
        } else {
            self.store.gc_unreferenced_revisions(&catalog)?;
        }
        Ok(())
    }

    fn gc_package_revisions(&self, package_id: &str) -> Result<()> {
        let state = self.state.lock().expect("package state poisoned");
        let catalog = state.catalog.clone();
        drop(state);
        self.store.gc_package_revisions(package_id, &catalog)
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
        let mut next = self.cached_catalog();
        ensure!(
            next.packages.contains_key(&record.id) || next.packages.len() < MAX_PACKAGES,
            "package catalog is full"
        );
        next.generation = next.generation.saturating_add(1);
        next.packages.insert(record.id.clone(), record);
        self.store.persist_catalog(&next)?;
        self.state.lock().expect("package state poisoned").catalog = next;
        Ok(())
    }
}

#[async_trait::async_trait]
impl FlatFileService for PackageService {
    fn files(&self) -> &'static [(&'static str, bool)] {
        FILES
    }

    async fn read(&self, name: &str) -> Result<Vec<u8>, ErrorCode> {
        if name == "catalog" {
            let catalog = self.catalog().map_err(|_| ErrorCode::Io)?;
            return serde_json::to_vec(&catalog).map_err(|_| ErrorCode::Io);
        }
        let state = self.state.lock().map_err(|_| ErrorCode::Io)?;
        match name {
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
        PackageCommand::Sync { packages, .. } => {
            for entry in packages {
                validate_package_id(&entry.package_id)?;
                validate_snapshot(&entry.snapshot)?;
            }
            Ok(())
        }
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

#[cfg(test)]
mod tests;
