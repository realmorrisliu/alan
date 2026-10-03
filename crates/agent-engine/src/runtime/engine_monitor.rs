use std::future::Future;

use tokio_util::sync::CancellationToken;

/// Keep execution polled while its control/heartbeat observer performs file IO.
/// A suspended execution may already own the next FIFO mutex permit, so awaiting
/// observer IO alone can deadlock even when no task currently holds the lock.
pub(super) async fn drive_with_monitor<F, M>(
    execution: F,
    monitor: M,
    stop: &CancellationToken,
) -> F::Output
where
    F: Future,
    M: Future<Output = ()>,
{
    tokio::pin!(monitor);
    let output = {
        tokio::pin!(execution);
        tokio::select! {
            output = &mut execution => output,
            () = &mut monitor => panic!("execution monitor stopped before execution settled"),
        }
    };
    // Finish any admitted control write before publishing terminal state. Do not
    // abandon a partially written request or allow a late heartbeat after idle.
    stop.cancel();
    monitor.await;
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::{Mutex, oneshot};

    #[tokio::test]
    async fn queued_execution_and_observer_both_progress_and_observer_settles() {
        let lock = Mutex::new(());
        let held = lock.lock().await;
        let (queued, ready) = oneshot::channel();
        let (observed, observation) = oneshot::channel();
        let stop = CancellationToken::new();
        let execution = async {
            queued.send(()).unwrap();
            // First poll queues this waiter before the observer releases held.
            drop(lock.lock().await);
            42
        };
        let monitor = async {
            ready.await.unwrap();
            drop(held);
            drop(lock.lock().await);
            stop.cancelled().await;
            observed.send(()).unwrap();
        };
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            drive_with_monitor(execution, monitor, &stop),
        )
        .await
        .expect("execution must keep polling while observer waits for its FIFO permit");
        assert_eq!(result, 42);
        observation
            .await
            .expect("observer must settle before return");
    }
}
