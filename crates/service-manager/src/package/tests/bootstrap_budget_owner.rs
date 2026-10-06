use super::*;

#[test]
fn nonwaiting_setup_preserves_budget_and_explicit_waits_accumulate() {
    let guard = PackageBootstrap::with_budget(Duration::from_millis(250));
    let wait = &guard.wait;
    assert_eq!(wait.remaining().unwrap(), Some(Duration::from_millis(250)));
    wait.charge_wait(Duration::from_millis(40));
    assert_eq!(wait.remaining().unwrap(), Some(Duration::from_millis(210)));
    // Wall-clock setup is deliberately longer than the original allowance;
    // only the lock owner's explicit contention debit may change remaining.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(wait.remaining().unwrap(), Some(Duration::from_millis(210)));
    wait.charge_wait(Duration::from_millis(40));
    assert_eq!(wait.remaining().unwrap(), Some(Duration::from_millis(170)));
    wait.charge_wait(Duration::from_millis(400));
    assert_eq!(wait.remaining().unwrap(), Some(Duration::ZERO));
    wait.charge_wait(Duration::from_millis(1));
    assert_eq!(wait.remaining().unwrap(), Some(Duration::ZERO));
}
