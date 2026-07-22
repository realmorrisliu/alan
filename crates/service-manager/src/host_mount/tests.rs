use super::delegation::validate_child_mount_requests;
use super::*;

use alan_agent_engine::SpawnMountAccess;
use alan_agent_engine::tools::{
    ReifiedMountAccess, Sandbox, SandboxHostMount, SandboxSpec, ToolExecutionAdapter,
    ToolExecutionBinding,
};
use alan_kernel::Namespace;

#[derive(Clone)]
struct TestExport {
    tree: InProcessTransport,
    host_root: PathBuf,
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

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Debug)]
struct TestMapping {
    namespace_path: PathBuf,
    host_root: PathBuf,
    access: HostMountAccess,
}

#[derive(Debug)]
struct TestToolAdapter {
    namespace_cwd: PathBuf,
    mappings: Vec<TestMapping>,
    sandbox: Sandbox,
}

impl TestToolAdapter {
    fn visible_path(&self, namespace_cwd: &Path, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            namespace_cwd.join(path)
        }
    }

    fn mapping_for_visible_path(&self, path: &Path) -> Option<&TestMapping> {
        self.mappings
            .iter()
            .filter(|mapping| path.starts_with(&mapping.namespace_path))
            .max_by_key(|mapping| mapping.namespace_path.components().count())
    }
}

impl ToolExecutionAdapter for TestToolAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        self.namespace_cwd.clone()
    }

    fn cwd(&self) -> Result<PathBuf> {
        self.resolve_path(&self.namespace_cwd, Path::new("."))
    }

    fn resolve_path(&self, namespace_cwd: &Path, path: &Path) -> Result<PathBuf> {
        let visible = self.visible_path(namespace_cwd, path);
        let mapping = self
            .mapping_for_visible_path(&visible)
            .with_context(|| format!("{} is outside the test projection", visible.display()))?;
        let suffix = visible
            .strip_prefix(&mapping.namespace_path)
            .expect("selected mapping contains visible path");
        Ok(mapping.host_root.join(suffix))
    }

    fn visible_path(&self, host_path: &Path) -> PathBuf {
        self.mappings
            .iter()
            .filter_map(|mapping| {
                host_path
                    .strip_prefix(&mapping.host_root)
                    .ok()
                    .map(|suffix| mapping.namespace_path.join(suffix))
            })
            .next()
            .unwrap_or_else(|| PathBuf::from("<unmapped-host-path>"))
    }

    fn project_text(&self, text: &str) -> String {
        self.mappings
            .iter()
            .fold(text.to_string(), |projected, mapping| {
                projected.replace(
                    mapping.host_root.to_string_lossy().as_ref(),
                    mapping.namespace_path.to_string_lossy().as_ref(),
                )
            })
    }

    fn sandbox(&self) -> Result<Sandbox> {
        Ok(self.sandbox.clone())
    }
}

#[derive(Debug, Default)]
struct TestAdapter;

impl HostMountExportAdapter for TestAdapter {
    fn tool_execution_adapter(
        &self,
        projections: &[HostMountToolProjection],
        requested_namespace_cwd: &Path,
    ) -> Result<Arc<dyn ToolExecutionAdapter>> {
        ensure!(
            !projections.is_empty(),
            "test adapter requires a projection"
        );
        let mappings = projections
            .iter()
            .map(|projection| {
                let export = projection
                    .export()
                    .as_any()
                    .downcast_ref::<TestExport>()
                    .context("unexpected test Host Mount export")?;
                Ok(TestMapping {
                    namespace_path: projection.namespace_path.clone(),
                    host_root: export.host_root.clone(),
                    access: projection.access,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let namespace_cwd = if mappings
            .iter()
            .any(|mapping| requested_namespace_cwd.starts_with(&mapping.namespace_path))
        {
            requested_namespace_cwd.to_path_buf()
        } else {
            mappings[0].namespace_path.clone()
        };
        let sandbox_mounts = mappings
            .iter()
            .map(|mapping| SandboxHostMount {
                namespace_path: mapping.namespace_path.clone(),
                host_path: mapping.host_root.clone(),
                access: match mapping.access {
                    HostMountAccess::ReadOnly => ReifiedMountAccess::ReadOnly,
                    HostMountAccess::ReadWrite => ReifiedMountAccess::ReadWrite,
                },
            })
            .collect::<Vec<_>>();
        Ok(Arc::new(TestToolAdapter {
            namespace_cwd,
            mappings,
            sandbox: Sandbox::from_spec(SandboxSpec::from_host_mounts(&sandbox_mounts)),
        }))
    }
}

fn service() -> Arc<HostMountService> {
    HostMountService::new(Arc::new(TestAdapter))
}

fn test_export(host_root: PathBuf) -> Arc<dyn HostMountExport> {
    Arc::new(TestExport {
        tree: InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        host_root,
    })
}

fn register(service: &HostMountService, pid: u64) -> LiveNamespace {
    let namespace = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(pid), namespace.clone());
    namespace
}

fn approve(
    service: &HostMountService,
    pid: u64,
    id: &str,
    namespace_path: &str,
    access: HostMountAccess,
    host_root: PathBuf,
) -> HostMountGrantRecord {
    service
        .enqueue(HostMountRequest {
            id: id.to_string(),
            label: id.to_string(),
            namespace_path: namespace_path.to_string(),
            access,
            reason: "test Host Mount authority".to_string(),
            requesting_pid: pid,
        })
        .unwrap();
    service
        .approve_export(id, test_export(host_root), "test-user", "test")
        .unwrap()
}

fn selection(id: &str, target: &str, access: SpawnMountAccess) -> SpawnHostMount {
    SpawnHostMount {
        grant: id.to_string(),
        target: PathBuf::from(target),
        access,
    }
}

fn binding(namespace_cwd: &str) -> ToolExecutionBinding {
    ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from(namespace_cwd),
        PathBuf::from("/tmp/alan-host-mount-test-scratch"),
    )
}

#[test]
fn approval_projects_an_opaque_handle_and_revocation_fails_closed() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let namespace = register(&service, 7);
    let record = approve(
        &service,
        7,
        "grant-a",
        "/mnt/project",
        HostMountAccess::ReadWrite,
        host.path().to_path_buf(),
    );

    assert_eq!(record.namespace_path, "/mnt/project");
    assert!(
        !serde_json::to_string(&record)
            .unwrap()
            .contains(host.path().to_string_lossy().as_ref())
    );
    assert_eq!(
        namespace.snapshot().resolve("/mnt/project").unwrap().access,
        Access::ReadWrite
    );

    let reconciled = service.reconcile(7, binding("/mnt/project")).unwrap();
    assert!(reconciled.has_adapter());
    let adapter = reconciled.adapter().unwrap();
    assert_eq!(adapter.cwd().unwrap(), host.path());
    assert!(adapter.sandbox().unwrap().is_writable(host.path()));

    service.revoke("grant-a", "test").unwrap();

    assert!(namespace.snapshot().resolve("/mnt/project").is_err());
    assert!(!service.grant_record("grant-a").unwrap().active);
    assert!(service.reconcile(7, reconciled).is_err());
}

#[test]
fn child_receives_no_host_authority_by_default() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    register(&service, 1);
    approve(
        &service,
        1,
        "grant-parent",
        "/mnt/source",
        HostMountAccess::ReadWrite,
        host.path().to_path_buf(),
    );
    let child = LiveNamespace::new(Namespace::new());

    service
        .register_child_process(Pid(1), Pid(2), child.clone(), &[])
        .unwrap();

    assert!(child.snapshot().resolve("/mnt/source").is_err());
    assert!(!service.grant_is_visible_to("grant-parent", Some(2)));
    assert!(!service.reconcile(2, binding("/")).unwrap().has_adapter());
}

#[test]
fn child_cannot_smuggle_an_ambient_parent_projection() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let parent = register(&service, 1);
    approve(
        &service,
        1,
        "grant-parent",
        "/mnt/source",
        HostMountAccess::ReadOnly,
        host.path().to_path_buf(),
    );
    let forged = LiveNamespace::new(parent.snapshot());

    let error = service
        .register_child_process(Pid(1), Pid(2), forged, &[])
        .unwrap_err();

    assert!(error.to_string().contains("ambient parent Host Mount"));
    assert!(!service.grant_is_visible_to("grant-parent", Some(2)));
}

#[test]
fn knowing_a_grant_id_is_not_authority() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    register(&service, 1);
    register(&service, 2);
    approve(
        &service,
        1,
        "grant-parent",
        "/mnt/source",
        HostMountAccess::ReadOnly,
        host.path().to_path_buf(),
    );

    let error = service
        .register_child_process(
            Pid(2),
            Pid(3),
            LiveNamespace::new(Namespace::new()),
            &[selection(
                "grant-parent",
                "/mnt/stolen",
                SpawnMountAccess::ReadOnly,
            )],
        )
        .unwrap_err();

    assert!(error.to_string().contains("does not hold Host Mount grant"));
}

#[test]
fn explicit_child_handle_can_remap_and_narrow_authority() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    register(&service, 1);
    approve(
        &service,
        1,
        "grant-source",
        "/mnt/source",
        HostMountAccess::ReadWrite,
        host.path().to_path_buf(),
    );
    let child = LiveNamespace::new(Namespace::new());

    service
        .register_child_process(
            Pid(1),
            Pid(2),
            child.clone(),
            &[selection(
                "grant-source",
                "/mnt/review",
                SpawnMountAccess::ReadOnly,
            )],
        )
        .unwrap();

    assert!(child.snapshot().resolve("/mnt/source").is_err());
    assert_eq!(
        child.snapshot().resolve("/mnt/review").unwrap().access,
        Access::ReadOnly
    );
    assert!(service.grant_is_visible_to("grant-source", Some(2)));
    let adapter = service
        .reconcile(2, binding("/mnt/review"))
        .unwrap()
        .adapter()
        .unwrap();
    assert_eq!(
        adapter
            .resolve_path(Path::new("/mnt/review"), Path::new("note"))
            .unwrap(),
        host.path().join("note")
    );
    assert!(!adapter.sandbox().unwrap().is_writable(host.path()));
}

#[test]
fn child_delegation_cannot_amplify_read_only_authority() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    register(&service, 1);
    approve(
        &service,
        1,
        "grant-read-only",
        "/mnt/source",
        HostMountAccess::ReadOnly,
        host.path().to_path_buf(),
    );

    let error = service
        .register_child_process(
            Pid(1),
            Pid(2),
            LiveNamespace::new(Namespace::new()),
            &[selection(
                "grant-read-only",
                "/mnt/review",
                SpawnMountAccess::ReadWrite,
            )],
        )
        .unwrap_err();

    assert!(error.to_string().contains("cannot amplify"));
}

#[test]
fn revocation_reaches_every_explicit_child_projection() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    let parent = register(&service, 1);
    approve(
        &service,
        1,
        "grant-shared",
        "/mnt/source",
        HostMountAccess::ReadWrite,
        host.path().to_path_buf(),
    );
    let child = LiveNamespace::new(Namespace::new());
    service
        .register_child_process(
            Pid(1),
            Pid(2),
            child.clone(),
            &[selection(
                "grant-shared",
                "/mnt/child",
                SpawnMountAccess::ReadOnly,
            )],
        )
        .unwrap();
    let child_binding = service.reconcile(2, binding("/mnt/child")).unwrap();

    service.revoke("grant-shared", "test").unwrap();

    assert!(parent.snapshot().resolve("/mnt/source").is_err());
    assert!(child.snapshot().resolve("/mnt/child").is_err());
    assert!(service.reconcile(2, child_binding).is_err());
}

#[test]
fn owner_projection_is_idempotent_and_fresh_bindings_recover_the_handle() {
    let host = tempfile::tempdir().unwrap();
    let service = service();
    register(&service, 1);
    approve(
        &service,
        1,
        "grant-repeat",
        "/mnt/source",
        HostMountAccess::ReadWrite,
        host.path().to_path_buf(),
    );

    service.project("grant-repeat", 1).unwrap();
    service.project("grant-repeat", 1).unwrap();

    let projection_count = service
        .state
        .lock()
        .unwrap()
        .grants
        .get("grant-repeat")
        .unwrap()
        .projections
        .iter()
        .filter(|projection| projection.pid == Pid(1))
        .count();
    assert_eq!(projection_count, 1);
    assert!(
        service
            .reconcile(1, binding("/mnt/source"))
            .unwrap()
            .has_adapter()
    );
}

#[test]
fn exact_path_replacement_retires_the_old_projection_identity() {
    let old_host = tempfile::tempdir().unwrap();
    let latest_host = tempfile::tempdir().unwrap();
    let service = service();
    let namespace = register(&service, 7);
    approve(
        &service,
        7,
        "grant-old",
        "/mnt/project",
        HostMountAccess::ReadOnly,
        old_host.path().to_path_buf(),
    );
    approve(
        &service,
        7,
        "grant-latest",
        "/mnt/project",
        HostMountAccess::ReadOnly,
        latest_host.path().to_path_buf(),
    );

    service.revoke("grant-old", "test").unwrap();

    assert!(namespace.snapshot().resolve("/mnt/project").is_ok());
    assert_eq!(
        service
            .reconcile(7, binding("/mnt/project"))
            .unwrap()
            .adapter()
            .unwrap()
            .cwd()
            .unwrap(),
        latest_host.path()
    );
}

#[test]
fn exact_path_replacement_uses_the_effective_child_projection_path() {
    let delegated_host = tempfile::tempdir().unwrap();
    let latest_host = tempfile::tempdir().unwrap();
    let service = service();
    register(&service, 1);
    approve(
        &service,
        1,
        "grant-delegated",
        "/mnt/source",
        HostMountAccess::ReadWrite,
        delegated_host.path().to_path_buf(),
    );
    let child = LiveNamespace::new(Namespace::new());
    service
        .register_child_process(
            Pid(1),
            Pid(2),
            child.clone(),
            &[selection(
                "grant-delegated",
                "/mnt/review",
                SpawnMountAccess::ReadOnly,
            )],
        )
        .unwrap();

    approve(
        &service,
        2,
        "grant-latest",
        "/mnt/review",
        HostMountAccess::ReadOnly,
        latest_host.path().to_path_buf(),
    );
    service.revoke("grant-delegated", "test").unwrap();

    assert!(child.snapshot().resolve("/mnt/review").is_ok());
    assert_eq!(
        service
            .reconcile(2, binding("/mnt/review"))
            .unwrap()
            .adapter()
            .unwrap()
            .cwd()
            .unwrap(),
        latest_host.path()
    );
}

#[test]
fn strict_namespace_overlap_is_rejected_without_partial_projection() {
    let first_host = tempfile::tempdir().unwrap();
    let nested_host = tempfile::tempdir().unwrap();
    let service = service();
    let namespace = register(&service, 1);
    approve(
        &service,
        1,
        "grant-first",
        "/mnt/project",
        HostMountAccess::ReadOnly,
        first_host.path().to_path_buf(),
    );
    service
        .enqueue(HostMountRequest {
            id: "grant-nested".to_string(),
            label: "nested".to_string(),
            namespace_path: "/mnt/project/private".to_string(),
            access: HostMountAccess::ReadOnly,
            reason: "overlap test".to_string(),
            requesting_pid: 1,
        })
        .unwrap();

    let error = service
        .approve_export(
            "grant-nested",
            test_export(nested_host.path().to_path_buf()),
            "test-user",
            "test",
        )
        .unwrap_err();

    assert!(error.to_string().contains("overlaps"));
    assert!(service.grant_record("grant-nested").is_none());
    assert!(namespace.snapshot().resolve("/mnt/project/private").is_ok());
    assert_eq!(
        namespace.snapshot().resolve("/mnt/project").unwrap().access,
        Access::ReadOnly
    );
}

#[test]
fn strict_namespace_overlap_uses_the_effective_child_projection_path() {
    let delegated_host = tempfile::tempdir().unwrap();
    let nested_host = tempfile::tempdir().unwrap();
    let service = service();
    register(&service, 1);
    approve(
        &service,
        1,
        "grant-delegated",
        "/mnt/source",
        HostMountAccess::ReadWrite,
        delegated_host.path().to_path_buf(),
    );
    let child = LiveNamespace::new(Namespace::new());
    service
        .register_child_process(
            Pid(1),
            Pid(2),
            child.clone(),
            &[selection(
                "grant-delegated",
                "/mnt/review",
                SpawnMountAccess::ReadOnly,
            )],
        )
        .unwrap();
    service
        .enqueue(HostMountRequest {
            id: "grant-nested".to_string(),
            label: "nested".to_string(),
            namespace_path: "/mnt/review/private".to_string(),
            access: HostMountAccess::ReadOnly,
            reason: "effective overlap test".to_string(),
            requesting_pid: 2,
        })
        .unwrap();

    let error = service
        .approve_export(
            "grant-nested",
            test_export(nested_host.path().to_path_buf()),
            "test-user",
            "test",
        )
        .unwrap_err();

    assert!(error.to_string().contains("overlaps"));
    assert!(service.grant_record("grant-nested").is_none());
    assert_eq!(
        child.snapshot().resolve("/mnt/review").unwrap().access,
        Access::ReadOnly
    );
}

#[test]
fn child_mount_targets_must_be_unique_normal_paths_below_mnt() {
    let requests = [
        vec![selection(
            "grant",
            "/srv/escape",
            SpawnMountAccess::ReadOnly,
        )],
        vec![
            selection("grant-a", "/mnt/project", SpawnMountAccess::ReadOnly),
            selection("grant-b", "/mnt/project/sub", SpawnMountAccess::ReadOnly),
        ],
        vec![
            selection("grant", "/mnt/a", SpawnMountAccess::ReadOnly),
            selection("grant", "/mnt/b", SpawnMountAccess::ReadOnly),
        ],
    ];

    for requested in requests {
        assert!(validate_child_mount_requests(&requested).is_err());
    }
}
