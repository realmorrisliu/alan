//! Turn and session token/cost accounting published at `machine/ui/usage`.
//!
//! The document lives in AgentFS, so session totals are the Agent Process's own: they survive
//! turns, and start from zero when the Process does. Publication is advisory; a failure is
//! logged and never fails the turn.

use alan_agent_protocol::UiUsageSnapshot;
use alan_llm::TokenUsage;
use tracing::warn;

use crate::runtime::transition::NamespaceAgentFiles;

fn tokens(value: i32) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn add(total: &mut Option<u64>, delta: Option<u64>) {
    if let Some(delta) = delta {
        *total = Some(total.unwrap_or(0).saturating_add(delta));
    }
}

fn add_cost(total: &mut Option<f64>, delta: Option<f64>) {
    if let Some(delta) = delta.filter(|cost| cost.is_finite() && *cost >= 0.0) {
        *total = Some(total.unwrap_or(0.0) + delta);
    }
}

/// Fold one generation's reported usage into the turn and session counters.
pub(super) fn accumulate(snapshot: &mut UiUsageSnapshot, usage: &TokenUsage, context_window: u64) {
    let input = tokens(usage.prompt_tokens);
    let output = tokens(usage.completion_tokens);
    let cached = usage.cached_prompt_tokens.map(tokens);
    let reasoning = usage.reasoning_tokens.map(tokens);
    snapshot.context_tokens = input;
    snapshot.context_window = context_window;
    snapshot.input_tokens = snapshot.input_tokens.saturating_add(input);
    snapshot.output_tokens = snapshot.output_tokens.saturating_add(output);
    add(&mut snapshot.cached_input_tokens, cached);
    add(&mut snapshot.reasoning_tokens, reasoning);
    add_cost(&mut snapshot.cost_usd, usage.cost_usd);
    snapshot.session_input_tokens = snapshot.session_input_tokens.saturating_add(input);
    snapshot.session_output_tokens = snapshot.session_output_tokens.saturating_add(output);
    add(&mut snapshot.session_cached_input_tokens, cached);
    add(&mut snapshot.session_reasoning_tokens, reasoning);
    add_cost(&mut snapshot.session_cost_usd, usage.cost_usd);
}

/// Zero the per-turn counters while keeping the session totals and context occupancy.
pub(super) fn reset_turn(snapshot: &mut UiUsageSnapshot, context_window: u64) {
    snapshot.input_tokens = 0;
    snapshot.cached_input_tokens = None;
    snapshot.output_tokens = 0;
    snapshot.reasoning_tokens = None;
    snapshot.cost_usd = None;
    snapshot.context_window = context_window;
}

async fn update(files: &NamespaceAgentFiles, change: impl FnOnce(&mut UiUsageSnapshot)) {
    let result = async {
        let mut snapshot = files.read_ui_usage_snapshot().await?;
        change(&mut snapshot);
        files.write_ui_usage_snapshot(&snapshot).await
    }
    .await;
    if let Err(error) = result {
        warn!(error = %error, "Failed to publish usage snapshot");
    }
}

/// A new logical turn starts: clear the turn counters.
pub(super) async fn begin_turn(files: &NamespaceAgentFiles, context_window: u64) {
    update(files, |snapshot| reset_turn(snapshot, context_window)).await;
}

/// One generation settled with provider-reported usage.
pub(super) async fn record(files: &NamespaceAgentFiles, usage: &TokenUsage, context_window: u64) {
    update(files, |snapshot| {
        accumulate(snapshot, usage, context_window)
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(prompt: i32, completion: i32) -> TokenUsage {
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            ..TokenUsage::default()
        }
    }

    #[test]
    fn turn_and_session_counters_accumulate_across_generations() {
        let mut snapshot = UiUsageSnapshot::empty();
        let first = TokenUsage {
            cached_prompt_tokens: Some(6),
            reasoning_tokens: Some(2),
            cost_usd: Some(0.5),
            ..usage(10, 4)
        };
        accumulate(&mut snapshot, &first, 1000);
        accumulate(&mut snapshot, &usage(30, 5), 1000);

        assert_eq!(snapshot.input_tokens, 40);
        assert_eq!(snapshot.output_tokens, 9);
        assert_eq!(snapshot.cached_input_tokens, Some(6));
        assert_eq!(snapshot.reasoning_tokens, Some(2));
        assert_eq!(snapshot.cost_usd, Some(0.5));
        assert_eq!(
            snapshot.context_tokens, 30,
            "latest prompt is the occupancy"
        );
        assert_eq!(snapshot.context_window, 1000);
        assert_eq!(snapshot.session_input_tokens, 40);
    }

    #[test]
    fn a_new_turn_keeps_session_totals_and_clears_turn_counters() {
        let mut snapshot = UiUsageSnapshot::empty();
        let metered = TokenUsage {
            cost_usd: Some(1.0),
            cached_prompt_tokens: Some(1),
            ..usage(10, 4)
        };
        accumulate(&mut snapshot, &metered, 1000);

        reset_turn(&mut snapshot, 2000);

        assert_eq!(snapshot.input_tokens, 0);
        assert_eq!(snapshot.cached_input_tokens, None);
        assert_eq!(snapshot.cost_usd, None);
        assert_eq!(snapshot.session_input_tokens, 10);
        assert_eq!(snapshot.session_output_tokens, 4);
        assert_eq!(snapshot.session_cost_usd, Some(1.0));
        assert_eq!(snapshot.context_tokens, 10);
        assert_eq!(snapshot.context_window, 2000);
    }

    #[test]
    fn unmetered_or_invalid_values_never_invent_cost_or_negative_tokens() {
        let mut snapshot = UiUsageSnapshot::empty();
        let invalid = TokenUsage {
            cost_usd: Some(f64::NAN),
            ..usage(-5, -1)
        };
        accumulate(&mut snapshot, &invalid, 0);
        assert_eq!(snapshot.input_tokens, 0);
        assert_eq!(snapshot.output_tokens, 0);
        assert_eq!(snapshot.cost_usd, None);
        assert_eq!(snapshot.session_cost_usd, None);
    }
}
