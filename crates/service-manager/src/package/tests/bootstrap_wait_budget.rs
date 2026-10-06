use super::lock_contention::contend;
use super::*;
use std::time::Duration;

#[test]
fn free_store_zero_budget_opens_seeds_and_acquires() {
    let directory = tempfile::tempdir().unwrap();
    let (service, guard) = PackageService::bootstrap_with_budget(
        "test".into(),
        Some(directory.path().join("packages")),
        Duration::ZERO,
    )
    .expect("a free flock needs no wait budget, including first open");
    service
        .seed_preinstalled("free-pack", native_snapshot("free", "body"))
        .unwrap();
    let lease = service.acquire("free-pack").unwrap();
    assert_eq!(service.catalog().unwrap().packages.len(), 1);
    guard.finish();
    drop(lease);
}

#[test]
fn exhausted_contention_budget_still_allows_free_transactions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let (service, guard) = PackageService::bootstrap_with_budget(
        "test".into(),
        Some(root.clone()),
        Duration::from_millis(40),
    )
    .unwrap();
    let blocked = service.clone();
    let (early, result, _, owned) =
        contend(&root, Duration::from_millis(150), move || blocked.catalog());
    assert!(owned && early);
    assert!(result.unwrap_err().to_string().contains("budget exhausted"));
    // The peer has exited normally. Exhaustion must not prohibit durable work
    // when the actual nonblocking lock attempt can now succeed.
    service
        .seed_preinstalled("free-pack", native_snapshot("free", "body"))
        .unwrap();
    let lease = service.acquire("free-pack").unwrap();
    service.catalog().unwrap();
    let blocked = service;
    let (early, result, _, owned) =
        contend(&root, Duration::from_millis(150), move || blocked.catalog());
    assert!(owned && early);
    assert!(result.unwrap_err().to_string().contains("budget exhausted"));
    guard.finish();
    drop(lease);
}
