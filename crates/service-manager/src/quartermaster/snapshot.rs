//! Bounded namespace traversal and snapshotting for Quartermaster `/bin/q`.

use alan_ap::{ErrorCode, FileKind};
use alan_shell::{BoundedListError, Shell};

use crate::{PackageSnapshot, PackageSnapshotEntry};

use super::QError;

const MAX_SOURCE_FILES: usize = 4_096;
const MAX_SOURCE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_SOURCE_NODES: usize = 8_192;
const MAX_SOURCE_DIRECTORY_BYTES: usize = 8 * 1024 * 1024;

/// Bounded read error raised when reading source package files from a namespace mount.
pub(super) enum BoundedReadError {
    /// Underlying aP protocol error.
    Protocol(ErrorCode),
    /// File exceeded the allowed per-file or aggregate byte budget.
    LimitExceeded,
}

/// Validates and extracts the leaf component of a normalized namespace source path.
pub(super) fn source_leaf(source: &str) -> Result<String, QError> {
    if !source.starts_with('/') || source == "/" {
        return Err(QError::Usage(
            "package source must be an absolute non-root namespace path".to_string(),
        ));
    }
    if !source
        .split('/')
        .filter(|component| !component.is_empty())
        .all(|component| component != "." && component != "..")
    {
        return Err(QError::Usage(
            "package source path must be normalized".to_string(),
        ));
    }
    source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|leaf| !leaf.is_empty())
        .map(str::to_string)
        .ok_or_else(|| QError::Usage("package source has no leaf name".to_string()))
}

/// Reads at most `max_bytes` from `path` in `shell`, failing closed if EOF does not arrive in budget.
pub(super) async fn read_bounded_source_file(
    shell: &Shell,
    path: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, BoundedReadError> {
    let mut reader = shell.tail(path).await.map_err(BoundedReadError::Protocol)?;
    let read_result = async {
        let mut bytes = Vec::new();
        loop {
            let remaining = max_bytes.saturating_sub(bytes.len() as u64);
            let count = remaining.saturating_add(1).min(4_096) as u32;
            let chunk = reader
                .read(count)
                .await
                .map_err(BoundedReadError::Protocol)?;
            if chunk.is_empty() {
                break;
            }
            if chunk.len() as u64 > remaining {
                return Err(BoundedReadError::LimitExceeded);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    .await;
    let close_result = reader.close().await;
    match (read_result, close_result) {
        (Err(error), _) => Err(error),
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Ok(_), Err(code)) => Err(BoundedReadError::Protocol(code)),
    }
}

/// Traverses `source` inside `shell` within bounded node and byte budgets to build a [`PackageSnapshot`].
pub(super) async fn snapshot_namespace_tree(
    shell: &Shell,
    source: &str,
    source_name: String,
) -> Result<PackageSnapshot, QError> {
    let source = source.trim_end_matches('/');
    let root_entries = list_source_directory(shell, source, MAX_SOURCE_NODES)
        .await?
        .into_iter()
        .filter(|entry| entry != ".git")
        .collect::<Vec<_>>();
    let mut discovered_nodes = root_entries.len();
    let mut pending = vec![(source.to_string(), String::new(), root_entries)];
    let mut entries = Vec::new();
    let mut total_bytes = 0u64;
    while let Some((absolute, relative, children)) = pending.pop() {
        let mut children = children;
        children.sort();
        for child in children.into_iter().rev() {
            if child.is_empty() || child.contains('/') || child == "." || child == ".." {
                return Err(QError::Operation(
                    "source File-Server returned an invalid directory entry".to_string(),
                ));
            }
            if child == ".git" {
                continue;
            }
            let child_absolute = format!("{absolute}/{child}");
            let child_relative = if relative.is_empty() {
                child.clone()
            } else {
                format!("{relative}/{child}")
            };
            let remaining_nodes = MAX_SOURCE_NODES - discovered_nodes;
            match list_source_directory(shell, &child_absolute, remaining_nodes).await {
                Ok(grandchildren) => {
                    discovered_nodes += grandchildren.len();
                    pending.push((child_absolute, child_relative, grandchildren));
                }
                Err(QError::Protocol {
                    code: ErrorCode::NotDirectory,
                    ..
                }) => {
                    if entries.len() >= MAX_SOURCE_FILES {
                        return Err(QError::Operation(
                            "package source has too many files".to_string(),
                        ));
                    }
                    let stat =
                        shell
                            .stat(&child_absolute)
                            .await
                            .map_err(|code| QError::Protocol {
                                action: "inspect source",
                                code,
                            })?;
                    if stat.qid.kind != FileKind::File {
                        return Err(QError::Operation(
                            "package source contains a non-file leaf".to_string(),
                        ));
                    }
                    if stat.length > MAX_SOURCE_FILE_BYTES {
                        return Err(QError::Operation(
                            "package source file is too large".to_string(),
                        ));
                    }
                    let remaining_total = MAX_SOURCE_BYTES.saturating_sub(total_bytes);
                    if stat.length > remaining_total {
                        return Err(QError::Operation("package source is too large".to_string()));
                    }
                    let read_limit = MAX_SOURCE_FILE_BYTES.min(remaining_total);
                    let bytes = read_bounded_source_file(shell, &child_absolute, read_limit)
                        .await
                        .map_err(|error| match error {
                            BoundedReadError::Protocol(code) => QError::Protocol {
                                action: "read source",
                                code,
                            },
                            BoundedReadError::LimitExceeded
                                if remaining_total < MAX_SOURCE_FILE_BYTES =>
                            {
                                QError::Operation("package source is too large".to_string())
                            }
                            BoundedReadError::LimitExceeded => {
                                QError::Operation("package source file is too large".to_string())
                            }
                        })?;
                    if bytes.len() as u64 != stat.length {
                        return Err(QError::Operation(
                            "package source changed while it was being imported".to_string(),
                        ));
                    }
                    total_bytes = total_bytes.checked_add(bytes.len() as u64).ok_or_else(|| {
                        QError::Operation("package source size overflow".to_string())
                    })?;
                    entries.push(PackageSnapshotEntry {
                        path: child_relative,
                        bytes,
                        executable: stat.executable,
                    });
                }
                Err(error) => return Err(error),
            }
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(PackageSnapshot {
        source_name,
        entries,
    })
}

async fn list_source_directory(
    shell: &Shell,
    path: &str,
    remaining_nodes: usize,
) -> Result<Vec<String>, QError> {
    shell
        .ls_bounded(path, remaining_nodes, MAX_SOURCE_DIRECTORY_BYTES)
        .await
        .map_err(|error| match error {
            BoundedListError::Protocol(code) => QError::Protocol {
                action: "read source",
                code,
            },
            BoundedListError::LimitExceeded => {
                QError::Operation("package source has too many entries".to_string())
            }
        })
}
