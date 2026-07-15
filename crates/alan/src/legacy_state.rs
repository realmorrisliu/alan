//! Bounded migration and cleanup for state created by the retired Host-directory model.
//!
//! This module is deliberately the only production owner of the former `.alan`,
//! `.alan-dev`, `.agents`, and `.agents-dev` paths. It probes fixed paths only;
//! it never discovers repositories or authored trees recursively.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use alan_agent_engine::InstallChannel;
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

use alan_os_host::{
    ConnectionMigrationReport, HostStorePaths, LegacyConnectionPaths, SystemStorePaths,
};

const LEGACY_STABLE_HOME: &str = ".alan";
const LEGACY_DEV_HOME: &str = ".alan-dev";
const LEGACY_STABLE_PUBLIC_SKILLS: &str = ".agents";
const LEGACY_DEV_PUBLIC_SKILLS: &str = ".agents-dev";
const SECRET_STORE_FILE: &str = "secrets.toml";

/// Fixed legacy roots for one channel and one Host user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyStatePaths {
    pub channel: InstallChannel,
    pub home_dir: PathBuf,
    pub alan_root: PathBuf,
    pub public_skills_root: PathBuf,
}

impl LegacyStatePaths {
    pub fn detect(channel: InstallChannel) -> Result<Option<Self>> {
        dirs::home_dir()
            .map(|home| Self::from_home_dir(&home, channel))
            .transpose()
    }

    pub fn from_home_dir(home_dir: &Path, channel: InstallChannel) -> Result<Self> {
        validate_absolute_path("Host home directory", home_dir)?;
        let (alan_home, public_skills) = match channel {
            InstallChannel::Stable => (LEGACY_STABLE_HOME, LEGACY_STABLE_PUBLIC_SKILLS),
            InstallChannel::Dev => (LEGACY_DEV_HOME, LEGACY_DEV_PUBLIC_SKILLS),
        };
        Ok(Self {
            channel,
            home_dir: home_dir.to_path_buf(),
            alan_root: home_dir.join(alan_home),
            public_skills_root: home_dir.join(public_skills).join("skills"),
        })
    }

    pub fn connections_metadata(&self) -> PathBuf {
        self.alan_root.join("connections.toml")
    }

    pub fn credential_file(&self) -> PathBuf {
        self.alan_root.join("credentials").join(SECRET_STORE_FILE)
    }

    pub fn managed_auth(&self) -> PathBuf {
        self.alan_root.join("auth.json")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoredRootKind {
    AgentDefinitions,
    Persona,
    Policy,
    Skills,
    MemoryStore,
    LegacyConfiguration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AuthoredRoot {
    pub kind: AuthoredRootKind,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LegacyInspection {
    pub generated_paths: Vec<PathBuf>,
    pub migratable_paths: Vec<PathBuf>,
    pub authored_roots: Vec<AuthoredRoot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LegacyCleanupReport {
    pub connection_migration: ConnectionMigrationReport,
    pub removed_generated_paths: Vec<PathBuf>,
    pub authored_roots: Vec<AuthoredRoot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoredImportKind {
    AgentDefinition,
    MemoryStore,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AuthoredImportReport {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_deleted: bool,
}

/// Inspect fixed legacy locations plus explicit project roots without recursively
/// discovering any other Host directory.
pub fn inspect_legacy_state(
    paths: &LegacyStatePaths,
    explicit_source_roots: &[PathBuf],
) -> Result<LegacyInspection> {
    let mut alan_roots = vec![paths.alan_root.clone()];
    let mut public_skill_roots = vec![paths.public_skills_root.clone()];
    for source in explicit_source_roots {
        validate_absolute_path("explicit legacy source root", source)?;
        alan_roots.push(explicit_alan_root(source, paths.channel));
        public_skill_roots.push(explicit_public_skills_root(source, paths.channel));
    }
    alan_roots.sort();
    alan_roots.dedup();
    public_skill_roots.sort();
    public_skill_roots.dedup();

    let mut inspection = LegacyInspection::default();
    for alan_root in &alan_roots {
        ensure_real_legacy_root_or_missing(alan_root)?;
        inspection
            .generated_paths
            .extend(existing_generated_paths(alan_root)?);
        inspection
            .authored_roots
            .extend(existing_authored_roots(alan_root)?);
    }
    for skills_root in public_skill_roots {
        let skills_owner = skills_root
            .parent()
            .context("legacy public Skills path has no owner root")?;
        ensure_real_legacy_root_or_missing(skills_owner)?;
        ensure_no_symlinked_parent(skills_owner, &skills_root)?;
        if path_exists_without_following(&skills_root)? {
            inspection.authored_roots.push(AuthoredRoot {
                kind: AuthoredRootKind::Skills,
                path: skills_root,
            });
        }
    }
    for path in [
        paths.connections_metadata(),
        paths.credential_file(),
        paths.managed_auth(),
    ] {
        if path_exists_without_following(&path)? {
            inspection.migratable_paths.push(path);
        }
    }

    inspection.generated_paths.sort();
    inspection.generated_paths.dedup();
    inspection.migratable_paths.sort();
    inspection.migratable_paths.dedup();
    inspection.authored_roots.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    inspection
        .authored_roots
        .dedup_by(|left, right| left.kind == right.kind && left.path == right.path);
    Ok(inspection)
}

/// Migrate channel connection state, delete only fixed generated paths, and
/// return every authored root untouched.
pub fn cleanup_legacy_state(
    paths: &LegacyStatePaths,
    system_store: &SystemStorePaths,
    host_store: &HostStorePaths,
    explicit_source_roots: &[PathBuf],
) -> Result<LegacyCleanupReport> {
    let inspection = inspect_legacy_state(paths, explicit_source_roots)?;
    let connection_migration = migrate_legacy_connections(paths, system_store, host_store)?;

    let mut removed_generated_paths = Vec::new();
    let mut roots = vec![paths.alan_root.clone()];
    for source in explicit_source_roots {
        roots.push(explicit_alan_root(source, paths.channel));
    }
    roots.sort();
    roots.dedup();
    for root in roots {
        for candidate in existing_generated_paths(&root)? {
            remove_owned_generated_path(&root, &candidate)?;
            removed_generated_paths.push(candidate);
        }
        prune_empty_generated_parents(&root)?;
    }
    removed_generated_paths.sort();
    removed_generated_paths.dedup();

    Ok(LegacyCleanupReport {
        connection_migration,
        removed_generated_paths,
        authored_roots: inspection.authored_roots,
    })
}

fn migrate_legacy_connections(
    paths: &LegacyStatePaths,
    system_store: &SystemStorePaths,
    host_store: &HostStorePaths,
) -> Result<ConnectionMigrationReport> {
    alan_os_host::migrate_legacy_connections(
        &LegacyConnectionPaths::from_home_dir(&paths.home_dir, paths.channel)?,
        system_store,
        host_store,
    )
}

pub fn import_authored_content(
    kind: AuthoredImportKind,
    source: &Path,
    name: &str,
    delete_source: bool,
    system_store: &SystemStorePaths,
) -> Result<AuthoredImportReport> {
    validate_import_name(name)?;
    validate_absolute_path("authored import source", source)?;
    ensure_import_source_has_no_symlinked_ancestors(source)?;
    let source_metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect import source {}", source.display()))?;
    ensure!(
        source_metadata.file_type().is_dir() && !source_metadata.file_type().is_symlink(),
        "authored import source must be a real directory: {}",
        source.display()
    );
    validate_import_shape(kind, source)?;

    let canonical_source = fs::canonicalize(source)
        .with_context(|| format!("failed to resolve import source {}", source.display()))?;
    ensure!(
        canonical_source.parent().is_some(),
        "authored import source must not be a filesystem root"
    );
    let canonical_store = canonicalize_prospective_path(&system_store.root)?;
    ensure!(
        !canonical_source.starts_with(&canonical_store)
            && !canonical_store.starts_with(&canonical_source),
        "import source and System Store must not overlap"
    );
    fs::create_dir_all(&system_store.root).with_context(|| {
        format!(
            "failed to create System Store {}",
            system_store.root.display()
        )
    })?;

    let destination_parent = match kind {
        AuthoredImportKind::AgentDefinition => system_store.agent_definitions()?,
        AuthoredImportKind::MemoryStore => system_store.memory_stores()?,
    };
    fs::create_dir_all(&destination_parent).with_context(|| {
        format!(
            "failed to create import destination {}",
            destination_parent.display()
        )
    })?;
    let destination = destination_parent.join(name);
    ensure!(
        !path_exists_without_following(&destination)?,
        "import destination already exists: {}",
        destination.display()
    );

    let staging = destination_parent.join(format!(
        ".{name}.import-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let staged = (|| {
        copy_tree_rejecting_symlinks(&canonical_source, &staging)?;
        let source_fingerprint = tree_fingerprint(&canonical_source)?;
        ensure!(
            tree_fingerprint(&staging)? == source_fingerprint,
            "import verification failed before install"
        );
        Ok::<_, anyhow::Error>(source_fingerprint)
    })();
    let source_fingerprint = match staged {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error).with_context(|| {
            format!(
                "failed to install imported content at {}",
                destination.display()
            )
        });
    }
    if tree_fingerprint(&destination)? != source_fingerprint {
        let _ = fs::remove_dir_all(&destination);
        bail!("import verification failed after install");
    }

    if delete_source {
        remove_import_source_if_unchanged(&canonical_source, &source_fingerprint)?;
    }

    Ok(AuthoredImportReport {
        source: canonical_source,
        destination,
        source_deleted: delete_source,
    })
}

fn ensure_import_source_has_no_symlinked_ancestors(source: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    let mut normal_depth = 0usize;
    let components = source.components().collect::<Vec<_>>();

    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component.as_os_str());
        if matches!(*component, Component::Normal(_)) {
            normal_depth += 1;
        }
        let Some(metadata) = optional_symlink_metadata(&current)? else {
            continue;
        };
        if !metadata.file_type().is_symlink() {
            continue;
        }

        // macOS exposes platform-owned root aliases such as /var and /tmp.
        // Rebase that first component, but reject every deeper symlink where a
        // Host user could redirect an import or its --delete-source target.
        ensure!(
            normal_depth == 1,
            "authored import source contains symlinked path component: {}",
            current.display()
        );
        current = fs::canonicalize(&current).with_context(|| {
            format!(
                "failed to resolve platform root alias in import source {}",
                current.display()
            )
        })?;
    }
    Ok(())
}

fn remove_import_source_if_unchanged(source: &Path, expected_fingerprint: &[u8]) -> Result<()> {
    ensure!(
        tree_fingerprint(source)? == expected_fingerprint,
        "import source changed after verification; installed content was kept and source was not deleted: {}",
        source.display()
    );
    fs::remove_dir_all(source).with_context(|| {
        format!(
            "import succeeded but source deletion failed for {}",
            source.display()
        )
    })
}

fn explicit_alan_root(source: &Path, channel: InstallChannel) -> PathBuf {
    match source.file_name().and_then(|name| name.to_str()) {
        Some(LEGACY_STABLE_HOME | LEGACY_DEV_HOME) => source.to_path_buf(),
        _ => source.join(match channel {
            InstallChannel::Stable => LEGACY_STABLE_HOME,
            InstallChannel::Dev => LEGACY_DEV_HOME,
        }),
    }
}

fn explicit_public_skills_root(source: &Path, channel: InstallChannel) -> PathBuf {
    match source.file_name().and_then(|name| name.to_str()) {
        Some("skills") => source.to_path_buf(),
        Some(LEGACY_STABLE_PUBLIC_SKILLS | LEGACY_DEV_PUBLIC_SKILLS) => source.join("skills"),
        _ => source
            .join(match channel {
                InstallChannel::Stable => LEGACY_STABLE_PUBLIC_SKILLS,
                InstallChannel::Dev => LEGACY_DEV_PUBLIC_SKILLS,
            })
            .join("skills"),
    }
}

fn existing_generated_paths(alan_root: &Path) -> Result<Vec<PathBuf>> {
    let mut candidates = vec![
        alan_root.join("registry.json"),
        alan_root.join("registry.json.bak"),
        alan_root.join("registry.json.tmp"),
        alan_root.join("daemon.pid"),
        alan_root.join("sessions"),
        alan_root.join("tasks"),
        alan_root.join("rollouts"),
        alan_root.join("checkpoints"),
        alan_root.join("cache"),
        alan_root.join("tmp"),
        alan_root.join("shell-restore"),
        alan_root.join("metadata"),
    ];
    for channel in ["stable", "dev"] {
        for generated in [
            "sessions",
            "rollouts",
            "checkpoints",
            "cache",
            "tmp",
            "shell-restore",
            "metadata",
        ] {
            candidates.push(alan_root.join("runtime").join(channel).join(generated));
        }
    }
    let mut existing = Vec::new();
    for path in candidates {
        ensure_no_symlinked_parent(alan_root, &path)?;
        if path_exists_without_following(&path)? {
            existing.push(path);
        }
    }
    Ok(existing)
}

fn existing_authored_roots(alan_root: &Path) -> Result<Vec<AuthoredRoot>> {
    let mut candidates = vec![
        (AuthoredRootKind::AgentDefinitions, alan_root.join("agents")),
        (
            AuthoredRootKind::Persona,
            alan_root.join("agents/default/persona"),
        ),
        (
            AuthoredRootKind::Policy,
            alan_root.join("agents/default/policy.yaml"),
        ),
        (
            AuthoredRootKind::Skills,
            alan_root.join("agents/default/skills"),
        ),
        (AuthoredRootKind::MemoryStore, alan_root.join("memory")),
        (
            AuthoredRootKind::LegacyConfiguration,
            alan_root.join("host.toml"),
        ),
        (
            AuthoredRootKind::LegacyConfiguration,
            alan_root.join("models.toml"),
        ),
    ];
    for channel in ["stable", "dev"] {
        candidates.push((
            AuthoredRootKind::MemoryStore,
            alan_root.join("runtime").join(channel).join("memory"),
        ));
    }
    let mut existing = Vec::new();
    for (kind, path) in candidates {
        ensure_no_symlinked_parent(alan_root, &path)?;
        if path_exists_without_following(&path)? {
            existing.push(AuthoredRoot { kind, path });
        }
    }
    Ok(existing)
}

fn remove_owned_generated_path(root: &Path, candidate: &Path) -> Result<()> {
    validate_owned_descendant(root, candidate)?;
    ensure_no_symlinked_parent(root, candidate)?;
    let Some(metadata) = optional_symlink_metadata(candidate)? else {
        return Ok(());
    };
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(candidate)
            .with_context(|| format!("failed to delete generated path {}", candidate.display()))
    } else {
        fs::remove_file(candidate)
            .with_context(|| format!("failed to delete generated path {}", candidate.display()))
    }
}

fn ensure_no_symlinked_parent(root: &Path, candidate: &Path) -> Result<()> {
    ensure_real_legacy_root_or_missing(root)?;
    let relative = candidate
        .strip_prefix(root)
        .context("generated path escaped its legacy root")?;
    let mut current = root.to_path_buf();
    let components: Vec<_> = relative.components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(component) = component else {
            bail!("generated path contains a non-normal component")
        };
        current.push(component);
        if let Some(metadata) = optional_symlink_metadata(&current)? {
            ensure!(
                !metadata.file_type().is_symlink(),
                "refusing generated cleanup through symlinked parent {}",
                current.display()
            );
        }
    }
    Ok(())
}

fn ensure_real_legacy_root_or_missing(root: &Path) -> Result<()> {
    let Some(metadata) = optional_symlink_metadata(root)? else {
        return Ok(());
    };
    ensure!(
        metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
        "refusing to traverse non-directory or symlinked legacy root {}",
        root.display()
    );
    Ok(())
}

fn prune_empty_generated_parents(root: &Path) -> Result<()> {
    for candidate in [
        root.join("runtime/stable"),
        root.join("runtime/dev"),
        root.join("runtime"),
    ] {
        match fs::remove_dir(&candidate) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to prune generated directory {}",
                        candidate.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn validate_import_shape(kind: AuthoredImportKind, source: &Path) -> Result<()> {
    match kind {
        AuthoredImportKind::AgentDefinition => ensure!(
            ["agent.toml", "persona", "skills", "policy.yaml"]
                .iter()
                .any(|entry| source.join(entry).exists()),
            "Agent Definition import must contain agent.toml, persona, skills, or policy.yaml"
        ),
        AuthoredImportKind::MemoryStore => {}
    }
    Ok(())
}

fn validate_import_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.chars().all(|character| character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'),
        "import name must use lowercase ASCII letters, digits, and hyphens"
    );
    Ok(())
}

fn copy_tree_rejecting_symlinks(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect import path {}", source.display()))?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "authored import contains a symlink: {}",
        source.display()
    );
    ensure!(
        metadata.file_type().is_dir(),
        "authored import contains unsupported file type: {}",
        source.display()
    );
    fs::create_dir(destination).with_context(|| {
        format!(
            "failed to create staged import directory {}",
            destination.display()
        )
    })?;
    let mut entries = fs::read_dir(source)
        .with_context(|| format!("failed to read import directory {}", source.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "authored import contains a symlink: {}",
            source_path.display()
        );
        if metadata.file_type().is_dir() {
            copy_tree_rejecting_symlinks(&source_path, &destination_path)?;
        } else if metadata.file_type().is_file() {
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!("failed to copy authored file {}", source_path.display())
            })?;
            fs::set_permissions(&destination_path, metadata.permissions()).with_context(|| {
                format!(
                    "failed to preserve authored file permissions {}",
                    destination_path.display()
                )
            })?;
        } else {
            bail!(
                "authored import contains unsupported file type: {}",
                source_path.display()
            );
        }
    }
    Ok(())
}

fn tree_fingerprint(root: &Path) -> Result<Vec<u8>> {
    let mut hasher = Sha256::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "cannot verify a tree containing symlinks: {}",
            path.display()
        );
        let relative = path.strip_prefix(root).unwrap_or(Path::new(""));
        let relative = relative.to_string_lossy();
        hasher.update((relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        if metadata.file_type().is_dir() {
            hasher.update(b"directory");
            let mut entries = fs::read_dir(&path)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(fs::DirEntry::file_name);
            for entry in entries.into_iter().rev() {
                pending.push(entry.path());
            }
        } else if metadata.file_type().is_file() {
            hasher.update(b"file");
            hasher.update(metadata.len().to_le_bytes());
            hasher.update(fs::read(&path)?);
        } else {
            bail!("cannot verify unsupported file type: {}", path.display());
        }
    }
    Ok(hasher.finalize().to_vec())
}

fn validate_owned_descendant(root: &Path, candidate: &Path) -> Result<()> {
    validate_absolute_path("legacy root", root)?;
    validate_absolute_path("generated path", candidate)?;
    ensure!(
        candidate != root && candidate.starts_with(root),
        "generated cleanup path escaped its legacy root: {}",
        candidate.display()
    );
    Ok(())
}

fn validate_absolute_path(label: &str, path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "{label} must be absolute: {}",
        path.display()
    );
    ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir)),
        "{label} must not contain relative components: {}",
        path.display()
    );
    Ok(())
}

fn canonicalize_prospective_path(path: &Path) -> Result<PathBuf> {
    let mut ancestor = path;
    while optional_symlink_metadata(ancestor)?.is_none() {
        ancestor = ancestor.parent().context("path has no existing ancestor")?;
    }
    let canonical_ancestor = fs::canonicalize(ancestor)
        .with_context(|| format!("failed to resolve path ancestor {}", ancestor.display()))?;
    Ok(canonical_ancestor.join(path.strip_prefix(ancestor)?))
}

fn optional_symlink_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn path_exists_without_following(path: &Path) -> Result<bool> {
    Ok(optional_symlink_metadata(path)?.is_some())
}

#[cfg(test)]
mod tests;
