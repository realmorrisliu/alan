use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use alan_agent_engine::runtime::{
    ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator, MountGrantApplicatorFactory,
};
use alan_agent_engine::tools::{ToolExecutionAuthority, ToolExecutionBinding};
use alan_ap::{ErrorCode, FileServer, InProcessTransport};
use alan_kernel::{Access, LiveNamespace, Namespace, Pid};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

mod file_server;

use file_server::{HostMountEventStreams, HostMountFs};

const RESERVED_MOUNT_NAMESPACE_ROOTS: &[&str] = &[
    "connections",
    "host-mount",
    "llm",
    "mem",
    "package",
    "route",
    "service-manager",
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
    /// Add native Tool authority at the Process projection's effective access.
    fn apply_tool_authority(
        &self,
        effective_access: HostMountAccess,
        binding: &mut ToolExecutionBinding,
    ) -> Result<()>;
}

/// Transitional platform boundary for pre-authorized launch declarations.
///
/// Logical runtime requests use [`HostMountExport`] directly after native authorization. Only the
/// Host implementation may inspect the raw path carried by the launch-only engine record while
/// that record is replaced with a service-issued handle.
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
#[serde(deny_unknown_fields)]
struct HostMountRequestDocument {
    namespace_path: String,
    access: HostMountAccess,
    reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostMountStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
    Failed,
}

impl HostMountStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
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
    access: HostMountAccess,
}

struct Grant {
    public: HostMountGrantRecord,
    owner: Pid,
    export: Arc<dyn HostMountExport>,
    projections: Vec<Projection>,
}

struct RequestState {
    request: HostMountRequest,
    status: HostMountStatus,
    decision_in_progress: bool,
    grant: Option<String>,
    error: Option<String>,
}

#[derive(Clone)]
pub(super) struct HostMountRequestSnapshot {
    pub(super) request: HostMountRequest,
    pub(super) status: HostMountStatus,
    pub(super) grant: Option<String>,
    pub(super) error: Option<String>,
}

#[derive(Default)]
struct State {
    requests: BTreeMap<String, RequestState>,
    grants: BTreeMap<String, Grant>,
    processes: BTreeMap<Pid, LiveNamespace>,
    audit: Vec<AuditRecord>,
    next_id: u64,
}

/// Alan OS-visible Host Mount authority. Raw Host paths remain private here.
pub struct HostMountService {
    adapter: Arc<dyn HostMountExportAdapter>,
    state: Mutex<State>,
    generation: AtomicU32,
    request_events: HostMountEventStreams,
    events: HostMountEventStreams,
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
            generation: AtomicU32::new(0),
            request_events: HostMountEventStreams::new(),
            events: HostMountEventStreams::new(),
        })
    }

    pub fn unavailable() -> Arc<Self> {
        Self::new(Arc::new(UnavailableHostMountExportAdapter))
    }

    pub fn file_server(self: &Arc<Self>) -> Arc<dyn FileServer> {
        Arc::new(HostMountFs::new(self.clone(), None))
    }

    /// Return the Process-scoped request view mounted into one Agent Process.
    pub fn file_server_for_process(self: &Arc<Self>, pid: u64) -> Arc<dyn FileServer> {
        Arc::new(HostMountFs::new(self.clone(), Some(pid)))
    }

    pub fn register_process(&self, pid: Pid, namespace: LiveNamespace) {
        self.state.lock().unwrap().processes.insert(pid, namespace);
    }

    fn register_process_with_inherited_grants(
        &self,
        pid: Pid,
        namespace: LiveNamespace,
        inherited_from: Option<Pid>,
        inherited_grant_references: &[String],
    ) {
        let visible = namespace.snapshot();
        let mut state = self.state.lock().unwrap();
        state.processes.insert(pid, namespace.clone());
        let Some(inherited_from) = inherited_from else {
            return;
        };
        let mut inherited = Vec::new();
        for id in inherited_grant_references {
            let Some(grant) = state.grants.get_mut(id) else {
                continue;
            };
            let visible_access = visible
                .union_at(&grant.public.namespace_path)
                .last()
                .map(|mount| mount.access);
            if grant.public.active
                && grant
                    .projections
                    .iter()
                    .any(|projection| projection.pid == inherited_from)
                && visible_access.is_some_and(|access| {
                    grant.public.access.kernel() == Access::ReadWrite || access == Access::ReadOnly
                })
                && !grant
                    .projections
                    .iter()
                    .any(|projection| projection.pid == pid)
            {
                let access = match visible_access.expect("validated exact mount access") {
                    Access::ReadOnly => HostMountAccess::ReadOnly,
                    Access::ReadWrite => grant.public.access,
                };
                grant.projections.push(Projection {
                    pid,
                    namespace: namespace.clone(),
                    access,
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
        self.state
            .lock()
            .unwrap()
            .requests
            .get(request_id)
            .filter(|request| {
                request.status == HostMountStatus::Pending && !request.decision_in_progress
            })
            .map(|request| request.request.clone())
    }

    pub fn approve_export(
        &self,
        request_id: &str,
        export: Arc<dyn HostMountExport>,
        provenance: impl Into<String>,
        actor: impl Into<String>,
    ) -> Result<HostMountGrantRecord> {
        let actor = actor.into();
        let request = self.claim_pending_request(request_id)?;
        let record = HostMountGrantRecord {
            id: request.id.clone(),
            label: request.label.clone(),
            namespace_path: request.namespace_path.clone(),
            access: request.access,
            provenance: provenance.into(),
            active: true,
        };
        let mut state = self.state.lock().unwrap();
        state.grants.insert(
            record.id.clone(),
            Grant {
                public: record.clone(),
                owner: Pid(request.requesting_pid),
                export,
                projections: Vec::new(),
            },
        );
        drop(state);
        if let Err(error) = self.project(&record.id, request.requesting_pid) {
            let mut state = self.state.lock().unwrap();
            state.grants.remove(&record.id);
            if let Some(request) = state.requests.get_mut(&record.id) {
                request.status = HostMountStatus::Failed;
                request.error = Some("Host Mount namespace projection failed".to_string());
            }
            audit(
                &mut state,
                "approval_failed",
                &record.id,
                "service-manager".to_string(),
                Vec::new(),
            );
            drop(state);
            self.bump_generation();
            self.append_request_event(
                &record.id,
                HostMountStatus::Failed,
                None,
                Some("Host Mount namespace projection failed"),
            );
            return Err(error);
        }
        let mut state = self.state.lock().unwrap();
        let request_state = state
            .requests
            .get_mut(&record.id)
            .expect("approved request remains retained");
        request_state.status = HostMountStatus::Approved;
        request_state.grant = Some(record.id.clone());
        audit(&mut state, "approve", &record.id, actor, Vec::new());
        drop(state);
        self.bump_generation();
        self.append_request_event(
            &record.id,
            HostMountStatus::Approved,
            Some(&record.id),
            None,
        );
        Ok(record)
    }

    fn claim_pending_request(&self, request_id: &str) -> Result<HostMountRequest> {
        let mut state = self.state.lock().unwrap();
        let request = state
            .requests
            .get_mut(request_id)
            .with_context(|| format!("unknown Host Mount request `{request_id}`"))?;
        ensure!(
            request.status == HostMountStatus::Pending && !request.decision_in_progress,
            "Host Mount request `{request_id}` is already settled or being settled"
        );
        request.decision_in_progress = true;
        Ok(request.request.clone())
    }

    pub fn reject_request(
        &self,
        request_id: &str,
        reason: impl Into<String>,
        actor: impl Into<String>,
    ) -> Result<()> {
        self.set_terminal_request(
            request_id,
            HostMountStatus::Rejected,
            reason.into(),
            actor.into(),
        )
    }

    pub fn cancel_request(
        &self,
        request_id: &str,
        reason: impl Into<String>,
        actor: impl Into<String>,
    ) -> Result<()> {
        self.set_terminal_request(
            request_id,
            HostMountStatus::Cancelled,
            reason.into(),
            actor.into(),
        )
    }

    pub fn fail_request(
        &self,
        request_id: &str,
        reason: impl Into<String>,
        actor: impl Into<String>,
    ) -> Result<()> {
        self.set_terminal_request(
            request_id,
            HostMountStatus::Failed,
            reason.into(),
            actor.into(),
        )
    }

    fn set_terminal_request(
        &self,
        request_id: &str,
        status: HostMountStatus,
        reason: String,
        actor: String,
    ) -> Result<()> {
        ensure!(status.is_terminal(), "terminal request status is required");
        let reason = concise_terminal_reason(&reason);
        let mut state = self.state.lock().unwrap();
        let request = state
            .requests
            .get_mut(request_id)
            .with_context(|| format!("unknown Host Mount request `{request_id}`"))?;
        ensure!(
            request.status == HostMountStatus::Pending && !request.decision_in_progress,
            "Host Mount request `{request_id}` is already settled or being settled"
        );
        request.decision_in_progress = true;
        request.status = status;
        request.error = Some(reason.clone());
        audit(&mut state, status.as_str(), request_id, actor, Vec::new());
        drop(state);
        self.bump_generation();
        self.append_request_event(request_id, status, None, Some(&reason));
        Ok(())
    }

    pub fn project(&self, grant_id: &str, pid: u64) -> Result<()> {
        let pid = Pid(pid);
        let mut state = self.state.lock().unwrap();
        let namespace = state
            .processes
            .get(&pid)
            .cloned()
            .with_context(|| format!("unknown Process {pid:?}"))?;
        let (namespace_path, access, export) = {
            let grant = state
                .grants
                .get(grant_id)
                .with_context(|| format!("unknown Host Mount grant `{grant_id}`"))?;
            ensure!(grant.public.active, "Host Mount grant is revoked");
            ensure!(
                grant.owner == pid,
                "Host Mount grant belongs to another Process"
            );
            (
                grant.public.namespace_path.clone(),
                grant.public.access,
                grant.export.clone(),
            )
        };
        for (id, grant) in &mut state.grants {
            if id != grant_id && grant.public.namespace_path == namespace_path {
                grant.projections.retain(|projection| projection.pid != pid);
            }
        }
        namespace.replace_mount(&namespace_path, export.file_tree(), access.kernel());
        let grant = state
            .grants
            .get_mut(grant_id)
            .expect("validated Host Mount grant remains retained");
        if grant
            .projections
            .iter()
            .any(|projection| projection.pid == pid)
        {
            return Ok(());
        }
        grant.projections.push(Projection {
            pid,
            namespace,
            access,
        });
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
        audit(
            &mut state,
            "revoke",
            grant_id,
            actor.into(),
            affected.clone(),
        );
        drop(state);
        self.bump_generation();
        self.append_service_event(
            serde_json::json!({
                "type": "grant_revoked",
                "grant_id": grant_id,
            }),
            &affected,
        );
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
        state.requests.insert(
            id.clone(),
            RequestState {
                request,
                status: HostMountStatus::Pending,
                decision_in_progress: false,
                grant: None,
                error: None,
            },
        );
        audit(
            &mut state,
            "request",
            &id,
            "process".to_string(),
            Vec::new(),
        );
        drop(state);
        self.bump_generation();
        self.append_request_event(&id, HostMountStatus::Pending, None, None);
        Ok(id)
    }

    pub(super) fn allocate_request_id(&self) -> String {
        let mut state = self.state.lock().unwrap();
        state.next_id += 1;
        format!("request-{}", state.next_id)
    }

    pub(super) fn commit_request(
        &self,
        requesting_pid: u64,
        request_id: String,
        bytes: &[u8],
    ) -> Result<(), ErrorCode> {
        let document: HostMountRequestDocument =
            serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let label = match document.label {
            Some(label) => {
                let label = label.trim();
                if label.is_empty() {
                    return Err(ErrorCode::BadRequest);
                }
                label.to_string()
            }
            None => default_label(&document.namespace_path),
        };
        let request = HostMountRequest {
            id: request_id,
            label,
            namespace_path: document.namespace_path,
            access: document.access,
            reason: document.reason.trim().to_string(),
            requesting_pid,
        };
        self.enqueue(request).map_err(|_| ErrorCode::BadRequest)?;
        Ok(())
    }

    pub(super) fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn has_request(&self, id: &str) -> bool {
        self.state.lock().unwrap().requests.contains_key(id)
    }

    pub(super) fn request_is_visible_to(&self, id: &str, pid: Option<u64>) -> bool {
        self.state
            .lock()
            .unwrap()
            .requests
            .get(id)
            .is_some_and(|request| pid.is_none_or(|pid| request.request.requesting_pid == pid))
    }

    pub(super) fn request_ids_visible_to(&self, pid: Option<u64>) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|(_, request)| pid.is_none_or(|pid| request.request.requesting_pid == pid))
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub(super) fn request_snapshot(&self, id: &str) -> Option<HostMountRequestSnapshot> {
        self.state
            .lock()
            .unwrap()
            .requests
            .get(id)
            .map(|request| HostMountRequestSnapshot {
                request: request.request.clone(),
                status: request.status,
                grant: request.grant.clone(),
                error: request.error.clone(),
            })
    }

    pub(super) fn grant_is_visible_to(&self, id: &str, pid: Option<u64>) -> bool {
        self.state
            .lock()
            .unwrap()
            .grants
            .get(id)
            .is_some_and(|grant| {
                pid.is_none_or(|pid| {
                    grant.owner.0 == pid
                        || grant
                            .projections
                            .iter()
                            .any(|projection| projection.pid.0 == pid)
                })
            })
    }

    pub(super) fn grant_ids_visible_to(&self, pid: Option<u64>) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .grants
            .iter()
            .filter(|(_, grant)| {
                pid.is_none_or(|pid| {
                    grant.owner.0 == pid
                        || grant
                            .projections
                            .iter()
                            .any(|projection| projection.pid.0 == pid)
                })
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub(super) fn grant_record(&self, id: &str) -> Option<HostMountGrantRecord> {
        self.state
            .lock()
            .unwrap()
            .grants
            .get(id)
            .map(|grant| grant.public.clone())
    }

    fn request_events(&self, pid: Option<u64>) -> file_server::HostMountEventStream {
        self.request_events.stream(pid)
    }

    fn events(&self, pid: Option<u64>) -> file_server::HostMountEventStream {
        self.events.stream(pid)
    }

    fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn append_request_event(
        &self,
        request_id: &str,
        status: HostMountStatus,
        grant: Option<&str>,
        error: Option<&str>,
    ) {
        let event = serde_json::json!({
            "type": "request_status",
            "request_id": request_id,
            "status": status,
            "grant": grant,
            "error": error,
        });
        let mut bytes = serde_json::to_vec(&event).expect("Host Mount event serializes");
        bytes.push(b'\n');
        let requesting_pid = self
            .state
            .lock()
            .unwrap()
            .requests
            .get(request_id)
            .expect("Host Mount request remains retained")
            .request
            .requesting_pid;
        self.request_events.append_for(requesting_pid, &bytes);
        self.events.append_for(requesting_pid, &bytes);
    }

    fn append_service_event(&self, event: serde_json::Value, affected_processes: &[u64]) {
        let mut bytes = serde_json::to_vec(&event).expect("Host Mount event serializes");
        bytes.push(b'\n');
        self.events.append_for_many(affected_processes, &bytes);
    }
}

impl ToolExecutionAuthority for HostMountService {
    fn reconcile(
        &self,
        pid: Pid,
        mut binding: ToolExecutionBinding,
    ) -> Result<ToolExecutionBinding> {
        let carried_host_mount_authority = !binding.host_mounts.is_empty();
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
            .filter_map(|grant| {
                if !grant.public.active {
                    return None;
                }
                grant
                    .projections
                    .iter()
                    .find(|projection| projection.pid == pid)
                    .map(|projection| (grant.export.clone(), projection.access))
            })
            .collect::<Vec<_>>();
        drop(state);

        binding.remove_host_mount_paths(&managed_paths)?;
        for (export, effective_access) in exports {
            export.apply_tool_authority(effective_access, &mut binding)?;
        }
        // An empty seed is a valid zero-authority binding for mount-free Tools. Fail closed only
        // when reconciliation removed Host Mount authority that the cached binding carried.
        ensure!(
            !carried_host_mount_authority || !binding.host_mounts.is_empty(),
            "Tool Process has no active Host Mount"
        );
        Ok(binding)
    }
}

#[derive(Debug)]
pub struct HostMountApplicatorFactory {
    service: Arc<HostMountService>,
    inherited_from: Option<Pid>,
}

impl HostMountApplicatorFactory {
    pub fn new(service: Arc<HostMountService>) -> Self {
        Self {
            service,
            inherited_from: None,
        }
    }

    /// Create a factory that may delegate grants currently projected to `parent_pid`.
    pub fn inheriting_from(service: Arc<HostMountService>, parent_pid: Pid) -> Self {
        Self {
            service,
            inherited_from: Some(parent_pid),
        }
    }
}

impl MountGrantApplicatorFactory for HostMountApplicatorFactory {
    fn create(
        &self,
        pid: Pid,
        live_namespace: LiveNamespace,
        inherited_mount_references: &[String],
    ) -> Arc<dyn MountGrantApplicator> {
        self.service.register_process_with_inherited_grants(
            pid,
            live_namespace.clone(),
            self.inherited_from,
            inherited_mount_references,
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
    validate_mount_namespace_path(&request.namespace_path, allow_agent_definition)?;
    ensure!(
        request.requesting_pid > 0,
        "requesting PID must be positive"
    );
    ensure!(
        !request.reason.trim().is_empty(),
        "Host Mount reason is empty"
    );
    Ok(())
}

fn validate_mount_namespace_path(path: &str, allow_agent_definition: bool) -> Result<()> {
    if allow_agent_definition && path == "/agent-definition" {
        return Ok(());
    }
    ensure!(
        path == path.trim(),
        "Host Mount namespace path is not normalized"
    );
    let suffix = path
        .strip_prefix("/mnt/")
        .context("Host Mount namespace path must be below /mnt")?;
    let components = suffix.split('/').collect::<Vec<_>>();
    ensure!(
        !components.is_empty()
            && components
                .iter()
                .all(|component| !component.is_empty() && !matches!(*component, "." | "..")),
        "Host Mount namespace path is not normalized"
    );
    ensure!(
        !RESERVED_MOUNT_NAMESPACE_ROOTS.contains(&components[0]),
        "Host Mount namespace path targets a reserved mount root"
    );
    Ok(())
}

fn default_label(namespace_path: &str) -> String {
    namespace_path
        .rsplit('/')
        .find(|component| !component.is_empty())
        .unwrap_or("Host Mount")
        .to_string()
}

fn concise_terminal_reason(reason: &str) -> String {
    let trimmed = reason.trim();
    let concise = if trimmed.is_empty() {
        "Host Mount request was not approved"
    } else {
        trimmed
    };
    concise.chars().take(512).collect()
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
#[path = "host_mount/tests.rs"]
mod tests;
