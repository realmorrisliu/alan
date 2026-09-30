//! Ordinary submissions retained by an Agent Machine across transition cancellation.
use std::collections::VecDeque;

use alan_agent_protocol::{ContentPart, Submission};

/// One admission contract shared by the Machine and its Process-loop observer.
pub(crate) async fn admit_input(
    queue: &std::sync::Arc<std::sync::Mutex<MachineInputQueue>>,
    recorder: Option<&crate::rollout::RolloutRecorder>,
    input: &Submission,
) -> anyhow::Result<()> {
    if !matches!(
        input.op,
        alan_agent_protocol::Op::Turn { .. } | alan_agent_protocol::Op::Input { .. }
    ) {
        return Ok(());
    }
    if queue
        .lock()
        .expect("input queue poisoned")
        .admitted_ids
        .contains(&input.id)
    {
        return Ok(());
    }
    persist_input_event(
        recorder,
        "machine_input_admitted_v1",
        serde_json::to_value(input)?,
    )
    .await?;
    queue
        .lock()
        .expect("input queue poisoned")
        .admitted_ids
        .insert(input.id.clone());
    Ok(())
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

#[derive(Debug)]
pub(crate) enum QueuedRuntimeItem {
    Submission(Submission),
    Deferred(super::DeferredRuntimeAction),
}

#[derive(Debug, Default)]
pub(crate) struct MachineInputQueue {
    pub(crate) pending: VecDeque<QueuedRuntimeItem>,
    pub(crate) paused: bool,
    /// Recovery never inherits the source Process's project authority.
    pub(crate) recovered: bool,
    pub(crate) admitted_ids: std::collections::HashSet<String>,
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
    pub(crate) async fn admit_input(&self, input: &Submission) -> anyhow::Result<()> {
        admit_input(&self.input_queue(), self.recorder.as_ref(), input).await
    }

    pub(crate) async fn dispatch_input(&self, input: &Submission) -> anyhow::Result<()> {
        self.admit_input(input).await?;
        self.persist_input_event(
            "machine_input_dispatched_v1",
            serde_json::json!({"submission_id": input.id}),
        )
        .await
    }

    /// Rejection/drop evidence must be acknowledged before visible settlement.
    /// On failure retain the input locally as well as in recovery evidence.
    pub(crate) async fn remove_input(&mut self, input: &Submission) -> anyhow::Result<()> {
        if let Err(error) =
            persist_input_removals(self.recorder.as_ref(), std::slice::from_ref(&input.id)).await
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
                "machine_input_admitted_v1" => {
                    let input: Submission = serde_json::from_value(event.payload.clone())?;
                    if queue.admitted_ids.insert(input.id.clone()) {
                        inputs.push(input);
                    }
                }
                "machine_inputs_removed_v1" => {
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
