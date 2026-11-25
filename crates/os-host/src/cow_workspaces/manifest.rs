//! Lightweight `BaseManifest` capture and directory tree scanning.
//!
//! Default exclusions (`.git`, `target`, `node_modules`, `.venv`) and caller
//! exclusions are skipped consistently across base capture, diff summaries,
//! 3-way sync-back, `.patch` export, and Git branch export.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::hash::{Sha256, sha256_file, to_hex};
use super::types::{DiffPathEntry, DiffSummary, FileChangeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BlobSlice {
    pub(crate) offset: u64,
    pub(crate) len: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ManifestEntry {
    File {
        size: u64,
        sha256: String,
        executable: bool,
        mode: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_blob: Option<BlobSlice>,
    },
    Symlink {
        target: PathBuf,
    },
}

impl ManifestEntry {
    pub(crate) fn logically_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::File {
                    size: s1,
                    sha256: h1,
                    executable: e1,
                    ..
                },
                Self::File {
                    size: s2,
                    sha256: h2,
                    executable: e2,
                    ..
                },
            ) => s1 == s2 && h1 == h2 && e1 == e2,
            (Self::Symlink { target: t1 }, Self::Symlink { target: t2 }) => t1 == t2,
            _ => false,
        }
    }
}

pub(crate) fn same_optional_entry(a: Option<&ManifestEntry>, b: Option<&ManifestEntry>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.logically_equal(y),
        _ => false,
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct TreeManifest {
    pub(crate) entries: BTreeMap<String, ManifestEntry>,
}

impl TreeManifest {
    /// Capture a `BaseManifest` from `tree_root` and persist small source blobs
    /// in `blob_path` so `.patch` exports remain exact even after concurrent Host edits.
    pub(crate) fn capture_base(
        tree_root: &Path,
        excludes: &[String],
        blob_path: &Path,
        max_blob_bytes_per_file: u64,
        max_total_blob_bytes: u64,
    ) -> Result<Self> {
        let blob_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(blob_path)
            .with_context(|| format!("create base blob pack {}", blob_path.display()))?;
        let mut blob_writer = BufWriter::new(blob_file);
        let mut blob_cursor = 0u64;
        let mut entries = BTreeMap::new();

        scan_dir_recursive(
            tree_root,
            tree_root,
            excludes,
            &mut entries,
            Some(&mut BlobCaptureContext {
                writer: &mut blob_writer,
                cursor: &mut blob_cursor,
                max_per_file: max_blob_bytes_per_file,
                max_total: max_total_blob_bytes,
            }),
        )?;

        blob_writer.flush()?;
        Ok(Self { entries })
    }

    /// Scan `root` without capturing base blobs.
    pub(crate) fn scan(root: &Path, excludes: &[String]) -> Result<Self> {
        let mut entries = BTreeMap::new();
        scan_dir_recursive(root, root, excludes, &mut entries, None)?;
        Ok(Self { entries })
    }
}

struct BlobCaptureContext<'a> {
    writer: &'a mut BufWriter<File>,
    cursor: &'a mut u64,
    max_per_file: u64,
    max_total: u64,
}

pub(crate) fn read_base_blob(blob_path: &Path, slice: BlobSlice) -> Result<Vec<u8>> {
    let mut file = File::open(blob_path)
        .with_context(|| format!("open base blob pack {}", blob_path.display()))?;
    file.seek(SeekFrom::Start(slice.offset))?;
    let len = usize::try_from(slice.len).context("base blob exceeds addressable usize")?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

pub(crate) fn compute_diff_summary(base: &TreeManifest, workspace: &TreeManifest) -> DiffSummary {
    let all_paths: BTreeSet<&String> = base
        .entries
        .keys()
        .chain(workspace.entries.keys())
        .collect();
    let mut summary = DiffSummary::default();

    for rel in all_paths {
        match (base.entries.get(rel), workspace.entries.get(rel)) {
            (None, Some(_)) => {
                summary.files_added += 1;
                summary.changed_paths.push(DiffPathEntry {
                    path: rel.clone(),
                    kind: FileChangeKind::Added,
                });
            }
            (Some(_), None) => {
                summary.files_deleted += 1;
                summary.changed_paths.push(DiffPathEntry {
                    path: rel.clone(),
                    kind: FileChangeKind::Deleted,
                });
            }
            (Some(b), Some(w)) if !b.logically_equal(w) => {
                summary.files_modified += 1;
                summary.changed_paths.push(DiffPathEntry {
                    path: rel.clone(),
                    kind: FileChangeKind::Modified,
                });
            }
            _ => {}
        }
    }

    summary
}

pub(crate) fn is_excluded(rel: &str, entry_name: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|pattern| {
        let trimmed = pattern.trim_matches('/');
        !trimmed.is_empty()
            && (entry_name == trimmed || rel == trimmed || rel.starts_with(&format!("{trimmed}/")))
    })
}

fn scan_dir_recursive(
    root: &Path,
    dir: &Path,
    excludes: &[String],
    entries: &mut BTreeMap<String, ManifestEntry>,
    mut blob_ctx: Option<&mut BlobCaptureContext<'_>>,
) -> Result<()> {
    let mut children = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read directory {}", dir.display()))? {
        children.push(entry?);
    }
    children.sort_by_key(|e| e.file_name());

    for entry in children {
        let path = entry.path();
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        let rel_path = path
            .strip_prefix(root)
            .with_context(|| format!("strip prefix {} from {}", root.display(), path.display()))?;
        let rel = normalize_rel_path(rel_path)?;
        if is_excluded(&rel, name_str, excludes) {
            continue;
        }

        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("inspect metadata for {}", path.display()))?;
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            let target =
                fs::read_link(&path).with_context(|| format!("read symlink {}", path.display()))?;
            entries.insert(rel, ManifestEntry::Symlink { target });
        } else if file_type.is_dir() {
            scan_dir_recursive(root, &path, excludes, entries, blob_ctx.as_deref_mut())?;
        } else if file_type.is_file() {
            let mode = meta.permissions().mode() & 0o7777;
            let executable = (mode & 0o111) != 0;
            let file_len = meta.len();

            let (digest, size, base_blob) = if let Some(ctx) = blob_ctx.as_deref_mut()
                && file_len <= ctx.max_per_file
                && ctx.cursor.saturating_add(file_len) <= ctx.max_total
            {
                let bytes = fs::read(&path)
                    .with_context(|| format!("read file for base blob {}", path.display()))?;
                let actual_len = bytes.len() as u64;
                let digest = Sha256::digest_bytes(&bytes);
                let offset = *ctx.cursor;
                ctx.writer.write_all(&bytes)?;
                *ctx.cursor += actual_len;
                (
                    digest,
                    actual_len,
                    Some(BlobSlice {
                        offset,
                        len: actual_len,
                    }),
                )
            } else {
                let (digest, size) = sha256_file(&path)?;
                (digest, size, None)
            };

            entries.insert(
                rel,
                ManifestEntry::File {
                    size,
                    sha256: to_hex(&digest),
                    executable,
                    mode,
                    base_blob,
                },
            );
        }
    }
    Ok(())
}

fn normalize_rel_path(rel: &Path) -> Result<String> {
    let parts = rel
        .iter()
        .map(|part| {
            part.to_str()
                .context("workspace contains non-UTF-8 path component")
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(parts.join("/"))
}
