use crate::llm::ToolDefinition;

use super::child_run_termination_tool::terminate_child_run_tool_definition;
use super::delegated_skill_tool::invoke_delegated_skill_tool_definition;
use super::interaction_tools::{
    request_confirmation_tool_definition, request_user_input_tool_definition,
    update_plan_tool_definition,
};
use super::mount_request_tool::request_mount_tool_definition;

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
