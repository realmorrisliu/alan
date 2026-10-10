use super::tests::{approve, binding, service};
use super::*;
use alan_agent_engine::tools::ToolExecutionAuthority;
use alan_kernel::{LiveNamespace, Namespace, Pid};

#[tokio::test]
async fn shell_projects_only_selected_and_same_process_read_only_grants() {
    let project = tempfile::tempdir().unwrap();
    let dependency = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let service = service();
    for pid in [7, 8] {
        service.register_process(Pid(pid), LiveNamespace::new(Namespace::new()));
    }
    let readonly = approve(
        &service,
        7,
        "/mnt/dependency",
        HostMountAccess::ReadOnly,
        dependency.path(),
    )
    .await;
    approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadWrite,
        project.path(),
    )
    .await;
    approve(
        &service,
        7,
        "/mnt/other",
        HostMountAccess::ReadWrite,
        other.path(),
    )
    .await;
    approve(
        &service,
        8,
        "/mnt/foreign",
        HostMountAccess::ReadOnly,
        foreign.path(),
    )
    .await;
    let mut current = service.reconcile(7, binding("/mnt/project")).unwrap();
    let adapter = current.adapter().unwrap();
    assert_eq!(
        adapter.cwd().unwrap(),
        std::fs::canonicalize(project.path()).unwrap()
    );
    let shell = adapter.shell_sandbox().unwrap();
    assert!(shell.is_readable(project.path()));
    assert!(shell.is_writable(project.path()));
    assert!(shell.is_readable(dependency.path()));
    assert!(!shell.is_writable(dependency.path()));
    for root in [other.path(), foreign.path()] {
        assert!(!shell.is_readable(root));
        assert!(!shell.is_writable(root));
    }
    assert!(adapter.sandbox().unwrap().is_writable(other.path()));
    service
        .revoke(&readonly.id, "candidate read-only dependency test")
        .unwrap();
    current = service.reconcile(7, current).unwrap();
    let shell = current.adapter().unwrap().shell_sandbox().unwrap();
    assert!(shell.is_writable(project.path()));
    assert!(!shell.is_readable(dependency.path()));
    assert!(!shell.is_readable(other.path()));
    assert!(!shell.is_readable(foreign.path()));
}

#[tokio::test]
async fn missing_cwd_never_selects_another_writable_grant() {
    let other = tempfile::tempdir().unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/other",
        HostMountAccess::ReadWrite,
        other.path(),
    )
    .await;
    let missing = service.reconcile(7, binding("/mnt/missing"));
    if let Ok(binding) = &missing {
        eprintln!(
            "missing cwd unexpectedly selected {}; other writable: {}",
            binding.namespace_cwd.display(),
            binding
                .adapter()
                .unwrap()
                .shell_sandbox()
                .unwrap()
                .is_writable(other.path())
        );
    }
    assert!(missing.is_err());
    assert!(service.reconcile(7, binding("/mnt/other")).is_ok());
}

#[tokio::test]
async fn retargeted_backing_root_does_not_move_grant_authority() {
    for access in [HostMountAccess::ReadOnly, HostMountAccess::ReadWrite] {
        let parent = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let dependency = parent.path().join("dependency");
        let original = parent.path().join("original");
        std::fs::create_dir(&dependency).unwrap();
        let service = service();
        service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
        approve(
            &service,
            7,
            "/mnt/project",
            HostMountAccess::ReadWrite,
            project.path(),
        )
        .await;
        approve(&service, 7, "/mnt/dependency", access, &dependency).await;
        let current = service.reconcile(7, binding("/mnt/project")).unwrap();
        std::fs::rename(&dependency, &original).unwrap();
        std::os::unix::fs::symlink(outside.path(), &dependency).unwrap();
        let error = service.reconcile(7, current.clone()).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("/mnt/dependency"), "{message}");
        assert!(!message.contains(&parent.path().display().to_string()));
        assert!(!message.contains(&outside.path().display().to_string()));
        std::fs::remove_file(&dependency).unwrap();
        assert!(service.reconcile(7, current.clone()).is_err());
        std::fs::write(&dependency, "not a directory").unwrap();
        assert!(service.reconcile(7, current.clone()).is_err());
        std::fs::remove_file(&dependency).unwrap();
        std::fs::rename(&original, &dependency).unwrap();
        assert!(service.reconcile(7, current).is_ok());
    }
}
