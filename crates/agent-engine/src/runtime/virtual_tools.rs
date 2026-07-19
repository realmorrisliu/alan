use alan_agent_protocol::Event;
use anyhow::Result;
#[cfg(test)]
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{agent_machine::NormalizedToolCall, llm::ToolDefinition};

use super::child_run_termination_tool::{
    handle_terminate_child_run, terminate_child_run_tool_definition,
};
use super::delegated_skill_tool::{
    handle_invoke_delegated_skill, invoke_delegated_skill_tool_definition,
};
use super::interaction_tools::{
    handle_request_confirmation, handle_request_user_input, handle_update_plan,
    request_confirmation_tool_definition, request_user_input_tool_definition,
    update_plan_tool_definition,
};
#[cfg(test)]
use super::interaction_tools::{
    parse_confirmation_request, parse_plan_status, parse_plan_update,
    parse_structured_user_input_request,
};
pub(super) use super::mount_request_tool::parse_mount_request;
use super::mount_request_tool::{handle_request_mount, request_mount_tool_definition};
use super::transition::RuntimeLoopState;
use super::turn_support::check_turn_cancelled;
pub(super) use super::virtual_tool::VirtualToolOutcome;

pub(super) fn virtual_tool_definitions(include_delegated_skill: bool) -> Vec<ToolDefinition> {
    let mut defs = vec![
        request_confirmation_tool_definition(),
        request_mount_tool_definition(),
        request_user_input_tool_definition(),
        update_plan_tool_definition(),
    ];
    if include_delegated_skill {
        defs.push(invoke_delegated_skill_tool_definition());
        defs.push(terminate_child_run_tool_definition());
    }
    defs
}

pub(super) async fn try_handle_virtual_tool_call<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let agent_files = state.agent_files();
    if cancel.is_cancelled()
        && check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel).await?
    {
        return Ok(VirtualToolOutcome::EndTurn);
    }

    match tool_call.name.as_str() {
        "request_confirmation" => {
            let runtime = super::transition::agent_interaction_runtime(state);
            handle_request_confirmation(runtime, tool_call, tool_arguments, emit).await
        }
        "request_mount" => {
            let runtime = super::transition::mount_request_runtime(state);
            handle_request_mount(runtime, tool_call, tool_arguments, emit).await
        }
        "request_user_input" => {
            let runtime = super::transition::agent_interaction_runtime(state);
            handle_request_user_input(runtime, tool_call, tool_arguments, emit).await
        }
        "update_plan" => {
            let runtime = super::transition::agent_interaction_runtime(state);
            handle_update_plan(runtime, tool_call, tool_arguments, emit).await
        }
        "invoke_delegated_skill" => {
            let runtime = super::transition::delegated_skill_runtime(state);
            handle_invoke_delegated_skill(runtime, tool_call, tool_arguments, cancel, emit).await
        }
        "terminate_child_run" => {
            let runtime = super::transition::child_run_termination_runtime(state);
            handle_terminate_child_run(
                runtime,
                tool_call,
                tool_arguments,
                allow_approved_tool_escalation_execution,
                emit,
            )
            .await
        }
        _ => Ok(VirtualToolOutcome::NotVirtual),
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;

    #[test]
    fn definitions_exclude_optional_delegation_tools_by_default() {
        let defs = virtual_tool_definitions(false);
        assert_eq!(defs.len(), 4);
        assert!(
            defs.iter()
                .any(|definition| definition.name == "request_confirmation")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "request_mount")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "request_user_input")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "update_plan")
        );
        assert!(
            !defs
                .iter()
                .any(|definition| definition.name == "invoke_delegated_skill")
        );
    }

    #[test]
    fn definitions_include_delegation_tools_when_supported() {
        let defs = virtual_tool_definitions(true);
        assert!(
            defs.iter()
                .any(|definition| definition.name == "invoke_delegated_skill")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "terminate_child_run")
        );
    }
}

#[cfg(test)]
#[path = "virtual_tools_tests.rs"]
mod tests;
