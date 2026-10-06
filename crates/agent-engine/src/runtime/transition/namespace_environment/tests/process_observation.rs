use super::*;
use std::sync::atomic::Ordering;

struct UnreadableCountedResult(Arc<AtomicUsize>);

#[async_trait::async_trait]
impl ProcessRunner for UnreadableCountedResult {
    async fn run(&self, _: ProcessInvocation) -> ProcessOutcome {
        self.0.fetch_add(1, Ordering::SeqCst);
        ProcessOutcome::exited(0, vec![0xff])
    }
}

#[tokio::test]
async fn observed_exit_without_readable_output_is_bounded_unknown() {
    let count = Arc::new(AtomicUsize::new(0));
    let (environment, shell) =
        tool_test_environment(Arc::new(UnreadableCountedResult(count.clone()))).await;
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        environment
            .tool_execution()
            .run_action("counted", "/bin/counted", Vec::<String>::new()),
    )
    .await
    .unwrap()
    .unwrap_err();
    let bounded = error.downcast_ref::<NamespaceToolProcessError>().unwrap();
    assert_eq!(bounded.pid, "2");
    assert_eq!(
        bounded.category,
        NamespaceToolObservationFailure::ResultUnavailable
    );
    assert_eq!(
        error.to_string(),
        "Tool Process 2: result unavailable with unknown effects"
    );
    assert!(std::error::Error::source(bounded).is_none());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        String::from_utf8(shell.cat("/proc/2/exit").await.unwrap()).unwrap(),
        "0"
    );
}

#[tokio::test]
async fn cancellation_before_spawn_has_no_process_or_counted_effect() {
    let count = Arc::new(AtomicUsize::new(0));
    let (environment, shell) =
        tool_test_environment(Arc::new(UnreadableCountedResult(count.clone()))).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = environment
        .tool_execution()
        .run_action_with_cancel("counted", "/bin/counted", Vec::<String>::new(), &cancel)
        .await
        .unwrap_err();
    assert!(error.downcast_ref::<NamespaceToolProcessError>().is_none());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert!(shell.cat("/proc/2/status").await.is_err());
}
