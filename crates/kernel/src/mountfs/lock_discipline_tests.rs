//! Lock discipline for the mount table.
//!
//! `MountFs::state` is a fair `tokio::sync::Mutex`: a waiter that is *handed the
//! permit* but then stops being polled keeps it, and every later waiter queues
//! behind it while no guard exists. Product code that holds an established future
//! in one `select!` arm must therefore never await inside another arm's body — the
//! awaiting arm would starve behind the permit its sibling was handed.
//!
//! These tests pin that behaviour so the rule in `wait_for_stdio_answer_after_submit`
//! stays legible: if a tokio upgrade ever makes the mutex starvation-free, the first
//! test fails and tells the reader the rule is obsolete instead of load-bearing.
use super::*;
use std::time::Duration;

fn mounted_fs() -> Arc<MountFs> {
    let mut ns = Namespace::new();
    ns.mount(
        "/data",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        crate::namespace::Access::ReadWrite,
    );
    Arc::new(MountFs::new(ns))
}

/// Acquiring the mount table succeeds immediately (it is free, not held).
async fn assert_mount_table_is_free(fs: &MountFs) {
    let guard = tokio::time::timeout(Duration::from_millis(100), fs.state.lock())
        .await
        .expect("mount table lock must be free: an assignment is stuck on an unpolled waiter");
    drop(guard);
}

#[tokio::test]
async fn a_waiter_handed_the_permit_strands_later_waiters_until_it_is_polled_again() {
    let fs = mounted_fs();
    let names = vec!["data".to_string()];
    // Hold the mount table so the walk below has to queue.
    let guard = fs.state.lock().await;
    let walk = fs.walk(Fid::ROOT, Fid(20_001), &names);
    tokio::pin!(walk);
    // Poll the walk exactly once: its lock acquisition joins the FIFO queue.
    let expired = tokio::time::Instant::now();
    assert!(
        tokio::time::timeout_at(expired, &mut walk).await.is_err(),
        "the walk cannot finish while the mount table is held"
    );
    drop(guard);

    // The permit is handed to the unpolled waiter: later acquisitions starve even
    // though no guard is alive.
    let starved = tokio::time::timeout(Duration::from_millis(100), fs.state.lock()).await;
    assert!(
        starved.is_err(),
        "an unpolled queued waiter must hold the permit; if this now succeeds, the \
         mutex is starvation-free and the select-arm rule is obsolete"
    );

    // Polling the waiter again releases the permit and the mount table recovers.
    let resumed = tokio::time::timeout(Duration::from_secs(1), &mut walk).await;
    assert!(
        resumed.is_ok(),
        "resuming the waiter must let the walk finish"
    );
    assert_mount_table_is_free(&fs).await;
    let _ = fs.clunk(Fid(20_001)).await;
}

/// The recovery half of the rule as product code uses it: a walker that is polled
/// keeps the mount table usable for everyone else.
#[tokio::test]
async fn polling_a_queued_waiter_keeps_the_mount_table_usable() {
    let fs = mounted_fs();
    let names = vec!["data".to_string()];
    let guard = fs.state.lock().await;
    let walk = fs.walk(Fid::ROOT, Fid(20_002), &names);
    tokio::pin!(walk);
    let expired = tokio::time::Instant::now();
    assert!(tokio::time::timeout_at(expired, &mut walk).await.is_err());
    drop(guard);
    // Poll it to completion instead of leaving it queued.
    let walked = tokio::time::timeout(Duration::from_secs(1), &mut walk)
        .await
        .expect("a polled waiter must finish");
    assert!(walked.is_ok(), "walk failed: {walked:?}");
    assert_mount_table_is_free(&fs).await;
    let _ = fs.clunk(Fid(20_002)).await;
}
