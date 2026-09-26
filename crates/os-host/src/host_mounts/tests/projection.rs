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
        .prefix("alan project (1) [$x] ")
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

    for name in [
        "notes here.txt",
        "notes#?.txt",
        "notes).txt",
        "notes(draft.txt",
        "notes[draft.txt",
        "notes{draft.txt",
    ] {
        let encoded = url::Url::from_file_path(root.join(name))
            .unwrap()
            .to_string()
            .replace('(', "%28")
            .replace(')', "%29")
            .replace('[', "%5B")
            .replace(']', "%5D")
            .replace('{', "%7B")
            .replace('}', "%7D");
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

    for suffix in [",", ";", ".", "!", ")", ":12", " "] {
        let sibling = format!("{}{suffix}", root.display());
        let encoded = serde_json::json!({"path": sibling}).to_string();
        for input in [encoded.clone(), encoded.replace('/', "\\/")] {
            let projected: serde_json::Value =
                serde_json::from_str(&adapter.project_text(&input)).unwrap();
            assert_eq!(projected["path"], sibling);
        }
    }

    for escaped_quote in ["\"\"backup/file", "\"\"", "\"/file"] {
        let input = format!("\"{}{escaped_quote}\"", root.display());
        assert_eq!(adapter.project_text(&input), input);
    }

    for (open, close) in [("(\"", "\")"), ("<\"", "\">"), ("\"", "\".")] {
        assert_eq!(
            adapter.project_text(&format!("{open}{}{close}", root.display())),
            format!("{open}.{close}")
        );
    }
    for ending in [",", ".", ";", "!"] {
        for (path, relative) in [(root.clone(), "."), (root.join("notes.txt"), "./notes.txt")] {
            let uri = url::Url::from_file_path(path).unwrap();
            let input = format!("see {uri}{ending} next");
            assert_eq!(
                adapter.project_text(&input),
                format!("see {relative}{ending} next")
            );
        }
    }
    assert_eq!(
        adapter.project_text(&format!("{uri}?query=value!#fragment,")),
        "./notes.txt?query=value!#fragment,"
    );
    use std::os::unix::ffi::OsStringExt;
    let name = std::ffi::OsString::from_vec(b"bytes-\xff.txt".to_vec());
    let byte_uri = url::Url::from_file_path(root.join(&name)).unwrap();
    let projected = adapter.project_text(byte_uri.as_str());
    assert_eq!(projected, "./bytes-%FF.txt");
    let round_trip = url::Url::parse("file:///public/cwd/")
        .unwrap()
        .join(&projected)
        .unwrap();
    assert_eq!(
        round_trip.to_file_path().unwrap(),
        PathBuf::from("/public/cwd").join(name)
    );

    let comma_uri = url::Url::from_file_path(root.join("notes,")).unwrap();
    assert_eq!(
        adapter.project_text(&format!("\"{comma_uri}\"")),
        "\"./notes%2C\""
    );

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

    let quoted = std::process::Command::new("/bin/bash")
        .args(["-c", "printf %q \"$1\"", "_"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(quoted.status.success());
    let shell_escaped_root = String::from_utf8(quoted.stdout).unwrap();
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
            format!(
                "{}/src\0{}\0{}/src/file\0",
                root.display(),
                root.display(),
                root.display()
            ),
            ".\0..\0./file\0".to_string(),
        ),
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
    for native in [root.join("src"), root.join("src/file.rs")] {
        let relative = if native.ends_with("file.rs") {
            "./file.rs"
        } else {
            "."
        };
        let uri = url::Url::from_file_path(&native)
            .unwrap()
            .to_string()
            .replacen("file:///", "file:/", 1);
        assert_eq!(adapter.project_text(&uri), relative);
        let json = serde_json::json!({"path": native})
            .to_string()
            .replace('/', "\\/");
        let projected = adapter.project_text(&json);
        let value: serde_json::Value = serde_json::from_str(&projected).unwrap();
        assert_eq!(value["path"], relative);
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
        ("/\0/etc\0/\0", ".\0./etc\0.\0"),
        ("file:///\0file:///etc\0", ".\0./etc\0"),
        ("file:///", "."),
        ("1 / 2", "1 / 2"),
        ("yes / no", "yes / no"),
        ("<div>text</div>", "<div>text</div>"),
        ("</svg:path >", "</svg:path >"),
        ("< /etc/hosts >", "< ./etc/hosts >"),
        ("/  \nnext", ".  \nnext"),
        ("__/__", "__.__"),
        ("**/**", "**.**"),
        ("_\x1b[31m/\x1b[0m_", "_\x1b[31m.\x1b[0m_"),
        ("/etc/hosts", "./etc/hosts"),
        (
            r#"{"cwd":"\/","path":"\/etc\/hosts"}"#,
            r#"{"cwd":".","path":"./etc/hosts"}"#,
        ),
        ("file:///etc/hosts", "./etc/hosts"),
        ("\x1b[31m/\x1b[0m", "\x1b[31m.\x1b[0m"),
        ("https://example.test/path", "https://example.test/path"),
    ] {
        assert_eq!(adapter.project_text(input), expected, "{input}");
    }
}

#[tokio::test]
async fn projection_prefers_active_grant_and_matches_escaped_cwd() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("sub dir")).unwrap();
    std::fs::create_dir(project.path().join("vendor")).unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    for (logical, native) in [
        ("/mnt/project", project.path().to_owned()),
        ("/mnt/vendor", project.path().join("vendor")),
    ] {
        approve(&service, 7, logical, HostMountAccess::ReadOnly, &native).await;
    }
    let adapter = service
        .reconcile(7, binding("/mnt/project/sub dir"))
        .unwrap()
        .adapter()
        .unwrap();
    let root = dunce::canonicalize(project.path()).unwrap();
    let quoted = std::process::Command::new("/bin/bash")
        .args(["-c", "printf %q \"$1\"", "_"])
        .arg(root.join("sub dir"))
        .output()
        .unwrap();
    assert!(quoted.status.success());
    assert_eq!(
        adapter.project_text(std::str::from_utf8(&quoted.stdout).unwrap()),
        "."
    );
    let target = root.join("vendor/x");
    assert_eq!(
        adapter.project_text(&target.to_string_lossy()),
        "../vendor/x"
    );
    let uri = url::Url::from_file_path(&target).unwrap();
    assert_eq!(adapter.project_text(uri.as_str()), "../vendor/x");
}

#[tokio::test]
async fn projection_uses_physical_cwd_and_bash_control_character_quoting() {
    let project = tempfile::Builder::new()
        .prefix("alan\nproject\t")
        .tempdir()
        .unwrap();
    std::fs::create_dir_all(project.path().join("deep/real")).unwrap();
    std::os::unix::fs::symlink("deep/real", project.path().join("link")).unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadOnly,
        project.path(),
    )
    .await;
    let adapter = service
        .reconcile(7, binding("/mnt/project/link"))
        .unwrap()
        .adapter()
        .unwrap();
    let root = dunce::canonicalize(project.path()).unwrap();
    let cwd = root.join("deep/real");
    assert_eq!(adapter.project_text(&cwd.to_string_lossy()), ".");
    assert_eq!(
        adapter.project_text(url::Url::from_file_path(&cwd).unwrap().as_str()),
        "."
    );
    assert_eq!(
        adapter.project_text(&root.join("sibling").to_string_lossy()),
        "../../sibling"
    );
    for (native, expected) in [(&cwd, "$'.'"), (&root, "$'../..'")] {
        let output = std::process::Command::new("/bin/bash")
            .args(["-c", "printf %q \"$1\"", "_"])
            .arg(native)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            adapter.project_text(std::str::from_utf8(&output.stdout).unwrap()),
            expected
        );
    }
}

#[tokio::test]
async fn projection_decodes_json_unicode_and_c_locale_shell_paths() {
    let project = tempfile::Builder::new()
        .prefix("prójéct🧪")
        .tempdir()
        .unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    approve(
        &service,
        7,
        "/mnt/project",
        HostMountAccess::ReadOnly,
        project.path(),
    )
    .await;
    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    let root = dunce::canonicalize(project.path()).unwrap();
    for (path, expected) in [(&root, "."), (&root.join("file.rs"), "./file.rs")] {
        let json = serde_json::json!({"path":path})
            .to_string()
            .replace('ó', "\\u00f3")
            .replace('é', "\\u00E9")
            .replace('🧪', "\\ud83e\\uddea");
        for encoded in [json.clone(), json.replace('/', "\\/")] {
            let projected: serde_json::Value =
                serde_json::from_str(&adapter.project_text(&encoded)).unwrap();
            assert_eq!(projected["path"], expected);
        }
        let output = std::process::Command::new("/bin/bash")
            .env("LC_ALL", "C")
            .args(["-c", "printf %q \"$1\"", "_"])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            adapter.project_text(std::str::from_utf8(&output.stdout).unwrap()),
            format!("$'{expected}'")
        );
    }
}

#[tokio::test]
async fn projected_json_escapes_public_namespace_components() {
    let project = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let service = service();
    service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
    for (logical, native) in [
        ("/mnt/project", project.path()),
        ("/mnt/do\"cs\\files", docs.path()),
    ] {
        approve(&service, 7, logical, HostMountAccess::ReadOnly, native).await;
    }
    let adapter = service
        .reconcile(7, binding("/mnt/project"))
        .unwrap()
        .adapter()
        .unwrap();
    let native = dunce::canonicalize(docs.path()).unwrap().join("file.txt");
    let json = serde_json::json!({"path":native}).to_string();
    assert!(!json.contains('\\'));
    let projected = adapter.project_text(&json);
    let value: serde_json::Value =
        serde_json::from_str(&projected).expect("projected JSON stays valid");
    assert_eq!(value["path"], "../do\"cs\\files/file.txt");
}

#[tokio::test]
async fn command_projection_retains_cwd_when_the_script_retargets_its_symlink() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join("deep/old")).unwrap();
    std::fs::create_dir(project.path().join("new")).unwrap();
    std::os::unix::fs::symlink("deep/old", project.path().join("link")).unwrap();
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
    let execution = service.reconcile(7, binding("/mnt/project/link")).unwrap();
    let context = ToolContext::from_binding(execution, Arc::new(Config::default()));
    let result = alan_tools::BashTool::new()
        .execute(
            json!({"command":"rm ../../link; ln -s new ../../link; pwd -P"}),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(result["exit_code"], 0, "{result}");
    assert_eq!(
        std::fs::read_link(project.path().join("link")).unwrap(),
        PathBuf::from("new")
    );
    assert_eq!(result["stdout"], ".\n", "{result}");
}

#[cfg(unix)]
#[test]
fn projection_preserves_shell_quoted_non_utf8_mount_bytes() {
    use std::os::unix::ffi::OsStringExt;
    // macOS filesystems reject these names; projection must still handle Unix byte paths.
    let root = PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/pr\xc3\xb3ject-\xff".to_vec(),
    ));
    let namespace_cwd = PathBuf::from("/mnt/project/src");
    let cwd = root.join("src");
    let sandbox_spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: root.clone(),
        access: ReifiedMountAccess::ReadOnly,
    }]);
    let adapter = NativeToolExecutionAdapter {
        mounts: vec![NativeToolMount {
            namespace_path: PathBuf::from("/mnt/project"),
            host_path: root.clone(),
            access: HostMountAccess::ReadOnly,
        }],
        projection_cwd: (cwd.clone(), namespace_cwd.clone()),
        namespace_cwd,
        cwd,
        sandbox: Sandbox::from_spec(sandbox_spec.clone()),
        shell_sandbox: Sandbox::from_spec(sandbox_spec),
    };
    for (path, expected) in [
        (&root, "$'..'"),
        (&root.join("src"), "$'.'"),
        (&root.join("src/file"), "$'./file'"),
    ] {
        let output = std::process::Command::new("/bin/bash")
            .env("LC_ALL", "C")
            .args(["-c", "printf %q \"$1\"", "_"])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            adapter.project_text(std::str::from_utf8(&output.stdout).unwrap()),
            expected
        );
    }
    assert_eq!(adapter.project_text("$'/tmp/próject-\\377/src'"), "$'.'");
}
