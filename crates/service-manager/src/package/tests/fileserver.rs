use super::*;
use alan_agent_engine::tools::ToolCapability;

fn fs_snapshot_with_manifest(
    source_name: &str,
    manifest: &str,
    extra: Vec<(&str, &[u8], bool)>,
) -> PackageSnapshot {
    let mut entries = vec![PackageSnapshotEntry {
        path: "alan-package.toml".to_string(),
        bytes: manifest.as_bytes().to_vec(),
        executable: false,
    }];
    for (path, bytes, executable) in extra {
        entries.push(PackageSnapshotEntry {
            path: path.to_string(),
            bytes: bytes.to_vec(),
            executable,
        });
    }
    PackageSnapshot {
        source_name: source_name.to_string(),
        entries,
    }
}

#[test]
fn installs_and_syncs_all_five_package_kinds_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let store_root = temp.path().join("packages");
    let service = PackageService::open(store_root.clone()).unwrap();

    let skill_snap = native_snapshot("helper-skill", "Helper instructions.");

    let tool_snap = fs_snapshot_with_manifest(
        "jq-tool",
        r#"
schema_version = 1
kind = "tool"
version = "0.1.0"
[tool]
name = "jq_filter"
description = "Filter JSON input"
entrypoint = "bin/filter.sh"
parameters = { type = "object" }
"#,
        vec![("bin/filter.sh", b"#!/bin/sh\necho '{}'\n", false)],
    );

    let mcp_snap = fs_snapshot_with_manifest(
        "mcp-pack",
        r#"
schema_version = 1
kind = "mcp"
version = "0.1.0"
[mcp]
id = "local-echo"
[mcp.transport]
type = "stdio"
command = "srv/echo.sh"
"#,
        vec![("srv/echo.sh", b"#!/bin/sh\nexit 0\n", true)],
    );

    let provider_snap = fs_snapshot_with_manifest(
        "custom-provider",
        r#"
schema_version = 1
kind = "provider"
version = "0.1.0"
[provider]
id = "custom_anthropic"
protocol = "anthropic_messages"
display_name = "Custom Anthropic"
default_base_url = "https://api.example.com"
"#,
        vec![],
    );

    let fileserver_snap = fs_snapshot_with_manifest(
        "github-fileserver",
        r#"
schema_version = 1
kind = "fileserver"
version = "0.1.0"
description = "GitHub file server"

[fileserver]
id = "github"
description = "Project GitHub resources"
entrypoint = "bin/githubfs"
allow_network = true
network_domains = ["api.github.com", "github.com"]
timeout_secs = 25
max_message_bytes = 262144

[fileserver.transport]
type = "stdio"
command = "bin/githubfs"
args = ["--stdio"]
env = { GITHUB_API_VERSION = "2022-11-28" }
"#,
        vec![("bin/githubfs", b"#!/bin/sh\nexit 0\n", false)],
    );

    let sync_result = service
        .execute(PackageCommand::Sync {
            request_id: "sync-five-kinds".to_string(),
            packages: vec![
                PackageSyncEntry {
                    package_id: "helper-skill".to_string(),
                    snapshot: skill_snap,
                },
                PackageSyncEntry {
                    package_id: "jq-tool".to_string(),
                    snapshot: tool_snap,
                },
                PackageSyncEntry {
                    package_id: "mcp-pack".to_string(),
                    snapshot: mcp_snap,
                },
                PackageSyncEntry {
                    package_id: "custom-provider".to_string(),
                    snapshot: provider_snap,
                },
                PackageSyncEntry {
                    package_id: "github-fileserver".to_string(),
                    snapshot: fileserver_snap,
                },
            ],
            prune: false,
        })
        .unwrap();
    assert!(sync_result.success, "{}", sync_result.message);
    let catalog = sync_result.catalog.unwrap();
    assert_eq!(catalog.packages.len(), 5);

    let fs_rec = service.resolve("github-fileserver").unwrap();
    assert_eq!(fs_rec.export_kind, PackageExportKind::Fileserver);
    let export = fs_rec.fileserver_export.clone().unwrap();
    assert_eq!(export.service_id, "github");
    assert_eq!(export.entrypoint, "fileservers/github/bin");
    assert_eq!(export.descriptor_path, "fileservers/github/server.json");
    assert!(export.allow_network);
    assert_eq!(
        export.network_domains,
        vec!["api.github.com".to_string(), "github.com".to_string()]
    );
    assert_eq!(export.timeout_secs, 25);
    assert_eq!(export.max_message_bytes, 262_144);
    assert_eq!(
        export.transport,
        FileserverTransportSpec::Stdio {
            command: "fileservers/github/bin".to_string(),
            args: vec!["--stdio".to_string()],
            env: BTreeMap::from([("GITHUB_API_VERSION".to_string(), "2022-11-28".to_string())]),
        }
    );

    let lease = service.acquire("github-fileserver").unwrap();
    let bin = lease.content_file_path("fileservers/github/bin").unwrap();
    assert!(bin.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&bin).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o555);
    }
    let descriptor = lease
        .content_file_path("fileservers/github/server.json")
        .unwrap();
    let decoded: FileserverPackageExport =
        serde_json::from_slice(&fs::read(descriptor).unwrap()).unwrap();
    assert_eq!(decoded, export);
    drop(lease);

    // Offline migration inspection accepts five-kind stores and catches tampering.
    let revision = fs_rec.revision;
    drop(service);
    assert!(validate_package_store_for_migration(&store_root).is_ok());

    let tampered = store_root
        .join("revisions/github-fileserver")
        .join(&revision)
        .join("content/fileservers/github/server.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            tampered.parent().unwrap(),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fs::set_permissions(&tampered, fs::Permissions::from_mode(0o644)).unwrap();
    }
    fs::write(&tampered, b"{}").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tampered, fs::Permissions::from_mode(0o444)).unwrap();
        fs::set_permissions(
            tampered.parent().unwrap(),
            fs::Permissions::from_mode(0o555),
        )
        .unwrap();
    }
    assert!(validate_package_store_for_migration(&store_root).is_err());
}

#[test]
fn fileserver_manifest_validation_fails_closed_on_invalid_inputs() {
    let service = PackageService::ephemeral().unwrap();

    let invalid_cases = [
        (
            "unknown-kind",
            fs_snapshot_with_manifest(
                "unknown",
                r#"
kind = "daemon"
"#,
                vec![],
            ),
        ),
        (
            "mixed-skill",
            PackageSnapshot {
                source_name: "mixed-skill".to_string(),
                entries: vec![
                    PackageSnapshotEntry {
                        path: "alan-package.toml".to_string(),
                        bytes: br#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
[fileserver.transport]
type = "stdio"
"#
                        .to_vec(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "bin/fs".to_string(),
                        bytes: b"#!/bin/sh\n".to_vec(),
                        executable: true,
                    },
                    PackageSnapshotEntry {
                        path: "SKILL.md".to_string(),
                        bytes: b"---\nname: mixed\ndescription: nope\n---\n".to_vec(),
                        executable: false,
                    },
                ],
            },
        ),
        (
            "mixed-tool",
            fs_snapshot_with_manifest(
                "mixed-tool",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
[fileserver.transport]
type = "stdio"
[tool]
name = "extra"
description = "d"
entrypoint = "bin/fs"
parameters = { type = "object" }
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "bad-service-id",
            fs_snapshot_with_manifest(
                "bad-id",
                r#"
kind = "fileserver"
[fileserver]
id = "BadCaps"
entrypoint = "bin/fs"
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "reserved-service-id",
            fs_snapshot_with_manifest(
                "reserved",
                r#"
kind = "fileserver"
[fileserver]
id = "package"
entrypoint = "bin/fs"
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "missing-entrypoint",
            fs_snapshot_with_manifest(
                "missing-bin",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/missing"
[fileserver.transport]
type = "stdio"
"#,
                vec![],
            ),
        ),
        (
            "traversing-entrypoint",
            fs_snapshot_with_manifest(
                "traverse-bin",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "../outside"
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "mismatched-command",
            fs_snapshot_with_manifest(
                "mismatch",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
[fileserver.transport]
type = "stdio"
command = "bin/other"
"#,
                vec![
                    ("bin/fs", b"#!/bin/sh\n", true),
                    ("bin/other", b"#!/bin/sh\n", true),
                ],
            ),
        ),
        (
            "unix-socket-missing-entrypoint",
            fs_snapshot_with_manifest(
                "sock-no-ep",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
[fileserver.transport]
type = "unix_socket"
socket_name = "demo.sock"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "unix-socket-traversal",
            fs_snapshot_with_manifest(
                "sock-bad-name",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
[fileserver.transport]
type = "unix_socket"
socket_name = "../escape.sock"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "domains-without-allow-network",
            fs_snapshot_with_manifest(
                "net-mismatch",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
allow_network = false
network_domains = ["api.github.com"]
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "invalid-domain-url",
            fs_snapshot_with_manifest(
                "bad-domain",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
allow_network = true
network_domains = ["https://api.github.com"]
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "secret-env-leak",
            fs_snapshot_with_manifest(
                "env-leak",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
[fileserver.transport]
type = "stdio"
env = { GITHUB_TOKEN = "ghp_secret" }
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "timeout-out-of-bounds",
            fs_snapshot_with_manifest(
                "bad-timeout",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
timeout_secs = 999
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "message-bound-too-small",
            fs_snapshot_with_manifest(
                "small-msg",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
max_message_bytes = 512
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
        (
            "message-bound-too-large",
            fs_snapshot_with_manifest(
                "large-msg",
                r#"
kind = "fileserver"
[fileserver]
id = "demo"
entrypoint = "bin/fs"
max_message_bytes = 2097152
[fileserver.transport]
type = "stdio"
"#,
                vec![("bin/fs", b"#!/bin/sh\n", true)],
            ),
        ),
    ];

    for (label, snapshot) in invalid_cases {
        let res = service
            .execute(PackageCommand::Install {
                request_id: format!("reject-{label}"),
                package_id: format!("pack-{label}"),
                snapshot,
            })
            .unwrap();
        assert!(!res.success, "expected `{label}` to fail closed");
    }

    // Confirm failed installs left the catalog untouched and ToolCapability import remains used cleanly.
    assert!(service.catalog().unwrap().packages.is_empty());
    assert_eq!(ToolCapability::Unknown, ToolCapability::Unknown);
}

#[test]
fn in_memory_fileserver_package_round_trip() {
    let service = PackageService::in_memory();
    let snap = fs_snapshot_with_manifest(
        "mem-fs",
        r#"
kind = "fileserver"
[fileserver]
id = "mem_demo"
entrypoint = "bin/fs"
[fileserver.transport]
type = "unix_socket"
socket_name = "mem.sock"
"#,
        vec![("bin/fs", b"#!/bin/sh\nexit 0\n", true)],
    );
    let installed = service
        .execute(PackageCommand::Install {
            request_id: "mem-install".to_string(),
            package_id: "mem-fs".to_string(),
            snapshot: snap,
        })
        .unwrap();
    assert!(installed.success, "{}", installed.message);
    let record = service.resolve("mem-fs").unwrap();
    assert_eq!(record.export_kind, PackageExportKind::Fileserver);
    assert_eq!(
        record.fileserver_export.as_ref().unwrap().transport,
        FileserverTransportSpec::UnixSocket {
            socket_name: "mem.sock".to_string()
        }
    );
    let removed = service
        .execute(PackageCommand::Uninstall {
            request_id: "mem-uninstall".to_string(),
            package_id: "mem-fs".to_string(),
        })
        .unwrap();
    assert!(removed.success, "{}", removed.message);
}

#[test]
fn bundled_self_iteration_packages_validate_and_install_cleanly() {
    let packages_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages");
    let service = PackageService::ephemeral().unwrap();

    for (pkg_id, expected_kind) in [
        ("github-fileserver", PackageExportKind::Fileserver),
        ("alan-self-iterate", PackageExportKind::Skill),
        ("alan-release-prep", PackageExportKind::Skill),
    ] {
        let pkg_dir = packages_root.join(pkg_id);
        assert!(pkg_dir.is_dir(), "missing {}", pkg_dir.display());
        let snapshot = PackageSnapshot::from_directory(&pkg_dir).unwrap();
        let res = service
            .execute(PackageCommand::Install {
                request_id: format!("bundled-{pkg_id}"),
                package_id: pkg_id.to_string(),
                snapshot,
            })
            .unwrap();
        assert!(res.success, "{pkg_id}: {}", res.message);
        let record = res.package.unwrap();
        assert_eq!(record.export_kind, expected_kind);
        if expected_kind == PackageExportKind::Fileserver {
            let export = record.fileserver_export.as_ref().unwrap();
            assert_eq!(export.service_id, "github");
            assert_eq!(export.entrypoint, "fileservers/github/bin");
            assert!(export.allow_network);
            assert_eq!(
                export.network_domains,
                vec!["api.github.com".to_string(), "github.com".to_string()]
            );
            let lease = service.acquire(pkg_id).unwrap();
            let bin = lease.content_file_path("fileservers/github/bin").unwrap();
            assert!(bin.is_file());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&bin).unwrap().permissions().mode() & 0o777,
                    0o555
                );
            }
        } else {
            assert_eq!(record.exports.len(), 1);
            assert_eq!(record.exports[0].skill_id, pkg_id);
        }
    }

    // `github-fileserver` bundles read-only Companion Skills beneath `skills/` while
    // ignoring `README.md`.
    let gh_record = service.resolve("github-fileserver").unwrap();
    let companion_ids: Vec<&str> = gh_record
        .exports
        .iter()
        .map(|export| export.skill_id.as_str())
        .collect();
    assert_eq!(
        companion_ids,
        vec!["github-setup", "release-prep", "self-iterate"]
    );
}

#[test]
fn companion_skills_and_root_bindings_enforce_isolation_collisions_and_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let store_root = temp.path().join("packages");
    let service = PackageService::open(store_root.clone()).unwrap();
    crate::runtime::seed_preinstalled_packages(&service).unwrap();

    // 1. Tool package with Companion Skills + top-level README.md (ignored).
    let tool_snap = fs_snapshot_with_manifest(
        "probe-with-skills",
        r#"
schema_version = 1
kind = "tool"
version = "0.1.0"
[tool]
name = "probe_companion"
description = "Probe"
entrypoint = "bin/probe.sh"
parameters = { type = "object" }
"#,
        vec![
            ("bin/probe.sh", b"#!/bin/sh\necho '{}'\n", true),
            ("README.md", b"# Top-level readme stays non-skill\n", false),
            (
                "skills/probe-guide/SKILL.md",
                b"---\nname: probe-guide\ndescription: How to use probe.\n---\nUse carefully.\n",
                false,
            ),
        ],
    );
    let res = service
        .execute(PackageCommand::Install {
            request_id: "install-tool".to_string(),
            package_id: "probe-with-skills".to_string(),
            snapshot: tool_snap,
        })
        .unwrap();
    assert!(res.success, "{}", res.message);
    let rec = res.package.unwrap();
    assert_eq!(rec.export_kind, PackageExportKind::Tool);
    assert_eq!(rec.exports.len(), 1);
    assert_eq!(rec.exports[0].skill_id, "probe-guide");

    // 2. Malformed Companion Skill fails installation atomically.
    let bad_tool = fs_snapshot_with_manifest(
        "bad-tool",
        r#"
schema_version = 1
kind = "tool"
[tool]
name = "bad_probe"
description = "Bad"
entrypoint = "bin/probe.sh"
parameters = { type = "object" }
"#,
        vec![
            ("bin/probe.sh", b"#!/bin/sh\necho '{}'\n", true),
            ("skills/broken/SKILL.md", b"---\nname: broken\n", false),
        ],
    );
    let bad_res = service
        .execute(PackageCommand::Install {
            request_id: "install-bad".to_string(),
            package_id: "bad-tool".to_string(),
            snapshot: bad_tool,
        })
        .unwrap();
    assert!(!bad_res.success);
    assert!(service.resolve("bad-tool").is_err());

    // 3. Binding Root persistence (`SkillsOnly` -> `Active` -> reopen -> `uninstall` cleanup).
    let bound = service
        .bind_for_root("probe-with-skills", RootBindingMode::SkillsOnly, None)
        .unwrap();
    assert_eq!(bound.mode, RootBindingMode::SkillsOnly);
    drop(service);

    let loaded_from_store = PackageService::load_root_bindings_from_store(&store_root).unwrap();
    assert_eq!(loaded_from_store.len(), 1);
    assert_eq!(loaded_from_store[0].package_id, "probe-with-skills");
    assert_eq!(loaded_from_store[0].mode, RootBindingMode::SkillsOnly);

    let reopened = PackageService::open(store_root).unwrap();
    assert_eq!(
        reopened
            .root_binding("probe-with-skills")
            .unwrap()
            .unwrap()
            .mode,
        RootBindingMode::SkillsOnly
    );

    // Colliding Companion Skill ID with an already bound Root package fails closed.
    let colliding = fs_snapshot_with_manifest(
        "colliding-pack",
        r#"
schema_version = 1
kind = "fileserver"
[fileserver]
id = "collide"
entrypoint = "bin/srv"
allow_network = false
[fileserver.transport]
type = "stdio"
"#,
        vec![
            ("bin/srv", b"#!/bin/sh\nexit 0\n", true),
            (
                "skills/probe-guide/SKILL.md",
                b"---\nname: probe-guide\ndescription: Colliding skill.\n---\n",
                false,
            ),
        ],
    );
    reopened
        .execute(PackageCommand::Install {
            request_id: "install-colliding".to_string(),
            package_id: "colliding-pack".to_string(),
            snapshot: colliding,
        })
        .unwrap();
    let collision_err = reopened
        .bind_for_root("colliding-pack", RootBindingMode::SkillsOnly, None)
        .unwrap_err();
    assert!(
        collision_err.to_string().contains("collides"),
        "{collision_err:#}"
    );

    // Network escalation on a fileserver with `allow_network = false` fails closed.
    let no_net = fs_snapshot_with_manifest(
        "local-only-fs",
        r#"
schema_version = 1
kind = "fileserver"
[fileserver]
id = "local_fs"
entrypoint = "bin/srv"
allow_network = false
[fileserver.transport]
type = "stdio"
"#,
        vec![("bin/srv", b"#!/bin/sh\nexit 0\n", true)],
    );
    reopened
        .execute(PackageCommand::Install {
            request_id: "install-no-net".to_string(),
            package_id: "local-only-fs".to_string(),
            snapshot: no_net,
        })
        .unwrap();
    let net_err = reopened
        .bind_for_root(
            "local-only-fs",
            RootBindingMode::Active,
            Some(RootFileserverActivation {
                access: FileserverMountAccess::ReadWrite,
                allow_network: true,
                host_grant_ids: Vec::new(),
            }),
        )
        .unwrap_err();
    assert!(net_err.to_string().contains("allow_network"), "{net_err:#}");

    // Uninstalling automatically removes its Root binding.
    reopened
        .execute(PackageCommand::Uninstall {
            request_id: "uninstall-probe".to_string(),
            package_id: "probe-with-skills".to_string(),
        })
        .unwrap();
    assert!(
        reopened
            .root_binding("probe-with-skills")
            .unwrap()
            .is_none()
    );
}
