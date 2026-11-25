//! Per-stream advisory/flock lease locks under `streams/<stream_name>.lock`.
//!
//! Prevents concurrent interactive sessions and scheduled Stateful Cron Streams
//! from corrupting a named workstream's checkpoint or rollout pointer.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{
    STREAMS_DIR_NAME, ensure_projects_subdir, validate_identifier, validate_project_key,
    validate_projects_root, verify_existing_regular_file,
};

const STREAM_LEASE_VERSION: u16 = 1;

/// Owner kind holding a per-stream lease lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamLeaseHolderKind {
    Interactive,
    Cron,
}

/// Metadata written inside `streams/<stream_name>.lock` while locked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamLeaseMetadata {
    pub version: u16,
    pub stream_name: String,
    pub holder_kind: StreamLeaseHolderKind,
    pub pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    pub acquired_at: String,
}

/// Result of attempting to acquire `streams/<stream_name>.lock`.
#[derive(Debug)]
pub enum StreamLeaseAcquireResult {
    Acquired(StreamLeaseGuard),
    Contention { holder: Option<StreamLeaseMetadata> },
}

/// RAII lease lock guard holding both an in-process reservation and OS `flock` on
/// `<projects_root>/<project_key>/streams/<stream_name>.lock`.
#[derive(Debug)]
pub struct StreamLeaseGuard {
    canonical_lock_path: PathBuf,
    file: Option<File>,
    metadata: StreamLeaseMetadata,
}

impl StreamLeaseGuard {
    /// Return `<projects_root>/<project_key>/streams/<name>.lock`.
    pub fn lock_path(projects_root: &Path, project_key: &str, name: &str) -> Result<PathBuf> {
        validate_projects_root(projects_root)?;
        validate_project_key(project_key)?;
        validate_identifier("stream_name", name)?;
        Ok(projects_root
            .join(project_key)
            .join(STREAMS_DIR_NAME)
            .join(format!("{name}.lock")))
    }

    /// Non-blockingly try to acquire the exclusive lease lock for `stream_name`.
    ///
    /// Returns [`StreamLeaseAcquireResult::Contention`] cleanly if another interactive
    /// session or scheduled cron run currently holds the lock.
    pub fn try_acquire(
        projects_root: &Path,
        project_key: &str,
        stream_name: &str,
        holder_kind: StreamLeaseHolderKind,
        workflow_id: Option<&str>,
    ) -> Result<StreamLeaseAcquireResult> {
        validate_project_key(project_key)?;
        validate_identifier("stream_name", stream_name)?;
        let s_dir = ensure_projects_subdir(projects_root, &[project_key, STREAMS_DIR_NAME])?;
        let lock_path = s_dir.join(format!("{stream_name}.lock"));
        let _ = verify_existing_regular_file(&s_dir, &lock_path)?;

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("open stream lease lock {}", lock_path.display()))?;
        let canonical_lock_path = fs::canonicalize(&lock_path)
            .with_context(|| format!("canonicalize stream lease lock {}", lock_path.display()))?;
        let canonical_dir = fs::canonicalize(&s_dir)?;
        ensure!(
            canonical_lock_path.starts_with(&canonical_dir),
            "stream lease lock escapes project store: {}",
            lock_path.display()
        );

        let metadata = StreamLeaseMetadata {
            version: STREAM_LEASE_VERSION,
            stream_name: stream_name.to_string(),
            holder_kind,
            pid: std::process::id(),
            workflow_id: workflow_id
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            acquired_at: Utc::now().to_rfc3339(),
        };

        // 1. Check in-process lease table so intra-process tasks contend deterministically.
        {
            let mut active = in_process_leases()
                .lock()
                .expect("in-process stream leases mutex poisoned");
            if let Some(existing) = active.get(&canonical_lock_path).cloned() {
                return Ok(StreamLeaseAcquireResult::Contention {
                    holder: Some(existing),
                });
            }
            active.insert(canonical_lock_path.clone(), metadata.clone());
        }

        // 2. Acquire OS exclusive `flock` so separate `alan` / `alan workflow` processes contend.
        match file.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => {
                in_process_leases()
                    .lock()
                    .expect("in-process stream leases mutex poisoned")
                    .remove(&canonical_lock_path);
                let holder = read_lock_metadata_best_effort(&mut file);
                return Ok(StreamLeaseAcquireResult::Contention { holder });
            }
            Err(fs::TryLockError::Error(err)) => {
                in_process_leases()
                    .lock()
                    .expect("in-process stream leases mutex poisoned")
                    .remove(&canonical_lock_path);
                return Err(err)
                    .with_context(|| format!("acquire stream lease lock {}", lock_path.display()));
            }
        }

        let encoded = serde_json::to_vec_pretty(&metadata)?;
        if let Err(err) = (|| -> std::io::Result<()> {
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&encoded)?;
            file.write_all(b"\n")?;
            file.flush()?;
            Ok(())
        })() {
            let _ = file.unlock();
            in_process_leases()
                .lock()
                .expect("in-process stream leases mutex poisoned")
                .remove(&canonical_lock_path);
            return Err(err)
                .with_context(|| format!("write stream lease metadata {}", lock_path.display()));
        }

        Ok(StreamLeaseAcquireResult::Acquired(Self {
            canonical_lock_path,
            file: Some(file),
            metadata,
        }))
    }

    /// Inspect metadata recorded for this held lease.
    pub fn metadata(&self) -> &StreamLeaseMetadata {
        &self.metadata
    }
}

impl Drop for StreamLeaseGuard {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            let _ = file.unlock();
        }
        in_process_leases()
            .lock()
            .expect("in-process stream leases mutex poisoned")
            .remove(&self.canonical_lock_path);
    }
}

fn in_process_leases() -> &'static Mutex<HashMap<PathBuf, StreamLeaseMetadata>> {
    static LEASES: OnceLock<Mutex<HashMap<PathBuf, StreamLeaseMetadata>>> = OnceLock::new();
    LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn read_lock_metadata_best_effort(file: &mut File) -> Option<StreamLeaseMetadata> {
    let mut raw = String::new();
    let _ = file.seek(SeekFrom::Start(0));
    let _ = file.read_to_string(&mut raw);
    serde_json::from_str::<StreamLeaseMetadata>(raw.trim()).ok()
}
