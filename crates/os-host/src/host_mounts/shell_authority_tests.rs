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
        std::fs::create_dir(&dependency).unwrap();
        let replaced = service.reconcile(7, current.clone());
        if replaced.is_ok() {
            eprintln!(
                "same-path replacement directory accepted despite retained original file tree"
            );
        }
        assert!(replaced.is_err());
        std::fs::remove_dir(&dependency).unwrap();
        std::fs::rename(&original, &dependency).unwrap();
        assert!(service.reconcile(7, current).is_ok());
    }
}

#[tokio::test]
async fn native_export_enforces_protected_subpath_writes_and_allows_project_edits() {
    use alan_ap::{ErrorCode, Fid, FileKind, OpenMode, Request, Response};

    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".git/hooks")).unwrap();
    std::fs::write(project.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(project.path().join(".git/config"), "[core]\n").unwrap();
    std::fs::write(project.path().join(".git/hooks/pre-commit"), "#!/bin/sh\n").unwrap();
    std::fs::create_dir_all(project.path().join(".alan")).unwrap();
    std::fs::write(project.path().join(".alan/settings.toml"), "mode = 1\n").unwrap();
    std::fs::create_dir_all(project.path().join(".agents")).unwrap();
    std::fs::write(project.path().join(".agents/skill.md"), "# skill\n").unwrap();
    std::fs::create_dir_all(project.path().join("src")).unwrap();
    std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n").unwrap();

    let export = native_export("/mnt/project", project.path(), HostMountAccess::ReadWrite).unwrap();
    let tree = export.file_tree();

    // .git/HEAD stays readable.
    tree.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(1),
        names: vec![".git".to_string(), "HEAD".to_string()],
    })
    .await
    .unwrap();
    tree.call(Request::Open {
        fid: Fid(1),
        mode: OpenMode::Read,
    })
    .await
    .unwrap();
    let Response::Read { data } = tree
        .call(Request::Read {
            fid: Fid(1),
            offset: 0,
            count: 64,
        })
        .await
        .unwrap()
    else {
        panic!("expected Read response");
    };
    assert_eq!(data, b"ref: refs/heads/main\n");
    tree.call(Request::Clunk { fid: Fid(1) }).await.unwrap();

    // Protected subpaths are non-writable and reject Open(Write).
    for (idx, path) in [
        vec![".git".to_string(), "config".to_string()],
        vec![
            ".git".to_string(),
            "hooks".to_string(),
            "pre-commit".to_string(),
        ],
        vec![".alan".to_string(), "settings.toml".to_string()],
        vec![".agents".to_string(), "skill.md".to_string()],
    ]
    .into_iter()
    .enumerate()
    {
        let fid = Fid(10 + idx as u64);
        tree.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: path,
        })
        .await
        .unwrap();
        let Response::Stat { stat } = tree.call(Request::Stat { fid }).await.unwrap() else {
            panic!("expected Stat response");
        };
        assert!(!stat.writable);
        assert_eq!(
            tree.call(Request::Open {
                fid,
                mode: OpenMode::Write,
            })
            .await
            .unwrap_err(),
            ErrorCode::NoAccess
        );
        tree.call(Request::Clunk { fid }).await.unwrap();
    }

    // Creating a new hook inside .git/hooks fails with NoAccess.
    tree.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(20),
        names: vec![".git".to_string(), "hooks".to_string()],
    })
    .await
    .unwrap();
    assert_eq!(
        tree.call(Request::Create {
            fid: Fid(20),
            newfid: Fid(21),
            name: "post-checkout".to_string(),
            kind: FileKind::File,
        })
        .await
        .unwrap_err(),
        ErrorCode::NoAccess
    );
    tree.call(Request::Clunk { fid: Fid(20) }).await.unwrap();

    // Ordinary project files remain writable.
    tree.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(30),
        names: vec!["src".to_string(), "main.rs".to_string()],
    })
    .await
    .unwrap();
    let Response::Stat { stat } = tree.call(Request::Stat { fid: Fid(30) }).await.unwrap() else {
        panic!("expected Stat response");
    };
    assert!(stat.writable);
    tree.call(Request::Open {
        fid: Fid(30),
        mode: OpenMode::Write,
    })
    .await
    .unwrap();
    tree.call(Request::Write {
        fid: Fid(30),
        offset: 0,
        data: b"fn main() { println!(\"ok\"); }\n".to_vec(),
    })
    .await
    .unwrap();
    tree.call(Request::Clunk { fid: Fid(30) }).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/main.rs")).unwrap(),
        "fn main() { println!(\"ok\"); }\n"
    );
}

#[tokio::test]
async fn shell_execution_at_resolves_subdirectory_or_sibling_mount_with_single_grant_isolation() {
    let project = tempfile::tempdir().unwrap();
    let sub = project.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let other = tempfile::tempdir().unwrap();
    let dependency = tempfile::tempdir().unwrap();

    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
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

    // Even when namespace_cwd is `/`, cwd() surfaces available mounts and shell_execution_at
    // resolves an explicit mount directory with strict single-RW-mount sandbox isolation.
    let root_binding = service.reconcile(7, binding("/")).unwrap();
    let root_adapter = root_binding.adapter().unwrap();
    let root_err = root_adapter.cwd().unwrap_err().to_string();
    assert!(
        root_err.contains("outside delegated Host Mounts"),
        "{root_err}"
    );
    assert!(root_err.contains("/mnt/project"), "{root_err}");

    let (other_host, other_sandbox) = root_adapter
        .shell_execution_at(&root_binding.namespace_cwd, Path::new("/mnt/other"))
        .unwrap();
    assert_eq!(other_host, std::fs::canonicalize(other.path()).unwrap());
    assert!(other_sandbox.is_writable(other.path()));
    assert!(!other_sandbox.is_writable(project.path()));
    assert!(!other_sandbox.is_readable(project.path()));
    assert!(other_sandbox.is_readable(dependency.path()));

    // Relative cwd inside /mnt/project resolves /mnt/project/sub while excluding /mnt/other.
    let proj_binding = service.reconcile(7, binding("/mnt/project")).unwrap();
    let (sub_host, sub_sandbox) = proj_binding
        .adapter()
        .unwrap()
        .shell_execution_at(&proj_binding.namespace_cwd, Path::new("sub"))
        .unwrap();
    assert_eq!(sub_host, std::fs::canonicalize(&sub).unwrap());
    assert!(sub_sandbox.is_writable(&sub));
    assert!(!sub_sandbox.is_writable(other.path()));

    // `/` and relative escapes fail closed.
    assert!(
        proj_binding
            .adapter()
            .unwrap()
            .shell_execution_at(&proj_binding.namespace_cwd, Path::new("/"))
            .is_err()
    );
    assert!(
        proj_binding
            .adapter()
            .unwrap()
            .shell_execution_at(&proj_binding.namespace_cwd, Path::new("../other"))
            .is_err()
    );
}
