use std::path::PathBuf;
use std::sync::Arc;

use alan_agent_engine::Config;
use alan_agent_engine::tools::{
    TOOL_READ_ONLY_SYSTEM_PREFIXES, Tool, ToolContext, ToolExecutionBinding,
};
use alan_agent_protocol::ToolResultPresentation;
use alan_ap::reference::MemFs as RefMemFs;
use alan_ap::{Fid, FileKind, FileServer, InProcessTransport, OpenMode};
use alan_hostfs::{HostDirAccess, HostDirFs};
use alan_kernel::{
    Access, ExecNamespaceManifest, ExecSpec, LiveNamespace, MountFs, Namespace, ProcFs,
};
use alan_memfs::MemFs as KnowledgeMemFs;
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

async fn writable_tree_with_files(files: &[(&str, &[u8])]) -> InProcessTransport {
    let fs = Arc::new(RefMemFs::writable());
    for (idx, (name, bytes)) in files.iter().enumerate() {
        let fid = Fid(idx as u64 + 1);
        fs.create(Fid::ROOT, fid, name, FileKind::File)
            .await
            .unwrap();
        fs.open(fid, OpenMode::Write).await.unwrap();
        fs.write(fid, 0, bytes).await.unwrap();
        fs.clunk(fid).await.unwrap();
    }
    InProcessTransport::new(fs)
}

#[tokio::test]
async fn hostdirfs_project_mount_supports_full_file_and_exploration_lifecycle_via_ap() {
    let rw_dir = TempDir::new().unwrap();
    let ro_dir = TempDir::new().unwrap();
    let outside_dir = TempDir::new().unwrap();
    std::fs::write(ro_dir.path().join("manual.txt"), "approved read\n").unwrap();
    std::fs::write(outside_dir.path().join("secret.txt"), "private\n").unwrap();
    std::os::unix::fs::symlink(outside_dir.path(), rw_dir.path().join("escape")).unwrap();
    std::fs::write(rw_dir.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();

    let rw_fs = Arc::new(HostDirFs::new(rw_dir.path(), HostDirAccess::ReadWrite).unwrap());
    let ro_fs = Arc::new(HostDirFs::new(ro_dir.path(), HostDirAccess::ReadOnly).unwrap());

    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/project-1",
        InProcessTransport::new(rw_fs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/project-ro",
        InProcessTransport::new(ro_fs),
        Access::ReadOnly,
    );
    let view = Arc::new(MountFs::new(ns).with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES));
    let ctx = tool_ctx_with_view("/mnt/project-1", view);
    assert!(ctx.sandbox().is_err(), "no Host adapter needed for aP path");

    // write_file creates nested parent dirs + file using cwd-relative path.
    let wrote = WriteFileTool::new()
        .execute(
            json!({
                "path": "src/nested/app.rs",
                "content": "fn main() {\n    println!(\"hello\");\n}\n"
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(wrote["success"], true);
    assert_eq!(wrote["path"], "/mnt/project-1/src/nested/app.rs");

    // read_file reads with line numbers and clean namespace path.
    let read = ReadFileTool::new()
        .execute(
            json!({"path": "src/nested/app.rs", "offset": 2, "limit": 1}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(read["type"], "text");
    assert_eq!(read["path"], "/mnt/project-1/src/nested/app.rs");
    assert_eq!(read["content"], "2|     println!(\"hello\");");
    assert_eq!(read["truncated"], true);

    // edit_file updates content and produces a Diff presentation.
    let edited = EditFileTool::new()
        .execute(
            json!({
                "path": "src/nested/app.rs",
                "old_string": "hello",
                "new_string": "alan9"
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(edited["success"], true);
    assert_eq!(edited["path"], "/mnt/project-1/src/nested/app.rs");
    assert_eq!(edited["replacements"], 1);
    let pres: ToolResultPresentation =
        serde_json::from_value(edited["presentation"].clone()).unwrap();
    assert!(
        matches!(pres, ToolResultPresentation::Diff { path, hunks } if path == "/mnt/project-1/src/nested/app.rs" && hunks.len() == 1)
    );
    assert_eq!(
        std::fs::read_to_string(rw_dir.path().join("src/nested/app.rs")).unwrap(),
        "fn main() {\n    println!(\"alan9\");\n}\n"
    );

    // write_file truncates cleanly on shorter overwrite and empty overwrite.
    WriteFileTool::new()
        .execute(
            json!({"path": "src/nested/app.rs", "content": "short"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(rw_dir.path().join("src/nested/app.rs")).unwrap(),
        "short"
    );
    WriteFileTool::new()
        .execute(json!({"path": "src/nested/app.rs", "content": ""}), &ctx)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(rw_dir.path().join("src/nested/app.rs")).unwrap(),
        ""
    );

    // Restore searchable content for grep/glob checks.
    WriteFileTool::new()
        .execute(
            json!({"path": "src/nested/app.rs", "content": "line1\nneedle_token\nline3\n"}),
            &ctx,
        )
        .await
        .unwrap();

    // list_dir on cwd, /mnt, and / all work cleanly and skip symlinks.
    let cwd_list = ListDirTool::new().execute(json!({}), &ctx).await.unwrap();
    assert_eq!(cwd_list["path"], "/mnt/project-1");
    let names: Vec<&str> = cwd_list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"src"));
    assert!(!names.contains(&"escape"), "symlinks must not be exposed");

    let mnt_list = ListDirTool::new()
        .execute(json!({"path": "/mnt"}), &ctx)
        .await
        .unwrap();
    let mnt_names: Vec<&str> = mnt_list["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(mnt_names, vec!["project-1", "project-ro"]);

    // grep and glob traverse via aP when no Host adapter is bound, skipping binary files.
    let grep_res = GrepTool::new()
        .execute(
            json!({
                "pattern": "needle_token",
                "path": ".",
                "file_type": "rust",
                "before_context": 1,
                "after_context": 1
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(grep_res["total"], 1);
    assert_eq!(
        grep_res["matches"][0]["path"],
        "/mnt/project-1/src/nested/app.rs"
    );
    assert_eq!(grep_res["matches"][0]["before"], json!(["line1"]));
    assert_eq!(grep_res["matches"][0]["after"], json!(["line3"]));

    let glob_res = GlobTool::new()
        .execute(json!({"pattern": "**/*.rs", "path": "."}), &ctx)
        .await
        .unwrap();
    assert_eq!(
        glob_res["matches"],
        json!(["/mnt/project-1/src/nested/app.rs"])
    );

    // Symlink escape is rejected by HostDirFs on both read and write.
    assert!(
        ReadFileTool::new()
            .execute(json!({"path": "/mnt/project-1/escape/secret.txt"}), &ctx)
            .await
            .is_err()
    );
    assert!(
        WriteFileTool::new()
            .execute(
                json!({"path": "/mnt/project-1/escape/secret.txt", "content": "x"}),
                &ctx
            )
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(outside_dir.path().join("secret.txt")).unwrap(),
        "private\n"
    );

    // Read-only mount allows reads/list/grep/glob and refuses write_file & edit_file.
    let ro_read = ReadFileTool::new()
        .execute(json!({"path": "/mnt/project-ro/manual.txt"}), &ctx)
        .await
        .unwrap();
    assert_eq!(ro_read["content"], "1| approved read");
    assert!(
        WriteFileTool::new()
            .execute(
                json!({"path": "/mnt/project-ro/manual.txt", "content": "nope"}),
                &ctx
            )
            .await
            .is_err()
    );
    assert!(
        EditFileTool::new()
            .execute(
                json!({
                    "path": "/mnt/project-ro/manual.txt",
                    "old_string": "approved",
                    "new_string": "tampered"
                }),
                &ctx
            )
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(ro_dir.path().join("manual.txt")).unwrap(),
        "approved read\n"
    );
}

#[tokio::test]
async fn memory_and_embedded_app_mounts_work_uniformly_without_host_adapter() {
    let mem_knowledge = Arc::new(KnowledgeMemFs::new());
    let mem_dir = TempDir::new().unwrap();
    let mem_hostfs = Arc::new(HostDirFs::new(mem_dir.path(), HostDirAccess::ReadWrite).unwrap());
    let slowpage_fs = Arc::new(RefMemFs::writable());

    let mut ns = Namespace::new();
    ns.mount(
        "/memory",
        InProcessTransport::new(mem_knowledge),
        Access::ReadWrite,
    );
    ns.mount(
        "/memory-dir",
        InProcessTransport::new(mem_hostfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/slowpage",
        InProcessTransport::new(slowpage_fs),
        Access::ReadWrite,
    );

    let view = Arc::new(MountFs::new(ns).with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES));
    let ctx = tool_ctx_with_view("/", view);

    // /memory backed by alan-memfs (content-addressed flat store): create, read, edit, grep, glob, list_dir.
    WriteFileTool::new()
        .execute(
            json!({"path": "/memory/USER.md", "content": "Preference: Rust\nMode: KISS\n"}),
            &ctx,
        )
        .await
        .unwrap();
    EditFileTool::new()
        .execute(
            json!({
                "path": "/memory/USER.md",
                "old_string": "Mode: KISS",
                "new_string": "Mode: KISS + DRY"
            }),
            &ctx,
        )
        .await
        .unwrap();
    let mem_read = ReadFileTool::new()
        .execute(json!({"path": "/memory/USER.md"}), &ctx)
        .await
        .unwrap();
    assert_eq!(
        mem_read["content"],
        "1| Preference: Rust\n2| Mode: KISS + DRY"
    );

    let mem_grep = GrepTool::new()
        .execute(json!({"pattern": "KISS \\+ DRY", "path": "/memory"}), &ctx)
        .await
        .unwrap();
    assert_eq!(mem_grep["total"], 1);
    assert_eq!(mem_grep["matches"][0]["path"], "/memory/USER.md");

    let mem_glob = GlobTool::new()
        .execute(json!({"pattern": "*.md", "path": "/memory"}), &ctx)
        .await
        .unwrap();
    assert_eq!(mem_glob["matches"], json!(["/memory/USER.md"]));

    // HostDirFs-backed memory store supports nested directories cleanly.
    WriteFileTool::new()
        .execute(
            json!({"path": "/memory-dir/topics/arch.md", "content": "alan9 namespace"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        ReadFileTool::new()
            .execute(json!({"path": "/memory-dir/topics/arch.md"}), &ctx)
            .await
            .unwrap()["content"],
        "1| alan9 namespace"
    );

    // Custom EmbeddedAlan writable mount `/mnt/slowpage` works with zero hardcoded allowlist!
    WriteFileTool::new()
        .execute(
            json!({"path": "/mnt/slowpage/pages/home.md", "content": "# Welcome\nDraft v1\n"}),
            &ctx,
        )
        .await
        .unwrap();
    EditFileTool::new()
        .execute(
            json!({
                "path": "/mnt/slowpage/pages/home.md",
                "old_string": "Draft v1",
                "new_string": "Published v2"
            }),
            &ctx,
        )
        .await
        .unwrap();
    let app_read = ReadFileTool::new()
        .execute(json!({"path": "/mnt/slowpage/pages/home.md"}), &ctx)
        .await
        .unwrap();
    assert_eq!(app_read["path"], "/mnt/slowpage/pages/home.md");
    assert_eq!(app_read["content"], "1| # Welcome\n2| Published v2");

    let app_list = ListDirTool::new()
        .execute(json!({"path": "/mnt/slowpage/pages"}), &ctx)
        .await
        .unwrap();
    assert_eq!(app_list["entries"][0]["name"], "home.md");
    assert_eq!(app_list["entries"][0]["type"], "file");

    let app_grep = GrepTool::new()
        .execute(
            json!({"pattern": "Published v2", "path": "/mnt/slowpage"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(app_grep["total"], 1);
    assert_eq!(
        app_grep["matches"][0]["path"],
        "/mnt/slowpage/pages/home.md"
    );

    let app_glob = GlobTool::new()
        .execute(json!({"pattern": "**/*.md", "path": "/mnt/slowpage"}), &ctx)
        .await
        .unwrap();
    assert_eq!(app_glob["matches"], json!(["/mnt/slowpage/pages/home.md"]));

    // Oversized overwrite (>1 MiB) must fail without truncating or wiping existing contents.
    let oversized = "x".repeat((1 << 20) + 128);
    for (path, expected) in [
        (
            "/memory/USER.md",
            "1| Preference: Rust\n2| Mode: KISS + DRY",
        ),
        (
            "/mnt/slowpage/pages/home.md",
            "1| # Welcome\n2| Published v2",
        ),
    ] {
        assert!(
            WriteFileTool::new()
                .execute(json!({"path": path, "content": oversized}), &ctx)
                .await
                .is_err(),
            "oversized write to {path} must fail"
        );
        let preserved = ReadFileTool::new()
            .execute(json!({"path": path}), &ctx)
            .await
            .unwrap();
        assert_eq!(preserved["content"], expected, "{path} must stay intact");
    }
}

async fn spawn_proc_entry(procfs: &ProcFs) -> u64 {
    let fid = Fid(1);
    procfs
        .walk(Fid::ROOT, fid, &["clone".to_string()])
        .await
        .unwrap();
    procfs.open(fid, OpenMode::ReadWrite).await.unwrap();
    let pid_bytes = procfs.read(fid, 0, 64).await.unwrap();
    let pid: u64 = String::from_utf8(pid_bytes)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let exec = ExecSpec {
        executable: "/bin/alan-agent".to_string(),
        args: Vec::new(),
        namespace: ExecNamespaceManifest::default(),
        descriptors: Default::default(),
    };
    procfs
        .write(fid, 0, &serde_json::to_vec(&exec).unwrap())
        .await
        .unwrap();
    procfs.clunk(fid).await.unwrap();
    pid
}

#[tokio::test]
async fn system_and_control_subtrees_are_read_only_and_unmounted_host_paths_fail() {
    let procfs = ProcFs::new();
    let proc_pid = spawn_proc_entry(&procfs).await;
    let proc_dir = format!("/proc/{proc_pid}");

    let mut ns = Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs)),
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/8/machine",
        writable_tree_with_files(&[("ctl", b"help\n"), ("status", b"idle\n")]).await,
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/8/actions/a2",
        InProcessTransport::new(Arc::new(RefMemFs::with_read_only_file(
            "output",
            b"retained output".to_vec(),
        ))),
        Access::ReadOnly,
    );
    ns.mount(
        "/agent/9/actions/a2",
        InProcessTransport::new(Arc::new(RefMemFs::with_read_only_file(
            "output",
            b"other agent secret".to_vec(),
        ))),
        Access::ReadOnly,
    );
    ns.mount(
        "/mnt/connections",
        writable_tree_with_files(&[("ctl", b"default\n")]).await,
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/host-mount",
        writable_tree_with_files(&[("ctl", b"grants\n")]).await,
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/package",
        writable_tree_with_files(&[("ctl", b"catalog\n")]).await,
        Access::ReadWrite,
    );
    ns.mount(
        "/man",
        InProcessTransport::new(Arc::new(RefMemFs::with_read_only_file(
            "alan.md",
            b"# Alan Manual\nEverything is a file.\n".to_vec(),
        ))),
        Access::ReadOnly,
    );
    ns.mount(
        "/memory",
        InProcessTransport::new(Arc::new(RefMemFs::writable())),
        Access::ReadWrite,
    );

    let view = Arc::new(MountFs::new(ns).with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES));
    let ctx = tool_ctx_with_view("/", view);

    // Listing works across /, /mnt, /proc, /proc/<pid>, /agent, /man.
    for dir in ["/", "/mnt", "/proc", &proc_dir, "/agent", "/man"] {
        let listed = ListDirTool::new()
            .execute(json!({"path": dir}), &ctx)
            .await
            .unwrap_or_else(|err| panic!("list_dir({dir}) failed: {err}"));
        assert_eq!(listed["path"], dir);
        assert!(
            listed["total"].as_u64().unwrap() > 0,
            "{dir} should not be empty"
        );
    }

    // Reading, grepping, and globbing work on /proc/<pid>/status and /man/alan.md.
    let proc_status = ReadFileTool::new()
        .execute(json!({"path": format!("{proc_dir}/status")}), &ctx)
        .await
        .unwrap();
    assert_eq!(proc_status["content"], "1| running");

    let man_grep = GrepTool::new()
        .execute(
            json!({"pattern": "Everything is a file", "path": "/man"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(man_grep["total"], 1);
    assert_eq!(man_grep["matches"][0]["path"], "/man/alan.md");

    let man_glob = GlobTool::new()
        .execute(json!({"pattern": "*.md", "path": "/man"}), &ctx)
        .await
        .unwrap();
    assert_eq!(man_glob["matches"], json!(["/man/alan.md"]));

    // Retained evidence contract remains intact and cannot be bypassed via traversal/non-canonical paths.
    let own_evidence = ReadFileTool::new()
        .execute(json!({"path": "/agent/8/actions/a2/output"}), &ctx)
        .await
        .unwrap();
    assert_eq!(own_evidence["content"], "retained output");
    for forbidden_read in [
        "/agent/9/actions/a2/output",
        "//agent/9/actions/a2/output",
        "/./agent/9/actions/a2/output",
        "/memory/../agent/9/actions/a2/output",
        "/etc/passwd",
        "/../etc/passwd",
        "/memory/../../etc/passwd",
    ] {
        assert!(
            ReadFileTool::new()
                .execute(json!({"path": forbidden_read}), &ctx)
                .await
                .is_err(),
            "read_file must reject '{forbidden_read}'"
        );
    }
    // grep must not bypass /agent/ retained-evidence restrictions directly or from '/'.
    for forbidden_grep in [
        "/agent",
        "/agent/8",
        "/agent/8/actions/a2/output",
        "/agent/9/actions/a2/output",
    ] {
        assert!(
            GrepTool::new()
                .execute(json!({"pattern": ".*", "path": forbidden_grep}), &ctx)
                .await
                .is_err(),
            "grep must reject '{forbidden_grep}'"
        );
    }
    let root_grep = GrepTool::new()
        .execute(
            json!({"pattern": "other agent secret|retained output", "path": "/"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        root_grep["total"], 0,
        "grep('/') must skip /agent: {root_grep}"
    );

    // Writes and edits to clamped system/control mounts and unmounted host paths are rejected.
    let proc_ctl = format!("{proc_dir}/ctl");
    let proc_status_path = format!("{proc_dir}/status");
    for forbidden_write in [
        proc_ctl.as_str(),
        proc_status_path.as_str(),
        "/proc/clone",
        "/agent/8/machine/ctl",
        "/agent/8/machine/status",
        "/mnt/connections/ctl",
        "/mnt/connections/new-file",
        "/mnt/host-mount/ctl",
        "/mnt/package/ctl",
        "/man/alan.md",
        "/etc/passwd",
        "/../etc/passwd",
        "/memory/../../etc/passwd",
    ] {
        assert!(
            WriteFileTool::new()
                .execute(json!({"path": forbidden_write, "content": "cancel"}), &ctx)
                .await
                .is_err(),
            "write_file must reject '{forbidden_write}'"
        );
        assert!(
            EditFileTool::new()
                .execute(
                    json!({
                        "path": forbidden_write,
                        "old_string": "a",
                        "new_string": "b"
                    }),
                    &ctx
                )
                .await
                .is_err(),
            "edit_file must reject '{forbidden_write}'"
        );
    }

    // Unmounted host paths fail cleanly on list_dir, grep, and glob too.
    assert!(
        ListDirTool::new()
            .execute(json!({"path": "/etc"}), &ctx)
            .await
            .is_err()
    );
    assert!(
        GrepTool::new()
            .execute(json!({"pattern": "root", "path": "/etc/passwd"}), &ctx)
            .await
            .is_err()
    );
    assert!(
        GlobTool::new()
            .execute(json!({"pattern": "*", "path": "/etc"}), &ctx)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn live_namespace_view_reflects_dynamic_mount_and_revoke_immediately() {
    let live_ns = LiveNamespace::new(Namespace::new());
    let view = Arc::new(
        MountFs::from_live_namespace(live_ns.clone())
            .with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES),
    );
    let ctx = tool_ctx_with_view("/", view);

    // Before mount, /mnt/project-live does not exist.
    assert!(
        ReadFileTool::new()
            .execute(json!({"path": "/mnt/project-live/doc.txt"}), &ctx)
            .await
            .is_err()
    );

    // Dynamically mount a HostDirFs project at runtime.
    let project_dir = TempDir::new().unwrap();
    let host_fs = Arc::new(HostDirFs::new(project_dir.path(), HostDirAccess::ReadWrite).unwrap());
    live_ns.mount(
        "/mnt/project-live",
        InProcessTransport::new(host_fs.clone()),
        Access::ReadWrite,
    );

    // Immediately visible to list_dir, write_file, edit_file, read_file, grep, and glob on the same context.
    let mnt_entries = ListDirTool::new()
        .execute(json!({"path": "/mnt"}), &ctx)
        .await
        .unwrap();
    assert_eq!(mnt_entries["entries"][0]["name"], "project-live");

    WriteFileTool::new()
        .execute(
            json!({"path": "/mnt/project-live/doc.txt", "content": "dynamic mount"}),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        ReadFileTool::new()
            .execute(json!({"path": "/mnt/project-live/doc.txt"}), &ctx)
            .await
            .unwrap()["content"],
        "1| dynamic mount"
    );

    // Dynamically revoke export + unmount from live namespace.
    host_fs.revoke();
    live_ns.unmount("/mnt/project-live");

    // Immediately rejected on subsequent tool calls.
    assert!(
        ReadFileTool::new()
            .execute(json!({"path": "/mnt/project-live/doc.txt"}), &ctx)
            .await
            .is_err()
    );
    assert!(
        WriteFileTool::new()
            .execute(
                json!({"path": "/mnt/project-live/doc.txt", "content": "after revoke"}),
                &ctx
            )
            .await
            .is_err()
    );
    assert!(
        GrepTool::new()
            .execute(
                json!({"pattern": "dynamic", "path": "/mnt/project-live"}),
                &ctx
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn namespace_grep_and_glob_respect_gitignore_rules() {
    let app_fs = Arc::new(RefMemFs::writable());
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/slowpage",
        InProcessTransport::new(app_fs),
        Access::ReadWrite,
    );
    let view = Arc::new(
        MountFs::new(ns).with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES.iter().copied()),
    );
    let ctx = tool_ctx_with_view("/mnt/slowpage", view);

    let write = WriteFileTool::new();
    for (path, content) in [
        ("/mnt/slowpage/.gitignore", "generated/\n*.log\n"),
        ("/mnt/slowpage/.git/info/exclude", "excluded.md\n"),
        ("/mnt/slowpage/pages/home.md", "needle home\n"),
        ("/mnt/slowpage/generated/out.md", "needle generated\n"),
        ("/mnt/slowpage/debug.log", "needle root log\n"),
        ("/mnt/slowpage/excluded.md", "needle git exclude\n"),
        ("/mnt/slowpage/sub/.gitignore", "local_only.md\n"),
        ("/mnt/slowpage/sub/local_only.md", "needle sub ignored\n"),
        ("/mnt/slowpage/sub/trace.log", "needle sub log\n"),
        ("/mnt/slowpage/sub/keep.md", "needle sub keep\n"),
        ("/mnt/slowpage/other/local_only.md", "needle other keep\n"),
    ] {
        write
            .execute(json!({"path": path, "content": content}), &ctx)
            .await
            .unwrap_or_else(|err| panic!("write_file({path}) failed: {err}"));
    }

    // Default respect_gitignore=true applies root .gitignore, .git/info/exclude, and nested .gitignore.
    let grepped = GrepTool::new()
        .execute(
            json!({"pattern": "needle", "path": "/mnt/slowpage", "files_only": true}),
            &ctx,
        )
        .await
        .unwrap();
    let mut grep_paths: Vec<String> = grepped["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    grep_paths.sort();
    assert_eq!(
        grep_paths,
        vec![
            "/mnt/slowpage/other/local_only.md",
            "/mnt/slowpage/pages/home.md",
            "/mnt/slowpage/sub/keep.md",
        ]
    );

    // Bump Qid.version on sub/keep.md via edits so its version is strictly higher than
    // earlier paths, confirming namespace glob sorts deterministically by path rather than Qid.version.
    for revision in 1..=3 {
        write
            .execute(
                json!({
                    "path": "/mnt/slowpage/sub/keep.md",
                    "content": format!("needle sub keep rev {revision}\n")
                }),
                &ctx,
            )
            .await
            .unwrap();
    }

    let globbed = GlobTool::new()
        .execute(json!({"pattern": "**/*.md", "path": "/mnt/slowpage"}), &ctx)
        .await
        .unwrap();
    let glob_paths: Vec<&str> = globbed["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        glob_paths,
        vec![
            "/mnt/slowpage/other/local_only.md",
            "/mnt/slowpage/pages/home.md",
            "/mnt/slowpage/sub/keep.md",
        ]
    );

    let all_globbed = GlobTool::new()
        .execute(json!({"pattern": "**/*", "path": "/mnt/slowpage"}), &ctx)
        .await
        .unwrap();
    let all_glob_paths: Vec<&str> = all_globbed["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(!all_glob_paths.contains(&"/mnt/slowpage/generated/out.md"));
    assert!(!all_glob_paths.contains(&"/mnt/slowpage/debug.log"));
    assert!(!all_glob_paths.contains(&"/mnt/slowpage/excluded.md"));
    assert!(!all_glob_paths.contains(&"/mnt/slowpage/sub/local_only.md"));
    assert!(!all_glob_paths.contains(&"/mnt/slowpage/sub/trace.log"));

    // Starting inside a subdirectory still inherits ancestor .gitignore rules.
    let sub_grepped = GrepTool::new()
        .execute(
            json!({"pattern": "needle", "path": "/mnt/slowpage/sub", "files_only": true}),
            &ctx,
        )
        .await
        .unwrap();
    let sub_paths: Vec<&str> = sub_grepped["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(sub_paths, vec!["/mnt/slowpage/sub/keep.md"]);

    // respect_gitignore=false includes ignored directories and files.
    let unfiltered_grep = GrepTool::new()
        .execute(
            json!({
                "pattern": "needle",
                "path": "/mnt/slowpage",
                "files_only": true,
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    let mut unfiltered_paths: Vec<String> = unfiltered_grep["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    unfiltered_paths.sort();
    assert_eq!(
        unfiltered_paths,
        vec![
            "/mnt/slowpage/debug.log",
            "/mnt/slowpage/excluded.md",
            "/mnt/slowpage/generated/out.md",
            "/mnt/slowpage/other/local_only.md",
            "/mnt/slowpage/pages/home.md",
            "/mnt/slowpage/sub/keep.md",
            "/mnt/slowpage/sub/local_only.md",
            "/mnt/slowpage/sub/trace.log",
        ]
    );

    let unfiltered_glob = GlobTool::new()
        .execute(
            json!({
                "pattern": "**/*",
                "path": "/mnt/slowpage",
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    let unfiltered_glob_paths: Vec<&str> = unfiltered_glob["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(unfiltered_glob_paths.contains(&"/mnt/slowpage/generated/out.md"));
    assert!(unfiltered_glob_paths.contains(&"/mnt/slowpage/debug.log"));
    assert!(unfiltered_glob_paths.contains(&"/mnt/slowpage/excluded.md"));
    assert!(unfiltered_glob_paths.contains(&"/mnt/slowpage/sub/local_only.md"));
    assert!(unfiltered_glob_paths.contains(&"/mnt/slowpage/sub/trace.log"));
}
