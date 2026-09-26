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
    assert_eq!(adapter.project_text(uri.as_str()), "./notes.txt");

    let spaced_uri = url::Url::from_file_path(root.join("notes with spaces.txt")).unwrap();
    assert_eq!(
        adapter.project_text(spaced_uri.as_str()),
        "./notes%20with%20spaces.txt"
    );
    assert_eq!(
        adapter.project_text(&format!("\x1b]8;;{uri}\x1b\\notes\x1b]8;;\x1b\\")),
        "\x1b]8;;./notes.txt\x1b\\notes\x1b]8;;\x1b\\"
    );
    for suffix in [
        "#backup/file",
        "?backup/file",
        "*backup/file",
        ":backup/file",
        ",backup/file",
    ] {
        let sibling = format!("{}{suffix}", root.display());
        assert_eq!(adapter.project_text(&sibling), sibling);
    }
    assert_eq!(
        adapter.project_text(&format!("**{}**", root.display())),
        "**.**"
    );

    let unrelated_scheme = format!("pro{uri}");
    assert_eq!(adapter.project_text(&unrelated_scheme), unrelated_scheme);

    for name in ["notes here.txt", "notes#?.txt", "notes).txt"] {
        let encoded = url::Url::from_file_path(root.join(name))
            .unwrap()
            .to_string()
            .replace(')', "%29");
        let projected = adapter.project_text(&encoded);
        assert!(!projected.contains([' ', '#', '?', ')']), "{projected}");
        let target = url::Url::parse("file:///public/cwd/")
            .unwrap()
            .join(&projected)
            .unwrap();
        assert_eq!(
            target.to_file_path().unwrap(),
            PathBuf::from("/public/cwd").join(name)
        );
        for wrapped in [
            format!("[notes]({encoded})"),
            format!("\x1b]8;;{encoded}\x1b\\notes\x1b]8;;\x1b\\"),
        ] {
            let expected = wrapped.replace(&encoded, &projected);
            assert_eq!(adapter.project_text(&wrapped), expected);
        }
    }
    for (input, expected) in [
        (
            format!(r#"{{"cwd":"{}","ok":true}}"#, root.display()),
            r#"{"cwd":".","ok":true}"#.to_string(),
        ),
        (
            format!(r#""{}",true"#, root.display()),
            r#"".",true"#.to_string(),
        ),
    ] {
        assert_eq!(adapter.project_text(&input), expected);
    }

    for escaped_quote in ["\"\"backup/file", "\"\"", "\"/file"] {
        let input = format!("\"{}{escaped_quote}\"", root.display());
        assert_eq!(adapter.project_text(&input), input);
    }

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
async fn captured_paths_follow_cwd_without_rewriting_project_file_data() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("src")).unwrap();
    let root = dunce::canonicalize(project.path()).unwrap();
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
    let execution = service.reconcile(7, binding("/mnt/project/src")).unwrap();
    let adapter = execution.adapter().unwrap();
    for (input, expected) in [
        (
            format!("{}/src/file.rs:12", root.display()),
            "./file.rs:12".to_string(),
        ),
        (
            format!("{}/README.md", root.display()),
            "../README.md".to_string(),
        ),
        (
            format!("\x1b[31m{}/src\x1b[0m", root.display()),
            "\x1b[31m.\x1b[0m".to_string(),
        ),
        (format!("({}/src).", root.display()), "(.).".to_string()),
        (
            format!("{}-backup/src", root.display()),
            format!("{}-backup/src", root.display()),
        ),
        (
            "https://example.test/path".into(),
            "https://example.test/path".into(),
        ),
    ] {
        assert_eq!(adapter.project_text(&input), expected, "{input}");
    }
    let context = ToolContext::from_binding(execution, Arc::new(Config::default()));
    let result = alan_tools::BashTool::new()
        .execute(json!({"command":"pwd > cwd.txt; pwd"}), &context)
        .await
        .unwrap();
    assert_eq!(result["stdout"], ".\n");
    let native_cwd = format!("{}\n", root.join("src").display());
    assert_eq!(
        std::fs::read_to_string(root.join("src/cwd.txt")).unwrap(),
        native_cwd
    );
    let read = alan_tools::ReadFileTool::new()
        .execute(json!({"path":"cwd.txt"}), &context)
        .await
        .unwrap();
    assert_eq!(read["content"], native_cwd.trim_end());
}

#[tokio::test]
async fn root_backed_mount_projects_bare_cwd_and_descendants() {
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadOnly,
        Path::new("/"),
    )
    .await;
    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    for (input, expected) in [
        ("/", "."),
        ("/\n", ".\n"),
        ("file:///", "."),
        ("/etc/hosts", "./etc/hosts"),
        ("file:///etc/hosts", "./etc/hosts"),
        ("\x1b[31m/\x1b[0m", "\x1b[31m.\x1b[0m"),
        ("https://example.test/path", "https://example.test/path"),
    ] {
        assert_eq!(adapter.project_text(input), expected, "{input}");
    }
}
