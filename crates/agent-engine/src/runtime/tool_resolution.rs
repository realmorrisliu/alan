//! Mounted-package resolution and unavailable-Tool recording for one Tool call.

use std::path::PathBuf;

use alan_agent_protocol::{Event, ToolCapability};
use anyhow::Result;
use serde_json::{Value, json};

use crate::agent_machine::NormalizedToolCall;

use super::turn_support::tool_result_preview;

mod runtime_inputs;

pub(super) use runtime_inputs::ToolResolutionRuntime;

pub(super) struct ToolResolutionRequest<'a> {
    pub(super) tool_call: &'a NormalizedToolCall,
    pub(super) tool_arguments: &'a Value,
}

#[derive(Debug)]
pub(super) struct ResolvedToolCall {
    pub(super) timeout_secs: usize,
    pub(super) capability: ToolCapability,
    pub(super) current_cwd: Option<PathBuf>,
}

#[derive(Debug)]
pub(super) enum ToolResolutionOutcome {
    Resolved(ResolvedToolCall),
    Unavailable,
}

pub(super) async fn resolve_tool_call<E, F>(
    runtime: ToolResolutionRuntime<'_>,
    request: ToolResolutionRequest<'_>,
    emit: &mut E,
) -> Result<ToolResolutionOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let ToolResolutionRequest {
        tool_call,
        tool_arguments,
    } = request;
    let tool_package = runtime
        .tool_execution
        .discover_packages()
        .await?
        .into_iter()
        .find(|package| package.name == tool_call.name);
    let Some(tool_package) = tool_package else {
        let payload = json!({
            "success": false,
            "error": format!(
                "Tool '{}' is unavailable because its executable and valid manifest are not both mounted",
                tool_call.name
            )
        });
        emit(Event::ToolCallStarted {
            title: None,
            id: tool_call.id.clone(),
            name: tool_call.name.clone(),
            audit: None,
        })
        .await;
        emit(Event::ToolCallCompleted {
            presentation: None,
            id: tool_call.id.clone(),
            name: Some(tool_call.name.clone()),
            success: Some(false),
            result_preview: tool_result_preview(&payload),
            audit: None,
        })
        .await;
        runtime.machine.record_tool_call(
            &tool_call.name,
            tool_arguments.clone(),
            payload.clone(),
            false,
        );
        runtime
            .machine
            .add_tool_message(&tool_call.id, &tool_call.name, payload);
        return Ok(ToolResolutionOutcome::Unavailable);
    };

    let capability = runtime
        .tool_execution
        .resolve_capability(&tool_package, tool_arguments);
    Ok(ToolResolutionOutcome::Resolved(ResolvedToolCall {
        timeout_secs: tool_package.timeout_secs,
        capability,
        current_cwd: runtime.tool_execution.default_cwd(),
    }))
}
