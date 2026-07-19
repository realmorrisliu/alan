use alan_agent_protocol::{Event, InputMode, Op};
use anyhow::Result;
use serde_json::json;

use super::transition::RuntimeLoopState;
use super::turn_driver::{MAX_BUFFERED_INBAND_USER_INPUTS, TurnInputBroker};
use super::turn_support::tool_result_preview;
use crate::agent_machine::NormalizedToolCall;

pub(super) async fn handle_queued_steering_inputs<E, F>(
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    remaining_start_idx: usize,
    steering_broker: Option<&TurnInputBroker>,
    emit: &mut E,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let Some(broker) = steering_broker else {
        return Ok(false);
    };

    let mut steering_inputs: Vec<Vec<crate::tape::ContentPart>> = Vec::new();
    while let Some(submission) = broker.try_recv().await {
        if let Op::Input {
            parts,
            mode: InputMode::Steer,
        } = &submission.op
        {
            steering_inputs.push(parts.clone());
            continue;
        }

        if matches!(
            &submission.op,
            Op::Input {
                mode: InputMode::FollowUp,
                ..
            }
        ) && state.machine.buffered_inband_user_input_count() >= MAX_BUFFERED_INBAND_USER_INPUTS
        {
            emit(Event::Error {
                message: format!(
                    "Too many queued in-turn user inputs (limit={MAX_BUFFERED_INBAND_USER_INPUTS}); dropping newest input."
                ),
                recoverable: true,
            })
            .await;
            continue;
        }

        state.machine.push_buffered_inband_submission(submission);
    }

    if steering_inputs.is_empty() {
        return Ok(false);
    }

    state.machine.note_resumed_user_input();
    for parts in steering_inputs {
        state.machine.add_user_message_parts(parts);
    }

    let remaining = &tool_calls[remaining_start_idx..];
    if !remaining.is_empty() {
        emit(Event::Error {
            message: format!(
                "Steering input received during tool batch; skipping {} pending tool call(s).",
                remaining.len()
            ),
            recoverable: true,
        })
        .await;
    }

    for skipped in remaining {
        let skipped_payload = json!({
            "status": "skipped_due_to_steering",
            "error": "Skipped due to queued user steering input."
        });
        emit(Event::ToolCallStarted {
            title: None,
            id: skipped.id.clone(),
            name: skipped.name.clone(),
            audit: None,
        })
        .await;
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: skipped.id.clone(),
            name: Some(skipped.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&skipped_payload),
            audit: None,
        })
        .await;
        state.machine.record_tool_call(
            &skipped.name,
            skipped.arguments.clone(),
            skipped_payload.clone(),
            false,
        );
        state
            .machine
            .add_tool_message(&skipped.id, &skipped.name, skipped_payload);
    }

    Ok(true)
}
