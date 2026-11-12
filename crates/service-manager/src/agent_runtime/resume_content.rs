//! What a resume launch tells the resumed Process.

use alan_agent_engine::{AgentExecutableRequest, ContentPart};

/// An inherited reconnect is the ancestor's authorization.
///
/// It must not arrive as empty content, because an empty answer to an inherited
/// stop is a rejection.
pub(super) fn resume_content(request: &AgentExecutableRequest) -> Vec<ContentPart> {
    if request.inherited_resume {
        vec![ContentPart::structured(serde_json::json!({
            "choice": "approve",
            "resume_kind": "inherited_budget",
        }))]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(inherited: bool) -> AgentExecutableRequest {
        AgentExecutableRequest {
            spawn: alan_agent_protocol::SpawnSpec {
                target: alan_agent_protocol::SpawnTarget::DefinitionDescriptor {
                    descriptor: "agent".to_string(),
                },
                launch: alan_agent_protocol::SpawnLaunchInputs {
                    task: "task".to_string(),
                    ..alan_agent_protocol::SpawnLaunchInputs::default()
                },
                handles: Vec::new(),
                host_mounts: Vec::new(),
                runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
                delegated: None,
            },
            initial_task: String::new(),
            resume: Some("inherited_budget_1".to_string()),
            recovery_rollout: None,
            inherited_resume: inherited,
        }
    }

    #[test]
    fn an_inherited_reconnect_carries_the_ancestors_authorization() {
        let value = serde_json::to_value(resume_content(&request(true))).unwrap();
        let text = value.to_string();
        assert!(text.contains("approve"), "must read as approval: {text}");
        assert!(
            text.contains("inherited_budget"),
            "must name the reconnect: {text}"
        );
    }

    #[test]
    fn an_ordinary_resume_stays_empty() {
        assert!(resume_content(&request(false)).is_empty());
    }
}
