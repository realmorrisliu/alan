use super::*;
use crate::runtime::delegation_capabilities::DelegatedSpawnRejected;

#[test]
fn inherited_mount_without_host_backed_cwd_uses_authorized_native_cwd() {
    let source = TempDir::new().unwrap();
    let scratch = TempDir::new().unwrap();
    let mut plan = capability_plan(Some(source.path().to_path_buf()), &["read_file"]);
    plan.cwd = None;
    plan.launch_context.cwd = "/".to_string();

    let binding = plan
        .runtime_execution_binding(Some(scratch.path().to_path_buf()))
        .unwrap()
        .expect("an inherited Host Mount should create a child Tool binding");

    assert_eq!(binding.cwd, dunce::canonicalize(source.path()).unwrap());
    assert_eq!(binding.namespace_cwd, PathBuf::from("/mnt/source"));
    assert_eq!(binding.host_mounts.len(), 1);
    assert_eq!(binding.host_mounts[0].namespace_path, "/mnt/source");
    let sandbox = binding.sandbox_spec.unwrap();
    assert!(
        !sandbox
            .readable_roots
            .iter()
            .any(|root| root == &dunce::canonicalize(scratch.path()).unwrap())
    );
}

#[test]
fn package_projection_does_not_create_host_tool_authority() {
    let mut plan = capability_plan(None, &["read_file"]);
    plan.launch_context.package_references.push(
        crate::ProcessPackageReference::new(
            "example",
            "a".repeat(64),
            crate::ProcessPackageKind::Installed,
            "/lib/pkg/example",
            Vec::new(),
            memfs_transport(),
        )
        .unwrap(),
    );

    assert!(plan.runtime_execution_binding(None).unwrap().is_none());
}

#[tokio::test]
async fn delegated_spawn_boundary_passes_satisfied_task_unchanged() {
    let temp = TempDir::new().unwrap();
    let parent = make_parent_state(
        &temp,
        RecordedRequests::default(),
        completed_response("unused"),
    );
    let host_mount = PathBuf::from("/tmp/repo");
    let mut spec = launch_spec(temp.path().join("agent"));
    spec.launch.task = "Inspect local files".to_string();
    spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
        requirements: vec![
            alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
                path: Some(PathBuf::from("/mnt/source")),
            },
            alan_agent_protocol::DelegatedCapabilityRequirement::LlmConnection,
        ],
    });

    let decision = evaluate_delegated_launch_capabilities(
        &parent,
        &mut spec,
        &capability_plan(Some(host_mount), &["read_file"]),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(
        decision.recovery,
        alan_agent_protocol::DelegatedCapabilityRecovery::Satisfied
    );
    assert_eq!(spec.launch.task, "Inspect local files");
}

#[tokio::test]
async fn delegated_spawn_boundary_rewrites_narrowed_task_explicitly() {
    let temp = TempDir::new().unwrap();
    let parent = make_parent_state(
        &temp,
        RecordedRequests::default(),
        completed_response("unused"),
    );
    let host_mount = PathBuf::from("/tmp/repo");
    let mut spec = launch_spec(temp.path().join("agent"));
    spec.launch.task = "Review GitHub issue against local code".to_string();
    spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
        requirements: vec![
            alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
                path: Some(PathBuf::from("/mnt/source")),
            },
            alan_agent_protocol::DelegatedCapabilityRequirement::Github,
        ],
    });

    let decision = evaluate_delegated_launch_capabilities(
        &parent,
        &mut spec,
        &capability_plan(Some(host_mount), &["read_file"]),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(
        decision.recovery,
        alan_agent_protocol::DelegatedCapabilityRecovery::Narrowed
    );
    assert!(spec.launch.task.contains("NARROWED DELEGATION SCOPE"));
    assert!(spec.launch.task.contains("Withheld capabilities: github"));
}

#[tokio::test]
async fn delegated_spawn_boundary_declines_unsatisfied_mount() {
    let temp = TempDir::new().unwrap();
    let parent = make_parent_state(
        &temp,
        RecordedRequests::default(),
        completed_response("unused"),
    );
    let mut spec = launch_spec(temp.path().join("agent"));
    spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
        requirements: vec![
            alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
                path: Some(PathBuf::from("/mnt/private")),
            },
        ],
    });

    let error = evaluate_delegated_launch_capabilities(
        &parent,
        &mut spec,
        &capability_plan(None, &["read_file"]),
    )
    .await
    .unwrap_err();
    let rejection = error.downcast_ref::<DelegatedSpawnRejected>().unwrap();

    assert_eq!(
        rejection.decision.recovery,
        alan_agent_protocol::DelegatedCapabilityRecovery::AskUser
    );
    assert_eq!(rejection.decision.unsatisfied.len(), 1);
}
