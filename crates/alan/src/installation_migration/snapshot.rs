//! Deterministic durable-content inventories used before and after migration copies.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use alan_os_host::installation::ExtendedMetadata;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    directory: bool,
    mode: u32,
    owner: u32,
    group: u32,
    length: u64,
    digest: String,
    metadata_digest: String,
}

/// Root-relative inventory; a missing component differs from an existing empty directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Snapshot(BTreeMap<PathBuf, Entry>);

impl Snapshot {
    pub(super) fn validate_inventory(&self) -> Result<()> {
        ensure!(
            self.0.contains_key(Path::new("")),
            "recovery inventory has no root"
        );
        for (path, entry) in &self.0 {
            ensure!(
                path.components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
                "recovery inventory path escapes its component"
            );
            ensure!(
                entry.metadata_digest.len() == 64
                    && entry
                        .metadata_digest
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit()),
                "invalid extended metadata fingerprint"
            );
            ensure!(entry.mode <= 0o7777, "unsupported recovery permission bits");
            if !path.as_os_str().is_empty() {
                ensure!(
                    self.0
                        .get(path.parent().unwrap())
                        .is_some_and(|parent| parent.directory),
                    "recovery inventory is missing a parent directory"
                );
            }
            ensure!(
                if entry.directory {
                    entry.length == 0 && entry.digest.is_empty()
                } else {
                    entry.digest.len() == 64
                        && entry.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                },
                "invalid recovery content fingerprint"
            );
        }
        Ok(())
    }

    /// `services` selects the two runtime-generated subtrees excluded from adoption.
    pub(super) fn read(root: &Path, services: bool) -> Result<Self> {
        let mut entries = BTreeMap::new();
        match fs::symlink_metadata(root) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self(entries)),
            Err(error) => return Err(error.into()),
        }
        let mut pending = vec![PathBuf::new()];
        while let Some(relative) = pending.pop() {
            if services
                && matches!(
                    relative.to_str(),
                    Some("agent-runtime/cache" | "agent-runtime/tmp")
                )
            {
                continue;
            }
            ensure!(
                relative.to_str().is_some(),
                "unsupported non-UTF-8 migration path"
            );
            let path = entry_path(root, &relative);
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_file() || metadata.is_dir(),
                "migration source contains a symlink or special file"
            );
            let (mode, owner, group) = permissions(&metadata);
            let file = File::open(&path)?;
            ensure!(
                same_file(&metadata, &file.metadata()?),
                "migration entry changed while opening metadata"
            );
            let metadata_digest = metadata_digest(&ExtendedMetadata::read(&file)?)?;
            let (length, digest) = if metadata.is_dir() {
                for child in fs::read_dir(&path)? {
                    pending.push(relative.join(child?.file_name()));
                }
                (0, String::new())
            } else {
                let mut source = File::open(&path)?;
                let opened = source.metadata()?;
                ensure!(
                    same_file(&metadata, &opened),
                    "migration source changed while opening a file"
                );
                let (length, digest) = hash_reader(&mut source)?;
                ensure!(
                    length == metadata.len(),
                    "migration source changed while reading a file"
                );
                (length, digest)
            };
            entries.insert(
                relative,
                Entry {
                    directory: metadata.is_dir(),
                    mode,
                    owner,
                    group,
                    length,
                    digest,
                    metadata_digest,
                },
            );
        }
        Ok(Self(entries))
    }

    pub(super) fn fingerprint(&self) -> Result<String> {
        Ok(hex(Sha256::digest(serde_json::to_vec(&self.0)?)))
    }

    pub(super) fn present(&self) -> bool {
        !self.0.is_empty()
    }

    pub(super) fn discard_partial(&self, destination: &Path) -> Result<()> {
        let current = Self::read(destination, false)?;
        for (path, entry) in &current.0 {
            ensure!(
                self.0
                    .get(path)
                    .is_some_and(|expected| expected.directory == entry.directory
                        && expected.owner == entry.owner),
                "migration staging contains unrecognized content; retain it for inspection"
            );
        }
        if let Some(root) = current.0.get(Path::new("")) {
            if root.directory {
                fs::remove_dir_all(destination)?;
            } else {
                fs::remove_file(destination)?;
            }
        }
        Ok(())
    }

    /// Copy only inventoried entries into an absent staging component, never into user data.
    /// The caller holds writer exclusion and rechecks the complete source before publication.
    pub(super) fn stage(&self, source: &Path, destination: &Path) -> Result<()> {
        ensure!(
            fs::symlink_metadata(destination)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "migration staging component already exists or cannot be inspected"
        );
        if !self.present() {
            return Ok(());
        }
        for (relative, expected) in &self.0 {
            let from = entry_path(source, relative);
            let to = entry_path(destination, relative);
            let metadata = fs::symlink_metadata(&from)?;
            ensure!(
                metadata.is_dir() == expected.directory
                    && (metadata.is_file() || metadata.is_dir()),
                "migration source file type changed"
            );
            ensure!(
                permissions(&metadata) == (expected.mode, expected.owner, expected.group),
                "migration source permissions or ownership changed"
            );
            let file = File::open(&from)?;
            ensure!(
                same_file(&metadata, &file.metadata()?),
                "migration entry changed while opening metadata"
            );
            let extended = ExtendedMetadata::read(&file)?;
            ensure!(
                metadata_digest(&extended)? == expected.metadata_digest,
                "migration extended metadata changed"
            );
            if expected.directory {
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(&to)?;
            } else {
                let mut input = File::open(&from)?;
                ensure!(
                    same_file(&metadata, &input.metadata()?),
                    "migration source changed while opening a file"
                );
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut output = options.open(&to)?;
                let mut hasher = Sha256::new();
                let mut length = 0;
                let mut buffer = [0; 64 * 1024];
                loop {
                    let count = input.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    output.write_all(&buffer[..count])?;
                    hasher.update(&buffer[..count]);
                    length += count as u64;
                }
                ensure!(
                    length == expected.length && hex(hasher.finalize()) == expected.digest,
                    "migration source bytes changed during staging"
                );
                extended.apply(&output)?;
                output.set_permissions(metadata.permissions())?;
                output.sync_all()?;
            }
        }
        // Keep directories writable until descendants are durably copied.
        for (relative, expected) in self.0.iter().rev().filter(|(_, entry)| entry.directory) {
            let path = entry_path(destination, relative);
            let directory = File::open(&path)?;
            let extended = ExtendedMetadata::read(&File::open(entry_path(source, relative))?)?;
            ensure!(
                metadata_digest(&extended)? == expected.metadata_digest,
                "migration directory metadata changed"
            );
            extended.apply(&directory)?;
            set_mode(&directory, expected.mode)?;
            directory.sync_all()?;
        }
        ensure!(
            Self::read(destination, false)? == *self,
            "staged migration content or ownership differs from source"
        );
        File::open(
            destination
                .parent()
                .context("migration stage has no parent")?,
        )?
        .sync_all()?;
        Ok(())
    }
}

fn hash_reader(reader: &mut File) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut length = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        length += count as u64;
    }
    Ok((length, hex(hasher.finalize())))
}

#[cfg(unix)]
fn metadata_digest(metadata: &ExtendedMetadata) -> Result<String> {
    Ok(hex(Sha256::digest(serde_json::to_vec(metadata)?)))
}

fn permissions(metadata: &fs::Metadata) -> (u32, u32, u32) {
    use std::os::unix::fs::MetadataExt;
    (metadata.mode() & 0o7777, metadata.uid(), metadata.gid())
}

#[cfg(unix)]
fn same_file(before: &fs::Metadata, opened: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    before.file_type() == opened.file_type()
        && before.dev() == opened.dev()
        && before.ino() == opened.ino()
}

#[cfg(unix)]
fn set_mode(file: &File, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(test)]
mod tests;

pub(super) fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn entry_path(root: &Path, relative: &Path) -> PathBuf {
    if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    }
}
