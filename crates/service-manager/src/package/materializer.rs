//! Package snapshot validation and immutable revision materialization.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use alan_agent_engine::skills::{
    SkillScope, SkillTypedDependency, name_to_id, parse_skill_metadata,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    MATERIALIZER_VERSION, PackageExport, PackageSnapshot, PackageSnapshotEntry,
    ensure_owned_directory, ensure_owned_file, revision_root, sync_tree, validate_package_id,
};

const MAX_SOURCE_FILES: usize = 4_096;
pub(super) const MAX_SOURCE_FILE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 12 * 1024 * 1024;

impl PackageSnapshot {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaterializationManifest {
    pub(super) exports: Vec<PackageExport>,
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

        let stage = self.store_root.join("staging").join(format!(
            "{}-{}",
            package_id,
            uuid::Uuid::new_v4().simple()
        ));
        let source_root = stage.join("source");
        let content_root = stage.join("content");
        let skills_root = content_root.join("skills");
        fs::create_dir_all(&source_root)?;
        fs::create_dir_all(&skills_root)?;
        let materialized = (|| -> Result<MaterializationManifest> {
            for entry in &snapshot.entries {
                let target = source_root.join(&entry.path);
                ensure!(
                    target.starts_with(&source_root),
                    "snapshot path escaped staging"
                );
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&target, &entry.bytes)?;
                set_executable(&target, entry.executable)?;
            }

            fs::write(
                stage.join("source-name.json"),
                serde_json::to_vec(&snapshot.source_name)?,
            )?;
            copy_tree(&source_root, &content_root.join("source"))?;

            let mut exports = Vec::new();
            let mut skill_ids = BTreeSet::new();
            let mut generated_files = BTreeSet::new();
            let native_roots = native_skill_roots(snapshot)?;
            for native_root in native_roots {
                let skill_document = fs::read(source_root.join(&native_root).join("SKILL.md"))?;
                let skill_id = skill_id_for_native_root(snapshot, &native_root)?;
                let declared_dependencies = parse_skill_dependencies(&skill_id, &skill_document)?;
                let dependencies = merge_dependencies(
                    declared_dependencies,
                    detect_availability_dependencies(&skill_document),
                );
                ensure!(
                    skill_ids.insert(skill_id.clone()),
                    "duplicate Skill id `{skill_id}`"
                );
                let target = skills_root.join(&skill_id);
                copy_tree(&source_root.join(&native_root), &target)?;
                exports.push(PackageExport {
                    skill_id: skill_id.clone(),
                    root: format!("skills/{skill_id}"),
                    dependencies,
                });
            }

            if !snapshot
                .entries
                .iter()
                .any(|entry| entry.path == "SKILL.md")
            {
                for entry in &snapshot.entries {
                    let path = Path::new(&entry.path);
                    if path.components().count() != 2
                        || path.parent() != Some(Path::new("skills"))
                        || path.extension().and_then(|value| value.to_str()) != Some("md")
                    {
                        continue;
                    }
                    let skill_name = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .context("command Skill name is not UTF-8")?;
                    let skill_id = name_to_id(skill_name);
                    validate_package_id(&skill_id)?;
                    ensure!(
                        skill_ids.insert(skill_id.clone()),
                        "duplicate Skill id `{skill_id}`"
                    );
                    let body = std::str::from_utf8(&entry.bytes)
                        .context("command-style Skill is not UTF-8")?;
                    let dependencies = detect_availability_dependencies(&entry.bytes);
                    let document = command_skill_document(&skill_id, body, &dependencies);
                    ensure!(
                        document.len() <= MAX_SOURCE_FILE_BYTES,
                        "generated command-style Skill exceeds descriptor file size limit"
                    );
                    let target = skills_root.join(&skill_id);
                    fs::create_dir_all(&target)?;
                    fs::write(target.join("SKILL.md"), document)?;
                    generated_files.insert(format!("{skill_id}/SKILL.md"));
                    exports.push(PackageExport {
                        skill_id: skill_id.clone(),
                        root: format!("skills/{skill_id}"),
                        dependencies,
                    });
                }
            }
            exports.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
            let generated_files = generated_files
                .into_iter()
                .map(|path| format!("skills/{path}"))
                .collect::<BTreeSet<_>>();
            let files = materialized_file_records(&content_root, &generated_files)?;
            let manifest = MaterializationManifest {
                tree_fingerprint: fingerprint_directory(&content_root)?,
                exports,
                files,
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
    Ok(())
}

pub(super) fn fingerprint(snapshot: &PackageSnapshot) -> Result<String> {
    validate_snapshot(snapshot)?;
    let mut entries = snapshot.entries.clone();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let mut digest = Sha256::new();
    digest.update(MATERIALIZER_VERSION.as_bytes());
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

fn parse_skill_dependencies(skill_id: &str, document: &[u8]) -> Result<Vec<SkillTypedDependency>> {
    let text = std::str::from_utf8(document).context("SKILL.md is not UTF-8")?;
    let virtual_path = Path::new(skill_id).join("SKILL.md");
    let metadata = parse_skill_metadata(text, &virtual_path, SkillScope::Installed)
        .map_err(anyhow::Error::from)
        .context("validate native SKILL.md")?;
    ensure!(
        metadata.id == skill_id,
        "native Skill runtime identity is not deterministic"
    );
    Ok(alan_agent_engine::skills::skill_declared_dependencies(
        &metadata,
    ))
}

fn detect_availability_dependencies(document: &[u8]) -> Vec<SkillTypedDependency> {
    let text = String::from_utf8_lossy(document);
    let mut dependencies = BTreeMap::new();
    for (needle, name) in [
        ("WebSearch", "web-search"),
        ("WebFetch", "web-fetch"),
        ("TeamCreate", "team-orchestration"),
        ("TaskCreate", "team-orchestration"),
    ] {
        if text.contains(needle) {
            dependencies.insert(
                format!("runtime_capability:{name}"),
                SkillTypedDependency::RuntimeCapability {
                    name: name.to_string(),
                    description: Some("Required by imported command vocabulary.".to_string()),
                },
            );
        }
    }
    dependencies.into_values().collect()
}

fn merge_dependencies(
    left: Vec<SkillTypedDependency>,
    right: Vec<SkillTypedDependency>,
) -> Vec<SkillTypedDependency> {
    let mut dependencies = BTreeMap::new();
    for dependency in left.into_iter().chain(right) {
        dependencies
            .entry(dependency.identity_key())
            .or_insert(dependency);
    }
    dependencies.into_values().collect()
}

fn command_skill_document(
    skill_id: &str,
    body: &str,
    dependencies: &[SkillTypedDependency],
) -> String {
    let yaml_skill_id = serde_yaml::to_string(skill_id)
        .expect("validated Skill id serialization must succeed")
        .trim_start_matches("---\n")
        .trim_end()
        .to_string();
    let mut document = format!(
        "---\nname: {yaml_skill_id}\ndescription: Imported command-style Skill `{skill_id}`.\n"
    );
    if !dependencies.is_empty() {
        document.push_str("compatibility:\n  dependencies:\n");
        for dependency in dependencies {
            let yaml = serde_yaml::to_string(dependency)
                .expect("typed dependency serialization must succeed");
            let yaml = yaml.trim_start_matches("---\n");
            let mut lines = yaml.lines();
            if let Some(first) = lines.next() {
                document.push_str(&format!("    - {first}\n"));
            }
            for line in lines {
                document.push_str(&format!("      {line}\n"));
            }
        }
    }
    document.push_str("---\n\n# Alan adapter\n\nThis Skill was imported by Package Service materializer `alan-skill-v1`. Treat foreign command placeholders as instruction context; unavailable capabilities remain explicit dependencies.\n\n");
    document.push_str(body);
    document
}

fn native_skill_roots(snapshot: &PackageSnapshot) -> Result<Vec<PathBuf>> {
    if snapshot
        .entries
        .iter()
        .any(|entry| entry.path == "SKILL.md")
    {
        return Ok(vec![PathBuf::new()]);
    }
    let mut roots = snapshot
        .entries
        .iter()
        .filter(|entry| entry.path.ends_with("/SKILL.md"))
        .map(|entry| {
            Path::new(&entry.path)
                .parent()
                .expect("nested SKILL.md has a parent")
                .to_path_buf()
        })
        .filter(|root| !has_non_export_component(root))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    for (index, root) in roots.iter().enumerate() {
        ensure!(
            roots
                .iter()
                .skip(index + 1)
                .all(|other| !other.starts_with(root)),
            "overlapping native Skill roots are ambiguous"
        );
    }
    Ok(roots)
}

fn skill_id_for_native_root(snapshot: &PackageSnapshot, root: &Path) -> Result<String> {
    let source = if root.as_os_str().is_empty() {
        snapshot.source_name.as_str()
    } else {
        root.file_name()
            .and_then(|name| name.to_str())
            .context("native Skill directory is not UTF-8")?
    };
    let skill_id = name_to_id(source);
    validate_package_id(&skill_id).context("native Skill directory has no canonical identity")?;
    Ok(skill_id)
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
    ensure!(
        fingerprint(&snapshot)? == revision,
        "package source fingerprint does not match its immutable revision"
    );
    let manifest_path = root.join("manifest.json");
    ensure_owned_file(&manifest_path, "package manifest is not an owned file")?;
    let manifest: MaterializationManifest = serde_json::from_slice(&fs::read(manifest_path)?)
        .context("decode materialization manifest")?;
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
    for export in &manifest.exports {
        validate_package_id(&export.skill_id)?;
        let export_root = root.join("content").join(&export.root);
        ensure!(
            export.root == format!("skills/{}", export.skill_id)
                && export_root.join("SKILL.md").is_file(),
            "materialization manifest contains an invalid Skill export"
        );
    }
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

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        let mut entries = fs::read_dir(&from)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let source_path = entry.path();
            let target_path = to.join(entry.file_name());
            let metadata = fs::symlink_metadata(&source_path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "materialized Skill contains a symlink"
            );
            if metadata.is_dir() {
                fs::create_dir_all(&target_path)?;
                pending.push((source_path, target_path));
            } else {
                fs::copy(&source_path, &target_path)?;
                set_executable(&target_path, executable_bit(&metadata))?;
            }
        }
    }
    Ok(())
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

fn has_non_export_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::Normal(value)
                if matches!(
                    value.to_str(),
                    Some(
                        "agents"
                            | "assets"
                            | "bin"
                            | "evals"
                            | "eval-viewer"
                            | "references"
                            | "scripts"
                    )
                )
        )
    })
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
