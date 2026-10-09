//! Read-only validation for Host-directed installation adoption.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, ensure};

use super::fs_safety::{ensure_owned_directory, ensure_owned_file};
use super::materializer::verify_materialized_revision;
use super::{PackageCatalog, store, validate_package_id};

/// Validate a stopped Package Store without opening its recovering service.
///
/// The Host must separately exclude source writers throughout staging/publication.
/// This function performs no recovery, garbage collection, or source modification.
pub fn validate_package_store_for_migration(root: &Path) -> Result<PackageCatalog> {
    ensure_owned_directory(root, "Package Store root is not a real directory")?;
    let mut locks = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        match name.to_str() {
            Some("catalog.json") => ensure_owned_file(&path, "package catalog is not a file")?,
            Some("store.lock") => {
                ensure_owned_file(&path, "package store lock is not a file")?;
                let file = File::open(&path)?;
                file.try_lock_shared()
                    .context("Package Store has an active writer")?;
                locks.push(file);
            }
            Some("revisions" | "staging" | "leases") => {
                ensure_owned_directory(&path, "package subtree is not a real directory")?;
                if name == "staging" {
                    ensure!(
                        fs::read_dir(path)?.next().is_none(),
                        "Package Store has unfinished staging; recover it before migration"
                    );
                }
            }
            _ => anyhow::bail!(
                "Package Store contains an unsupported entry: {}",
                path.display()
            ),
        }
    }
    let catalog = store::load_catalog(root)?;
    let revisions = root.join("revisions");
    if revisions.exists() {
        for package in fs::read_dir(revisions)? {
            let package = package?;
            ensure_owned_directory(
                &package.path(),
                "package revision parent is not a directory",
            )?;
            validate_package_id(
                package
                    .file_name()
                    .to_str()
                    .context("package id is not UTF-8")?,
            )?;
            for revision in fs::read_dir(package.path())? {
                let revision = revision?;
                let name = revision.file_name();
                let name = name.to_str().context("package revision is not UTF-8")?;
                store::validate_revision_id(name)?;
                ensure_owned_directory(
                    &revision.path(),
                    "package revision is not a real directory",
                )?;
                for entry in fs::read_dir(revision.path())? {
                    let entry = entry?;
                    ensure!(
                        matches!(
                            entry.file_name().to_str(),
                            Some("source" | "content" | "source-name.json" | "manifest.json")
                        ),
                        "package revision contains unsupported content"
                    );
                }
                verify_materialized_revision(&revision.path(), name)?;
            }
        }
    }
    store::verify_catalog(root, &catalog)?;
    let leases = root.join("leases");
    if leases.exists() {
        for entry in fs::read_dir(leases)? {
            let path = entry?.path();
            ensure_owned_file(&path, "package lease is not a file")?;
            let mut file = File::open(path)?;
            file.try_lock()
                .context("Package Store has an active reference")?;
            let mut bytes = Vec::new();
            Read::by_ref(&mut file).take(1025).read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 1024, "package lease is oversized");
            let (package, revision): (String, String) = serde_json::from_slice(&bytes)?;
            validate_package_id(&package)?;
            store::validate_revision_id(&revision)?;
            verify_materialized_revision(
                &root.join("revisions").join(package).join(&revision),
                &revision,
            )?;
            locks.push(file);
        }
    }
    Ok(catalog)
}
