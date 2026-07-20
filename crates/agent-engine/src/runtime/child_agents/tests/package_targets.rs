use super::*;

#[tokio::test]
async fn spawn_child_runtime_resolves_package_child_agent_target() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Package child target resolved.");
    let capability_view = capability_view_with_package_child_agent();
    let mut parent = make_parent_state_with_capability_view(
        &temp,
        requests.clone(),
        response.clone(),
        capability_view,
    );
    install_parent_package_reference(&mut parent);
    let spec = SpawnSpec {
        target: SpawnTarget::PackageChildAgent {
            package_id: "skill:repo-review".to_string(),
            export_name: "reviewer".to_string(),
        },
        launch: alan_agent_protocol::SpawnLaunchInputs {
            task: "Review the repository changes".to_string(),
            cwd: Some(PathBuf::from("/mnt/source")),
            timeout_secs: Some(30),
            ..alan_agent_protocol::SpawnLaunchInputs::default()
        },
        handles: Vec::new(),
        host_mounts: vec![alan_agent_protocol::SpawnHostMount {
            grant: "grant-source".to_string(),
            target: PathBuf::from("/mnt/source"),
            access: alan_agent_protocol::SpawnMountAccess::ReadWrite,
        }],
        runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
        delegated: None,
    };

    let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    assert_eq!(result.output_text, "Package child target resolved.");
}
