use super::*;
use alan_agent_engine::{
    Config,
    tools::{Tool, ToolContext},
};
use serde_json::json;
use std::sync::Arc;

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

#[tokio::test]
async fn shell_sandbox_contains_only_the_mount_selected_by_shared_cwd() {
    let project = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let inactive_secret = docs.path().join("secret.txt");
    std::fs::write(&inactive_secret, "inactive grant marker").unwrap();

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

    let project_adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    let project_shell_sandbox = project_adapter.shell_sandbox().unwrap();
    assert!(project_shell_sandbox.is_writable(project.path()));
    assert!(!project_shell_sandbox.is_readable(docs.path()));
    assert!(project_adapter.sandbox().unwrap().is_readable(docs.path()));

    let context = ToolContext::from_binding(
        binding("/mnt/project").with_adapter(project_adapter.clone()),
        Arc::new(Config::default()),
    );
    let shell_result = alan_tools::BashTool::new()
        .execute(
            json!({
                "command": format!("cat '{}'", inactive_secret.display())
            }),
            &context,
        )
        .await;
    assert!(
        shell_result.is_err() || shell_result.is_ok_and(|result| result["success"] == false),
        "shell action reached the inactive Host Mount"
    );

    let docs_adapter = service
        .reconcile(7, binding("/mnt/docs"))
        .unwrap()
        .adapter()
        .unwrap();
    let docs_shell_sandbox = docs_adapter.shell_sandbox().unwrap();
    assert!(docs_shell_sandbox.is_readable(docs.path()));
    assert!(!docs_shell_sandbox.is_readable(project.path()));
}
