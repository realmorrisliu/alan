//! Store-local `.revisions/` snapshots, path safety checks, and atomic file IO.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REVISIONS_DIR: &str = ".revisions";

/// Metadata for one stored snapshot inside `.revisions/<target-key>/history.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRevisionEntry {
    /// Monotonic revision identifier (`rev-0001`, ...).
    pub revision_id: String,
    /// RFC 3339 timestamp when the snapshot was captured.
    pub created_at: String,
    /// SHA-256 hex digest of the snapshot bytes.
    pub sha256: String,
    /// Byte length of the snapshot.
    pub bytes: usize,
    /// Snapshot provenance (`pre_edit` or `pre_revert`).
    pub kind: String,
    /// Snapshot filename inside `.revisions/<target-key>/`.
    pub snapshot_file: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct RevisionHistory {
    pub(super) next_seq: u64,
    pub(super) entries: Vec<MemoryRevisionEntry>,
}

pub(super) fn normalize_relative_memory_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let stripped = trimmed.strip_prefix("/memory/").unwrap_or(trimmed);
    ensure!(!stripped.is_empty(), "memory path must not be empty");
    ensure!(
        !stripped.starts_with('/') && !stripped.contains('\\') && !stripped.contains('\0'),
        "invalid memory path `{raw}`: must be a relative path inside the domain store"
    );

    let mut parts = Vec::new();
    for component in Path::new(stripped).components() {
        match component {
            Component::Normal(seg) => {
                let s = seg
                    .to_str()
                    .with_context(|| format!("non-UTF-8 segment in memory path `{raw}`"))?;
                ensure!(
                    !s.is_empty() && !s.starts_with('.'),
                    "invalid hidden or control path segment `{s}` in `{raw}`"
                );
                parts.push(s.to_string());
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                bail!("path traversal or absolute segment forbidden in memory path `{raw}`");
            }
        }
    }
    ensure!(!parts.is_empty(), "memory path must name a file");
    Ok(parts.join("/"))
}

pub(super) fn ensure_domain_dir_is_real(domain_dir: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(domain_dir).with_context(|| {
        format!(
            "memory domain directory not found: {}",
            domain_dir.display()
        )
    })?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "memory domain directory must not be a symlink: {}",
        domain_dir.display()
    );
    Ok(())
}

pub(super) fn ensure_safe_existing_file(base_dir: &Path, rel_path: &str) -> Result<PathBuf> {
    ensure_domain_dir_is_real(base_dir)?;
    let canonical_base = fs::canonicalize(base_dir)?;
    let mut cursor = base_dir.to_path_buf();
    let parts: Vec<&str> = rel_path.split('/').collect();
    for (idx, part) in parts.iter().enumerate() {
        cursor.push(part);
        let meta = fs::symlink_metadata(&cursor)
            .with_context(|| format!("memory path `{rel_path}` not found"))?;
        ensure!(
            !meta.file_type().is_symlink(),
            "refusing symlink in memory path `{rel_path}`"
        );
        if idx + 1 < parts.len() {
            ensure!(
                meta.is_dir(),
                "expected directory component in `{rel_path}`"
            );
        } else {
            ensure!(meta.is_file(), "memory target `{rel_path}` is not a file");
        }
    }
    let canonical_target = fs::canonicalize(&cursor)?;
    ensure!(
        canonical_target.starts_with(&canonical_base),
        "memory path `{rel_path}` escapes domain store root"
    );
    Ok(cursor)
}

pub(super) fn ensure_safe_writable_target(domain_dir: &Path, rel_path: &str) -> Result<PathBuf> {
    ensure_domain_dir_is_real(domain_dir)?;
    let canonical_domain = fs::canonicalize(domain_dir)?;
    let mut cursor = domain_dir.to_path_buf();
    let parts: Vec<&str> = rel_path.split('/').collect();
    for (idx, part) in parts.iter().enumerate() {
        cursor.push(part);
        let is_last = idx + 1 == parts.len();
        match fs::symlink_metadata(&cursor) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "refusing symlink in memory path `{rel_path}`"
                );
                if is_last {
                    ensure!(
                        meta.is_file(),
                        "memory target `{rel_path}` exists and is not a regular file"
                    );
                } else {
                    ensure!(
                        meta.is_dir(),
                        "memory parent `{part}` in `{rel_path}` is not a directory"
                    );
                }
                let canonical_cursor = fs::canonicalize(&cursor)?;
                ensure!(
                    canonical_cursor.starts_with(&canonical_domain),
                    "memory path `{rel_path}` escapes domain store root"
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                if !is_last {
                    fs::create_dir(&cursor).with_context(|| {
                        format!("create memory subdirectory {}", cursor.display())
                    })?;
                    let canonical_cursor = fs::canonicalize(&cursor)?;
                    ensure!(
                        canonical_cursor.starts_with(&canonical_domain),
                        "memory subdirectory escapes domain store root"
                    );
                }
            }
            Err(err) => {
                return Err(err).with_context(|| format!("inspect memory path `{rel_path}`"));
            }
        }
    }
    Ok(cursor)
}

pub(super) fn revision_target_dir(domain_dir: &Path, rel_path: &str) -> Result<PathBuf> {
    let rev_root = domain_dir.join(REVISIONS_DIR);
    if let Ok(meta) = fs::symlink_metadata(&rev_root) {
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "{REVISIONS_DIR} must be a real non-symlink directory"
        );
    }
    let key = revision_target_key(rel_path);
    let target_dir = rev_root.join(key);
    if let Ok(meta) = fs::symlink_metadata(&target_dir) {
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "revision target dir must not be a symlink"
        );
    }
    Ok(target_dir)
}

pub(super) fn load_history(domain_dir: &Path, rel_path: &str) -> Result<RevisionHistory> {
    let target_dir = revision_target_dir(domain_dir, rel_path)?;
    let history_path = target_dir.join("history.json");
    match fs::symlink_metadata(&history_path) {
        Ok(meta) => {
            ensure!(
                !meta.file_type().is_symlink() && meta.is_file(),
                "revision history file must not be a symlink"
            );
            let bytes = fs::read(&history_path)?;
            serde_json::from_slice(&bytes)
                .with_context(|| format!("parse revision history {}", history_path.display()))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(RevisionHistory::default()),
        Err(err) => Err(err).context("inspect revision history"),
    }
}

pub(super) fn record_snapshot(
    domain_dir: &Path,
    rel_path: &str,
    scrubbed_content: &str,
    kind: &str,
) -> Result<MemoryRevisionEntry> {
    let target_dir = revision_target_dir(domain_dir, rel_path)?;
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("create revision directory {}", target_dir.display()))?;
    let mut history = load_history(domain_dir, rel_path)?;
    history.next_seq = history.next_seq.saturating_add(1);
    let revision_id = format!("rev-{:04}", history.next_seq);
    let snapshot_file = format!("{revision_id}.snap");
    let snapshot_path = target_dir.join(&snapshot_file);

    write_atomic(&snapshot_path, scrubbed_content.as_bytes())?;

    let entry = MemoryRevisionEntry {
        revision_id,
        created_at: Utc::now().to_rfc3339(),
        sha256: sha256_hex(scrubbed_content.as_bytes()),
        bytes: scrubbed_content.len(),
        kind: kind.to_string(),
        snapshot_file,
    };
    history.entries.push(entry.clone());

    let history_path = target_dir.join("history.json");
    let history_bytes = serde_json::to_vec_pretty(&history)?;
    write_atomic(&history_path, &history_bytes)?;
    Ok(entry)
}

pub(super) fn collect_domain_files(
    dir: &Path,
    domain_dir: &Path,
    canonical_domain: &Path,
    out: &mut Vec<String>,
) -> Result<()> {
    let canonical_dir = fs::canonicalize(dir)?;
    if !canonical_dir.starts_with(canonical_domain) {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();

    for path in entries {
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if meta.is_dir() {
            collect_domain_files(&path, domain_dir, canonical_domain, out)?;
        } else if meta.is_file()
            && !name.ends_with(".lock")
            && let Ok(rel) = path.strip_prefix(domain_dir)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("atomic write path missing parent directory")?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("atomic write path missing filename")?;
    let tmp_path = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    fs::write(&tmp_path, bytes)
        .with_context(|| format!("write temporary file {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path).with_context(|| {
        let _ = fs::remove_file(&tmp_path);
        format!("atomically replace {}", path.display())
    })?;
    Ok(())
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

pub(super) fn single_line_preview(text: &str, max_chars: usize) -> String {
    let mut seen_space = false;
    let mut compact = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !seen_space && !compact.is_empty() {
                compact.push(' ');
                seen_space = true;
            }
        } else {
            compact.push(ch);
            seen_space = false;
        }
        if compact.chars().count() >= max_chars {
            break;
        }
    }
    compact
}

fn revision_target_key(rel_path: &str) -> String {
    let digest = Sha256::digest(rel_path.as_bytes());
    let mut hex = String::with_capacity(24);
    for byte in digest.iter().take(12) {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}
