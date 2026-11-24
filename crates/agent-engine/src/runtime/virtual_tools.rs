use crate::llm::ToolDefinition;

use super::child_run_termination_tool::terminate_child_run_tool_definition;
use super::delegated_skill_tool::invoke_delegated_skill_tool_definition;
use super::interaction_tools::{
    request_confirmation_tool_definition, request_user_input_tool_definition,
    update_plan_tool_definition,
};
use super::mount_request_tool::request_mount_tool_definition;
use super::service_activation_tool::request_service_tool_definition;
use super::subagent_tool::spawn_subagent_tool_definition;

pub(super) fn virtual_tool_definitions(
    include_delegated_skill: bool,
    include_subagent_spawn: bool,
    include_service_activation: bool,
) -> Vec<ToolDefinition> {
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
    if include_subagent_spawn {
        defs.push(spawn_subagent_tool_definition());
    }
    if include_service_activation {
        defs.push(request_service_tool_definition());
    }
    defs
}

#[cfg(test)]
mod definition_tests {
    use super::*;

    #[test]
    fn definitions_exclude_optional_delegation_tools_by_default() {
        let defs = virtual_tool_definitions(false, false, false);
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
        assert!(
            !defs
                .iter()
                .any(|definition| definition.name == "spawn_subagent")
        );
        assert!(
            !defs
                .iter()
                .any(|definition| definition.name == "request_service")
        );
    }

    #[test]
    fn definitions_include_delegation_tools_when_supported() {
        let defs = virtual_tool_definitions(true, false, false);
        assert!(
            defs.iter()
                .any(|definition| definition.name == "invoke_delegated_skill")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "terminate_child_run")
        );
        assert!(
            !defs
                .iter()
                .any(|definition| definition.name == "spawn_subagent")
        );
        assert!(
            !defs
                .iter()
                .any(|definition| definition.name == "request_service")
        );

        let root_defs = virtual_tool_definitions(true, true, true);
        assert!(
            root_defs
                .iter()
                .any(|definition| definition.name == "spawn_subagent")
        );
        assert!(
            root_defs
                .iter()
                .any(|definition| definition.name == "request_service")
        );
    }
}

#[cfg(test)]
#[path = "virtual_tools_tests.rs"]
mod tests;
