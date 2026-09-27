use std::{sync::Weak, time::Duration};

use alan_kernel::Pid;

use super::{AgentRuntimeService, wait_for_process_exit};

pub(super) struct ProcessCleanup {
    service: Weak<AgentRuntimeService>,
    pid: Pid,
}

impl ProcessCleanup {
    pub(super) fn new(service: Weak<AgentRuntimeService>, pid: Pid) -> Self {
        Self { service, pid }
    }
}

impl Drop for ProcessCleanup {
    fn drop(&mut self) {
        let service = self.service.clone();
        let pid = self.pid;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Some(service) = service.upgrade() {
                    if let Err(error) =
                        wait_for_process_exit(&service.procfs, pid, Duration::from_secs(12)).await
                    {
                        tracing::warn!(pid = pid.0, %error, "Agent Process cleanup deferred");
                        return;
                    }
                    let managed_root = service
                        .root_runtime_handles
                        .lock()
                        .expect("Root runtime handles mutex poisoned")
                        .contains_key(&pid.0);
                    if managed_root {
                        return;
                    }
                    if let Err(error) = service.release_process(pid).await {
                        tracing::warn!(pid = pid.0, %error, "Agent Process cleanup failed");
                    }
                }
            });
        }
    }
}
