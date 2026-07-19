use super::*;
use std::path::PathBuf;

use alan_agent_engine::{HostMountGrant, tools::ToolExecutionBinding};

struct TestExport {
    tree: InProcessTransport,
    grant: HostMountGrant,
}

impl std::fmt::Debug for TestExport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TestExport")
    }
}

impl HostMountExport for TestExport {
    fn file_tree(&self) -> InProcessTransport {
        self.tree.clone()
    }

    fn apply_tool_authority(
        &self,
        effective_access: HostMountAccess,
        binding: &mut ToolExecutionBinding,
    ) -> Result<()> {
        let mut grant = self.grant.clone();
        grant.access = effective_access.kernel();
        binding.apply_host_mount(grant)
    }
}

#[derive(Debug, Default)]
struct TestAdapter;

impl HostMountExportAdapter for TestAdapter {
    fn export_approved(&self, grant: &ApprovedMountGrant) -> Result<Arc<dyn HostMountExport>> {
        Ok(test_export(
            &grant.namespace_path,
            grant.host_path.clone(),
            match grant.access {
                ApprovedMountGrantAccess::ReadOnly => HostMountAccess::ReadOnly,
                ApprovedMountGrantAccess::ReadWrite => HostMountAccess::ReadWrite,
            },
        ))
    }
}

fn service() -> Arc<HostMountService> {
    HostMountService::new(Arc::new(TestAdapter))
}

fn test_export(
    namespace_path: &str,
    host_path: PathBuf,
    access: HostMountAccess,
) -> Arc<dyn HostMountExport> {
    Arc::new(TestExport {
        tree: InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        grant: HostMountGrant::new(namespace_path, host_path, access.kernel()).unwrap(),
    })
}

#[test]
fn grants_project_explicitly_hide_paths_and_revoke() {
    let host = tempfile::tempdir().unwrap();
    std::fs::write(host.path().join("note"), "hello").unwrap();
    let service = service();
    let namespace = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(7), namespace.clone());
    let id = service
        .enqueue(HostMountRequest {
            id: "grant-a".to_string(),
            label: "project".to_string(),
            namespace_path: "/mnt/project".to_string(),
            access: HostMountAccess::ReadWrite,
            reason: "work".to_string(),
            requesting_pid: 7,
        })
        .unwrap();
    let export = test_export(
        "/mnt/project",
        host.path().to_path_buf(),
        HostMountAccess::ReadWrite,
    );
    let record = service
        .approve_export(&id, export.clone(), "user", "tester")
        .unwrap();
    assert_eq!(record.namespace_path, "/mnt/project");
    assert!(
        !serde_json::to_string(&record)
            .unwrap()
            .contains(host.path().to_str().unwrap())
    );
    assert!(
        namespace
            .describe()
            .iter()
            .any(|(path, _)| path == "/mnt/project")
    );
    let mut cached =
        ToolExecutionBinding::new(host.path().to_path_buf(), host.path().join("scratch"));
    export
        .apply_tool_authority(HostMountAccess::ReadWrite, &mut cached)
        .unwrap();
    let reconciled = service.reconcile(Pid(7), cached.clone()).unwrap();
    assert_eq!(reconciled.host_mounts.len(), 1);
    assert_eq!(reconciled.sandbox_spec.unwrap().writable_roots.len(), 1);

    service.revoke(&id, "tester").unwrap();
    assert!(
        !namespace
            .describe()
            .iter()
            .any(|(path, _)| path == "/mnt/project")
    );
    assert!(service.reconcile(Pid(7), cached).is_err());
}

#[test]
fn replacing_a_process_mount_path_retires_the_old_projection_identity() {
    let old_host = tempfile::tempdir().unwrap();
    let latest_host = tempfile::tempdir().unwrap();
    let service = service();
    let namespace = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(7), namespace.clone());

    for (id, host) in [
        ("grant-old", old_host.path()),
        ("grant-latest", latest_host.path()),
    ] {
        service
            .enqueue(HostMountRequest {
                id: id.to_string(),
                label: id.to_string(),
                namespace_path: "/mnt/project".to_string(),
                access: HostMountAccess::ReadOnly,
                reason: "replace exact mount".to_string(),
                requesting_pid: 7,
            })
            .unwrap();
        service
            .approve_export(
                id,
                test_export(
                    "/mnt/project",
                    host.to_path_buf(),
                    HostMountAccess::ReadOnly,
                ),
                "user",
                "tester",
            )
            .unwrap();
    }

    let binding = ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/mnt/project"),
        latest_host.path().join("scratch"),
    );
    let reconciled = service.reconcile(Pid(7), binding.clone()).unwrap();
    assert_eq!(reconciled.cwd, latest_host.path());

    service.revoke("grant-old", "tester").unwrap();

    assert!(namespace.snapshot().resolve("/mnt/project").is_ok());
    assert_eq!(
        service.reconcile(Pid(7), binding).unwrap().cwd,
        latest_host.path()
    );
}

#[test]
fn mount_free_tool_binding_reconciles_without_acquiring_authority() {
    let service = service();
    let namespace = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(7), namespace);
    let binding = ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/"),
        PathBuf::from("/tmp/alan-scratch"),
    );

    let reconciled = service.reconcile(Pid(7), binding.clone()).unwrap();

    assert_eq!(reconciled, binding);
    assert!(reconciled.host_mounts.is_empty());
    assert!(reconciled.sandbox_spec.is_none());
}

#[test]
fn knowing_grant_id_does_not_project_to_another_process() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let parent = LiveNamespace::new(Namespace::new());
    let child = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(1), parent.clone());
    service.register_process(Pid(2), child.clone());
    let id = service
        .enqueue(HostMountRequest {
            id: "grant-parent".to_string(),
            label: "parent".to_string(),
            namespace_path: "/mnt/data".to_string(),
            access: HostMountAccess::ReadOnly,
            reason: "read".to_string(),
            requesting_pid: 1,
        })
        .unwrap();
    service
        .approve_export(
            &id,
            test_export(
                "/mnt/data",
                host.path().to_path_buf(),
                HostMountAccess::ReadOnly,
            ),
            "user",
            "tester",
        )
        .unwrap();
    assert!(
        parent
            .describe()
            .iter()
            .any(|(path, _)| path == "/mnt/data")
    );
    assert!(!child.describe().iter().any(|(path, _)| path == "/mnt/data"));
    assert!(service.project(&id, 2).is_err());
}

#[test]
fn failed_initial_projection_is_terminal_and_removes_the_grant() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let namespace = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(7), namespace.clone());
    let id = service
        .enqueue(HostMountRequest {
            id: "grant-retry".to_string(),
            label: "retry".to_string(),
            namespace_path: "/mnt/retry".to_string(),
            access: HostMountAccess::ReadOnly,
            reason: "retry after Process exit".to_string(),
            requesting_pid: 7,
        })
        .unwrap();
    let export = test_export(
        "/mnt/retry",
        host.path().to_path_buf(),
        HostMountAccess::ReadOnly,
    );

    service.unregister_process(Pid(7));
    assert!(
        service
            .approve_export(&id, export.clone(), "user", "tester")
            .is_err()
    );
    assert!(service.pending_request(&id).is_none());
    assert!(!service.state.lock().unwrap().grants.contains_key(&id));
    let failed = service.request_snapshot(&id).unwrap();
    assert_eq!(failed.status, HostMountStatus::Failed);
    assert_eq!(
        failed.error.as_deref(),
        Some("Host Mount namespace projection failed")
    );

    service.register_process(Pid(7), namespace.clone());
    assert!(
        service
            .approve_export(&id, export, "user", "tester")
            .is_err()
    );
    assert!(namespace.snapshot().resolve("/mnt/retry").is_err());
}

#[test]
fn claimed_approval_cannot_be_overtaken_by_another_terminal_decision() {
    let service = service();
    let id = service
        .enqueue(HostMountRequest {
            id: "grant-race".to_string(),
            label: "race".to_string(),
            namespace_path: "/mnt/race".to_string(),
            access: HostMountAccess::ReadOnly,
            reason: "verify immutable settlement".to_string(),
            requesting_pid: 7,
        })
        .unwrap();
    assert_eq!(service.claim_pending_request(&id).unwrap().id, id);
    assert!(
        service
            .reject_request(&id, "late rejection", "second-decider")
            .is_err()
    );
    assert!(service.claim_pending_request(&id).is_err());
    assert!(service.pending_request(&id).is_none());
}

#[test]
fn approved_agent_definition_uses_the_internal_projection_path_only() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let namespace = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(3), namespace.clone());
    assert!(
        service
            .enqueue(HostMountRequest {
                id: "public-definition".to_string(),
                label: "definition".to_string(),
                namespace_path: "/agent-definition".to_string(),
                access: HostMountAccess::ReadOnly,
                reason: "public request".to_string(),
                requesting_pid: 3,
            })
            .is_err()
    );

    let factory = HostMountApplicatorFactory::new(service);
    let applicator = factory.create(Pid(3), namespace.clone(), &[]);
    applicator
        .apply_mount_grant(&ApprovedMountGrant::new(
            "/agent-definition",
            host.path().to_path_buf(),
            ApprovedMountGrantAccess::ReadOnly,
            "Agent Definition launch reference",
        ))
        .unwrap();
    assert!(
        namespace
            .describe()
            .iter()
            .any(|(path, access)| path == "/agent-definition" && *access == Access::ReadOnly)
    );
}

#[test]
fn explicitly_passed_child_projection_is_revoked_with_its_parent() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let parent = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(1), parent.clone());
    let id = service
        .enqueue(HostMountRequest {
            id: "grant-parent".to_string(),
            label: "parent".to_string(),
            namespace_path: "/mnt/data".to_string(),
            access: HostMountAccess::ReadWrite,
            reason: "write".to_string(),
            requesting_pid: 1,
        })
        .unwrap();
    let export = test_export(
        "/mnt/data",
        host.path().to_path_buf(),
        HostMountAccess::ReadWrite,
    );
    service
        .approve_export(&id, export.clone(), "user", "tester")
        .unwrap();

    let child = LiveNamespace::new(parent.snapshot());
    let factory = HostMountApplicatorFactory::inheriting_from(service.clone(), Pid(1));
    factory.create(Pid(2), child.clone(), std::slice::from_ref(&id));
    let cached = ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/mnt/data"),
        host.path().join("scratch"),
    );
    let reconciled = service.reconcile(Pid(2), cached.clone()).unwrap();
    assert_eq!(reconciled.namespace_cwd, PathBuf::from("/mnt/data"));
    assert_eq!(reconciled.cwd, host.path());
    assert_eq!(reconciled.host_mounts.len(), 1);

    service.revoke(&id, "tester").unwrap();
    assert!(parent.snapshot().resolve("/mnt/data").is_err());
    assert!(child.snapshot().resolve("/mnt/data").is_err());
    let revoked = service.reconcile(Pid(2), cached).unwrap();
    assert!(revoked.host_mounts.is_empty());
    assert!(revoked.sandbox_spec.is_none());
}

#[test]
fn child_inheritance_uses_the_exact_grant_held_by_its_parent() {
    let first_host = tempfile::tempdir().unwrap();
    let other_host = tempfile::tempdir().unwrap();
    let service = service();
    let first_parent = LiveNamespace::new(Namespace::new());
    let other_parent = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(1), first_parent.clone());
    service.register_process(Pid(2), other_parent);

    for (id, pid, host) in [
        ("grant-a", Pid(1), first_host.path()),
        ("grant-z", Pid(2), other_host.path()),
    ] {
        service
            .enqueue(HostMountRequest {
                id: id.to_string(),
                label: id.to_string(),
                namespace_path: "/mnt/project".to_string(),
                access: HostMountAccess::ReadOnly,
                reason: "test exact inheritance".to_string(),
                requesting_pid: pid.0,
            })
            .unwrap();
        service
            .approve_export(
                id,
                test_export(
                    "/mnt/project",
                    host.to_path_buf(),
                    HostMountAccess::ReadOnly,
                ),
                "user",
                "tester",
            )
            .unwrap();
    }

    let child = LiveNamespace::new(first_parent.snapshot());
    HostMountApplicatorFactory::inheriting_from(service.clone(), Pid(1)).create(
        Pid(3),
        child,
        &["grant-a".to_string()],
    );
    let binding = ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/mnt/project"),
        first_host.path().join("scratch"),
    );

    let reconciled = service.reconcile(Pid(3), binding).unwrap();

    assert_eq!(reconciled.host_mounts.len(), 1);
    assert_eq!(reconciled.cwd, first_host.path());
    assert_ne!(reconciled.cwd, other_host.path());
}

#[test]
fn inherited_read_only_projection_does_not_recover_write_authority() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let parent = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(1), parent.clone());
    let id = service
        .enqueue(HostMountRequest {
            id: "grant-writable-parent".to_string(),
            label: "project".to_string(),
            namespace_path: "/mnt/project".to_string(),
            access: HostMountAccess::ReadWrite,
            reason: "test narrowed delegation".to_string(),
            requesting_pid: 1,
        })
        .unwrap();
    service
        .approve_export(
            &id,
            test_export(
                "/mnt/project",
                host.path().to_path_buf(),
                HostMountAccess::ReadWrite,
            ),
            "user",
            "tester",
        )
        .unwrap();

    let child_namespace = parent
        .snapshot()
        .project_mounts([("/mnt/project", "/mnt/project", Access::ReadOnly)])
        .unwrap();
    let child = LiveNamespace::new(child_namespace);
    HostMountApplicatorFactory::inheriting_from(service.clone(), Pid(1)).create(
        Pid(2),
        child,
        std::slice::from_ref(&id),
    );
    let binding = ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/mnt/project"),
        host.path().join("scratch"),
    );

    let reconciled = service.reconcile(Pid(2), binding).unwrap();

    assert_eq!(reconciled.host_mounts.len(), 1);
    assert_eq!(reconciled.host_mounts[0].access, Access::ReadOnly);
    assert!(
        reconciled
            .sandbox_spec
            .expect("read-only Host Mount still has a sandbox spec")
            .writable_roots
            .is_empty()
    );
}

#[tokio::test]
async fn projection_enforces_read_only_and_read_write_access() {
    for (pid, access, writable) in [
        (Pid(10), HostMountAccess::ReadOnly, false),
        (Pid(11), HostMountAccess::ReadWrite, true),
    ] {
        let service = service();
        let namespace = LiveNamespace::new(Namespace::new());
        service.register_process(pid, namespace.clone());
        let id = service
            .enqueue(HostMountRequest {
                id: format!("grant-{}", pid.0),
                label: "data".to_string(),
                namespace_path: "/mnt/data".to_string(),
                access,
                reason: "test".to_string(),
                requesting_pid: pid.0,
            })
            .unwrap();
        service
            .approve_export(
                &id,
                test_export("/mnt/data", PathBuf::from("/tmp/data"), access),
                "test",
                "tester",
            )
            .unwrap();
        let shell = alan_shell::Shell::new(InProcessTransport::new(Arc::new(
            alan_kernel::MountFs::from_live_namespace(namespace),
        )));
        assert_eq!(shell.cat("/mnt/data/greeting").await.unwrap(), b"hi");
        let write = shell.write("/mnt/data/submit", b"{}").await;
        if writable {
            assert_eq!(write, Ok(()));
        } else {
            assert_eq!(write, Err(ErrorCode::NoAccess));
        }
    }
}
