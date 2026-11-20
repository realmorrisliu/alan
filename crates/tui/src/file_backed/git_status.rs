use std::path::{Path, PathBuf};

/// Branch and dirty-tree status observed for an explicitly mounted project root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_backed) struct GitStatus {
    pub branch: String,
    pub dirty: bool,
}

impl GitStatus {
    /// Format a compact status chip label (`git:<branch>[*]`).
    pub fn label(&self) -> String {
        if self.dirty {
            format!("git:{}*", self.branch)
        } else {
            format!("git:{}", self.branch)
        }
    }
}

pub(in crate::file_backed) fn read_git_status_for_root(root: &Path) -> Option<GitStatus> {
    let branch = read_branch_with_git(root).or_else(|| read_head_file_branch(root))?;
    let dirty = check_dirty_with_git(root).unwrap_or(false);
    Some(GitStatus { branch, dirty })
}

fn read_branch_with_git(root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "HEAD" {
        return read_short_sha_with_git(root).or_else(|| Some("HEAD".to_string()));
    }
    Some(trimmed.to_string())
}

fn read_short_sha_with_git(root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?;
    let sha = sha.trim();
    (!sha.is_empty()).then(|| sha.to_string())
}

fn check_dirty_with_git(root: &Path) -> Option<bool> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain", "-uno"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(!output.stdout.is_empty())
}

fn read_head_file_branch(root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(root.join(".git/HEAD")).ok()?;
    let trimmed = head.trim();
    if let Some(reference) = trimmed.strip_prefix("ref: ") {
        let short = reference
            .strip_prefix("refs/heads/")
            .unwrap_or(reference)
            .trim();
        return (!short.is_empty()).then(|| short.to_string());
    }
    let short_sha: String = trimmed.chars().take(7).collect();
    (!short_sha.is_empty()).then_some(short_sha)
}

pub(super) fn spawn_refresh(
    root: PathBuf,
    jobs: &mut tokio::task::JoinSet<()>,
    tx: &tokio::sync::mpsc::Sender<super::FileBackedEvent>,
) {
    let tx = tx.clone();
    jobs.spawn(async move {
        let status = tokio::task::spawn_blocking(move || (read_git_status_for_root(&root), root))
            .await
            .ok()
            .map(|(status, root)| (root, status));
        if let Some((root, status)) = status {
            let _ = tx
                .send(super::FileBackedEvent::GitStatusUpdated { root, status })
                .await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_branch_and_dirty_flag_for_explicit_repo_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(read_git_status_for_root(root).is_none());

        let init = std::process::Command::new("git")
            .args(["init", "-b", "feature/m2"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(init.status.success());

        std::fs::write(root.join("a.txt"), "hello\n").unwrap();
        let add = std::process::Command::new("git")
            .args(["add", "a.txt"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(add.status.success());
        let commit = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init",
            ])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(commit.status.success());

        let clean = read_git_status_for_root(root).unwrap();
        assert_eq!(clean.branch, "feature/m2");
        assert!(!clean.dirty);
        assert_eq!(clean.label(), "git:feature/m2");

        std::fs::write(root.join("a.txt"), "modified\n").unwrap();
        let dirty = read_git_status_for_root(root).unwrap();
        assert_eq!(dirty.branch, "feature/m2");
        assert!(dirty.dirty);
        assert_eq!(dirty.label(), "git:feature/m2*");
    }
}
