use super::lock_contention::contend;
use super::*;
use std::time::Duration;

#[test]
fn bootstrap_waits_for_peer_beyond_operator_budget_then_restores_operator_policy() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    PackageService::open("test", root.clone()).unwrap();
    let peer_root = root.clone();
    let (early, result, _, owned) = contend(&root, Duration::from_millis(900), move || {
        PackageService::bootstrap("test", Some(peer_root))
    });
    assert!(
        owned && !early,
        "boot must wait while peer owns the transaction"
    );
    let (service, guard) = result.unwrap();
    service
        .seed_preinstalled("boot-pack", native_snapshot("boot", "body"))
        .unwrap();
    let lease = service.acquire("boot-pack").unwrap();
    guard.finish();
    drop(lease);
    let (early, result, _, owned) =
        contend(&root, Duration::from_millis(900), move || service.catalog());
    assert!(owned && early);
    assert!(result.unwrap_err().to_string().contains("busy"));
}

#[test]
fn bootstrap_budget_is_shared_across_transactions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let (service, guard) = PackageService::bootstrap_with_budget(
        "test".into(),
        Some(root.clone()),
        Duration::from_millis(650),
    )
    .unwrap();
    let first = service.clone();
    let (early, result, _, owned) =
        contend(&root, Duration::from_millis(350), move || first.catalog());
    assert!(owned && !early);
    result.unwrap();
    let (early, result, elapsed, owned) =
        contend(&root, Duration::from_millis(900), move || service.catalog());
    assert!(owned && early);
    assert!(
        elapsed < Duration::from_millis(650),
        "budget must not restart"
    );
    assert!(result.unwrap_err().to_string().contains("busy"));
    drop(guard);
}

#[tokio::test]
async fn dropping_boot_guard_cancels_outstanding_blocking_open() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    PackageService::open("test", root.clone()).unwrap();
    let guard = crate::package::PackageBootstrap::new();
    let open = guard.opener("test".into(), Some(root.clone()));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let task = tokio::task::spawn_blocking(move || {
        contend(&root, Duration::from_millis(900), move || {
            started_tx.send(()).unwrap();
            open()
        })
    });
    tokio::task::spawn_blocking(move || started_rx.recv_timeout(Duration::from_secs(3)).unwrap())
        .await
        .unwrap();
    // Cancellation belongs to the async owner, not the blocking worker.
    drop(guard);
    let (early, result, elapsed, owned) = task.await.unwrap();
    assert!(owned && early);
    assert!(elapsed < Duration::from_millis(500));
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}
