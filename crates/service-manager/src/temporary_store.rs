//! Ownership evidence for disposable service stores, including abrupt-exit cleanup.

use std::fs::{File, OpenOptions};
use std::path::Path;

use anyhow::Result;

pub(crate) struct TemporaryStore {
    directory: tempfile::TempDir,
    // Retain the marker lock until after the directory is removed.
    _lease: File,
}

impl TemporaryStore {
    pub(crate) fn new(kind: &'static str) -> Result<Self> {
        Self::new_in(&std::env::temp_dir(), kind)
    }

    pub(crate) fn new_in(parent: &Path, kind: &'static str) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("alan-service-scratch-")
            .tempdir_in(parent)?;
        let mut lease = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(directory.path().join(".alan-scratch.json"))?;
        lease.lock_shared()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            lease.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            let root = directory.path().canonicalize()?;
            let metadata = root.metadata()?;
            serde_json::to_writer(
                &mut lease,
                &serde_json::json!({
                    "version": 1,
                    "kind": kind,
                    "root": root,
                    "pid": std::process::id(),
                    "identity": [metadata.dev(), metadata.ino()],
                }),
            )?;
            lease.sync_all()?;
        }
        Ok(Self {
            directory,
            _lease: lease,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        self.directory.path()
    }

    pub(crate) fn close(self) -> Result<()> {
        self.directory.close()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_identifies_the_store_and_is_locked_for_its_lifetime() {
        let store = TemporaryStore::new("connection").unwrap();
        let root = store.path().to_path_buf();
        let marker = File::open(root.join(".alan-scratch.json")).unwrap();
        assert!(marker.try_lock().is_err());
        let receipt: serde_json::Value = serde_json::from_reader(&marker).unwrap();
        assert_eq!(receipt["kind"], "connection");
        assert_eq!(receipt["pid"], std::process::id());
        assert_eq!(
            receipt["root"],
            root.canonicalize().unwrap().to_str().unwrap()
        );
        drop(marker);
        store.close().unwrap();
        assert!(!root.exists());
    }
}
