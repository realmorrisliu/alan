use super::*;

#[tokio::test]
async fn project_text_projects_paths_from_every_delegated_mount() {
    let project = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let target = docs.path().join("notes.txt");
    std::fs::write(&target, "notes").unwrap();
    std::os::unix::fs::symlink(&target, project.path().join("notes-link.txt")).unwrap();

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
    approve(
        &service,
        7,
        "/mnt/docs",
        HostMountAccess::ReadOnly,
        docs.path(),
    )
    .await;

    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    let resolved = dunce::canonicalize(project.path().join("notes-link.txt")).unwrap();
    assert_eq!(
        adapter.project_text(&format!("realpath {}", resolved.display())),
        "realpath ../docs/notes.txt"
    );
}
