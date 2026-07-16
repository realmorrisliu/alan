//! Package Service (Quartermaster): installed package ownership and lifecycle.

use std::collections::BTreeMap;
use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

use alan_agent_engine::skills::SkillTypedDependency;
use alan_ap::{ErrorCode, FileServer};
use alan_hostfs::{HostDirAccess, HostDirFs};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::flat_fs::{FlatFileService, FlatServiceFs};

mod fs_safety;
mod materializer;
mod store;

use materializer::{PackageMaterializer, fingerprint, validate_snapshot};
use store::PackageStore;

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
    store: PackageStore,
    _temporary_store: Option<tempfile::TempDir>,
    state: Mutex<State>,
    operation: Mutex<()>,
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
        let (store, catalog) = PackageStore::open(store_root)?;
        Ok(Arc::new(Self {
            channel_id,
            store,
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
        let materialization = PackageMaterializer::new(self.store.root())
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
        self.store.verify_revision(&record)?;
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
        self.store.verify_revision(&record)?;
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
            service: Arc::downgrade(self),
            token,
            content_root: self
                .store
                .revision_root(package_id, &record.revision)
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
        let materialization = PackageMaterializer::new(self.store.root()).materialize(
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
        let materialization = PackageMaterializer::new(self.store.root()).materialize(
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
            self.store.stage_package_revisions(&package_id)?
        };
        if let Err(error) = self.store.persist_catalog(&next) {
            self.store
                .rollback_staged_package_revisions(staged_revisions.as_ref(), error)?;
        }
        self.state.lock().expect("package state poisoned").catalog = next;
        self.store
            .discard_staged_package_revisions(staged_revisions);
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
        let catalog = state.catalog.clone();
        let leases = state.leases.clone();
        drop(state);
        if remove_package {
            self.store
                .discard_staged_package_revisions(staged_revisions);
        } else {
            self.store.gc_unreferenced_revisions(&catalog, &leases)?;
        }
        Ok(())
    }

    fn gc_package_revisions(&self, package_id: &str) -> Result<()> {
        let state = self.state.lock().expect("package state poisoned");
        let catalog = state.catalog.clone();
        let leases = state.leases.clone();
        drop(state);
        self.store
            .gc_package_revisions(package_id, &catalog, &leases)
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

#[cfg(test)]
mod tests;
