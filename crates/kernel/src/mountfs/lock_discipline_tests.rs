//! Lock discipline for the mount table.
//!
//! `MountFs::state` is a fair `tokio::sync::Mutex`. A waiter that has been *handed
//! the permit* and then stops being polled keeps it, so every later waiter queues
//! behind it while no guard is alive. That is the shape behind the redirected-input
//! stall: one `select!` arm held a pinned future that had queued on this lock while
//! the other arm's body awaited the root-agent probe.
//!
//! Two things are pinned here:
//! - the mechanism, with the waiter's and the prober's task ids recorded so the
//!   "same task re-locked" explanation is excluded by measurement, not by argument
//!   (`just lock-discipline` builds with `--cfg tokio_unstable` to print real ids);
//! - the cancel pressure the product path applies, as walk/clunk/cancel rounds over
//!   a backing server the test can hold open.
#![allow(
    unexpected_cfgs,
    reason = "task ids are read only under a `--cfg tokio_unstable` build"
)]

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc as StdArc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::Notify;

/// Current tokio task id, when the build enables `--cfg tokio_unstable`.
fn task_id() -> Option<String> {
    #[cfg(tokio_unstable)]
    {
        tokio::task::try_id().map(|id| format!("{id:?}"))
    }
    #[cfg(not(tokio_unstable))]
    {
        None
    }
}

fn show_id(id: Option<String>) -> String {
    match id {
        Some(id) => format!("task {id}"),
        None => "task id unavailable (build with --cfg tokio_unstable)".to_string(),
    }
}

/// Backing server whose walks park until the test opens the gate.
struct GatedFs {
    inner: Arc<dyn FileServer>,
    open: AtomicBool,
    changed: Notify,
}

impl GatedFs {
    fn new(inner: Arc<dyn FileServer>) -> Self {
        Self {
            inner,
            open: AtomicBool::new(true),
            changed: Notify::new(),
        }
    }

    fn hold(&self) {
        self.open.store(false, Ordering::SeqCst);
    }

    fn release(&self) {
        self.open.store(true, Ordering::SeqCst);
        self.changed.notify_waiters();
    }

    async fn wait_open(&self) {
        loop {
            let changed = self.changed.notified();
            if self.open.load(Ordering::SeqCst) {
                return;
            }
            changed.await;
        }
    }
}

#[async_trait]
impl FileServer for GatedFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.wait_open().await;
        self.inner.walk(fid, newfid, names).await
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }
}

fn mounted_fs(backing: Arc<dyn FileServer>) -> Arc<MountFs> {
    let mut ns = Namespace::new();
    ns.mount(
        "/data",
        InProcessTransport::new(backing),
        crate::namespace::Access::ReadWrite,
    );
    Arc::new(MountFs::new(ns))
}

async fn assert_mount_table_is_free(fs: &MountFs) {
    let guard = tokio::time::timeout(Duration::from_millis(250), fs.state.lock())
        .await
        .expect("mount table lock must be free");
    drop(guard);
}

/// A waiter handed the permit keeps it while it is not polled, and the prober that
/// starves behind it is a *different* task: this excludes the "same task re-locked"
/// explanation by measurement.
#[tokio::test]
async fn an_unpolled_waiter_holds_the_permit_and_a_different_task_starves() {
    let fs = mounted_fs(Arc::new(alan_ap::reference::MemFs::new()));
    let names = vec!["data".to_string()];
    let queued = StdArc::new(Notify::new());
    let resume = StdArc::new(Notify::new());
    let waiter_id: StdArc<StdMutex<Option<String>>> = StdArc::new(StdMutex::new(None));

    // Hold the mount table so the walker below has to queue.
    let guard = fs.state.lock().await;
    let walker = {
        let fs = fs.clone();
        let queued = queued.clone();
        let resume = resume.clone();
        let waiter_id = waiter_id.clone();
        tokio::spawn(async move {
            *waiter_id.lock().expect("id slot") = task_id();
            let walk = fs.walk(Fid::ROOT, Fid(50_001), &names);
            tokio::pin!(walk);
            // One poll: the lock acquisition joins the FIFO queue, then this future is
            // parked for good while the task awaits the gate instead.
            let expired = tokio::time::Instant::now();
            let _first_poll = tokio::time::timeout_at(expired, &mut walk).await;
            queued.notify_waiters();
            resume.notified().await;
            tokio::time::timeout(Duration::from_secs(1), &mut walk)
                .await
                .expect("resumed waiter must finish")
        })
    };
    tokio::time::timeout(Duration::from_secs(1), queued.notified())
        .await
        .expect("the walker must reach the lock queue");
    drop(guard);

    // A different task probes the (free) mount table and starves behind the permit
    // the unpolled waiter is holding.
    let starved = {
        let fs = fs.clone();
        tokio::spawn(async move {
            let prober = task_id();
            let blocked = tokio::time::timeout(Duration::from_millis(150), fs.state.lock()).await;
            (prober, blocked.is_err())
        })
    }
    .await
    .expect("prober task must not panic");
    let (prober_id, starved) = starved;
    let waiter_id = waiter_id.lock().expect("id slot").clone();
    eprintln!(
        "lock discipline: waiter={} prober={} starved={starved}",
        show_id(waiter_id.clone()),
        show_id(prober_id.clone())
    );
    assert!(
        starved,
        "a later waiter must starve while the permit sits with an unpolled waiter"
    );
    if let (Some(waiter), Some(prober)) = (&waiter_id, &prober_id) {
        assert_ne!(
            waiter,
            prober,
            "the waiter ({}) must not be the probing task ({}), or this would be a same-task re-lock",
            show_id(Some(waiter.clone())),
            show_id(Some(prober.clone()))
        );
    }

    // Polling the waiter again hands the permit on and the mount table recovers.
    resume.notify_waiters();
    walker
        .await
        .expect("walker task must not panic")
        .expect("walk must succeed");
    assert_mount_table_is_free(&fs).await;
    let _ = fs.clunk(Fid(50_001)).await;
}

/// The cancel pressure the product path applies: walks cancelled mid-acquisition and
/// mid-backing-call, with a clunk racing each reservation, must never leave the
/// mount table stranded.
#[tokio::test]
async fn walk_clunk_and_cancel_rounds_never_strand_the_mount_table() {
    let gated = Arc::new(GatedFs::new(Arc::new(alan_ap::reference::MemFs::new())));
    let fs = mounted_fs(gated.clone());
    let names = vec!["data".to_string()];
    for round in 0..200u64 {
        let fid = Fid(60_000 + round);
        if round % 3 == 0 {
            // Cancel while the backing walk is parked inside the gated server.
            gated.hold();
            let cancelled =
                tokio::time::timeout(Duration::from_millis(1), fs.walk(Fid::ROOT, fid, &names))
                    .await;
            assert!(
                cancelled.is_err(),
                "round {round}: gated walk must be cancelled"
            );
            gated.release();
        } else {
            // Cancel while the walk is queued for the mount table: hold it to force
            // the queue, then let the deadline fire.
            let guard = fs.state.lock().await;
            let cancelled = tokio::time::timeout_at(
                tokio::time::Instant::now(),
                fs.walk(Fid::ROOT, fid, &names),
            )
            .await;
            drop(guard);
            assert!(
                cancelled.is_err(),
                "round {round}: queued walk must be cancelled"
            );
        }
        // A clunk races the reservation the cancelled walk may have left behind.
        let _ = fs.clunk(fid).await;
        assert_mount_table_is_free(&fs).await;
    }
}
