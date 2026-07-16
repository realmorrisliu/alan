use super::*;

#[tokio::test]
async fn spawn_child_runtime_resolves_package_child_agent_target() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Package child target resolved.");
    let capability_view = capability_view_with_package_child_agent(&temp);
    let parent = make_parent_state_with_capability_view(
        &temp,
        requests.clone(),
        response.clone(),
        capability_view,
    );
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
        handles: vec![SpawnHandle::HostMounts],
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

#[tokio::test]
async fn spawn_child_runtime_resolves_package_child_agent_target_from_refreshed_view() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Package child target resolved after refresh.");
    let package_store = temp.path().join("package-store");
    let package_root = package_store.join("repo-review");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(
        package_root.join("SKILL.md"),
        r#"---
name: Repo Review
description: Review repository changes
---

Body
"#,
    )
    .unwrap();

    let capability_view = crate::skills::ResolvedCapabilityView::from_package_dirs(vec![
        crate::skills::ScopedPackageDir {
            path: package_store,
            scope: crate::skills::SkillScope::Descriptor,
        },
    ]);
    let parent = make_parent_state_with_capability_view(
        &temp,
        requests.clone(),
        response.clone(),
        capability_view,
    );

    std::fs::create_dir_all(package_root.join("agents/reviewer")).unwrap();
    std::fs::write(
        package_root.join("agents/reviewer/agent.toml"),
        "tool_repeat_limit = 4\n",
    )
    .unwrap();

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
        handles: vec![SpawnHandle::HostMounts],
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
    assert_eq!(
        result.output_text,
        "Package child target resolved after refresh."
    );
}
