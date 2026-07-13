use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use alan_agent_engine::runtime::{
    ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator, MountGrantApplicatorFactory,
};
use alan_agent_engine::tools::{ToolExecutionAuthority, ToolExecutionBinding};
use alan_ap::{ErrorCode, FileServer, InProcessTransport};
use alan_kernel::{Access, LiveNamespace, Namespace, Pid};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::flat_fs::{FlatFileService, FlatServiceFs};

const FILES: &[(&str, bool)] = &[
    ("request", true),
    ("grants", false),
    ("status", false),
    ("audit", false),
    ("projection", true),
    ("revoke", true),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostMountAccess {
    ReadOnly,
    ReadWrite,
}

impl HostMountAccess {
    fn kernel(self) -> Access {
        match self {
            Self::ReadOnly => Access::ReadOnly,
            Self::ReadWrite => Access::ReadWrite,
        }
    }
}

/// Opaque Host-side export returned after native authorization.
///
/// The Service Manager can mount the file tree and ask the adapter to project
/// native Tool authority, but it never receives or stores the raw Host path.
pub trait HostMountExport: std::fmt::Debug + Send + Sync {
    fn file_tree(&self) -> InProcessTransport;
    fn apply_tool_authority(&self, binding: &mut ToolExecutionBinding) -> Result<()>;
}

/// Platform boundary used by runtime-approved mount requests.
///
/// Only the Host implementation may inspect the raw path carried by the legacy
/// engine approval object while that engine surface is being replaced.
pub trait HostMountExportAdapter: std::fmt::Debug + Send + Sync {
    fn export_approved(&self, grant: &ApprovedMountGrant) -> Result<Arc<dyn HostMountExport>>;
}

#[derive(Debug, Default)]
pub struct UnavailableHostMountExportAdapter;

impl HostMountExportAdapter for UnavailableHostMountExportAdapter {
    fn export_approved(&self, _grant: &ApprovedMountGrant) -> Result<Arc<dyn HostMountExport>> {
        anyhow::bail!("Host Mount export adapter is unavailable")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostMountRequest {
    pub id: String,
    pub label: String,
    pub namespace_path: String,
    pub access: HostMountAccess,
    pub reason: String,
    pub requesting_pid: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostMountGrantRecord {
    pub id: String,
    pub label: String,
    pub namespace_path: String,
    pub access: HostMountAccess,
    pub provenance: String,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize)]
struct AuditRecord {
    action: String,
    grant_id: String,
    actor: String,
    timestamp_ms: u128,
    affected_processes: Vec<u64>,
}

struct Projection {
    pid: Pid,
    namespace: LiveNamespace,
}

struct Grant {
    public: HostMountGrantRecord,
    owner: Pid,
    export: Arc<dyn HostMountExport>,
    projections: Vec<Projection>,
}

#[derive(Default)]
struct State {
    requests: BTreeMap<String, HostMountRequest>,
    grants: BTreeMap<String, Grant>,
    processes: BTreeMap<Pid, LiveNamespace>,
    audit: Vec<AuditRecord>,
    next_id: u64,
}

/// Alan OS-visible Host Mount authority. Raw Host paths remain private here.
pub struct HostMountService {
    adapter: Arc<dyn HostMountExportAdapter>,
    state: Mutex<State>,
}

impl std::fmt::Debug for HostMountService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("HostMountService")
    }
}

impl HostMountService {
    pub fn new(adapter: Arc<dyn HostMountExportAdapter>) -> Arc<Self> {
        Arc::new(Self {
            adapter,
            state: Mutex::new(State::default()),
        })
    }

    pub fn unavailable() -> Arc<Self> {
        Self::new(Arc::new(UnavailableHostMountExportAdapter))
    }

    pub fn file_server(self: &Arc<Self>) -> Arc<dyn FileServer> {
        Arc::new(FlatServiceFs::new(self.clone()))
    }

    pub fn register_process(&self, pid: Pid, namespace: LiveNamespace) {
        self.state.lock().unwrap().processes.insert(pid, namespace);
    }

    fn register_process_with_inherited_mounts(
        &self,
        pid: Pid,
        namespace: LiveNamespace,
        inherited_mount_paths: &[String],
    ) {
        let visible = namespace.snapshot();
        let mut state = self.state.lock().unwrap();
        state.processes.insert(pid, namespace.clone());
        let mut inherited = Vec::new();
        for (id, grant) in &mut state.grants {
            if grant.public.active
                && inherited_mount_paths
                    .iter()
                    .any(|path| path == &grant.public.namespace_path)
                && visible.resolve(&grant.public.namespace_path).is_ok()
                && !grant
                    .projections
                    .iter()
                    .any(|projection| projection.pid == pid)
            {
                grant.projections.push(Projection {
                    pid,
                    namespace: namespace.clone(),
                });
                inherited.push(id.clone());
            }
        }
        for id in inherited {
            audit(
                &mut state,
                "pass",
                &id,
                "service-manager".to_string(),
                vec![pid.0],
            );
        }
    }

    pub fn unregister_process(&self, pid: Pid) {
        let mut state = self.state.lock().unwrap();
        state.processes.remove(&pid);
        for grant in state.grants.values_mut() {
            grant.projections.retain(|projection| projection.pid != pid);
        }
    }

    pub fn pending_request(&self, request_id: &str) -> Option<HostMountRequest> {
        self.state.lock().unwrap().requests.get(request_id).cloned()
    }

    pub fn approve_export(
        &self,
        request_id: &str,
        export: Arc<dyn HostMountExport>,
        provenance: impl Into<String>,
        actor: impl Into<String>,
    ) -> Result<HostMountGrantRecord> {
        let mut state = self.state.lock().unwrap();
        let request = state
            .requests
            .remove(request_id)
            .with_context(|| format!("unknown Host Mount request `{request_id}`"))?;
        let record = HostMountGrantRecord {
            id: request.id.clone(),
            label: request.label,
            namespace_path: request.namespace_path,
            access: request.access,
            provenance: provenance.into(),
            active: true,
        };
        state.grants.insert(
            record.id.clone(),
            Grant {
                public: record.clone(),
                owner: Pid(request.requesting_pid),
                export,
                projections: Vec::new(),
            },
        );
        audit(&mut state, "approve", &record.id, actor.into(), Vec::new());
        drop(state);
        self.project(&record.id, request.requesting_pid)?;
        Ok(record)
    }

    pub fn project(&self, grant_id: &str, pid: u64) -> Result<()> {
        let pid = Pid(pid);
        let mut state = self.state.lock().unwrap();
        let namespace = state
            .processes
            .get(&pid)
            .cloned()
            .with_context(|| format!("unknown Process {pid:?}"))?;
        let grant = state
            .grants
            .get_mut(grant_id)
            .with_context(|| format!("unknown Host Mount grant `{grant_id}`"))?;
        ensure!(grant.public.active, "Host Mount grant is revoked");
        ensure!(
            grant.owner == pid,
            "Host Mount grant belongs to another Process"
        );
        if grant
            .projections
            .iter()
            .any(|projection| projection.pid == pid)
        {
            return Ok(());
        }
        namespace.replace_mount(
            &grant.public.namespace_path,
            grant.export.file_tree(),
            grant.public.access.kernel(),
        );
        grant.projections.push(Projection { pid, namespace });
        audit(
            &mut state,
            "project",
            grant_id,
            "service-manager".to_string(),
            vec![pid.0],
        );
        Ok(())
    }

    pub fn revoke(&self, grant_id: &str, actor: impl Into<String>) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let grant = state
            .grants
            .get_mut(grant_id)
            .with_context(|| format!("unknown Host Mount grant `{grant_id}`"))?;
        ensure!(grant.public.active, "Host Mount grant is already revoked");
        grant.public.active = false;
        let affected = grant
            .projections
            .iter()
            .map(|projection| {
                projection.namespace.unmount(&grant.public.namespace_path);
                projection.pid.0
            })
            .collect::<Vec<_>>();
        audit(&mut state, "revoke", grant_id, actor.into(), affected);
        Ok(())
    }

    fn enqueue(&self, request: HostMountRequest) -> Result<String> {
        self.enqueue_inner(request, false)
    }

    fn enqueue_approved_definition(&self, request: HostMountRequest) -> Result<String> {
        self.enqueue_inner(request, true)
    }

    fn enqueue_inner(
        &self,
        mut request: HostMountRequest,
        allow_agent_definition: bool,
    ) -> Result<String> {
        validate_request(&request, allow_agent_definition)?;
        let mut state = self.state.lock().unwrap();
        if request.id.is_empty() {
            state.next_id += 1;
            request.id = format!("grant-{}", state.next_id);
        }
        ensure!(
            !state.requests.contains_key(&request.id) && !state.grants.contains_key(&request.id),
            "duplicate Host Mount id `{}`",
            request.id
        );
        let id = request.id.clone();
        state.requests.insert(id.clone(), request);
        audit(
            &mut state,
            "request",
            &id,
            "process".to_string(),
            Vec::new(),
        );
        Ok(id)
    }
}

impl ToolExecutionAuthority for HostMountService {
    fn reconcile(
        &self,
        pid: Pid,
        mut binding: ToolExecutionBinding,
    ) -> Result<ToolExecutionBinding> {
        let state = self.state.lock().unwrap();
        let managed_paths = state
            .grants
            .values()
            .filter(|grant| {
                grant
                    .projections
                    .iter()
                    .any(|projection| projection.pid == pid)
            })
            .map(|grant| grant.public.namespace_path.clone())
            .collect::<Vec<_>>();
        let exports = state
            .grants
            .values()
            .filter(|grant| {
                grant.public.active
                    && grant
                        .projections
                        .iter()
                        .any(|projection| projection.pid == pid)
            })
            .map(|grant| grant.export.clone())
            .collect::<Vec<_>>();
        drop(state);

        binding.remove_host_mount_paths(&managed_paths)?;
        for export in exports {
            export.apply_tool_authority(&mut binding)?;
        }
        ensure!(
            !binding.host_mounts.is_empty(),
            "Tool Process has no active Host Mount"
        );
        Ok(binding)
    }
}

#[async_trait::async_trait]
impl FlatFileService for HostMountService {
    fn files(&self) -> &'static [(&'static str, bool)] {
        FILES
    }

    fn read(&self, name: &str) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().unwrap();
        let rendered = match name {
            "request" => serde_json::to_string(&state.requests.values().collect::<Vec<_>>()),
            "grants" => serde_json::to_string(
                &state
                    .grants
                    .values()
                    .map(|grant| &grant.public)
                    .collect::<Vec<_>>(),
            ),
            "status" => Ok(format!(
                "requests={} grants={} active={}\n",
                state.requests.len(),
                state.grants.len(),
                state
                    .grants
                    .values()
                    .filter(|grant| grant.public.active)
                    .count()
            )),
            "audit" => serde_json::to_string(&state.audit),
            "projection" => Ok("write {\"grant_id\":\"...\",\"pid\":1}\n".to_string()),
            "revoke" => Ok("write a grant id\n".to_string()),
            _ => return Err(ErrorCode::NotFound),
        }
        .map_err(|_| ErrorCode::Io)?;
        Ok(rendered.into_bytes())
    }

    async fn commit(&self, name: &str, bytes: &[u8]) -> Result<(), ErrorCode> {
        match name {
            "request" => {
                let request = serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
                self.enqueue(request).map_err(|_| ErrorCode::BadRequest)?;
                Ok(())
            }
            "projection" => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Command {
                    grant_id: String,
                    pid: u64,
                }
                let command: Command =
                    serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
                self.project(&command.grant_id, command.pid)
                    .map_err(|_| ErrorCode::BadRequest)
            }
            "revoke" => {
                let id = std::str::from_utf8(bytes)
                    .map_err(|_| ErrorCode::BadRequest)?
                    .trim();
                self.revoke(id, "file-control")
                    .map_err(|_| ErrorCode::BadRequest)
            }
            _ => Err(ErrorCode::NoAccess),
        }
    }
}

#[derive(Debug)]
pub struct HostMountApplicatorFactory {
    service: Arc<HostMountService>,
}

impl HostMountApplicatorFactory {
    pub fn new(service: Arc<HostMountService>) -> Self {
        Self { service }
    }
}

impl MountGrantApplicatorFactory for HostMountApplicatorFactory {
    fn create(
        &self,
        pid: Pid,
        live_namespace: LiveNamespace,
        inherited_mount_paths: &[String],
    ) -> Arc<dyn MountGrantApplicator> {
        self.service.register_process_with_inherited_mounts(
            pid,
            live_namespace.clone(),
            inherited_mount_paths,
        );
        Arc::new(HostMountApplicator {
            service: self.service.clone(),
            pid,
            live_namespace,
        })
    }

    fn tool_execution_authority(&self) -> Option<Arc<dyn ToolExecutionAuthority>> {
        Some(self.service.clone())
    }
}

#[derive(Debug)]
struct HostMountApplicator {
    service: Arc<HostMountService>,
    pid: Pid,
    live_namespace: LiveNamespace,
}

impl MountGrantApplicator for HostMountApplicator {
    fn apply_mount_grant(&self, grant: &ApprovedMountGrant) -> Result<Namespace> {
        self.service
            .register_process(self.pid, self.live_namespace.clone());
        let id = self.service.enqueue_approved_definition(HostMountRequest {
            id: String::new(),
            label: grant.reason.clone(),
            namespace_path: grant.namespace_path.clone(),
            access: match grant.access {
                ApprovedMountGrantAccess::ReadOnly => HostMountAccess::ReadOnly,
                ApprovedMountGrantAccess::ReadWrite => HostMountAccess::ReadWrite,
            },
            reason: grant.reason.clone(),
            requesting_pid: self.pid.0,
        })?;
        let export = self.service.adapter.export_approved(grant)?;
        self.service
            .approve_export(&id, export, "runtime-approval", "host-adapter")?;
        Ok(self.live_namespace.snapshot())
    }
}

fn validate_request(request: &HostMountRequest, allow_agent_definition: bool) -> Result<()> {
    ensure!(
        !request.label.trim().is_empty(),
        "Host Mount label is empty"
    );
    ensure!(
        (request.namespace_path.starts_with("/mnt/")
            || (allow_agent_definition && request.namespace_path == "/agent-definition"))
            && !request
                .namespace_path
                .split('/')
                .any(|component| matches!(component, "." | "..")),
        "Host Mount namespace path must be below /mnt"
    );
    ensure!(
        request.requesting_pid > 0,
        "requesting PID must be positive"
    );
    Ok(())
}

fn audit(
    state: &mut State,
    action: &str,
    grant_id: &str,
    actor: String,
    affected_processes: Vec<u64>,
) {
    state.audit.push(AuditRecord {
        action: action.to_string(),
        grant_id: grant_id.to_string(),
        actor,
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        affected_processes,
    });
}

#[cfg(test)]
mod tests {
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

        fn apply_tool_authority(&self, binding: &mut ToolExecutionBinding) -> Result<()> {
            binding.apply_host_mount(self.grant.clone())
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
        export.apply_tool_authority(&mut cached).unwrap();
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
        let factory = HostMountApplicatorFactory::new(service.clone());
        factory.create(Pid(2), child.clone(), &["/mnt/data".to_string()]);
        let mut cached =
            ToolExecutionBinding::new(host.path().to_path_buf(), host.path().join("scratch"));
        export.apply_tool_authority(&mut cached).unwrap();
        assert!(service.reconcile(Pid(2), cached.clone()).is_ok());

        service.revoke(&id, "tester").unwrap();
        assert!(parent.snapshot().resolve("/mnt/data").is_err());
        assert!(child.snapshot().resolve("/mnt/data").is_err());
        assert!(service.reconcile(Pid(2), cached).is_err());
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
}
