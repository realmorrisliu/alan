//! Package snapshot validation and immutable revision materialization.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::fs_safety::{ensure_owned_directory, ensure_owned_file, sync_tree};
use super::manifest::parse_package_manifest;
use super::materializer_exports::{build_content_exports, verify_materialized_exports};
use super::{
    FileserverPackageExport, LEGACY_SKILL_MATERIALIZER_VERSION, MATERIALIZER_VERSION,
    McpPackageExport, PackageExport, PackageExportKind, PackageSnapshot, PackageSnapshotEntry,
    ProviderPackageExport, ToolPackageExport,
};

const MAX_SOURCE_FILES: usize = 4_096;
pub(super) const MAX_SOURCE_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 12 * 1024 * 1024;

impl PackageSnapshot {
    /// Validate embedded first-party entries through the ordinary snapshot boundary.
    pub fn from_preinstalled(
        source: &alan_agent_engine::skills::PreinstalledSkillPackageSource,
    ) -> Result<Self> {
        let mut entries = source
            .files
            .iter()
            .map(|file| {
                let path = slash_path(file.path)?;
                ensure!(
                    file.path.to_str() == Some(path.as_str()),
                    "snapshot path is not canonical"
                );
                Ok(PackageSnapshotEntry {
                    path,
                    bytes: file.bytes.to_vec(),
                    executable: file.executable,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let snapshot = Self {
            source_name: source.source_name.to_string(),
            entries,
        };
        validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }

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

    pub(super) fn from_directory_named(root: &Path, source_name: String) -> Result<Self> {
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

fn default_legacy_materializer_version() -> String {
    LEGACY_SKILL_MATERIALIZER_VERSION.to_string()
}

/// Complete materialization metadata written inside `manifest.json` per package revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaterializationManifest {
    #[serde(default = "default_legacy_materializer_version")]
    pub(super) materializer_version: String,
    #[serde(default)]
    pub(super) export_kind: PackageExportKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) manifest_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
    pub(super) exports: Vec<PackageExport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) tool_exports: Vec<ToolPackageExport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) mcp_exports: Vec<McpPackageExport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) provider_export: Option<ProviderPackageExport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) fileserver_export: Option<FileserverPackageExport>,
    pub(super) files: Vec<MaterializedFileRecord>,
    pub(super) tree_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaterializedFileRecord {
    pub(super) path: String,
    pub(super) generated: bool,
}

pub(super) struct PackageMaterializer<'a> {
    store_root: &'a Path,
}

pub(super) fn snapshot_materializer_version(snapshot: &PackageSnapshot) -> &'static str {
    if snapshot
        .entries
        .iter()
        .any(|entry| entry.path == "alan-package.toml" || entry.path == "package.toml")
    {
        MATERIALIZER_VERSION
    } else {
        LEGACY_SKILL_MATERIALIZER_VERSION
    }
}

pub(super) fn materialize_in_memory(
    snapshot: &PackageSnapshot,
) -> Result<(BTreeMap<String, Vec<u8>>, MaterializationManifest)> {
    let parsed_manifest = parse_package_manifest(snapshot)?;
    let materializer_version = snapshot_materializer_version(snapshot).to_string();

    let mut content_files: BTreeMap<String, (Vec<u8>, bool)> = BTreeMap::new();
    for entry in &snapshot.entries {
        content_files.insert(
            format!("source/{}", entry.path),
            (entry.bytes.clone(), entry.executable),
        );
    }

    let mut generated_files = BTreeSet::new();
    let computed = build_content_exports(
        snapshot,
        parsed_manifest,
        &mut content_files,
        &mut generated_files,
        MAX_SOURCE_FILE_BYTES,
    )?;

    let mut digest = Sha256::new();
    let mut files = Vec::with_capacity(content_files.len());
    let mut plain_files = BTreeMap::new();
    for (relative, (bytes, executable)) in content_files {
        digest.update((relative.len() as u64).to_be_bytes());
        digest.update(relative.as_bytes());
        digest.update([u8::from(executable)]);
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(&bytes);
        files.push(MaterializedFileRecord {
            generated: generated_files.contains(&relative),
            path: relative.clone(),
        });
        plain_files.insert(relative, bytes);
    }

    let manifest = MaterializationManifest {
        materializer_version,
        export_kind: computed.export_kind,
        manifest_version: computed.manifest_version,
        description: computed.description,
        exports: computed.exports,
        tool_exports: computed.tool_exports,
        mcp_exports: computed.mcp_exports,
        provider_export: computed.provider_export,
        fileserver_export: computed.fileserver_export,
        files,
        tree_fingerprint: hex::encode(digest.finalize()),
    };
    Ok((plain_files, manifest))
}

impl<'a> PackageMaterializer<'a> {
    pub(super) fn new(store_root: &'a Path) -> Self {
        Self { store_root }
    }

    pub(super) fn materialize(
        &self,
        package_id: &str,
        revision: &str,
        snapshot: &PackageSnapshot,
    ) -> Result<MaterializationManifest> {
        let final_root = revision_root(self.store_root, package_id, revision);
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

        let parsed_manifest = parse_package_manifest(snapshot)?;
        let materializer_version = snapshot_materializer_version(snapshot).to_string();

        let mut content_files: BTreeMap<String, (Vec<u8>, bool)> = BTreeMap::new();
        for entry in &snapshot.entries {
            content_files.insert(
                format!("source/{}", entry.path),
                (entry.bytes.clone(), entry.executable),
            );
        }
        let mut generated_files = BTreeSet::new();
        let computed = build_content_exports(
            snapshot,
            parsed_manifest,
            &mut content_files,
            &mut generated_files,
            MAX_SOURCE_FILE_BYTES,
        )?;

        let stage = self.store_root.join("staging").join(format!(
            "{}-{}",
            package_id,
            uuid::Uuid::new_v4().simple()
        ));
        let source_root = stage.join("source");
        let content_root = stage.join("content");
        fs::create_dir_all(&source_root)?;
        fs::create_dir_all(&content_root)?;

        let materialized = (|| -> Result<MaterializationManifest> {
            for entry in &snapshot.entries {
                write_staged_file(&source_root, &entry.path, &entry.bytes, entry.executable)?;
            }
            fs::write(
                stage.join("source-name.json"),
                serde_json::to_vec(&snapshot.source_name)?,
            )?;

            for (relative, (bytes, executable)) in &content_files {
                write_staged_file(&content_root, relative, bytes, *executable)?;
            }

            let files = materialized_file_records(&content_root, &generated_files)?;
            let manifest = MaterializationManifest {
                materializer_version,
                export_kind: computed.export_kind,
                manifest_version: computed.manifest_version,
                description: computed.description,
                exports: computed.exports,
                tool_exports: computed.tool_exports,
                mcp_exports: computed.mcp_exports,
                provider_export: computed.provider_export,
                fileserver_export: computed.fileserver_export,
                files,
                tree_fingerprint: fingerprint_directory(&content_root)?,
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
}

fn write_staged_file(root: &Path, relative: &str, bytes: &[u8], executable: bool) -> Result<()> {
    let target = root.join(relative);
    ensure!(target.starts_with(root), "snapshot path escaped staging");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target, bytes)?;
    set_executable(&target, executable)?;
    Ok(())
}

pub(super) fn validate_snapshot(snapshot: &PackageSnapshot) -> Result<()> {
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
    for path in &paths {
        let mut ancestor = path.parent();
        while let Some(dir) = ancestor {
            if dir.as_os_str().is_empty() {
                break;
            }
            if let Some(dir_str) = dir.to_str() {
                ensure!(
                    !store_paths.contains(&dir_str.to_lowercase()),
                    "snapshot path collides with an ancestor file `{dir_str}`"
                );
            }
            ancestor = dir.parent();
        }
    }
    Ok(())
}

pub(super) fn fingerprint(snapshot: &PackageSnapshot) -> Result<String> {
    fingerprint_with_version(snapshot, snapshot_materializer_version(snapshot))
}

pub(super) fn fingerprint_with_version(
    snapshot: &PackageSnapshot,
    materializer_version: &str,
) -> Result<String> {
    validate_snapshot(snapshot)?;
    let _ = parse_package_manifest(snapshot)?;
    let mut entries = snapshot.entries.clone();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let mut digest = Sha256::new();
    digest.update(materializer_version.as_bytes());
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

pub(super) fn verify_materialized_revision(
    root: &Path,
    revision: &str,
) -> Result<MaterializationManifest> {
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
    let manifest_path = root.join("manifest.json");
    ensure_owned_file(&manifest_path, "package manifest is not an owned file")?;
    let manifest: MaterializationManifest = serde_json::from_slice(&fs::read(manifest_path)?)
        .context("decode materialization manifest")?;
    ensure!(
        matches!(
            manifest.materializer_version.as_str(),
            LEGACY_SKILL_MATERIALIZER_VERSION | MATERIALIZER_VERSION
        ),
        "materialization manifest uses unsupported version"
    );
    ensure!(
        fingerprint_with_version(&snapshot, &manifest.materializer_version)? == revision,
        "package source fingerprint does not match its immutable revision"
    );
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
    verify_materialized_exports(
        &content_root,
        manifest.export_kind,
        &manifest.exports,
        &manifest.tool_exports,
        &manifest.mcp_exports,
        manifest.provider_export.as_ref(),
        manifest.fileserver_export.as_ref(),
    )?;
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

fn revision_root(root: &Path, package_id: &str, revision: &str) -> PathBuf {
    root.join("revisions").join(package_id).join(revision)
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
