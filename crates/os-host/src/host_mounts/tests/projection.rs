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
async fn project_text_projects_percent_encoded_file_uri_roots_without_matching_siblings() {
    let project = tempfile::Builder::new()
        .prefix("alan project with spaces ")
        .tempdir()
        .unwrap();

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
    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    let root = dunce::canonicalize(project.path()).unwrap();
    let uri = url::Url::from_file_path(root.join("notes.txt")).unwrap();
    assert!(uri.as_str().contains("%20"));
    assert_eq!(adapter.project_text(uri.as_str()), "file://./notes.txt");

    let sibling_name = format!("{}-backup", root.file_name().unwrap().to_string_lossy());
    let sibling_uri =
        url::Url::from_file_path(root.with_file_name(sibling_name).join("notes.txt")).unwrap();
    assert_eq!(
        adapter.project_text(sibling_uri.as_str()),
        sibling_uri.as_str()
    );

    let emphasized_root = format!("__{}__", root.display());
    assert_eq!(adapter.project_text(&emphasized_root), "__.__");
    let ansi_emphasized_root = format!("__{}__\x1b[0m", root.display());
    assert_eq!(adapter.project_text(&ansi_emphasized_root), "__.__\x1b[0m");
    let single_emphasized_root = format!("_{}_", root.display());
    assert_eq!(adapter.project_text(&single_emphasized_root), "_._");

    let emphasized_sibling = format!("__{}_backup__", root.display());
    assert_eq!(
        adapter.project_text(&emphasized_sibling),
        emphasized_sibling
    );
    let single_emphasized_sibling = format!("_{}_backup_", root.display());
    assert_eq!(
        adapter.project_text(&single_emphasized_sibling),
        single_emphasized_sibling
    );

    let shell_escaped_root = root.to_string_lossy().replace(' ', "\\ ");
    assert_eq!(
        adapter.project_text(&format!("working directory: {shell_escaped_root}")),
        "working directory: ."
    );
    let shell_escaped_sibling = format!("{shell_escaped_root}-backup/notes.txt");
    assert_eq!(
        adapter.project_text(&shell_escaped_sibling),
        shell_escaped_sibling
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
    let docs_grant = approve(
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

    // File Tools can read another delegated grant without changing shell cwd.
    let read = alan_tools::ReadFileTool::new()
        .execute(json!({"path":"/mnt/docs/secret.txt"}), &context)
        .await
        .unwrap();
    assert_eq!(read["content"], "inactive grant marker");
    assert!(
        alan_tools::WriteFileTool::new()
            .execute(
                json!({"path":"/mnt/docs/secret.txt", "content":"overwrite"}),
                &context,
            )
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(&inactive_secret).unwrap(),
        "inactive grant marker"
    );

    // Relative structured paths use the same native file as the selected shell cwd.
    std::fs::create_dir(project.path().join("src")).unwrap();
    let nested = ToolContext::from_binding(
        service.reconcile(7, binding("/mnt/project/src")).unwrap(),
        Arc::new(Config::default()),
    );
    let ordinary_content = format!(
        "project data: {}",
        dunce::canonicalize(project.path()).unwrap().display()
    );
    alan_tools::WriteFileTool::new()
        .execute(
            json!({"path":"shared.txt", "content":ordinary_content}),
            &nested,
        )
        .await
        .unwrap();
    for path in ["shared.txt", "/mnt/project/src/shared.txt"] {
        let read = alan_tools::ReadFileTool::new()
            .execute(json!({"path":path}), &nested)
            .await
            .unwrap();
        assert_eq!(read["content"], ordinary_content);
        assert_eq!(read["path"], "/mnt/project/src/shared.txt");
        let found = alan_tools::GrepTool::new()
            .execute(json!({"path":path, "pattern":"project data"}), &nested)
            .await
            .unwrap();
        assert_eq!(found["total"], 1);
        assert_eq!(found["matches"][0]["path"], "/mnt/project/src/shared.txt");
        assert_eq!(found["matches"][0]["content"], ordinary_content);
    }
    let found = alan_tools::GrepTool::new()
        .execute(
            json!({"path":"/mnt/docs", "pattern":"inactive grant"}),
            &nested,
        )
        .await
        .unwrap();
    assert_eq!(found["matches"][0]["path"], "/mnt/docs/secret.txt");
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/shared.txt")).unwrap(),
        ordinary_content
    );
    let native_read = alan_tools::BashTool::new()
        .execute(json!({"command":"cat shared.txt"}), &nested)
        .await
        .unwrap();
    assert_eq!(native_read["stdout"], "project data: ..");
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/shared.txt")).unwrap(),
        ordinary_content
    );

    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("private.txt"), "not delegated").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("private.txt"),
        project.path().join("escape"),
    )
    .unwrap();
    assert!(
        alan_tools::ReadFileTool::new()
            .execute(json!({"path":"escape"}), &context)
            .await
            .is_err()
    );
    assert!(
        alan_tools::WriteFileTool::new()
            .execute(json!({"path":"escape", "content":"overwrite"}), &context,)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(outside.path().join("private.txt")).unwrap(),
        "not delegated"
    );
    // A failed save is an error; there is no buffered project copy to report as saved.
    std::fs::create_dir(project.path().join("directory")).unwrap();
    assert!(
        alan_tools::WriteFileTool::new()
            .execute(
                json!({"path":"directory", "content":"cannot save over a directory"}),
                &context,
            )
            .await
            .is_err()
    );
    assert!(project.path().join("directory").is_dir());

    let docs_adapter = service
        .reconcile(7, binding("/mnt/docs"))
        .unwrap()
        .adapter()
        .unwrap();
    let docs_shell_sandbox = docs_adapter.shell_sandbox().unwrap();
    assert!(docs_shell_sandbox.is_readable(docs.path()));
    assert!(!docs_shell_sandbox.is_readable(project.path()));
    let docs_context = ToolContext::from_binding(
        binding("/mnt/docs").with_adapter(docs_adapter),
        Arc::new(Config::default()),
    );
    let selected_read = alan_tools::BashTool::new()
        .execute(json!({"command":"cat secret.txt"}), &docs_context)
        .await
        .unwrap();
    assert_eq!(selected_read["stdout"], "inactive grant marker");
    let denied_write = alan_tools::BashTool::new()
        .execute(
            json!({"command":"printf overwrite > secret.txt"}),
            &docs_context,
        )
        .await;
    assert!(denied_write.is_err() || denied_write.is_ok_and(|result| result["success"] == false));
    assert_eq!(
        std::fs::read_to_string(&inactive_secret).unwrap(),
        "inactive grant marker"
    );

    let mut registry = alan_agent_engine::ToolRegistry::new();
    registry.register(alan_tools::BashTool::new());
    let runner = registry.process_runner();
    runner.register_process_binding(7, service.reconcile(7, binding("/mnt/docs")).unwrap());
    runner.register_process_authority(7, service.clone());
    service.revoke(&docs_grant.id, "test").unwrap();
    let refreshed = service.reconcile(7, binding("/mnt/project")).unwrap();
    let refreshed_context = ToolContext::from_binding(refreshed, Arc::new(Config::default()));
    assert!(
        alan_tools::ReadFileTool::new()
            .execute(json!({"path":"/mnt/docs/secret.txt"}), &refreshed_context,)
            .await
            .is_err()
    );
    // Check the actual Process launch boundary, which preserves the caller's cwd
    // when the adapter offers a different remaining grant after reconciliation.
    let outcome = runner
        .run(alan_agent_engine::tools::ToolProcessInvocation {
            pid: 8,
            parent: Some(7),
            executable: "/bin/bash".into(),
            args: vec![json!({"command":"printf escaped > wrong-grant.txt"}).to_string()],
        })
        .await;
    assert_eq!(outcome.exit_code, 1);
    let result: serde_json::Value = serde_json::from_slice(&outcome.output).unwrap();
    assert_eq!(result["success"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("choose an explicit directory")
    );
    assert!(!project.path().join("wrong-grant.txt").exists());
}
