//! Ordinary submissions retained by an Agent Machine across transition cancellation.
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use alan_agent_protocol::{ContentPart, Submission};

/// Admission evidence does not imply that a fresh external delivery may enqueue again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionDisposition {
    New,
    AlreadyAdmitted,
    NotInput,
}

/// One admission contract shared by the Machine and its Process-loop observer.
pub(crate) async fn admit_input(
    queue: &std::sync::Arc<std::sync::Mutex<MachineInputQueue>>,
    recorder: Option<&crate::rollout::RolloutRecorder>,
    input: &Submission,
) -> anyhow::Result<AdmissionDisposition> {
    if !matches!(
        input.op,
        alan_agent_protocol::Op::Turn { .. } | alan_agent_protocol::Op::Input { .. }
    ) {
        return Ok(AdmissionDisposition::NotInput);
    }
    if queue
        .lock()
        .expect("input queue poisoned")
        .admitted_ids
        .contains(&input.id)
    {
        return Ok(AdmissionDisposition::AlreadyAdmitted);
    }
    let mut payload = serde_json::to_value(input)?;
    if let Some(binding) = queue
        .lock()
        .expect("input queue poisoned")
        .bindings
        .get(&input.id)
    {
        payload["callable_binding"] = serde_json::to_value(&binding.callable_binding)?;
        payload["request_controls"] = serde_json::to_value(&binding.request_controls)?;
    }
    persist_input_event(recorder, "machine_input_admitted_v1", payload).await?;
    {
        let mut state = queue.lock().expect("input queue poisoned");
        state.admitted_ids.insert(input.id.clone());
        state.queue_evidence_known |= recorder.is_some();
    }
    crate::runtime::queue_publication::observe(queue).await;
    Ok(AdmissionDisposition::New)
}

pub(crate) async fn persist_input_event(
    recorder: Option<&crate::rollout::RolloutRecorder>,
    kind: &str,
    payload: serde_json::Value,
) -> anyhow::Result<()> {
    if let Some(recorder) = recorder {
        recorder
            .persist_batch(vec![crate::rollout::RolloutItem::Event(
                crate::rollout::EventRecord {
                    event_type: kind.into(),
                    payload,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            )])
            .await?;
    }
    Ok(())
}

/// One correlated removal boundary for all queue/drop callers. A single JSONL
/// record prevents a valid per-ID prefix from becoming partial recovery evidence.
pub(crate) async fn persist_input_removals(
    recorder: Option<&crate::rollout::RolloutRecorder>,
    submission_ids: &[String],
) -> anyhow::Result<()> {
    if submission_ids.is_empty() {
        return Ok(());
    }
    let Some(recorder) = recorder else {
        return Ok(());
    };
    // Reconcile the exact full-set record before retrying an uncertain write.
    // Inspect complete JSONL records only; never interpret a torn suffix as evidence.
    async fn recorded(
        recorder: &crate::rollout::RolloutRecorder,
        ids: &[String],
    ) -> anyhow::Result<bool> {
        let items =
            crate::rollout::RolloutRecorder::load_history(&recorder.path().to_path_buf()).await?;
        Ok(items.into_iter().any(|item| {
            matches!(item, crate::rollout::RolloutItem::Event(event)
                if event.event_type == "machine_inputs_removed_v1"
                    && event.payload["submission_ids"] == serde_json::json!(ids))
        }))
    }
    if recorded(recorder, submission_ids).await? {
        return Ok(());
    }
    match persist_input_event(
        Some(recorder),
        "machine_inputs_removed_v1",
        serde_json::json!({"submission_ids": submission_ids}),
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            if recorded(recorder, submission_ids).await.unwrap_or(false) {
                // The complete set is recovery-visible even though flush/ack failed.
                // Apply that same disposition locally instead of falsely rejecting it.
                tracing::warn!(%error, ?submission_ids, "Removal ack failed; reconciled complete rollout evidence");
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Input removal acknowledgement failed; disposition is uncertain for submission IDs {submission_ids:?}; retained locally without claiming settlement: {error}"
                ))
            }
        }
    }
}

/// Apply binding cleanup only after the shared durable removal owner acknowledges
/// the complete correlated set. Dedup evidence and active snapshots are unrelated.
pub(crate) async fn remove_input_bindings(
    queue: &std::sync::Arc<std::sync::Mutex<MachineInputQueue>>,
    recorder: Option<&crate::rollout::RolloutRecorder>,
    submission_ids: &[String],
) -> anyhow::Result<()> {
    if let Err(error) = persist_input_removals(recorder, submission_ids).await {
        queue
            .lock()
            .expect("input queue poisoned")
            .queue_uncertain_ids
            .extend(submission_ids.iter().cloned());
        crate::runtime::queue_publication::observe(queue).await;
        return Err(error);
    }
    {
        let mut state = queue.lock().expect("input queue poisoned");
        for id in submission_ids {
            state.bindings.remove(id);
            state.settled_ids.insert(id.clone());
            state.queue_uncertain_ids.remove(id);
        }
    }
    crate::runtime::queue_publication::observe(queue).await;
    Ok(())
}

#[derive(Debug)]
pub(crate) enum QueuedRuntimeItem {
    Submission(Submission),
    Deferred(super::DeferredRuntimeAction),
}

#[derive(Debug, Default)]
pub(crate) struct MachineInputQueue {
    pub(crate) queue_publisher: Option<Arc<crate::runtime::queue_publication::QueuePublisher>>,
    pub(crate) queue_evidence_known: bool,
    pub(crate) queue_uncertain_ids: HashSet<String>,
    pub(crate) pending: VecDeque<QueuedRuntimeItem>,
    pub(crate) paused: bool,
    /// Recovery never inherits the source Process's project authority.
    pub(crate) recovered: bool,
    pub(crate) admitted_ids: std::collections::HashSet<String>,
    /// Acknowledged dispositions prevent repeated delivery from re-enqueueing work.
    pub(crate) settled_ids: std::collections::HashSet<String>,
    /// Accepted inputs whose rejection has not crossed the durable removal barrier.
    pub(crate) pending_binding_rejections: std::collections::HashSet<String>,
    pub(crate) confirmed_binding: Option<crate::runtime::model_binding::InputBinding>,
    pub(crate) bindings:
        std::collections::HashMap<String, crate::runtime::model_binding::InputBinding>,
    /// Derived lookup for Process controls while a transition borrows the Machine.
    pub(crate) active_submission_ids: Vec<String>,
    /// Accepted control request, observed by the Machine at settlement.
    pub(crate) active_cancel_requested: bool,
    pub(crate) inband: VecDeque<Submission>,
    pub(crate) buffered_inband_submissions: VecDeque<Submission>,
    pub(crate) queued_next_turn_inputs: VecDeque<(Option<String>, Vec<ContentPart>)>,
    pub(crate) notify: std::sync::Arc<tokio::sync::Notify>,
}

impl super::AgentMachine {
    /// Acknowledged admission precedes acceptance into any runtime queue.
    pub(crate) async fn admit_input(
        &self,
        input: &Submission,
    ) -> anyhow::Result<AdmissionDisposition> {
        admit_input(&self.input_queue(), self.recorder.as_ref(), input).await
    }

    pub(crate) async fn dispatch_input(&self, input: &Submission) -> anyhow::Result<()> {
        self.admit_input(input).await?;
        self.dispatch_input_ids(std::slice::from_ref(&input.id))
            .await
    }

    /// The dispatch owner records the complete consumed set in one durable record.
    pub(crate) async fn dispatch_input_ids(&self, ids: &[String]) -> anyhow::Result<()> {
        let (kind, payload) = if ids.len() == 1 {
            (
                "machine_input_dispatched_v1",
                serde_json::json!({"submission_id": ids[0]}),
            )
        } else {
            (
                "machine_inputs_dispatched_v1",
                serde_json::json!({"submission_ids": ids}),
            )
        };
        let disposition = self.persist_input_event(kind, payload).await;
        let queue = self.input_queue();
        {
            let mut state = queue.lock().expect("input queue poisoned");
            for id in ids {
                if disposition.is_ok() {
                    state.bindings.remove(id);
                    state.settled_ids.insert(id.clone());
                    state.queue_uncertain_ids.remove(id);
                } else {
                    state.queue_uncertain_ids.insert(id.clone());
                }
            }
        }
        crate::runtime::queue_publication::observe(&queue).await;
        disposition
    }

    /// Rejection/drop evidence must be acknowledged before visible settlement.
    /// On failure retain the input locally as well as in recovery evidence.
    pub(crate) async fn remove_input(&mut self, input: &Submission) -> anyhow::Result<()> {
        if let Err(error) = remove_input_bindings(
            &self.input_queue(),
            self.recorder.as_ref(),
            std::slice::from_ref(&input.id),
        )
        .await
        {
            self.push_buffered_inband_submission(input.clone());
            return Err(error);
        }
        Ok(())
    }

    pub(crate) async fn persist_input_event(
        &self,
        kind: &str,
        payload: serde_json::Value,
    ) -> anyhow::Result<()> {
        persist_input_event(self.recorder.as_ref(), kind, payload).await
    }

    pub(crate) fn recover_input_queue(
        &self,
        events: &[crate::rollout::EventRecord],
    ) -> anyhow::Result<()> {
        let mut inputs = Vec::new();
        let mut excluded = std::collections::HashSet::new();
        let queue = self.input_queue();
        let mut queue = queue.lock().expect("input queue poisoned");
        for event in events {
            match event.event_type.as_str() {
                "machine_model_selected_v1" => {
                    queue.confirmed_binding = Some(serde_json::from_value(event.payload.clone())?);
                }
                "machine_input_admitted_v1" => {
                    let input: Submission = serde_json::from_value(event.payload.clone())?;
                    queue.queue_evidence_known = true;
                    if event.payload.get("callable_binding").is_some() {
                        let binding = crate::runtime::model_binding::InputBinding {
                            callable_binding: serde_json::from_value(
                                event.payload["callable_binding"].clone(),
                            )?,
                            request_controls: serde_json::from_value(
                                event.payload["request_controls"].clone(),
                            )?,
                        };
                        queue.bindings.insert(input.id.clone(), binding);
                    }
                    if queue.admitted_ids.insert(input.id.clone()) {
                        inputs.push(input);
                    }
                }
                "machine_inputs_removed_v1" | "machine_inputs_dispatched_v1" => {
                    let ids: Vec<String> =
                        serde_json::from_value(event.payload["submission_ids"].clone())?;
                    excluded.extend(ids);
                }
                "machine_input_dispatched_v1" | "machine_input_removed_v1" => {
                    let id = event.payload["submission_id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("input evidence missing submission ID"))?;
                    excluded.insert(id.to_owned());
                }
                _ => {}
            }
        }
        queue.bindings.retain(|id, _| !excluded.contains(id));
        queue.settled_ids.extend(excluded.iter().cloned());
        queue.pending.extend(
            inputs
                .into_iter()
                .filter(|input| !excluded.contains(&input.id))
                .map(QueuedRuntimeItem::Submission),
        );
        // Only reliable undispatched work suspends scheduling. Completed or
        // legacy history is context, not a paused queue or inherited authority.
        queue.paused = !queue.pending.is_empty();
        queue.recovered = queue.paused;
        Ok(())
    }
    /// A Process-loop observer can acknowledge input while a transition borrows the Machine.
    pub(crate) fn input_recorder(&self) -> Option<crate::rollout::RolloutRecorder> {
        self.recorder.clone()
    }

    #[cfg(test)]
    pub(crate) fn set_input_recorder_for_test(
        &mut self,
        recorder: crate::rollout::RolloutRecorder,
    ) {
        self.recorder = Some(recorder);
    }

    pub(crate) fn input_queue(&self) -> std::sync::Arc<std::sync::Mutex<MachineInputQueue>> {
        self.transition_state.input_queue.clone()
    }
}
