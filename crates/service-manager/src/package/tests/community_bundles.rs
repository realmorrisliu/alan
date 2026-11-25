use super::*;
use crate::quartermaster::{QUARTERMASTER_EXECUTABLE, QuartermasterProcessRunner};
use crate::{CLI_IMPORT_MOUNT_PATH, run_q_command_with_package_service};
use alan_agent_engine::skills::{SkillScope, SkillTypedDependency, parse_skill_metadata};
use alan_ap::InProcessTransport;
use alan_kernel::{
    Access, Credentials, ExecSpec, Namespace, Pid, ProcessInvocation, ProcessRunner,
};
use alan_shell::Shell;
use std::collections::BTreeMap;

#[tokio::test]
async fn q_install_materializes_pstack_shaped_community_bundle_with_shared_assets() {
    let fixture = tempfile::tempdir().unwrap();
    let pstack_dir = fixture.path().join("pstack");
    fs::create_dir_all(pstack_dir.join("skills/architect")).unwrap();
    fs::create_dir_all(pstack_dir.join("commands")).unwrap();
    fs::create_dir_all(pstack_dir.join("playbooks/nested")).unwrap();
    fs::create_dir_all(pstack_dir.join("references")).unwrap();
    fs::create_dir_all(pstack_dir.join("templates")).unwrap();
    fs::create_dir_all(pstack_dir.join("assets")).unwrap();

    fs::write(
        pstack_dir.join("skills/architect/SKILL.md"),
        r#"---
name: architect
description: Design and compare software architectures.
license: MIT
model: claude-opus-4-6
allowed-tools:
  - Read
  - Edit
  - Bash
alwaysApply: true
compatibility: Requires git and ripgrep
metadata:
  author: poteto
  tags: "architecture, planning"
---

Follow `playbooks/architecture.md` and consult `references/patterns.md`.
"#,
    )
    .unwrap();
    fs::write(
        pstack_dir.join("skills/how.md"),
        "Use Read and Subagent to investigate the codebase before answering.",
    )
    .unwrap();
    fs::write(
        pstack_dir.join("commands/prove-it.md"),
        r#"---
description: Verify implementation claims with evidence.
argument-hint: "[target]"
allowed-tools: Read, Edit, Bash, WebSearch
---

Use Read, Edit, Bash, and WebSearch to verify every claim against `playbooks/architecture.md`.
"#,
    )
    .unwrap();
    fs::write(
        pstack_dir.join("playbooks/architecture.md"),
        "# Shared Architecture Playbook\n",
    )
    .unwrap();
    fs::write(
        pstack_dir.join("playbooks/nested/SKILL.md"),
        "---\nname: nested-example\ndescription: Asset only.\n---\nIgnored.\n",
    )
    .unwrap();
    fs::write(
        pstack_dir.join("references/patterns.md"),
        "# Reference Patterns\n",
    )
    .unwrap();
    fs::write(pstack_dir.join("templates/adr.md"), "# ADR Template\n").unwrap();
    fs::write(pstack_dir.join("assets/diagram.txt"), "A -> B\n").unwrap();

    let service = PackageService::ephemeral().unwrap();
    let q_res = run_q_command_with_package_service(
        &service,
        Some(&pstack_dir),
        &[
            "install".to_string(),
            "--name".to_string(),
            "pstack".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        q_res.exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&q_res.output)
    );

    let record = service.resolve("pstack").unwrap();
    assert_eq!(record.export_kind, PackageExportKind::Skill);
    assert_eq!(
        record
            .exports
            .iter()
            .map(|export| export.skill_id.as_str())
            .collect::<Vec<_>>(),
        vec!["architect", "how", "prove-it"]
    );

    // Unknown frontmatter keys grant zero runtime authority; `WebSearch` remains gated while
    // `Read`/`Edit`/`Bash`/`Subagent` do not create false-positive unavailable dependencies.
    assert!(record.exports[0].dependencies.is_empty());
    assert!(record.exports[1].dependencies.is_empty());
    assert_eq!(
        record.exports[2]
            .dependencies
            .iter()
            .map(SkillTypedDependency::identity_key)
            .collect::<Vec<_>>(),
        vec!["runtime_capability:web-search"]
    );

    let lease = service.acquire("pstack").unwrap();
    let pkg_shell = Shell::new(InProcessTransport::new(lease.file_server().unwrap()));

    let architect_doc =
        String::from_utf8(pkg_shell.cat("/skills/architect/SKILL.md").await.unwrap()).unwrap();
    let architect_meta = parse_skill_metadata(
        &architect_doc,
        Path::new("architect/SKILL.md"),
        SkillScope::Installed,
    )
    .unwrap();
    assert_eq!(
        architect_meta.compatibility.requirements.as_deref(),
        Some("Requires git and ripgrep")
    );
    assert!(
        architect_meta
            .capabilities
            .as_ref()
            .is_some_and(|cap| cap.required_tools.is_empty())
    );

    let how_doc = String::from_utf8(pkg_shell.cat("/skills/how/SKILL.md").await.unwrap()).unwrap();
    assert!(how_doc.contains("`Read` -> `read_file`"), "{how_doc}");
    assert!(
        how_doc.contains("`Subagent` -> `spawn_subagent`"),
        "{how_doc}"
    );

    let prove_it_doc =
        String::from_utf8(pkg_shell.cat("/skills/prove-it/SKILL.md").await.unwrap()).unwrap();
    assert!(
        prove_it_doc.contains("`Read` -> `read_file`"),
        "{prove_it_doc}"
    );
    assert!(
        prove_it_doc.contains("`Edit` -> `edit_file`"),
        "{prove_it_doc}"
    );
    assert!(prove_it_doc.contains("`Bash` -> `bash`"), "{prove_it_doc}");
    assert!(prove_it_doc.contains("name: web-search"), "{prove_it_doc}");

    // Shared package-level read-only directories are preserved under `/lib/pkg/<pkg>/`.
    for (rel_path, expected_prefix) in [
        (
            "/playbooks/architecture.md",
            "# Shared Architecture Playbook",
        ),
        ("/playbooks/nested/SKILL.md", "---\nname: nested-example"),
        ("/references/patterns.md", "# Reference Patterns"),
        ("/templates/adr.md", "# ADR Template"),
        ("/assets/diagram.txt", "A -> B"),
    ] {
        let content = String::from_utf8(pkg_shell.cat(rel_path).await.unwrap()).unwrap();
        assert!(
            content.starts_with(expected_prefix),
            "{rel_path}: {content}"
        );
        let stat = pkg_shell.stat(rel_path).await.unwrap();
        assert!(!stat.writable, "{rel_path} must remain read-only");
        #[cfg(unix)]
        assert!(!stat.executable, "{rel_path} must remain non-executable");
    }
}

#[test]
fn community_bundle_rejects_command_and_skill_id_collisions_atomically() {
    let service = PackageService::ephemeral().unwrap();
    let result = service
        .execute(PackageCommand::Install {
            request_id: "command-collision".to_string(),
            package_id: "colliding-pack".to_string(),
            snapshot: PackageSnapshot {
                source_name: "colliding-pack".to_string(),
                entries: vec![
                    PackageSnapshotEntry {
                        path: "skills/review/SKILL.md".to_string(),
                        bytes: b"---\nname: review\ndescription: Native skill.\n---\nBody\n"
                            .to_vec(),
                        executable: false,
                    },
                    PackageSnapshotEntry {
                        path: "commands/review.md".to_string(),
                        bytes: b"Command body.".to_vec(),
                        executable: false,
                    },
                ],
            },
        })
        .unwrap();

    assert!(!result.success);
    assert!(
        result.message.contains("duplicate Skill id `review`"),
        "{}",
        result.message
    );
    assert!(service.catalog().unwrap().packages.is_empty());
}

#[tokio::test]
async fn skills_only_projection_grants_zero_authority_and_composes_pstack_with_alan_arena() {
    let fixture = tempfile::tempdir().unwrap();
    let pstack_dir = fixture.path().join("pstack");
    fs::create_dir_all(pstack_dir.join("commands")).unwrap();
    fs::create_dir_all(pstack_dir.join("playbooks")).unwrap();
    fs::write(
        pstack_dir.join("commands/architect.md"),
        "---\ndescription: Architecture review\nallowed-tools: Bash, Edit\n---\nUse Read and Edit.\n",
    )
    .unwrap();
    fs::write(
        pstack_dir.join("playbooks/architect.md"),
        "# Pstack Architect Playbook\n",
    )
    .unwrap();

    let arena_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/alan-arena");
    assert!(arena_dir.is_dir(), "missing {}", arena_dir.display());
    assert!(arena_dir.join("README.md").is_file());

    let service = PackageService::ephemeral().unwrap();
    for (pkg_id, dir) in [
        ("pstack", pstack_dir.as_path()),
        ("alan-arena", arena_dir.as_path()),
    ] {
        let res = run_q_command_with_package_service(
            &service,
            Some(dir),
            &[
                "install".to_string(),
                "--name".to_string(),
                pkg_id.to_string(),
                CLI_IMPORT_MOUNT_PATH.to_string(),
            ],
        )
        .await
        .unwrap();
        assert_eq!(res.exit_code, 0, "{}", String::from_utf8_lossy(&res.output));
        service
            .bind_for_root(pkg_id, crate::RootBindingMode::SkillsOnly, None)
            .unwrap();
    }

    let arena_rec = service.resolve("alan-arena").unwrap();
    assert_eq!(arena_rec.export_kind, PackageExportKind::Skill);
    assert_eq!(
        arena_rec
            .exports
            .iter()
            .map(|e| e.skill_id.as_str())
            .collect::<Vec<_>>(),
        vec!["alan-arena"]
    );

    assert_eq!(
        service.root_binding("pstack").unwrap().unwrap().mode,
        crate::RootBindingMode::SkillsOnly
    );
    assert_eq!(
        service.root_binding("alan-arena").unwrap().unwrap().mode,
        crate::RootBindingMode::SkillsOnly
    );

    let pstack_rec = service.resolve("pstack").unwrap();
    assert!(pstack_rec.tool_exports.is_empty());
    assert!(pstack_rec.mcp_exports.is_empty());
    assert!(pstack_rec.provider_export.is_none());
    assert!(pstack_rec.fileserver_export.is_none());

    let mut namespace = Namespace::new();
    for pkg_id in ["pstack", "alan-arena"] {
        let lease = service.acquire(pkg_id).unwrap();
        let handle = InProcessTransport::new(lease.file_server().unwrap());
        namespace.mount(&format!("/lib/pkg/{pkg_id}"), handle, Access::ReadOnly);
    }

    let described = namespace.describe();
    assert!(
        described
            .iter()
            .any(|(path, access)| path == "/lib/pkg/pstack" && *access == Access::ReadOnly)
    );
    assert!(
        described
            .iter()
            .any(|(path, access)| path == "/lib/pkg/alan-arena" && *access == Access::ReadOnly)
    );
    assert!(
        !described
            .iter()
            .any(|(path, _)| path.starts_with("/bin/") || path.starts_with("/mnt/"))
    );

    let arena_lease = service.acquire("alan-arena").unwrap();
    let arena_shell = Shell::new(InProcessTransport::new(arena_lease.file_server().unwrap()));
    assert!(
        arena_shell
            .write("/skills/alan-arena/SKILL.md", b"tamper")
            .await
            .is_err()
    );
    let arena_skill = fs::read_to_string(
        arena_lease
            .content_root()
            .join("skills/alan-arena/SKILL.md"),
    )
    .unwrap();
    assert!(arena_skill.contains("spawn_subagent"));
    assert!(arena_skill.contains("read_only: true"));
    assert!(arena_skill.contains("verifier"));
}

#[tokio::test]
async fn ambient_host_skill_directories_and_remote_urls_are_rejected() {
    let fixture = tempfile::tempdir().unwrap();
    let mounted_bundle = fixture.path().join("mounted-bundle");
    let ambient_claude = fixture.path().join(".claude/skills/ambient-secret");
    fs::create_dir_all(mounted_bundle.join("skills/safe")).unwrap();
    fs::create_dir_all(&ambient_claude).unwrap();

    fs::write(
        mounted_bundle.join("skills/safe/SKILL.md"),
        "---\nname: safe\ndescription: Explicit mounted skill.\n---\nSafe body.\n",
    )
    .unwrap();
    fs::write(
        ambient_claude.join("SKILL.md"),
        "---\nname: ambient-secret\ndescription: Should never be scanned.\n---\nAmbient.\n",
    )
    .unwrap();

    let service = PackageService::ephemeral().unwrap();
    let installed = run_q_command_with_package_service(
        &service,
        Some(&mounted_bundle),
        &[
            "install".to_string(),
            "--name".to_string(),
            "mounted-bundle".to_string(),
            CLI_IMPORT_MOUNT_PATH.to_string(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(installed.exit_code, 0);
    let record = service.resolve("mounted-bundle").unwrap();
    assert_eq!(
        record
            .exports
            .iter()
            .map(|e| e.skill_id.as_str())
            .collect::<Vec<_>>(),
        vec!["safe"]
    );

    // Remote URLs are rejected before touching PackageService.
    for remote_url in [
        "https://github.com/poteto/pstack.git",
        "http://example.invalid/bundle.tar.gz",
        "git@github.com:poteto/pstack.git",
    ] {
        let outcome = QuartermasterProcessRunner
            .run(ProcessInvocation {
                pid: Pid(3),
                parent: None,
                credentials: Credentials::user("test"),
                namespace: Namespace::new(),
                exec: ExecSpec {
                    executable: QUARTERMASTER_EXECUTABLE.to_string(),
                    args: vec!["install".to_string(), remote_url.to_string()],
                    namespace: alan_kernel::ExecNamespaceManifest::default(),
                    descriptors: BTreeMap::new(),
                },
            })
            .await;
        assert_eq!(outcome.exit_code, 2, "{remote_url}");
    }
}
