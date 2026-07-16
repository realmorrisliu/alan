//! Durable Package Store ownership, recovery, catalog commits, and revision GC.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use alan_agent_engine::skills::{SkillCompatibility, validate_skill_compatibility};
use anyhow::{Context, Result, bail, ensure};

use super::fs_safety::ensure_owned_directory;
use super::materializer::verify_materialized_revision;
use super::{
    MATERIALIZER_VERSION, MAX_PACKAGES, PackageCatalog, PackageRecord, PackageState,
    validate_package_id,
};

pub(super) struct PackageStore {
    root: PathBuf,
    _lock: PackageStoreLock,
}

impl PackageStore {
    pub(super) fn open(store_root: PathBuf) -> Result<(Self, PackageCatalog)> {
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
        Ok((
            Self {
                root: store_root,
                _lock: store_lock,
            },
            catalog,
        ))
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn persist_catalog(&self, catalog: &PackageCatalog) -> Result<()> {
        persist_catalog(&self.root, catalog)
    }

    pub(super) fn verify_revision(&self, record: &PackageRecord) -> Result<()> {
        verify_revision(&self.root, record)
    }

    pub(super) fn revision_root(&self, package_id: &str, revision: &str) -> PathBuf {
        revision_root(&self.root, package_id, revision)
    }

    pub(super) fn stage_package_revisions(
        &self,
        package_id: &str,
    ) -> Result<Option<StagedPackageRevisions>> {
        stage_package_revisions(&self.root, package_id)
    }

    pub(super) fn rollback_staged_package_revisions(
        &self,
        staged: Option<&StagedPackageRevisions>,
        catalog_error: anyhow::Error,
    ) -> Result<()> {
        rollback_staged_package_revisions(staged, catalog_error)
    }

    pub(super) fn discard_staged_package_revisions(&self, staged: Option<StagedPackageRevisions>) {
        discard_staged_package_revisions(staged);
    }

    pub(super) fn gc_unreferenced_revisions(
        &self,
        catalog: &PackageCatalog,
        leases: &BTreeMap<u64, (String, String)>,
    ) -> Result<()> {
        gc_unreferenced_store_revisions(&self.root, catalog, leases)
    }

    pub(super) fn gc_package_revisions(
        &self,
        package_id: &str,
        catalog: &PackageCatalog,
        leases: &BTreeMap<u64, (String, String)>,
    ) -> Result<()> {
        gc_one_package_revisions(&self.root, package_id, catalog, leases)
    }
}

#[cfg(test)]
pub(super) fn persist_catalog_at(root: &Path, catalog: &PackageCatalog) -> Result<()> {
    persist_catalog(root, catalog)
}

#[cfg(test)]
pub(super) fn revision_root_at(root: &Path, package_id: &str, revision: &str) -> PathBuf {
    revision_root(root, package_id, revision)
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

pub(super) struct StagedPackageRevisions {
    pub(super) active: PathBuf,
    pub(super) staged: PathBuf,
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
