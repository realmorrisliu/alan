//! Host-owned saved project delegation grant store and identity revalidation.
//!
//! Saved grants live strictly in System Store (`services/host-mount/grants.json`),
//! never auto-discovered from repository files. Each grant binds a canonical directory
//! path + Unix `(dev, ino)` identity + monotonic revision and rejects symlinks,
//! directory replacements, or revoked revisions before activation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use alan_agent_engine::GitPushDelegationScope;
use alan_kernel::Pid;
use alan_service_manager::{HostMountAccess, HostMountGrantRecord, HostMountService};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::native_export;

const STORE_VERSION: u32 = 1;

/// Narrow ordinary `git push` rule saved inside a project delegation grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedGitPushRule {
    /// Allowed git remote name (`None` defaults to `"origin"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// Allowed exact branch names or trailing-`*` prefix patterns (e.g. `"feat/*"`).
    pub branches: Vec<String>,
}

/// Persisted Host-side standing project delegation grant record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedProjectGrantRecord {
    pub id: String,
    pub label: String,
    pub namespace_path: String,
    pub canonical_path: PathBuf,
    pub dev: u64,
    pub ino: u64,
    pub access: HostMountAccess,
    pub revision: u64,
    pub revoked: bool,
    pub created_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub git_push_rules: Vec<SavedGitPushRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedGrantStoreFile {
    version: u32,
    grants: BTreeMap<String, SavedProjectGrantRecord>,
}

impl Default for SavedGrantStoreFile {
    fn default() -> Self {
        Self {
            version: STORE_VERSION,
            grants: BTreeMap::new(),
        }
    }
}

/// System-Store-backed persistence for explicit standing Host Mount delegations.
#[derive(Clone, Debug)]
pub struct SavedProjectGrantStore {
    path: PathBuf,
}

impl SavedProjectGrantStore {
    /// Open the default System Store saved-grant file (`services/host-mount/grants.json`).
    pub fn system() -> Result<Self> {
        Ok(Self {
            path: crate::SystemStorePaths::detect()?.host_mount_grants()?,
        })
    }

    /// Open a saved-grant file at an explicit app-owned path (for tests/isolated hosts).
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Save or update an explicit standing project delegation grant after validating
    /// non-symlink canonical directory identity and Unix `(dev, ino)` metadata.
    pub fn save_grant(
        &self,
        id: &str,
        host_path: &Path,
        access: HostMountAccess,
        label: Option<&str>,
        git_push_rules: Vec<SavedGitPushRule>,
    ) -> Result<SavedProjectGrantRecord> {
        validate_grant_id(id)?;
        validate_git_push_rules(&git_push_rules)?;

        let (canonical_path, dev, ino) = inspect_non_symlink_directory(host_path)?;
        let label = label
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                canonical_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(id)
                    .to_string()
            });
        let namespace_path = format!("/mnt/grants/{id}");

        let mut file = self.load()?;
        let revision = file
            .grants
            .get(id)
            .map_or(1, |existing| existing.revision.saturating_add(1));
        let record = SavedProjectGrantRecord {
            id: id.to_string(),
            label,
            namespace_path,
            canonical_path,
            dev,
            ino,
            access,
            revision,
            revoked: false,
            created_at_ms: now_ms(),
            revoked_at_ms: None,
            git_push_rules,
        };
        file.grants.insert(id.to_string(), record.clone());
        self.persist(&file)?;
        Ok(record)
    }

    /// Return all saved project delegation grants ordered by grant id.
    pub fn list_grants(&self) -> Result<Vec<SavedProjectGrantRecord>> {
        let file = self.load()?;
        Ok(file.grants.into_values().collect())
    }

    /// Look up one saved grant record without modifying it.
    pub fn get_grant(&self, id: &str) -> Result<SavedProjectGrantRecord> {
        let file = self.load()?;
        file.grants
            .get(id)
            .cloned()
            .with_context(|| format!("unknown saved project grant `{id}`"))
    }

    /// Instantly revoke a saved grant by bumping revision and setting `revoked = true`.
    pub fn revoke_grant(&self, id: &str) -> Result<SavedProjectGrantRecord> {
        let mut file = self.load()?;
        let record = file
            .grants
            .get_mut(id)
            .with_context(|| format!("unknown saved project grant `{id}`"))?;
        if !record.revoked {
            record.revoked = true;
            record.revision = record.revision.saturating_add(1);
            record.revoked_at_ms = Some(now_ms());
        }
        let cloned = record.clone();
        self.persist(&file)?;
        Ok(cloned)
    }

    /// Revalidate canonical path, non-symlink status, Unix `(dev, ino)`, and non-revoked
    /// state before mounting or replaying a saved grant.
    pub fn revalidate_grant(&self, id: &str) -> Result<SavedProjectGrantRecord> {
        let record = self.get_grant(id)?;
        ensure!(
            !record.revoked,
            "saved project grant `{id}` has been revoked"
        );
        let (canonical_now, dev_now, ino_now) =
            inspect_non_symlink_directory(&record.canonical_path).with_context(|| {
                format!(
                    "saved project grant `{id}` directory {} is no longer valid",
                    record.canonical_path.display()
                )
            })?;
        ensure!(
            canonical_now == record.canonical_path
                && dev_now == record.dev
                && ino_now == record.ino,
            "saved project grant `{id}` identity mismatch (expected {} dev={} ino={}, got {} dev={} ino={}); directory replaced or moved",
            record.canonical_path.display(),
            record.dev,
            record.ino,
            canonical_now.display(),
            dev_now,
            ino_now
        );
        Ok(record)
    }

    /// Revalidate and activate a saved grant onto a running Process in `HostMountService`.
    pub fn activate_grant_for_process(
        &self,
        id: &str,
        service: &HostMountService,
        pid: Pid,
    ) -> Result<HostMountGrantRecord> {
        let grant = self.revalidate_grant(id)?;
        let export = native_export(&grant.namespace_path, &grant.canonical_path, grant.access)?;
        let scopes = grant
            .git_push_rules
            .iter()
            .map(|rule| GitPushDelegationScope {
                grant_id: grant.id.clone(),
                namespace_root: PathBuf::from(&grant.namespace_path),
                remote: rule.remote.clone(),
                branches: rule.branches.clone(),
            })
            .collect();
        service.grant_direct_with_scopes(
            pid,
            grant.id,
            grant.label,
            grant.namespace_path,
            grant.access,
            export,
            format!("saved-grant-rev-{}", grant.revision),
            scopes,
        )
    }

    fn load(&self) -> Result<SavedGrantStoreFile> {
        match std::fs::read(&self.path) {
            Ok(raw) => {
                let parsed: SavedGrantStoreFile = serde_json::from_slice(&raw)
                    .with_context(|| format!("parse saved grants at {}", self.path.display()))?;
                ensure!(
                    parsed.version == STORE_VERSION,
                    "unsupported saved grant store version {}",
                    parsed.version
                );
                Ok(parsed)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Ok(SavedGrantStoreFile::default())
            }
            Err(err) => {
                Err(err).with_context(|| format!("read saved grants at {}", self.path.display()))
            }
        }
    }

    fn persist(&self, file: &SavedGrantStoreFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create saved grant directory {}", parent.display()))?;
        }
        let encoded = serde_json::to_vec_pretty(file).context("serialize saved grants")?;
        let tmp_name = format!(
            ".{}.tmp-{}",
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("grants.json"),
            std::process::id()
        );
        let tmp_path = self.path.with_file_name(tmp_name);
        std::fs::write(&tmp_path, encoded)
            .with_context(|| format!("write temporary saved grants {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, &self.path)
            .with_context(|| format!("replace saved grants at {}", self.path.display()))?;
        Ok(())
    }
}

fn validate_grant_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "invalid grant id `{id}`; use 1..=64 ASCII alphanumeric, `-`, or `_` characters"
    );
    Ok(())
}

fn validate_git_push_rules(rules: &[SavedGitPushRule]) -> Result<()> {
    for rule in rules {
        if let Some(remote) = rule.remote.as_deref() {
            ensure!(
                !remote.trim().is_empty() && !remote.contains('/'),
                "invalid delegated git remote `{remote}`"
            );
        }
        ensure!(
            !rule.branches.is_empty()
                && rule
                    .branches
                    .iter()
                    .all(|branch| !branch.trim().is_empty() && !branch.starts_with('*')),
            "delegated git push rules require explicit branch names or trailing-`*` prefixes"
        );
    }
    Ok(())
}

fn inspect_non_symlink_directory(path: &Path) -> Result<(PathBuf, u64, u64)> {
    let symlink_meta = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspect directory {}", path.display()))?;
    ensure!(
        !symlink_meta.file_type().is_symlink(),
        "host grant directory {} must not be a symlink",
        path.display()
    );
    ensure!(
        symlink_meta.is_dir(),
        "host grant target {} is not a directory",
        path.display()
    );
    let canonical = dunce::canonicalize(path)
        .with_context(|| format!("canonicalize directory {}", path.display()))?;
    let canonical_meta = std::fs::symlink_metadata(&canonical)
        .with_context(|| format!("inspect canonical directory {}", canonical.display()))?;
    ensure!(
        canonical_meta.is_dir() && !canonical_meta.file_type().is_symlink(),
        "canonical grant directory {} must be a real directory",
        canonical.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            symlink_meta.dev() == canonical_meta.dev()
                && symlink_meta.ino() == canonical_meta.ino(),
            "host grant path {} crosses symlinked parent identity mismatch",
            path.display()
        );
        Ok((canonical, canonical_meta.dev(), canonical_meta.ino()))
    }
    #[cfg(not(unix))]
    {
        Ok((canonical, 0, 0))
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::tools::ToolExecutionAuthority;
    use std::sync::Arc;

    #[test]
    fn saved_grant_revalidates_inode_rejects_replacement_and_revokes_instantly() {
        let store_dir = tempfile::tempdir().unwrap();
        let store = SavedProjectGrantStore::from_path(store_dir.path().join("grants.json"));

        let work = tempfile::tempdir().unwrap();
        let repo = work.path().join("repo");
        std::fs::create_dir(&repo).unwrap();

        let saved = store
            .save_grant(
                "repo",
                &repo,
                HostMountAccess::ReadWrite,
                Some("Repo"),
                vec![SavedGitPushRule {
                    remote: Some("origin".to_string()),
                    branches: vec!["feat/*".to_string()],
                }],
            )
            .unwrap();
        assert_eq!(saved.revision, 1);
        assert!(!saved.revoked);

        // Live ServiceManager HostMountService projection activates cleanly.
        let service = HostMountService::new(Arc::new(super::super::NativeHostMountExportAdapter));
        let pid = Pid(42);
        let ns = alan_kernel::LiveNamespace::new(alan_kernel::Namespace::new());
        service.register_process(pid, ns);
        let active = store
            .activate_grant_for_process("repo", &service, pid)
            .unwrap();
        assert!(active.active);
        assert_eq!(service.git_push_delegations(42).len(), 1);
        assert_eq!(
            service.project_knowledge_roots(42),
            vec![PathBuf::from("/mnt/grants/repo")]
        );

        // Instant revocation drops live mount, git_push delegations, and project knowledge roots.
        service.revoke("repo", "operator").unwrap();
        assert!(service.git_push_delegations(42).is_empty());
        assert!(service.project_knowledge_roots(42).is_empty());

        let revoked = store.revoke_grant("repo").unwrap();
        assert!(revoked.revoked);
        assert!(store.revalidate_grant("repo").is_err());

        // Directory replacement with same path fails inode/dev revalidation even if re-saved earlier.
        let replaced = tempfile::tempdir().unwrap();
        let dir_b = replaced.path().join("swap");
        std::fs::create_dir(&dir_b).unwrap();
        store
            .save_grant("swap", &dir_b, HostMountAccess::ReadOnly, None, Vec::new())
            .unwrap();
        std::fs::remove_dir(&dir_b).unwrap();
        std::fs::create_dir(&dir_b).unwrap();
        assert!(
            store.revalidate_grant("swap").is_err(),
            "replaced directory must fail inode/identity revalidation"
        );

        // Symlink grant root is rejected on save.
        #[cfg(unix)]
        {
            let link_path = work.path().join("link-repo");
            std::os::unix::fs::symlink(&repo, &link_path).unwrap();
            assert!(
                store
                    .save_grant(
                        "link",
                        &link_path,
                        HostMountAccess::ReadOnly,
                        None,
                        Vec::new()
                    )
                    .is_err()
            );
        }
    }
}
