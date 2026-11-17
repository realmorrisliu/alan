//! Storage backend abstraction for [`super::PackageService`] (disk-backed or in-memory).

use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, ensure};

use super::materializer::{self, MaterializationManifest, PackageMaterializer};
use super::store::{self, PackageStore};
use super::{PackageCatalog, PackageRecord, PackageSnapshot};

type InMemoryRevisionMap = BTreeMap<(String, String), Arc<BTreeMap<String, Vec<u8>>>>;

pub(super) enum PackageStoreBackend {
    Disk(PackageStore),
    InMemory(Mutex<InMemoryRevisionMap>),
}

pub(super) enum PackageLeaseContent {
    Disk(PathBuf),
    InMemory(Arc<BTreeMap<String, Vec<u8>>>),
}

impl PackageStoreBackend {
    pub(super) fn transaction(&self) -> Result<Option<store::PackageStoreLock>> {
        match self {
            Self::Disk(store) => Ok(Some(store.transaction()?)),
            Self::InMemory(_) => Ok(None),
        }
    }

    pub(super) fn load(&self) -> Result<Option<PackageCatalog>> {
        match self {
            Self::Disk(store) => Ok(Some(store.load()?)),
            Self::InMemory(_) => Ok(None),
        }
    }

    pub(super) fn persist_catalog(&self, catalog: &PackageCatalog) -> Result<()> {
        match self {
            Self::Disk(store) => store.persist_catalog(catalog),
            Self::InMemory(_) => Ok(()),
        }
    }

    pub(super) fn verify_revision(&self, record: &PackageRecord) -> Result<()> {
        match self {
            Self::Disk(store) => store.verify_revision(record),
            Self::InMemory(revisions) => {
                let guard = revisions.lock().expect("in-memory revisions poisoned");
                ensure!(
                    guard.contains_key(&(record.id.clone(), record.revision.clone())),
                    "in-memory package revision missing for `{}`",
                    record.id
                );
                Ok(())
            }
        }
    }

    pub(super) fn lease(
        &self,
        package_id: &str,
        revision: &str,
    ) -> Result<Option<store::PackageLease>> {
        match self {
            Self::Disk(store) => Ok(Some(store.lease(package_id, revision)?)),
            Self::InMemory(_) => Ok(None),
        }
    }

    pub(super) fn materialize(
        &self,
        package_id: &str,
        revision: &str,
        snapshot: &PackageSnapshot,
    ) -> Result<MaterializationManifest> {
        match self {
            Self::Disk(store) => {
                PackageMaterializer::new(store.root()).materialize(package_id, revision, snapshot)
            }
            Self::InMemory(revisions) => {
                let (files, manifest) = materializer::materialize_in_memory(snapshot)?;
                revisions
                    .lock()
                    .expect("in-memory revisions poisoned")
                    .insert(
                        (package_id.to_string(), revision.to_string()),
                        Arc::new(files),
                    );
                Ok(manifest)
            }
        }
    }

    pub(super) fn lease_content(
        &self,
        package_id: &str,
        revision: &str,
    ) -> Result<PackageLeaseContent> {
        match self {
            Self::Disk(store) => Ok(PackageLeaseContent::Disk(
                store.revision_root(package_id, revision).join("content"),
            )),
            Self::InMemory(revisions) => {
                let files = revisions
                    .lock()
                    .expect("in-memory revisions poisoned")
                    .get(&(package_id.to_string(), revision.to_string()))
                    .cloned()
                    .with_context(|| format!("in-memory package `{package_id}` missing"))?;
                Ok(PackageLeaseContent::InMemory(files))
            }
        }
    }

    pub(super) fn stage_package_revisions(
        &self,
        package_id: &str,
    ) -> Result<Option<store::StagedPackageRevisions>> {
        match self {
            Self::Disk(store) => store.stage_package_revisions(package_id),
            Self::InMemory(_) => Ok(None),
        }
    }

    pub(super) fn rollback_staged_package_revisions(
        &self,
        staged: Option<&store::StagedPackageRevisions>,
        error: anyhow::Error,
    ) -> Result<()> {
        match self {
            Self::Disk(store) => store.rollback_staged_package_revisions(staged, error),
            Self::InMemory(_) => Err(error),
        }
    }

    pub(super) fn discard_staged_package_revisions(
        &self,
        staged: Option<store::StagedPackageRevisions>,
    ) {
        if let Self::Disk(store) = self {
            store.discard_staged_package_revisions(staged);
        }
    }

    pub(super) fn remove_in_memory_revisions(&self, package_id: &str) {
        if let Self::InMemory(revisions) = self {
            revisions
                .lock()
                .expect("in-memory revisions poisoned")
                .retain(|(id, _), _| id != package_id);
        }
    }

    pub(super) fn gc_unreferenced_revisions(&self, catalog: &PackageCatalog) -> Result<()> {
        match self {
            Self::Disk(store) => store.gc_unreferenced_revisions(catalog),
            Self::InMemory(revisions) => {
                let referenced: BTreeSet<(String, String)> = catalog
                    .packages
                    .values()
                    .map(|record| (record.id.clone(), record.revision.clone()))
                    .collect();
                revisions
                    .lock()
                    .expect("in-memory revisions poisoned")
                    .retain(|key, _| referenced.contains(key));
                Ok(())
            }
        }
    }

    pub(super) fn gc_package_revisions(
        &self,
        package_id: &str,
        catalog: &PackageCatalog,
    ) -> Result<()> {
        match self {
            Self::Disk(store) => store.gc_package_revisions(package_id, catalog),
            Self::InMemory(revisions) => {
                let active = catalog
                    .packages
                    .get(package_id)
                    .map(|record| record.revision.clone());
                revisions
                    .lock()
                    .expect("in-memory revisions poisoned")
                    .retain(|(id, rev), _| {
                        id != package_id || active.as_deref() == Some(rev.as_str())
                    });
                Ok(())
            }
        }
    }

    #[cfg(test)]
    pub(super) fn root(&self) -> &Path {
        match self {
            Self::Disk(store) => store.root(),
            Self::InMemory(_) => panic!("in-memory package store has no filesystem root"),
        }
    }

    #[cfg(test)]
    pub(super) fn revision_root(&self, package_id: &str, revision: &str) -> PathBuf {
        match self {
            Self::Disk(store) => store.revision_root(package_id, revision),
            Self::InMemory(_) => panic!("in-memory package store has no filesystem revision_root"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{PackageCommand, PackageService, PackageSnapshot, PackageSnapshotEntry};

    fn snapshot_for(name: &str) -> PackageSnapshot {
        PackageSnapshot {
            source_name: name.to_string(),
            entries: vec![PackageSnapshotEntry {
                path: "SKILL.md".to_string(),
                bytes: format!("---\nname: {name}\ndescription: test\n---\nbody\n").into_bytes(),
                executable: false,
            }],
        }
    }

    #[test]
    fn in_memory_uninstall_and_final_lease_release_reclaim_revisions() {
        let service = PackageService::in_memory();
        let installed = service
            .execute(PackageCommand::Install {
                request_id: "req-install".to_string(),
                package_id: "temp-pack".to_string(),
                snapshot: snapshot_for("temp-pack"),
            })
            .unwrap();
        assert!(installed.success);

        let lease = service.acquire("temp-pack").unwrap();
        let uninstall = service
            .execute(PackageCommand::Uninstall {
                request_id: "req-uninstall".to_string(),
                package_id: "temp-pack".to_string(),
            })
            .unwrap();
        assert_eq!(uninstall.message, "retiring");

        // While the lease is held, the revision remains in memory.
        if let PackageStoreBackend::InMemory(revisions) = &service.store {
            assert_eq!(revisions.lock().unwrap().len(), 1);
        }

        drop(lease);

        // Once the last lease drops, the retiring package's revisions are reclaimed.
        if let PackageStoreBackend::InMemory(revisions) = &service.store {
            assert!(revisions.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn in_memory_install_rejects_file_ancestor_collisions() {
        let service = PackageService::in_memory();
        let err = service
            .execute(PackageCommand::Install {
                request_id: "req-collide".to_string(),
                package_id: "collide-pack".to_string(),
                snapshot: PackageSnapshot {
                    source_name: "collide-pack".to_string(),
                    entries: vec![
                        PackageSnapshotEntry {
                            path: "SKILL.md".to_string(),
                            bytes: b"---\nname: collide-pack\ndescription: test\n---\nbody\n"
                                .to_vec(),
                            executable: false,
                        },
                        PackageSnapshotEntry {
                            path: "a".to_string(),
                            bytes: b"file".to_vec(),
                            executable: false,
                        },
                        PackageSnapshotEntry {
                            path: "a/b/c".to_string(),
                            bytes: b"nested".to_vec(),
                            executable: false,
                        },
                    ],
                },
            })
            .unwrap_err();
        assert!(
            err.to_string().contains("ancestor file"),
            "unexpected error: {err}"
        );
    }
}
