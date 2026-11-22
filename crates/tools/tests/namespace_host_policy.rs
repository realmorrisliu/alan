use std::path::PathBuf;
use std::sync::Arc;

use alan_agent_engine::Config;
use alan_agent_engine::tools::{
    PROTECTED_SUBPATHS, SandboxSpec, TOOL_READ_ONLY_SYSTEM_PREFIXES, Tool, ToolContext,
    ToolExecutionBinding,
};
use alan_ap::InProcessTransport;
use alan_hostfs::{HostDirAccess, HostDirFs, HostDirPolicy};
use alan_kernel::{Access, MountFs, Namespace};
use alan_tools::{EditFileTool, GlobTool, GrepTool, ListDirTool, ReadFileTool, WriteFileTool};
use serde_json::json;
use tempfile::TempDir;

fn tool_ctx_with_view(namespace_cwd: &str, view: Arc<MountFs>) -> ToolContext {
    ToolContext::from_binding(
        ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from(namespace_cwd),
            PathBuf::new(),
        ),
        Arc::new(Config::default()),
    )
    .with_namespace(8, InProcessTransport::new(view))
}

#[tokio::test]
async fn host_mount_sensitive_read_exclusions_enforced_across_all_file_tools() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join(".ssh")).unwrap();
    std::fs::write(home.path().join(".ssh/id_rsa"), "SECRET_SSH_KEY\n").unwrap();
    std::fs::create_dir_all(home.path().join(".config/gh")).unwrap();
    std::fs::write(
        home.path().join(".config/gh/hosts.yml"),
        "SECRET_GH_TOKEN\n",
    )
    .unwrap();
    std::fs::create_dir_all(home.path().join(".config/app")).unwrap();
    std::fs::write(
        home.path().join(".config/app/config.toml"),
        "PUBLIC_CONFIG\n",
    )
    .unwrap();
    std::fs::create_dir_all(home.path().join("Library/Keychains")).unwrap();
    std::fs::write(
        home.path().join("Library/Keychains/login.keychain"),
        "SECRET_KEYCHAIN\n",
    )
    .unwrap();
    std::fs::write(home.path().join("notes.txt"), "PUBLIC_NOTES\n").unwrap();

    let denylist = SandboxSpec::sensitive_read_denylist_for_home(home.path());
    let policy = HostDirPolicy::for_host_mount(home.path(), &denylist, PROTECTED_SUBPATHS);
    let host_fs = Arc::new(
        HostDirFs::new(home.path(), HostDirAccess::ReadWrite)
            .unwrap()
            .with_policy(policy),
    );

    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/project-1",
        InProcessTransport::new(host_fs),
        Access::ReadWrite,
    );
    let view = Arc::new(
        MountFs::new(ns).with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES.iter().copied()),
    );
    let ctx = tool_ctx_with_view("/mnt/project-1", view);

    // Direct reads of sensitive paths fail.
    for sensitive in [
        "/mnt/project-1/.ssh/id_rsa",
        "/mnt/project-1/.config/gh/hosts.yml",
        "/mnt/project-1/Library/Keychains/login.keychain",
    ] {
        assert!(
            ReadFileTool::new()
                .execute(json!({"path": sensitive}), &ctx)
                .await
                .is_err(),
            "read_file({sensitive}) must be denied"
        );
    }

    // list_dir hides .ssh at mount root and gh inside .config.
    let root_list = ListDirTool::new()
        .execute(json!({"path": "/mnt/project-1"}), &ctx)
        .await
        .unwrap();
    let root_names: Vec<&str> = root_list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert!(root_names.contains(&".config"));
    assert!(root_names.contains(&"notes.txt"));
    assert!(!root_names.contains(&".ssh"));

    let config_list = ListDirTool::new()
        .execute(json!({"path": "/mnt/project-1/.config"}), &ctx)
        .await
        .unwrap();
    let config_names: Vec<&str> = config_list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(config_names, vec!["app"]);

    // Direct list_dir on excluded directories fails.
    assert!(
        ListDirTool::new()
            .execute(json!({"path": "/mnt/project-1/.ssh"}), &ctx)
            .await
            .is_err()
    );
    assert!(
        ListDirTool::new()
            .execute(json!({"path": "/mnt/project-1/.config/gh"}), &ctx)
            .await
            .is_err()
    );

    // grep and glob never see excluded credentials (neither hidden nor non-hidden).
    let grepped = GrepTool::new()
        .execute(
            json!({
                "pattern": "SECRET_",
                "path": "/mnt/project-1",
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(grepped["total"], 0);

    let cfg_grepped = GrepTool::new()
        .execute(
            json!({
                "pattern": "SECRET_|PUBLIC_",
                "path": "/mnt/project-1/.config",
                "files_only": true,
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        cfg_grepped["matches"],
        json!(["/mnt/project-1/.config/app/config.toml"])
    );

    let globbed = GlobTool::new()
        .execute(
            json!({
                "pattern": "**/*",
                "path": "/mnt/project-1",
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(globbed["matches"], json!(["/mnt/project-1/notes.txt"]));

    // Explicit dot-prefixed glob activates hidden traversal, yet still excludes .ssh and .config/gh.
    let hidden_globbed = GlobTool::new()
        .execute(
            json!({
                "pattern": ".*/**/*",
                "path": "/mnt/project-1",
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        hidden_globbed["matches"],
        json!(["/mnt/project-1/.config/app/config.toml"])
    );

    // Non-sensitive files remain readable.
    let config_read = ReadFileTool::new()
        .execute(
            json!({"path": "/mnt/project-1/.config/app/config.toml"}),
            &ctx,
        )
        .await
        .unwrap();
    assert!(
        config_read["content"]
            .as_str()
            .unwrap()
            .contains("PUBLIC_CONFIG")
    );
}

#[tokio::test]
async fn host_mount_protected_subpath_writes_rejected_by_write_and_edit_tools() {
    let project = TempDir::new().unwrap();
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

    let policy = HostDirPolicy::for_host_mount(project.path(), &[], PROTECTED_SUBPATHS);
    let host_fs = Arc::new(
        HostDirFs::new(project.path(), HostDirAccess::ReadWrite)
            .unwrap()
            .with_policy(policy),
    );

    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/project-1",
        InProcessTransport::new(host_fs),
        Access::ReadWrite,
    );
    let view = Arc::new(
        MountFs::new(ns).with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES.iter().copied()),
    );
    let ctx = tool_ctx_with_view("/mnt/project-1", view);

    // Reading .git/HEAD still works.
    let head = ReadFileTool::new()
        .execute(json!({"path": "/mnt/project-1/.git/HEAD"}), &ctx)
        .await
        .unwrap();
    assert!(
        head["content"]
            .as_str()
            .unwrap()
            .contains("refs/heads/main")
    );

    // Overwriting or creating under .git, .alan, .agents fails for both write_file and edit_file.
    for path in [
        "/mnt/project-1/.git/config",
        "/mnt/project-1/.git/hooks/pre-commit",
        "/mnt/project-1/.git/hooks/post-checkout",
        "/mnt/project-1/.alan/settings.toml",
        "/mnt/project-1/.agents/skill.md",
        "/mnt/project-1/sub/.git/config",
    ] {
        assert!(
            WriteFileTool::new()
                .execute(json!({"path": path, "content": "tampered\n"}), &ctx)
                .await
                .is_err(),
            "write_file({path}) must be rejected"
        );
    }

    for (path, old) in [
        ("/mnt/project-1/.git/config", "[core]"),
        ("/mnt/project-1/.git/hooks/pre-commit", "#!/bin/sh"),
        ("/mnt/project-1/.alan/settings.toml", "mode = 1"),
        ("/mnt/project-1/.agents/skill.md", "# skill"),
    ] {
        assert!(
            EditFileTool::new()
                .execute(
                    json!({"path": path, "old_string": old, "new_string": "tampered"}),
                    &ctx
                )
                .await
                .is_err(),
            "edit_file({path}) must be rejected"
        );
    }

    assert_eq!(
        std::fs::read_to_string(project.path().join(".git/config")).unwrap(),
        "[core]\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join(".git/hooks/pre-commit")).unwrap(),
        "#!/bin/sh\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join(".alan/settings.toml")).unwrap(),
        "mode = 1\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join(".agents/skill.md")).unwrap(),
        "# skill\n"
    );

    // Regular project files remain writable and editable.
    EditFileTool::new()
        .execute(
            json!({
                "path": "/mnt/project-1/src/main.rs",
                "old_string": "fn main() {}",
                "new_string": "fn main() { println!(\"ok\"); }"
            }),
            &ctx,
        )
        .await
        .unwrap();
    WriteFileTool::new()
        .execute(
            json!({"path": "/mnt/project-1/src/lib.rs", "content": "pub fn hi() {}\n"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/main.rs")).unwrap(),
        "fn main() { println!(\"ok\"); }\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("src/lib.rs")).unwrap(),
        "pub fn hi() {}\n"
    );
}
