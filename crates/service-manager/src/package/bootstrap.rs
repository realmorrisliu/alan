//! Boot-only lock acquisition budget; ordinary Package operations remain bounded at 500 ms.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};

use super::PackageService;

pub(super) struct BootstrapWait {
    deadline: Instant,
    cancelled: AtomicBool,
    finished: AtomicBool,
}

impl BootstrapWait {
    pub(super) fn deadline(&self) -> Result<Option<Instant>> {
        if self.finished.load(Ordering::Acquire) {
            return Ok(None);
        }
        ensure!(
            !self.cancelled.load(Ordering::Acquire),
            "Package bootstrap cancelled"
        );
        Ok(Some(self.deadline))
    }
}

/// Dropping the boot future cancels any outstanding Package lock wait. The budget
/// is shared across open, seeding, and initial reference acquisition, not renewed
/// for each transaction. No durable transaction work is interrupted after locking.
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
                deadline: Instant::now() + budget,
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
