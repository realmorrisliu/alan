//! Bounded migration and cleanup for state created by the retired Host-directory model.
//!
//! This module is deliberately the only production owner of the former `.alan`,
//! `.alan-dev`, `.agents`, and `.agents-dev` paths. It probes fixed paths only;
//! it never discovers repositories or authored trees recursively.

use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use alan_agent_engine::{ConnectionsFile, InstallChannel};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::system_store::{HostStorePaths, SystemStorePaths};

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
pub struct ConnectionMigrationReport {
    pub metadata_migrated: bool,
    pub credential_file_migrated: bool,
    pub managed_auth_migrated: bool,
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
    Skill,
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
        alan_roots.push(explicit_alan_root(source));
        public_skill_roots.push(explicit_public_skills_root(source));
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
        roots.push(explicit_alan_root(source));
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

/// Perform the one-shot migrate-verify-delete pass needed before reading the
/// current Connection Service store.
pub fn migrate_legacy_connections(
    paths: &LegacyStatePaths,
    system_store: &SystemStorePaths,
    host_store: &HostStorePaths,
) -> Result<ConnectionMigrationReport> {
    ensure_real_legacy_root_or_missing(&paths.alan_root)?;
    ensure!(
        paths.channel == system_store.channel,
        "legacy and System Store channels differ"
    );

    // Preserve secret-bearing Host files before deleting any metadata pointer.
    let credential_file_migrated = migrate_host_file(
        &paths.credential_file(),
        &host_store.credentials.join(SECRET_STORE_FILE),
        "Host credential file",
    )?;
    let managed_auth_migrated = migrate_host_file(
        &paths.managed_auth(),
        &host_store.managed_auth,
        "managed auth file",
    )?;
    let metadata_migrated = migrate_connection_metadata(
        &paths.connections_metadata(),
        &system_store.connections_metadata()?,
    )?;

    Ok(ConnectionMigrationReport {
        metadata_migrated,
        credential_file_migrated,
        managed_auth_migrated,
    })
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
    let source_metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect import source {}", source.display()))?;
    ensure!(
        source_metadata.file_type().is_dir() && !source_metadata.file_type().is_symlink(),
        "authored import source must be a real directory: {}",
        source.display()
    );
    validate_import_shape(kind, source)?;

    let destination_parent = match kind {
        AuthoredImportKind::AgentDefinition => system_store.agent_definitions()?,
        AuthoredImportKind::Skill => system_store.imported_skills()?,
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

    let canonical_source = fs::canonicalize(source)
        .with_context(|| format!("failed to resolve import source {}", source.display()))?;
    ensure!(
        canonical_source.parent().is_some(),
        "authored import source must not be a filesystem root"
    );
    let canonical_store = fs::canonicalize(&system_store.root).with_context(|| {
        format!(
            "failed to resolve System Store {}",
            system_store.root.display()
        )
    })?;
    ensure!(
        !canonical_source.starts_with(&canonical_store)
            && !canonical_store.starts_with(&canonical_source),
        "import source and System Store must not overlap"
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
        fs::remove_dir_all(&canonical_source).with_context(|| {
            format!(
                "import succeeded but source deletion failed for {}",
                canonical_source.display()
            )
        })?;
    }

    Ok(AuthoredImportReport {
        source: canonical_source,
        destination,
        source_deleted: delete_source,
    })
}

fn migrate_connection_metadata(source: &Path, target: &Path) -> Result<bool> {
    let Some(source_metadata) = optional_symlink_metadata(source)? else {
        return Ok(false);
    };
    ensure!(
        source_metadata.file_type().is_file() && !source_metadata.file_type().is_symlink(),
        "legacy connection metadata must be a real file: {}",
        source.display()
    );
    let (legacy, _) = ConnectionsFile::load_from_path(source)?;
    let (current, _) = ConnectionsFile::load_from_path(target)?;
    let merged = merge_connections(current, legacy)?;

    if !target.is_file() || ConnectionsFile::load_from_path(target)?.0 != merged {
        save_connections_atomically(&merged, target)?;
    }
    let verified = ConnectionsFile::load_from_path(target)?.0;
    ensure!(
        verified == merged,
        "Connection Service verification failed at {}",
        target.display()
    );
    fs::remove_file(source).with_context(|| {
        format!(
            "connection metadata migrated but legacy deletion failed for {}",
            source.display()
        )
    })?;
    prune_empty_parents(source.parent(), source.parent().and_then(Path::parent));
    Ok(true)
}

fn merge_connections(
    mut current: ConnectionsFile,
    legacy: ConnectionsFile,
) -> Result<ConnectionsFile> {
    match (&current.default_profile, &legacy.default_profile) {
        (None, Some(value)) => current.default_profile = Some(value.clone()),
        (Some(current_value), Some(legacy_value)) => ensure!(
            current_value == legacy_value,
            "legacy and current default connection profiles conflict (`{legacy_value}` vs `{current_value}`)"
        ),
        _ => {}
    }
    for (id, credential) in legacy.credentials {
        if let Some(existing) = current.credentials.get(&id) {
            ensure!(
                existing == &credential,
                "legacy and current credential metadata conflict for `{id}`"
            );
        } else {
            current.credentials.insert(id, credential);
        }
    }
    for (id, profile) in legacy.profiles {
        if let Some(existing) = current.profiles.get(&id) {
            ensure!(
                existing == &profile,
                "legacy and current connection profiles conflict for `{id}`"
            );
        } else {
            current.profiles.insert(id, profile);
        }
    }
    Ok(current)
}

fn save_connections_atomically(connections: &ConnectionsFile, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .context("Connection Service metadata path has no parent")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create Connection Service directory {}",
            parent.display()
        )
    })?;
    let staging = parent.join(format!(
        ".connections-migration-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    connections.save_to_path(&staging)?;
    ensure!(
        ConnectionsFile::load_from_path(&staging)?.0 == *connections,
        "staged Connection Service metadata failed verification"
    );
    fs::rename(&staging, target).with_context(|| {
        format!(
            "failed to atomically install Connection Service metadata {}",
            target.display()
        )
    })?;
    Ok(())
}

fn migrate_host_file(source: &Path, target: &Path, label: &str) -> Result<bool> {
    let Some(source_metadata) = optional_symlink_metadata(source)? else {
        return Ok(false);
    };
    ensure!(
        source_metadata.file_type().is_file() && !source_metadata.file_type().is_symlink(),
        "legacy {label} must be a real file: {}",
        source.display()
    );
    let source_bytes = fs::read(source)
        .with_context(|| format!("failed to read legacy {label} {}", source.display()))?;
    if let Some(target_metadata) = optional_symlink_metadata(target)? {
        ensure!(
            target_metadata.file_type().is_file() && !target_metadata.file_type().is_symlink(),
            "current {label} must be a real file: {}",
            target.display()
        );
        let target_bytes = fs::read(target)
            .with_context(|| format!("failed to read current {label} {}", target.display()))?;
        ensure!(
            target_bytes == source_bytes,
            "legacy and current {label} conflict; both files were preserved"
        );
    } else {
        write_sensitive_file_atomically(target, &source_bytes)?;
        ensure!(
            fs::read(target)? == source_bytes,
            "{label} verification failed at {}",
            target.display()
        );
    }
    fs::remove_file(source).with_context(|| {
        format!(
            "{label} migrated but legacy deletion failed for {}",
            source.display()
        )
    })?;
    Ok(true)
}

fn write_sensitive_file_atomically(target: &Path, bytes: &[u8]) -> Result<()> {
    let parent = target
        .parent()
        .context("Host-owned sensitive path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create Host Store directory {}", parent.display()))?;
    let staging = parent.join(format!(
        ".host-migration-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&staging)
        .with_context(|| format!("failed to stage Host Store file {}", staging.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("failed to stage Host Store file {}", staging.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync Host Store file {}", staging.display()))?;
    drop(file);
    ensure!(
        fs::read(&staging)? == bytes,
        "staged Host Store file failed verification"
    );
    fs::rename(&staging, target)
        .with_context(|| format!("failed to install Host Store file {}", target.display()))?;
    Ok(())
}

fn explicit_alan_root(source: &Path) -> PathBuf {
    match source.file_name().and_then(|name| name.to_str()) {
        Some(LEGACY_STABLE_HOME | LEGACY_DEV_HOME) => source.to_path_buf(),
        _ => source.join(LEGACY_STABLE_HOME),
    }
}

fn explicit_public_skills_root(source: &Path) -> PathBuf {
    match source.file_name().and_then(|name| name.to_str()) {
        Some("skills") => source.to_path_buf(),
        Some(LEGACY_STABLE_PUBLIC_SKILLS | LEGACY_DEV_PUBLIC_SKILLS) => source.join("skills"),
        _ => source.join(LEGACY_STABLE_PUBLIC_SKILLS).join("skills"),
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

fn prune_empty_parents(mut path: Option<&Path>, stop_before: Option<&Path>) {
    while let Some(current) = path {
        if stop_before.is_some_and(|stop| current == stop) {
            break;
        }
        if fs::remove_dir(current).is_err() {
            break;
        }
        path = current.parent();
    }
}

fn validate_import_shape(kind: AuthoredImportKind, source: &Path) -> Result<()> {
    match kind {
        AuthoredImportKind::AgentDefinition => ensure!(
            ["agent.toml", "persona", "skills", "policy.yaml"]
                .iter()
                .any(|entry| source.join(entry).exists()),
            "Agent Definition import must contain agent.toml, persona, skills, or policy.yaml"
        ),
        AuthoredImportKind::Skill => ensure!(
            source.join("SKILL.md").is_file(),
            "Skill import must contain SKILL.md"
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
mod tests {
    use super::*;
    use alan_agent_engine::{
        ConnectionProfile, LlmProvider,
        skills::{ResolvedCapabilityView, ScopedPackageDir, SkillScope, SkillsRegistry},
    };
    use chrono::Utc;
    use tempfile::TempDir;

    fn stores(root: &Path, channel: InstallChannel) -> (SystemStorePaths, HostStorePaths) {
        let data = root.join("data");
        fs::create_dir_all(&data).unwrap();
        (
            SystemStorePaths::from_data_dir(&data, channel).unwrap(),
            HostStorePaths::from_data_dir(&data, channel).unwrap(),
        )
    }

    fn connection_file(profile_id: &str) -> ConnectionsFile {
        let mut file = ConnectionsFile {
            default_profile: Some(profile_id.to_string()),
            ..ConnectionsFile::default()
        };
        file.profiles.insert(
            profile_id.to_string(),
            ConnectionProfile {
                provider: LlmProvider::OpenAiResponses,
                label: None,
                credential_id: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                source: "test".to_string(),
                settings: Default::default(),
            },
        );
        file
    }

    #[test]
    fn connection_metadata_is_merged_verified_and_deleted() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
        fs::create_dir_all(&paths.alan_root).unwrap();
        let legacy = connection_file("legacy-main");
        legacy.save_to_path(&paths.connections_metadata()).unwrap();
        let (system, host) = stores(temp.path(), InstallChannel::Stable);

        let report = migrate_legacy_connections(&paths, &system, &host).unwrap();

        assert!(report.metadata_migrated);
        assert!(!paths.connections_metadata().exists());
        assert_eq!(
            ConnectionsFile::load_from_path(&system.connections_metadata().unwrap())
                .unwrap()
                .0,
            legacy
        );
    }

    #[test]
    fn conflicting_connection_metadata_preserves_both_files() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
        fs::create_dir_all(&paths.alan_root).unwrap();
        connection_file("legacy-main")
            .save_to_path(&paths.connections_metadata())
            .unwrap();
        let (system, host) = stores(temp.path(), InstallChannel::Stable);
        connection_file("current-main")
            .save_to_path(&system.connections_metadata().unwrap())
            .unwrap();

        let error = migrate_legacy_connections(&paths, &system, &host).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("default connection profiles conflict")
        );
        assert!(paths.connections_metadata().is_file());
        assert!(system.connections_metadata().unwrap().is_file());
    }

    #[test]
    fn secrets_move_only_between_host_owned_paths() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Dev).unwrap();
        fs::create_dir_all(paths.credential_file().parent().unwrap()).unwrap();
        fs::write(paths.credential_file(), b"[secrets]\nmain = 'secret'\n").unwrap();
        fs::create_dir_all(&paths.alan_root).unwrap();
        fs::write(paths.managed_auth(), b"{\"token\":\"secret\"}").unwrap();
        let (system, host) = stores(temp.path(), InstallChannel::Dev);

        let report = migrate_legacy_connections(&paths, &system, &host).unwrap();

        assert!(report.credential_file_migrated);
        assert!(report.managed_auth_migrated);
        assert_eq!(
            fs::read(host.credentials.join(SECRET_STORE_FILE)).unwrap(),
            b"[secrets]\nmain = 'secret'\n"
        );
        assert_eq!(
            fs::read(host.managed_auth).unwrap(),
            b"{\"token\":\"secret\"}"
        );
        assert!(!system.root.join("secrets.toml").exists());
    }

    #[test]
    fn cleanup_deletes_generated_state_and_preserves_authored_content() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
        fs::create_dir_all(paths.alan_root.join("runtime/stable/rollouts")).unwrap();
        fs::write(
            paths.alan_root.join("runtime/stable/rollouts/one.jsonl"),
            "x",
        )
        .unwrap();
        fs::create_dir_all(paths.alan_root.join("runtime/stable/memory")).unwrap();
        fs::write(
            paths.alan_root.join("runtime/stable/memory/MEMORY.md"),
            "mine",
        )
        .unwrap();
        fs::create_dir_all(paths.alan_root.join("agents/default/persona")).unwrap();
        fs::write(
            paths.alan_root.join("agents/default/persona/SOUL.md"),
            "mine",
        )
        .unwrap();
        fs::write(paths.alan_root.join("registry.json"), "{}").unwrap();
        let (system, host) = stores(temp.path(), InstallChannel::Stable);

        let report = cleanup_legacy_state(&paths, &system, &host, &[]).unwrap();

        assert!(!paths.alan_root.join("runtime/stable/rollouts").exists());
        assert!(!paths.alan_root.join("registry.json").exists());
        assert!(
            paths
                .alan_root
                .join("runtime/stable/memory/MEMORY.md")
                .is_file()
        );
        assert!(
            paths
                .alan_root
                .join("agents/default/persona/SOUL.md")
                .is_file()
        );
        assert!(
            report
                .authored_roots
                .iter()
                .any(|root| root.kind == AuthoredRootKind::MemoryStore)
        );
        assert!(
            report
                .authored_roots
                .iter()
                .any(|root| root.kind == AuthoredRootKind::Persona)
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_never_traverses_symlinked_runtime_parent() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let outside = temp.path().join("outside");
        fs::create_dir_all(outside.join("stable/rollouts")).unwrap();
        fs::write(outside.join("stable/rollouts/keep"), "safe").unwrap();
        let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
        fs::create_dir_all(&paths.alan_root).unwrap();
        symlink(&outside, paths.alan_root.join("runtime")).unwrap();
        let (system, host) = stores(temp.path(), InstallChannel::Stable);

        let error = cleanup_legacy_state(&paths, &system, &host, &[]).unwrap_err();

        assert!(error.to_string().contains("symlinked parent"));
        assert!(outside.join("stable/rollouts/keep").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_never_traverses_symlinked_legacy_root() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(outside.join("runtime/stable/rollouts")).unwrap();
        fs::write(outside.join("runtime/stable/rollouts/keep"), "safe").unwrap();
        let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();
        symlink(&outside, &paths.alan_root).unwrap();
        let (system, host) = stores(temp.path(), InstallChannel::Stable);

        let error = cleanup_legacy_state(&paths, &system, &host, &[]).unwrap_err();

        assert!(error.to_string().contains("symlinked legacy root"));
        assert!(outside.join("runtime/stable/rollouts/keep").is_file());
    }

    #[test]
    fn explicit_skill_import_is_verified_and_loadable_as_installed() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("host-skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("SKILL.md"),
            "---\nname: imported-skill\ndescription: Explicitly imported\n---\nBody\n",
        )
        .unwrap();
        let (system, _) = stores(temp.path(), InstallChannel::Stable);

        let report = import_authored_content(
            AuthoredImportKind::Skill,
            &source,
            "imported-skill",
            true,
            &system,
        )
        .unwrap();
        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: system.imported_skills().unwrap(),
            scope: SkillScope::Installed,
        }]);
        let registry = SkillsRegistry::load_capability_view(&view, &[]).unwrap();

        assert!(report.source_deleted);
        assert!(!source.exists());
        assert!(report.destination.join("SKILL.md").is_file());
        assert_eq!(
            registry.get(&"imported-skill".to_string()).unwrap().scope,
            SkillScope::Installed
        );
    }

    #[cfg(unix)]
    #[test]
    fn explicit_import_rejects_symlinks_without_installing() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let source = temp.path().join("host-definition");
        fs::create_dir_all(source.join("persona")).unwrap();
        fs::write(temp.path().join("outside"), "secret").unwrap();
        symlink(temp.path().join("outside"), source.join("persona/SOUL.md")).unwrap();
        let (system, _) = stores(temp.path(), InstallChannel::Stable);

        let error = import_authored_content(
            AuthoredImportKind::AgentDefinition,
            &source,
            "default",
            false,
            &system,
        )
        .unwrap_err();

        assert!(error.to_string().contains("contains a symlink"));
        let definitions = system.agent_definitions().unwrap();
        assert!(!definitions.join("default").exists());
        assert_eq!(fs::read_dir(definitions).unwrap().count(), 0);
    }

    #[test]
    fn inspection_checks_only_fixed_and_explicit_roots() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let unrelated = temp.path().join("unrelated/repo/.alan/agents");
        fs::create_dir_all(&unrelated).unwrap();
        fs::create_dir_all(&home).unwrap();
        let paths = LegacyStatePaths::from_home_dir(&home, InstallChannel::Stable).unwrap();

        let implicit = inspect_legacy_state(&paths, &[]).unwrap();
        let explicit = inspect_legacy_state(&paths, &[temp.path().join("unrelated/repo")]).unwrap();

        assert!(implicit.authored_roots.is_empty());
        assert!(
            explicit
                .authored_roots
                .iter()
                .any(|root| root.path == unrelated)
        );
    }
}
