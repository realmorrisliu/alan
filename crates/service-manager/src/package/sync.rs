//! Atomic multi-package sync transactions and record construction helpers for [`PackageService`].

use std::collections::BTreeSet;

use anyhow::{Result, ensure};

use super::materializer::{MaterializationManifest, fingerprint};
use super::types::{
    PackageCommandResult, PackageKind, PackageRecord, PackageState, PackageSyncEntry,
};
use super::{MAX_PACKAGES, PackageService};

impl PackageService {
    pub(super) fn sync(
        &self,
        request_id: String,
        packages: Vec<PackageSyncEntry>,
        prune: bool,
    ) -> Result<PackageCommandResult> {
        let current = self.cached_catalog();
        if packages.is_empty() && !prune {
            for record in current.packages.values() {
                self.store.verify_revision(record)?;
            }
            self.store.gc_unreferenced_revisions(&current)?;
            return Ok(PackageCommandResult {
                request_id,
                success: true,
                action: "sync".to_string(),
                message: format!("synced ({} packages)", current.packages.len()),
                package: None,
                catalog: Some(current),
            });
        }

        let mut seen = BTreeSet::new();
        let mut prepared = Vec::with_capacity(packages.len());
        for entry in packages {
            ensure!(
                seen.insert(entry.package_id.clone()),
                "sync contains duplicate package id `{}`",
                entry.package_id
            );
            let existing = current.packages.get(&entry.package_id).cloned();
            if let Some(ref record) = existing {
                ensure!(
                    record.kind == PackageKind::Installed,
                    "preinstalled package `{}` cannot be modified by operator sync",
                    entry.package_id
                );
                ensure!(
                    record.state == PackageState::Installed,
                    "retiring package `{}` cannot be synced until leases release",
                    entry.package_id
                );
            }
            let revision = fingerprint(&entry.snapshot)?;
            prepared.push((entry, existing, revision));
        }

        let mut next = current.clone();
        let now = chrono::Utc::now();
        let mut touched_ids = Vec::new();
        let mut single_record = None;
        let single_message = if prepared.len() == 1 && !prune {
            let (_, ref existing, ref rev) = prepared[0];
            match existing {
                Some(rec) if &rec.revision == rev => "already current".to_string(),
                Some(_) => "upgraded".to_string(),
                None => "installed".to_string(),
            }
        } else {
            format!("synced ({} packages)", prepared.len())
        };

        let materialize_result: Result<()> = (|| {
            for (entry, existing, revision) in prepared {
                if let Some(ref prev) = existing
                    && prev.revision == revision
                {
                    single_record = Some(prev.clone());
                    continue;
                }
                let materialization =
                    self.store
                        .materialize(&entry.package_id, &revision, &entry.snapshot)?;
                let record = record_from_materialization(
                    entry.package_id.clone(),
                    revision,
                    PackageKind::Installed,
                    existing.as_ref().map_or(0, |r| r.reference_count),
                    existing.as_ref().map_or(now, |r| r.created_at),
                    now,
                    materialization,
                );
                touched_ids.push(entry.package_id.clone());
                single_record = Some(record.clone());
                next.packages.insert(entry.package_id, record);
            }
            Ok(())
        })();

        if let Err(error) = materialize_result {
            let _ = self.store.gc_unreferenced_revisions(&current);
            return Err(error);
        }

        let mut staged_removals = Vec::new();
        if prune {
            let removed_ids = current
                .packages
                .iter()
                .filter(|(id, record)| {
                    record.kind == PackageKind::Installed && !seen.contains(id.as_str())
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            for removed_id in removed_ids {
                if let Some(existing) = next.packages.get(&removed_id).cloned() {
                    if existing.reference_count > 0 {
                        let mut retiring = existing;
                        retiring.state = PackageState::Retiring;
                        next.packages.insert(removed_id, retiring);
                    } else {
                        next.packages.remove(&removed_id);
                        if let Some(staged) = self.store.stage_package_revisions(&removed_id)? {
                            staged_removals.push((removed_id, staged));
                        }
                    }
                }
            }
        }

        ensure!(
            next.packages.len() <= MAX_PACKAGES,
            "package catalog is full"
        );
        if next.packages != current.packages {
            next.generation = next.generation.saturating_add(1);
            if let Err(error) = self.store.persist_catalog(&next) {
                for (_, staged) in &staged_removals {
                    let _ = self.store.rollback_staged_package_revisions(
                        Some(staged),
                        anyhow::anyhow!("sync rollback"),
                    );
                }
                let _ = self.store.gc_unreferenced_revisions(&current);
                return Err(error);
            }
            self.state.lock().expect("package state poisoned").catalog = next.clone();
            for (removed_id, staged) in staged_removals {
                self.store.discard_staged_package_revisions(Some(staged));
                self.store.remove_in_memory_revisions(&removed_id);
            }
            for id in &touched_ids {
                self.gc_package_revisions_after_commit(id, "sync package");
            }
        }

        Ok(PackageCommandResult {
            request_id,
            success: true,
            action: "sync".to_string(),
            message: single_message,
            package: single_record,
            catalog: Some(next),
        })
    }
}

pub(super) fn record_from_materialization(
    id: String,
    revision: String,
    kind: PackageKind,
    reference_count: u64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    materialization: MaterializationManifest,
) -> PackageRecord {
    PackageRecord {
        id,
        revision,
        kind,
        state: PackageState::Installed,
        materializer_version: materialization.materializer_version,
        materialized_fingerprint: materialization.tree_fingerprint,
        exports: materialization.exports,
        reference_count,
        created_at,
        updated_at,
        export_kind: materialization.export_kind,
        manifest_version: materialization.manifest_version,
        description: materialization.description,
        tool_exports: materialization.tool_exports,
        mcp_exports: materialization.mcp_exports,
        provider_export: materialization.provider_export,
        fileserver_export: materialization.fileserver_export,
    }
}
