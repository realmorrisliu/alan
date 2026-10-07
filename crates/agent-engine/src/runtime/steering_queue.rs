use alan_agent_protocol::{Event, InputMode, Op};
use anyhow::Result;
use serde_json::json;

use super::transition::NamespaceRuntimeEnvironment;
use super::turn_input::{MAX_BUFFERED_INBAND_USER_INPUTS, TurnInputBroker, reject_inband_overflow};
use super::turn_support::tool_result_preview;
use crate::agent_machine::{AgentMachine, NormalizedToolCall};
use tokio_util::sync::CancellationToken;

pub(super) async fn handle_queued_steering_inputs<E, F>(
    machine: &mut AgentMachine,
    environment: &NamespaceRuntimeEnvironment,
    writer: &super::transition::NamespaceTapeWriter,
    cancel: &CancellationToken,
    remaining: &[NormalizedToolCall],
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

    let mut consumed_steering = false;
    while let Some(submission) = broker.try_recv().await {
        if let Op::Input {
            parts,
            mode: InputMode::Steer,
        } = &submission.op
        {
            if let Err(error) =
                super::shadow_evaluation::dispatch_input(machine, environment, &submission, cancel)
                    .await
            {
                machine.push_buffered_inband_submission(submission);
                return Err(error);
            }
            // Durable dispatch makes this input part of the active turn even if
            // the subsequent namespace Tape projection fails.
            machine.accept_steering_submission(submission.id.clone());
            writer
                .append_record(
                    "user",
                    &crate::tape::parts_to_text(parts),
                    Some(&submission.id),
                    &[],
                )
                .await?;
            machine.note_resumed_user_input();
            machine.add_user_message_parts(parts.clone());
            consumed_steering = true;
            continue;
        }

        if matches!(
            &submission.op,
            Op::Input {
                mode: InputMode::FollowUp,
                ..
            }
        ) && machine.buffered_inband_user_input_count() >= MAX_BUFFERED_INBAND_USER_INPUTS
        {
            reject_inband_overflow(machine, &environment.agent_files(), &submission, emit).await?;
            continue;
        }

        machine.push_buffered_inband_submission(submission);
    }

    if !consumed_steering {
        return Ok(false);
    }

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
        machine.record_tool_call(
            &skipped.name,
            skipped.arguments.clone(),
            skipped_payload.clone(),
            false,
        );
        machine.add_tool_message(&skipped.id, &skipped.name, skipped_payload);
    }

    Ok(true)
}
