//! Materialize clean CoW workspace changes onto a fresh Git branch in the Host repository.
//!
//! Uses a temporary detached worktree so the Host repository's checked-out branch
//! and working tree are never modified or clobbered.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use super::clone::is_git_repo_root;
use super::manifest::{ManifestEntry, TreeManifest, compute_diff_summary};
use super::types::{
    CowWorkspaceError, FileChangeKind, GitBranchExportReport, IntentProvenance, WorkspaceRef,
};

pub(crate) fn export_workspace_to_git_branch(
    workspace_ref: &WorkspaceRef,
    base: &TreeManifest,
    workspace_tree: &Path,
    host_root: &Path,
    excludes: &[String],
    branch_name: &str,
    provenance: &IntentProvenance,
) -> std::result::Result<GitBranchExportReport, CowWorkspaceError> {
    if !is_git_repo_root(host_root) {
        return Err(CowWorkspaceError::NotAGitRepository);
    }

    let branch_name = branch_name.trim();
    if branch_name.is_empty() {
        return Err(CowWorkspaceError::InvalidInput(
            "Git branch name must not be empty".to_string(),
        ));
    }

    let ref_check = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["check-ref-format", &format!("refs/heads/{branch_name}")])
        .output()?;
    if !ref_check.status.success() {
        return Err(CowWorkspaceError::InvalidInput(format!(
            "invalid Git branch name `{branch_name}`"
        )));
    }

    let exists = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch_name}"),
        ])
        .status()?;
    if exists.success() {
        return Err(CowWorkspaceError::BranchAlreadyExists(
            branch_name.to_string(),
        ));
    }

    let head_check = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()?;
    if !head_check.status.success() {
        return Err(CowWorkspaceError::UnbornGitHead);
    }

    let workspace_now = TreeManifest::scan(workspace_tree, excludes)?;
    let diff_summary = compute_diff_summary(base, &workspace_now);
    if diff_summary.is_empty() {
        return Err(CowWorkspaceError::NoChangesToExport);
    }

    let temp_parent = tempfile::tempdir()?;
    let temp_wt = temp_parent.path().join("branch-export");
    let add_out = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["worktree", "add", "--detach"])
        .arg(&temp_wt)
        .arg("HEAD")
        .output()?;
    if !add_out.status.success() {
        return Err(CowWorkspaceError::Other(anyhow::anyhow!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&add_out.stderr).trim()
        )));
    }

    let result = apply_commit_and_branch(
        &temp_wt,
        host_root,
        workspace_tree,
        &workspace_now,
        &diff_summary,
        branch_name,
        provenance,
    );

    let _ = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["worktree", "remove", "--force"])
        .arg(&temp_wt)
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["worktree", "prune"])
        .output();

    let commit_oid = result?;
    Ok(GitBranchExportReport {
        workspace_ref: workspace_ref.clone(),
        branch_name: branch_name.to_string(),
        commit_oid,
        diff_summary,
        provenance: provenance.clone(),
    })
}

fn apply_commit_and_branch(
    temp_wt: &Path,
    host_root: &Path,
    workspace_tree: &Path,
    workspace_now: &TreeManifest,
    diff_summary: &super::types::DiffSummary,
    branch_name: &str,
    provenance: &IntentProvenance,
) -> std::result::Result<String, CowWorkspaceError> {
    for changed in &diff_summary.changed_paths {
        let rel = &changed.path;
        let dst = temp_wt.join(rel);
        match changed.kind {
            FileChangeKind::Deleted => {
                if dst.symlink_metadata().is_ok() {
                    fs::remove_file(&dst)?;
                }
            }
            FileChangeKind::Added | FileChangeKind::Modified => {
                let entry = workspace_now
                    .entries
                    .get(rel)
                    .context("missing changed workspace entry")?;
                copy_entry_into_worktree(workspace_tree, rel, entry, &dst)?;
            }
        }
    }

    let stage_out = Command::new("git")
        .arg("-C")
        .arg(temp_wt)
        .args(["add", "-A"])
        .output()?;
    if !stage_out.status.success() {
        return Err(CowWorkspaceError::Other(anyhow::anyhow!(
            "git add failed: {}",
            String::from_utf8_lossy(&stage_out.stderr).trim()
        )));
    }

    let commit_msg = format!(
        "{}\n\nAlan-Workstream: {}\nAlan-Rollout: {}\n",
        provenance.why, provenance.workstream, provenance.rollout_ref
    );

    let mut commit_cmd = Command::new("git");
    commit_cmd.arg("-C").arg(temp_wt);
    if !git_config_present(host_root, "user.name") {
        commit_cmd.env("GIT_AUTHOR_NAME", "Alan");
        commit_cmd.env("GIT_COMMITTER_NAME", "Alan");
    }
    if !git_config_present(host_root, "user.email") {
        commit_cmd.env("GIT_AUTHOR_EMAIL", "alan@local");
        commit_cmd.env("GIT_COMMITTER_EMAIL", "alan@local");
    }
    let commit_out = commit_cmd
        .args(["commit", "--allow-empty", "-m", &commit_msg])
        .output()?;
    if !commit_out.status.success() {
        return Err(CowWorkspaceError::Other(anyhow::anyhow!(
            "git commit failed: {}",
            String::from_utf8_lossy(&commit_out.stderr).trim()
        )));
    }

    let oid_out = Command::new("git")
        .arg("-C")
        .arg(temp_wt)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !oid_out.status.success() {
        return Err(CowWorkspaceError::Other(anyhow::anyhow!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&oid_out.stderr).trim()
        )));
    }
    let commit_oid = String::from_utf8_lossy(&oid_out.stdout).trim().to_string();

    let branch_out = Command::new("git")
        .arg("-C")
        .arg(temp_wt)
        .args(["branch", branch_name, &commit_oid])
        .output()?;
    if !branch_out.status.success() {
        return Err(CowWorkspaceError::Other(anyhow::anyhow!(
            "git branch failed: {}",
            String::from_utf8_lossy(&branch_out.stderr).trim()
        )));
    }

    Ok(commit_oid)
}

fn copy_entry_into_worktree(
    workspace_tree: &Path,
    rel: &str,
    entry: &ManifestEntry,
    dst: &Path,
) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    match entry {
        ManifestEntry::File { mode, .. } => {
            let bytes = fs::read(workspace_tree.join(rel))?;
            if let Ok(meta) = fs::symlink_metadata(dst)
                && meta.file_type().is_symlink()
            {
                fs::remove_file(dst)?;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(dst)?;
            file.write_all(&bytes)?;
            file.flush()?;
            fs::set_permissions(dst, fs::Permissions::from_mode(*mode))?;
        }
        ManifestEntry::Symlink { target } => {
            if fs::symlink_metadata(dst).is_ok() {
                fs::remove_file(dst)?;
            }
            std::os::unix::fs::symlink(target, dst)?;
        }
    }
    Ok(())
}

fn git_config_present(repo: &Path, key: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["config", "--get", key])
        .output()
        .is_ok_and(|out| out.status.success() && !out.stdout.is_empty())
}
