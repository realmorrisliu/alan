//! Serialized projection of acknowledged Machine queue truth, never activity/heartbeat state.
use std::sync::{Arc, Mutex};

use alan_agent_protocol::UiQueueSnapshot;
use anyhow::Result;

use crate::agent_machine::input_queue::{MachineInputQueue, QueuedRuntimeItem};
use crate::runtime::transition::NamespaceAgentFiles;

pub(crate) struct QueuePublisher {
    files: NamespaceAgentFiles,
    last: tokio::sync::Mutex<UiQueueSnapshot>,
}

impl std::fmt::Debug for QueuePublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuePublisher").finish_non_exhaustive()
    }
}

impl QueuePublisher {
    pub(crate) fn new(files: NamespaceAgentFiles) -> Self {
        Self {
            files,
            last: tokio::sync::Mutex::new(UiQueueSnapshot::default()),
        }
    }

    pub(crate) async fn publish(&self, queue: &Arc<Mutex<MachineInputQueue>>) -> Result<()> {
        // Serialize snapshot acquisition as well as IO: an older writer cannot
        // acquire a newer revision and overwrite a later durable disposition.
        let mut last = self.last.lock().await;
        let mut next = {
            let queue = queue.lock().expect("input queue poisoned");
            if !queue.queue_evidence_known {
                UiQueueSnapshot::default()
            } else {
                let active_submission_ids = queue
                    .active_submission_ids
                    .iter()
                    .filter(|id| {
                        queue.admitted_ids.contains(*id) && queue.settled_ids.contains(*id)
                    })
                    .cloned()
                    .collect();
                let mut pending_submission_ids = Vec::new();
                let mut add = |id: &String| {
                    if queue.admitted_ids.contains(id)
                        && !queue.settled_ids.contains(id)
                        && !pending_submission_ids.contains(id)
                    {
                        pending_submission_ids.push(id.clone());
                    }
                };
                for item in &queue.pending {
                    if let QueuedRuntimeItem::Submission(input) = item {
                        add(&input.id);
                    }
                }
                for input in queue
                    .inband
                    .iter()
                    .chain(queue.buffered_inband_submissions.iter())
                {
                    add(&input.id);
                }
                for (id, _) in &queue.queued_next_turn_inputs {
                    if let Some(id) = id {
                        add(id);
                    }
                }
                // Admission precedes physical enqueue; dispatch failure may have
                // removed the physical item. Neither erases the accepted receipt.
                let mut remaining: Vec<_> = queue.admitted_ids.iter().collect();
                remaining.sort();
                for id in remaining {
                    add(id);
                }
                let mut uncertain_submission_ids: Vec<_> = queue
                    .queue_uncertain_ids
                    .iter()
                    .chain(queue.pending_binding_rejections.iter())
                    .cloned()
                    .collect();
                uncertain_submission_ids.sort();
                uncertain_submission_ids.dedup();
                UiQueueSnapshot {
                    known: true,
                    pending_submission_ids,
                    active_submission_ids,
                    paused: queue.paused,
                    deferred: queue
                        .pending
                        .iter()
                        .any(|item| matches!(item, QueuedRuntimeItem::Deferred(_))),
                    uncertain_submission_ids,
                    ..Default::default()
                }
            }
        };
        next.revision = last.revision;
        if next == *last && last.revision > 0 {
            return Ok(());
        }
        if next.known {
            next.revision = last
                .revision
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("queue revision exhausted"))?;
        }
        self.files.write_ui_queue_snapshot(&next).await?;
        *last = next;
        Ok(())
    }
}

pub(crate) async fn initialize(
    queue: &Arc<Mutex<MachineInputQueue>>,
    files: NamespaceAgentFiles,
) -> Result<()> {
    queue.lock().expect("input queue poisoned").queue_publisher =
        Some(Arc::new(QueuePublisher::new(files)));
    publish(queue).await
}

pub(crate) async fn publish(queue: &Arc<Mutex<MachineInputQueue>>) -> Result<()> {
    let publisher = queue
        .lock()
        .expect("input queue poisoned")
        .queue_publisher
        .clone();
    if let Some(publisher) = publisher {
        publisher.publish(queue).await?;
    }
    Ok(())
}

pub(crate) async fn observe(queue: &Arc<Mutex<MachineInputQueue>>) {
    if let Err(error) = publish(queue).await {
        tracing::warn!(%error, "Queue projection write failed; durable disposition unchanged");
    }
}
