use alan_agent_protocol::Event;
use anyhow::Result;
use tokio_util::sync::CancellationToken;

use super::{
    RuntimeLoopState, agent_interaction_runtime, child_run_termination_runtime,
    delegated_skill_runtime, mount_request_runtime, service_activation_runtime,
};
use crate::agent_machine::NormalizedToolCall;
use crate::runtime::turn_support::check_turn_cancelled;
use crate::runtime::virtual_tool::VirtualToolOutcome;

pub(in crate::runtime) async fn dispatch_virtual_tool_call<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    task_cancel: &CancellationToken,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let agent_files = state.agent_files();
    let host_mount_requests = state.environment.host_mount_requests();
    if task_cancel.is_cancelled()
        && check_turn_cancelled(
            &mut state.machine,
            &agent_files,
            &host_mount_requests,
            emit,
            task_cancel,
        )
        .await?
    {
        return Ok(VirtualToolOutcome::EndTurn);
    }

    match tool_call.name.as_str() {
        "request_confirmation" => {
            let runtime = agent_interaction_runtime(state);
            crate::runtime::interaction_tools::handle_request_confirmation(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "request_mount" => {
            let runtime = mount_request_runtime(state);
            crate::runtime::mount_request_tool::handle_request_mount(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "request_service" => {
            let Some(runtime) = service_activation_runtime(state) else {
                return Ok(VirtualToolOutcome::NotVirtual);
            };
            crate::runtime::service_activation_tool::handle_request_service(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "request_user_input" => {
            let runtime = agent_interaction_runtime(state);
            crate::runtime::interaction_tools::handle_request_user_input(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "update_plan" => {
            let runtime = agent_interaction_runtime(state);
            crate::runtime::interaction_tools::handle_update_plan(
                runtime,
                tool_call,
                tool_arguments,
                emit,
            )
            .await
        }
        "invoke_delegated_skill" => {
            let runtime = delegated_skill_runtime(state);
            crate::runtime::delegated_skill_tool::handle_invoke_delegated_skill(
                runtime,
                tool_call,
                tool_arguments,
                cancel,
                task_cancel,
                emit,
            )
            .await
        }
        "spawn_subagent" => {
            let subagent_enabled = state.runtime_config.ad_hoc_subagent_spawn_enabled;
            let runtime = delegated_skill_runtime(state);
            crate::runtime::subagent_tool::handle_spawn_subagent(
                runtime,
                subagent_enabled,
                tool_call,
                tool_arguments,
                cancel,
                task_cancel,
                emit,
            )
            .await
        }
        "terminate_child_run" => {
            let runtime = child_run_termination_runtime(state);
            crate::runtime::child_run_termination_tool::handle_terminate_child_run(
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
