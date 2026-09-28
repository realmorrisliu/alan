use super::*;
use alan_agent_engine::{
    Config,
    tools::{Tool, ToolContext},
};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn project_text_projects_paths_from_the_active_mount() {
    let project = tempfile::tempdir().unwrap();
    let target = project.path().join("notes.txt");
    let root = dunce::canonicalize(project.path()).unwrap();
    std::fs::write(&target, "notes").unwrap();
    let symlink = project.path().join("notes-link.txt");
    std::os::unix::fs::symlink(&target, &symlink).unwrap();

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
    let resolved = dunce::canonicalize(&target).unwrap();
    assert_eq!(
        adapter.project_text(&format!("realpath {}", resolved.display())),
        "realpath ./notes.txt"
    );
    let json_root = root.to_string_lossy().replace('/', "\\/");
    assert_eq!(
        adapter.project_text(&format!(r#"{{"cwd":"{json_root}"}}"#)),
        r#"{"cwd":"."}"#
    );
    assert_eq!(
        adapter.project_text(&format!("<a href=\"{}/report.html\">", root.display())),
        "<a href=\"./report.html\">"
    );
    let symlink_url = url::Url::from_file_path(&symlink).unwrap();
    assert_eq!(
        adapter.project_text(symlink_url.as_str()),
        "./notes-link.txt"
    );
    let url_with_metadata = format!(
        "{}?download=1#preview",
        url::Url::from_file_path(&target).unwrap()
    );
    assert_eq!(adapter.project_text(&url_with_metadata), "./notes.txt");
    let outside = tempfile::tempdir().unwrap();
    let outside_link = project.path().join("outside-link.txt");
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), &outside_link).unwrap();
    let outside_url = url::Url::from_file_path(&outside_link).unwrap();
    assert_eq!(
        adapter.project_text(outside_url.as_str()),
        "./outside-link.txt"
    );
    let spaced_target = root.join("notes file.txt");
    std::fs::write(&spaced_target, "notes").unwrap();
    let spaced_url = url::Url::from_file_path(&spaced_target).unwrap();
    assert_eq!(
        adapter.project_text(spaced_url.as_str()),
        "./notes file.txt"
    );
}

#[tokio::test]
async fn project_text_preserves_root_relative_urls_for_a_root_mount() {
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadWrite,
        std::path::Path::new(std::path::MAIN_SEPARATOR_STR),
    )
    .await;

    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    assert_eq!(adapter.project_text("pwd: /"), "pwd: .");
    assert_eq!(adapter.project_text("pwd: / is cwd"), "pwd: . is cwd");
    assert_eq!(
        adapter.project_text("realpath /etc/passwd"),
        "realpath ./etc/passwd"
    );
    assert_eq!(
        adapter.project_text(r#"{"one":"/etc/passwd","two":"/usr/bin/env"}"#),
        r#"{"one":"./etc/passwd","two":"./usr/bin/env"}"#
    );
    assert_eq!(
        adapter.project_text(r#"{"cwd":"\/etc\/passwd"}"#),
        r#"{"cwd":"./etc/passwd"}"#
    );
    assert_eq!(
        adapter.project_text("path=/etc/passwd"),
        "path=./etc/passwd"
    );
    let scheme_relative_url = r#"{"url":"//cdn.example.test/app.js"}"#;
    assert_eq!(
        adapter.project_text(scheme_relative_url),
        scheme_relative_url
    );
    let root_relative_url = r#"{"url":"/api/items"}"#;
    assert_eq!(adapter.project_text(root_relative_url), root_relative_url);
    let cwd = adapter.cwd().unwrap();
    assert_eq!(
        adapter.project_text(&format!("\x1b[31m{}\x1b[0m", cwd.display())),
        "\x1b[31m.\x1b[0m"
    );
    assert_eq!(
        adapter.project_text(&format!("\x1b[31m{}\x1b[0m/src", cwd.display())),
        "\x1b[31m./src\x1b[0m"
    );
    assert_eq!(
        adapter.project_text(&format!(
            "cwd={},url=https://example.test/path",
            adapter.cwd().unwrap().display()
        )),
        "cwd=.,url=https://example.test/path"
    );
    let url_before_cwd = format!(
        r#"{{"url":"https://example.test","cwd":"{}"}}"#,
        adapter.cwd().unwrap().display()
    );
    assert_eq!(
        adapter.project_text(&url_before_cwd),
        r#"{"url":"https://example.test","cwd":"."}"#
    );
    let file_url = url::Url::from_file_path("/etc/passwd").unwrap();
    assert_eq!(adapter.project_text(file_url.as_str()), "./etc/passwd");
    let special_dir = tempfile::Builder::new()
        .prefix("alan project (1) ")
        .tempdir()
        .unwrap();
    let native_special_dir = dunce::canonicalize(special_dir.path()).unwrap();
    let namespace_special_dir = Path::new("/mnt/project").join(
        native_special_dir
            .strip_prefix(std::path::MAIN_SEPARATOR_STR)
            .unwrap(),
    );
    let special_cwd = service
        .reconcile(7, binding(namespace_special_dir.to_str().unwrap()))
        .unwrap()
        .adapter()
        .unwrap();
    let escaped_cwd = std::process::Command::new("bash")
        .args(["-c", "printf '%q' \"$1\"", "_"])
        .arg(special_cwd.cwd().unwrap())
        .output()
        .unwrap();
    assert!(escaped_cwd.status.success());
    let escaped_cwd = String::from_utf8(escaped_cwd.stdout).unwrap();
    assert!(escaped_cwd.contains("\\ "));
    assert!(escaped_cwd.contains("\\("));
    assert_eq!(special_cwd.project_text(&escaped_cwd), ".");
    assert_eq!(
        special_cwd.project_text(&format!("{escaped_cwd}/src/lib.rs")),
        "./src/lib.rs"
    );
    assert_eq!(adapter.project_text("[guide]: /guide"), "[guide]: /guide");
    assert_eq!(adapter.project_text("[docs](/guide)"), "[docs](/guide)");
    assert_eq!(
        adapter.project_text("body { background: url(/assets/bg.png) }"),
        "body { background: url(/assets/bg.png) }"
    );
    let css_with_space = "body { background: url( \"/assets/bg.png\" ) }";
    assert_eq!(adapter.project_text(css_with_space), css_with_space);
    for html in [
        "<form action=\"/submit\">",
        "<video poster=\"/poster.png\">",
        "<blockquote cite=\"/quote\">",
    ] {
        assert_eq!(adapter.project_text(html), html);
    }

    let nested = service
        .reconcile(7, binding("/mnt/project/tmp"))
        .unwrap()
        .adapter()
        .unwrap();
    let projected = nested
        .project_text("realpath /usr/bin/env")
        .strip_prefix("realpath ")
        .unwrap()
        .to_owned();
    let cwd = dunce::canonicalize(std::path::Path::new(std::path::MAIN_SEPARATOR_STR).join("tmp"))
        .unwrap();
    assert_eq!(
        dunce::canonicalize(cwd.join(projected)).unwrap(),
        dunce::canonicalize("/usr/bin/env").unwrap()
    );

    let parent = tempfile::tempdir().unwrap();
    let cwd = parent.path().join("project");
    let sibling = parent.path().join("project backup");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::create_dir(cwd.join("src")).unwrap();
    std::fs::write(cwd.join("src/lib.rs"), "source").unwrap();
    std::fs::create_dir(&sibling).unwrap();
    let path_with_space = sibling.join("file.txt");
    std::fs::write(&path_with_space, "notes").unwrap();
    let cwd = dunce::canonicalize(cwd).unwrap();
    let path_with_space = dunce::canonicalize(path_with_space).unwrap();
    let namespace_cwd = std::path::Path::new("/mnt/project")
        .join(cwd.strip_prefix(std::path::MAIN_SEPARATOR_STR).unwrap());
    let nested = service
        .reconcile(7, binding(namespace_cwd.to_str().unwrap()))
        .unwrap()
        .adapter()
        .unwrap();
    let projected = nested
        .project_text(&format!("realpath {}", path_with_space.display()))
        .strip_prefix("realpath ")
        .unwrap()
        .to_owned();
    assert_eq!(
        dunce::canonicalize(cwd.join(projected)).unwrap(),
        path_with_space,
        "root-grant paths with a space remain usable from a nested cwd"
    );
    let projected_sibling = nested
        .project_text(&format!("realpath {}", sibling.display()))
        .strip_prefix("realpath ")
        .unwrap()
        .to_owned();
    assert_eq!(
        dunce::canonicalize(cwd.join(projected_sibling)).unwrap(),
        dunce::canonicalize(sibling).unwrap(),
        "root-grant paths with a spaced final component remain usable"
    );
    assert_eq!(
        nested.project_text(&format!("open failed: {} missing", cwd.display())),
        "open failed: ../project missing",
        "nonexistent path diagnostics preserve spaced final components"
    );
    assert_eq!(
        nested.project_text(&format!("{} src/lib.rs", cwd.display())),
        ". src/lib.rs",
        "a relative path token after cwd remains a separate path"
    );
    assert_eq!(
        nested.project_text(&format!("pwd: {} is cwd", cwd.display())),
        "pwd: . is cwd",
        "ordinary prose after the cwd is not part of the path"
    );
}

#[tokio::test]
async fn project_text_preserves_unmatched_paths_with_space_siblings() {
    let parent = tempfile::tempdir().unwrap();
    let parent = dunce::canonicalize(parent.path()).unwrap();
    let project = parent.join("project");
    let project_with_space = parent.join("project backup");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&project_with_space).unwrap();

    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadWrite,
        &project,
    )
    .await;
    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    let sibling = format!("{}/file", project_with_space.display());
    assert_eq!(
        adapter.project_text(&sibling),
        sibling,
        "an unmatched filename may continue a root through a space"
    );
    assert_eq!(
        adapter.project_text(&format!("{} is cwd", project.display())),
        ". is cwd",
        "ordinary prose after the active root must not expose its backing path"
    );
    assert_eq!(
        adapter.project_text(&format!("{} exists", project.display())),
        ". exists",
        "a final prose word after the active root must not expose its backing path"
    );
    let sibling = project_with_space.display().to_string();
    assert_eq!(
        adapter.project_text(&sibling),
        sibling,
        "an existing sibling whose name follows a space remains unchanged"
    );
    assert_eq!(
        adapter.project_text(&format!("{} /etc/passwd", project.display())),
        ". /etc/passwd",
        "a following absolute path is a separate token"
    );
}

#[tokio::test]
async fn project_text_does_not_use_an_inactive_root_grant_for_path_projection() {
    let project = tempfile::tempdir().unwrap();
    let project_root = dunce::canonicalize(project.path()).unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/root",
        HostMountAccess::ReadWrite,
        std::path::Path::new(std::path::MAIN_SEPARATOR_STR),
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
    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();

    assert_eq!(
        adapter.project_text(&format!("path={}/file", project_root.display())),
        "path=./file"
    );
}

#[tokio::test]
async fn project_text_keeps_disjoint_grant_paths_out_of_the_active_cwd() {
    let project = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let other_path = dunce::canonicalize(docs.path()).unwrap().join("notes.txt");
    std::fs::write(&other_path, "notes").unwrap();

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
        HostMountAccess::ReadWrite,
        docs.path(),
    )
    .await;
    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    let output = format!("realpath {}", other_path.display());
    let projected = adapter
        .project_text(&output)
        .strip_prefix("realpath ")
        .unwrap()
        .to_owned();

    assert_eq!(projected, "[another grant]/notes.txt");
    let other_file_url = url::Url::from_file_path(&other_path).unwrap();
    assert_eq!(
        adapter.project_text(other_file_url.as_str()),
        "[another grant]/notes.txt"
    );
}

#[tokio::test]
async fn project_text_keeps_nested_native_backing_relative_to_the_active_grant() {
    let parent_dir = tempfile::tempdir().unwrap();
    let parent = dunce::canonicalize(parent_dir.path()).unwrap();
    let nested = parent.join("nested");
    std::fs::create_dir(&nested).unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/parent",
        HostMountAccess::ReadWrite,
        &parent,
    )
    .await;
    approve(
        &service,
        7,
        "/mnt/nested",
        HostMountAccess::ReadWrite,
        &nested,
    )
    .await;

    let adapter = service
        .reconcile(7, binding("/mnt/parent"))
        .unwrap()
        .adapter()
        .unwrap();
    assert_eq!(
        adapter.project_text(&format!("{}/nested/file", parent.display())),
        "./nested/file"
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
    assert_eq!(adapter.project_text(uri.path()), uri.path());

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
    assert_eq!(
        adapter.project_text(&format!("__{}/src/lib.rs__", root.display())),
        "__./src/lib.rs__"
    );
    let ansi_unicode_descendant = format!("___{}\x1b[0m/src/é___", root.display());
    assert_eq!(
        adapter.project_text(&ansi_unicode_descendant),
        "___.\x1b[0m/src/é___"
    );
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

    std::fs::create_dir(root.join("source (1)")).unwrap();
    let special_cwd = service
        .reconcile(7, binding("/mnt/project/source (1)"))
        .unwrap()
        .adapter()
        .unwrap();
    let escaped_cwd = std::process::Command::new("bash")
        .args(["-c", "printf '%q' \"$1\"", "_"])
        .arg(special_cwd.cwd().unwrap())
        .output()
        .unwrap();
    assert!(escaped_cwd.status.success());
    let escaped_cwd = String::from_utf8(escaped_cwd.stdout).unwrap();
    assert!(escaped_cwd.contains("\\("));
    assert_eq!(special_cwd.project_text(&escaped_cwd), ".");
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
            format!("{}/src is cwd", root.display()),
            ". is cwd".to_string(),
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
    let host_cwd = root.join("src").to_string_lossy().replace('\'', "'\\''");
    let result = alan_tools::BashTool::new()
        .execute(
            json!({"command":format!("pwd > cwd.txt; pwd; printf '%s is cwd\\n' '{host_cwd}'")}),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(result["stdout"], ".\n. is cwd\n");
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
