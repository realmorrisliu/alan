//! Boot-only lock acquisition budget; ordinary Package operations remain bounded at 500 ms.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, ensure};

use super::PackageService;

#[cfg(test)]
#[path = "tests/bootstrap_budget_owner.rs"]
mod budget_owner_tests;

pub(super) struct BootstrapWait {
    remaining: Mutex<Duration>,
    cancelled: AtomicBool,
    finished: AtomicBool,
}

impl BootstrapWait {
    pub(super) fn remaining(&self) -> Result<Option<Duration>> {
        if self.finished.load(Ordering::Acquire) {
            return Ok(None);
        }
        ensure!(
            !self.cancelled.load(Ordering::Acquire),
            "Package bootstrap cancelled"
        );
        Ok(Some(
            *self.remaining.lock().expect("bootstrap wait poisoned"),
        ))
    }

    pub(super) fn charge_wait(&self, elapsed: Duration) {
        let mut remaining = self.remaining.lock().expect("bootstrap wait poisoned");
        *remaining = remaining.saturating_sub(elapsed);
    }
}

/// Dropping the boot future cancels any outstanding Package lock wait.
/// Actual contention wait is shared across open, seeding, and initial reference
/// acquisition, not renewed for each transaction. Unrelated setup and durable
/// transaction work consume no allowance and are never interrupted after locking.
pub(crate) struct PackageBootstrap {
    wait: Arc<BootstrapWait>,
}

impl PackageBootstrap {
    pub(crate) fn new() -> Self {
        Self::with_budget(Duration::from_secs(10))
    }

    fn with_budget(budget: Duration) -> Self {
        Self {
            wait: Arc::new(BootstrapWait {
                remaining: Mutex::new(budget),
                cancelled: AtomicBool::new(false),
                finished: AtomicBool::new(false),
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn open(
        &self,
        channel_id: String,
        store_root: Option<std::path::PathBuf>,
    ) -> Result<Arc<PackageService>> {
        PackageService::open_bootstrap(channel_id, store_root, self.wait.clone())
    }

    pub(crate) fn opener(
        &self,
        channel_id: String,
        store_root: Option<std::path::PathBuf>,
    ) -> impl FnOnce() -> Result<Arc<PackageService>> + Send + 'static {
        let wait = self.wait.clone();
        move || PackageService::open_bootstrap(channel_id, store_root, wait)
    }

    pub(crate) fn finish(self) {
        self.wait.finished.store(true, Ordering::Release);
    }
}

impl Drop for PackageBootstrap {
    fn drop(&mut self) {
        self.wait.cancelled.store(true, Ordering::Release);
    }
}

impl PackageService {
    #[cfg(test)]
    pub(crate) fn bootstrap(
        channel_id: impl Into<String>,
        store_root: Option<std::path::PathBuf>,
    ) -> Result<(Arc<Self>, PackageBootstrap)> {
        Self::bootstrap_with_budget(channel_id.into(), store_root, Duration::from_secs(10))
    }

    #[cfg(test)]
    pub(super) fn bootstrap_with_budget(
        channel_id: String,
        store_root: Option<std::path::PathBuf>,
        budget: Duration,
    ) -> Result<(Arc<Self>, PackageBootstrap)> {
        let guard = PackageBootstrap::with_budget(budget);
        let service = guard.open(channel_id, store_root)?;
        Ok((service, guard))
    }

    fn open_bootstrap(
        channel_id: String,
        store_root: Option<std::path::PathBuf>,
        wait: Arc<BootstrapWait>,
    ) -> Result<Arc<Self>> {
        let temporary = if store_root.is_none() {
            Some(
                tempfile::Builder::new()
                    .prefix("alan-package-service-")
                    .tempdir()?,
            )
        } else {
            None
        };
        let root = store_root.unwrap_or_else(|| temporary.as_ref().unwrap().path().to_path_buf());
        Self::open_inner(channel_id, root, temporary, Some(wait))
    }
}
