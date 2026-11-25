//! Unified `.patch` generator with Intent Provenance headers.
//!
//! Generates standard `diff --git` unified diffs compatible with `git apply`
//! and `patch -p1`, excluding `.git/` and build/cache paths.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::Result;

use super::manifest::{ManifestEntry, TreeManifest, compute_diff_summary, read_base_blob};
use super::types::{
    CowWorkspaceError, FileChangeKind, IntentProvenance, PatchExportReport, WorkspaceBacking,
    WorkspaceRef,
};

const CONTEXT_LINES: usize = 3;

#[allow(
    clippy::too_many_arguments,
    reason = "Unified patch generation takes explicit manifest, blob, tree, host, and provenance inputs"
)]
pub(crate) fn generate_patch(
    workspace_ref: &WorkspaceRef,
    backing_kind: WorkspaceBacking,
    base: &TreeManifest,
    base_blob_path: &Path,
    workspace_tree: &Path,
    host_root: &Path,
    excludes: &[String],
    provenance: &IntentProvenance,
) -> std::result::Result<PatchExportReport, CowWorkspaceError> {
    let workspace_now = TreeManifest::scan(workspace_tree, excludes)?;
    let diff_summary = compute_diff_summary(base, &workspace_now);
    if diff_summary.is_empty() {
        return Err(CowWorkspaceError::NoChangesToExport);
    }

    let mut patch = String::new();
    let _ = writeln!(patch, "From: Alan CoW Workspace <alan@local>");
    let _ = writeln!(patch, "Subject: [PATCH] {}", provenance.why);
    let _ = writeln!(patch, "Alan-Workstream: {}", provenance.workstream);
    let _ = writeln!(patch, "Alan-Rollout: {}", provenance.rollout_ref);
    let _ = writeln!(patch, "Alan-Why: {}", provenance.why);
    let _ = writeln!(patch, "Alan-Workspace: {workspace_ref}");
    let _ = writeln!(patch, "Alan-Backing: {backing_kind}");
    patch.push('\n');

    for entry in &diff_summary.changed_paths {
        let rel = &entry.path;
        let base_entry = base.entries.get(rel);
        let ws_entry = workspace_now.entries.get(rel);
        append_file_diff(
            &mut patch,
            rel,
            entry.kind,
            base_entry,
            ws_entry,
            base_blob_path,
            workspace_tree,
            host_root,
        )?;
    }

    Ok(PatchExportReport {
        workspace_ref: workspace_ref.clone(),
        backing_kind,
        patch_text: patch,
        diff_summary,
        provenance: provenance.clone(),
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Per-file diff builder takes base/workspace entries and their respective backing roots"
)]
fn append_file_diff(
    out: &mut String,
    rel: &str,
    kind: FileChangeKind,
    base_entry: Option<&ManifestEntry>,
    ws_entry: Option<&ManifestEntry>,
    base_blob_path: &Path,
    workspace_tree: &Path,
    host_root: &Path,
) -> Result<()> {
    let old_bytes = load_base_bytes(rel, base_entry, base_blob_path, host_root)?;
    let new_bytes = load_workspace_bytes(rel, ws_entry, workspace_tree)?;

    let _ = writeln!(out, "diff --git a/{rel} b/{rel}");
    match kind {
        FileChangeKind::Added => {
            let mode = entry_git_mode(ws_entry);
            let _ = writeln!(out, "new file mode {mode}");
        }
        FileChangeKind::Deleted => {
            let mode = entry_git_mode(base_entry);
            let _ = writeln!(out, "deleted file mode {mode}");
        }
        FileChangeKind::Modified => {
            let old_mode = entry_git_mode(base_entry);
            let new_mode = entry_git_mode(ws_entry);
            if old_mode != new_mode {
                let _ = writeln!(out, "old mode {old_mode}");
                let _ = writeln!(out, "new mode {new_mode}");
            }
        }
    }

    let (Some(old_text), Some(new_text)) = (as_utf8_text(&old_bytes), as_utf8_text(&new_bytes))
    else {
        let _ = writeln!(out, "Binary files a/{rel} and b/{rel} differ");
        return Ok(());
    };

    match kind {
        FileChangeKind::Added => {
            let _ = writeln!(out, "--- /dev/null");
            let _ = writeln!(out, "+++ b/{rel}");
        }
        FileChangeKind::Deleted => {
            let _ = writeln!(out, "--- a/{rel}");
            let _ = writeln!(out, "+++ /dev/null");
        }
        FileChangeKind::Modified => {
            let _ = writeln!(out, "--- a/{rel}");
            let _ = writeln!(out, "+++ b/{rel}");
        }
    }

    write_unified_hunks(out, old_text, new_text);
    Ok(())
}

fn entry_git_mode(entry: Option<&ManifestEntry>) -> &'static str {
    match entry {
        Some(ManifestEntry::File {
            executable: true, ..
        }) => "100755",
        Some(ManifestEntry::Symlink { .. }) => "120000",
        _ => "100644",
    }
}

fn load_base_bytes(
    rel: &str,
    entry: Option<&ManifestEntry>,
    base_blob_path: &Path,
    host_root: &Path,
) -> Result<Vec<u8>> {
    match entry {
        None => Ok(Vec::new()),
        Some(ManifestEntry::Symlink { target }) => Ok(target.to_string_lossy().as_bytes().to_vec()),
        Some(ManifestEntry::File {
            base_blob: Some(slice),
            ..
        }) => read_base_blob(base_blob_path, *slice),
        Some(ManifestEntry::File {
            base_blob: None, ..
        }) => {
            let host_path = host_root.join(rel);
            if host_path.is_file() {
                Ok(fs::read(host_path)?)
            } else {
                Ok(Vec::new())
            }
        }
    }
}

fn load_workspace_bytes(
    rel: &str,
    entry: Option<&ManifestEntry>,
    workspace_tree: &Path,
) -> Result<Vec<u8>> {
    match entry {
        None => Ok(Vec::new()),
        Some(ManifestEntry::Symlink { target }) => Ok(target.to_string_lossy().as_bytes().to_vec()),
        Some(ManifestEntry::File { .. }) => Ok(fs::read(workspace_tree.join(rel))?),
    }
}

fn as_utf8_text(bytes: &[u8]) -> Option<&str> {
    if bytes.contains(&0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

struct TextLines<'a> {
    lines: Vec<&'a str>,
    trailing_newline: bool,
}

fn split_text_lines(text: &str) -> TextLines<'_> {
    if text.is_empty() {
        return TextLines {
            lines: Vec::new(),
            trailing_newline: true,
        };
    }
    let trailing_newline = text.ends_with('\n');
    let trimmed = text.strip_suffix('\n').unwrap_or(text);
    let lines = trimmed.split('\n').collect();
    TextLines {
        lines,
        trailing_newline,
    }
}

#[derive(Clone, Copy)]
enum DiffOp<'a> {
    Context(&'a str, bool),
    Delete(&'a str, bool),
    Insert(&'a str, bool),
}

fn write_unified_hunks(out: &mut String, old_text: &str, new_text: &str) {
    let old = split_text_lines(old_text);
    let new = split_text_lines(new_text);
    let ops = compute_line_ops(&old, &new);

    // Collect indices of non-context operations.
    let changed_indices: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter_map(|(idx, op)| (!matches!(op, DiffOp::Context(_, _))).then_some(idx))
        .collect();
    if changed_indices.is_empty() {
        return;
    }

    // Group changed indices into unified hunks with up to CONTEXT_LINES around each change.
    let mut hunk_ranges: Vec<(usize, usize)> = Vec::new();
    for idx in changed_indices {
        let start = idx.saturating_sub(CONTEXT_LINES);
        let end = (idx + CONTEXT_LINES + 1).min(ops.len());
        if let Some((_, prev_end)) = hunk_ranges.last_mut()
            && start <= *prev_end
        {
            *prev_end = end;
        } else {
            hunk_ranges.push((start, end));
        }
    }

    // Precompute 1-based line positions before each op.
    let mut old_pos = Vec::with_capacity(ops.len() + 1);
    let mut new_pos = Vec::with_capacity(ops.len() + 1);
    let mut cur_old = 1usize;
    let mut cur_new = 1usize;
    for op in &ops {
        old_pos.push(cur_old);
        new_pos.push(cur_new);
        match op {
            DiffOp::Context(_, _) => {
                cur_old += 1;
                cur_new += 1;
            }
            DiffOp::Delete(_, _) => cur_old += 1,
            DiffOp::Insert(_, _) => cur_new += 1,
        }
    }
    old_pos.push(cur_old);
    new_pos.push(cur_new);

    for (start, end) in hunk_ranges {
        let mut old_count = 0usize;
        let mut new_count = 0usize;
        for op in &ops[start..end] {
            match op {
                DiffOp::Context(_, _) => {
                    old_count += 1;
                    new_count += 1;
                }
                DiffOp::Delete(_, _) => old_count += 1,
                DiffOp::Insert(_, _) => new_count += 1,
            }
        }
        let old_start = if old_count == 0 {
            old_pos[start].saturating_sub(1)
        } else {
            old_pos[start]
        };
        let new_start = if new_count == 0 {
            new_pos[start].saturating_sub(1)
        } else {
            new_pos[start]
        };
        let _ = writeln!(
            out,
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@"
        );
        for op in &ops[start..end] {
            match op {
                DiffOp::Context(line, has_nl) => {
                    let _ = writeln!(out, " {line}");
                    if !has_nl {
                        let _ = writeln!(out, "\\ No newline at end of file");
                    }
                }
                DiffOp::Delete(line, has_nl) => {
                    let _ = writeln!(out, "-{line}");
                    if !has_nl {
                        let _ = writeln!(out, "\\ No newline at end of file");
                    }
                }
                DiffOp::Insert(line, has_nl) => {
                    let _ = writeln!(out, "+{line}");
                    if !has_nl {
                        let _ = writeln!(out, "\\ No newline at end of file");
                    }
                }
            }
        }
    }
}

fn compute_line_ops<'a>(old: &TextLines<'a>, new: &TextLines<'a>) -> Vec<DiffOp<'a>> {
    let n = old.lines.len();
    let m = new.lines.len();

    let line_has_nl = |lines: &TextLines<'a>, idx: usize| -> bool {
        idx + 1 < lines.lines.len() || lines.trailing_newline
    };

    // Guard DP memory on huge files by falling back to whole-file replacement above 4,000 lines.
    if n.saturating_mul(m) > 16_000_000 {
        let mut ops = Vec::with_capacity(n + m);
        for i in 0..n {
            ops.push(DiffOp::Delete(old.lines[i], line_has_nl(old, i)));
        }
        for j in 0..m {
            ops.push(DiffOp::Insert(new.lines[j], line_has_nl(new, j)));
        }
        return ops;
    }

    let width = m + 1;
    let mut dp = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let same_nl = line_has_nl(old, i) == line_has_nl(new, j);
            if old.lines[i] == new.lines[j] && same_nl {
                dp[i * width + j] = dp[(i + 1) * width + (j + 1)] + 1;
            } else {
                let down = dp[(i + 1) * width + j];
                let right = dp[i * width + (j + 1)];
                dp[i * width + j] = down.max(right);
            }
        }
    }

    let mut ops = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < n && j < m {
        let same_nl = line_has_nl(old, i) == line_has_nl(new, j);
        if old.lines[i] == new.lines[j] && same_nl {
            ops.push(DiffOp::Context(old.lines[i], line_has_nl(old, i)));
            i += 1;
            j += 1;
        } else if dp[(i + 1) * width + j] >= dp[i * width + (j + 1)] {
            ops.push(DiffOp::Delete(old.lines[i], line_has_nl(old, i)));
            i += 1;
        } else {
            ops.push(DiffOp::Insert(new.lines[j], line_has_nl(new, j)));
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffOp::Delete(old.lines[i], line_has_nl(old, i)));
        i += 1;
    }
    while j < m {
        ops.push(DiffOp::Insert(new.lines[j], line_has_nl(new, j)));
        j += 1;
    }
    ops
}
