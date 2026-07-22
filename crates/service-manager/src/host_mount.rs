use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    any::Any,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use alan_agent_engine::SpawnHostMount;
use alan_agent_engine::tools::{
    ToolExecutionAdapter, ToolExecutionAuthority, ToolExecutionBinding,
};
use alan_ap::{ErrorCode, FileServer, InProcessTransport};
use alan_kernel::{Access, LiveNamespace, Pid};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

mod delegation;
mod file_server;

use delegation::{ensure_no_ambient_child_projections, resolve_child_delegations};
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
    fn as_any(&self) -> &dyn Any;
}

/// One service-owned projection passed opaquely back to its Host adapter.
#[derive(Clone)]
pub struct HostMountToolProjection {
    pub namespace_path: PathBuf,
    pub access: HostMountAccess,
    export: Arc<dyn HostMountExport>,
}

impl std::fmt::Debug for HostMountToolProjection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostMountToolProjection")
            .field("namespace_path", &self.namespace_path)
            .field("access", &self.access)
            .finish_non_exhaustive()
    }
}

impl HostMountToolProjection {
    pub fn export(&self) -> &Arc<dyn HostMountExport> {
        &self.export
    }
}

/// Host boundary that turns service-owned opaque exports into Tool Process authority.
pub trait HostMountExportAdapter: std::fmt::Debug + Send + Sync {
    fn tool_execution_adapter(
        &self,
        projections: &[HostMountToolProjection],
        requested_namespace_cwd: &Path,
    ) -> Result<Arc<dyn ToolExecutionAdapter>>;
}

#[derive(Debug, Default)]
pub struct UnavailableHostMountExportAdapter;

impl HostMountExportAdapter for UnavailableHostMountExportAdapter {
    fn tool_execution_adapter(
        &self,
        _projections: &[HostMountToolProjection],
        _requested_namespace_cwd: &Path,
    ) -> Result<Arc<dyn ToolExecutionAdapter>> {
        anyhow::bail!("Host Mount Tool adapter is unavailable")
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
    namespace_path: String,
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

    /// Register a child Process and project only handles explicitly delegated by its parent.
    pub fn register_child_process(
        &self,
        parent_pid: Pid,
        pid: Pid,
        namespace: LiveNamespace,
        requested: &[SpawnHostMount],
    ) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        ensure!(
            !state.processes.contains_key(&pid),
            "Process {pid:?} is already registered with Host Mount Service"
        );
        ensure_no_ambient_child_projections(&state, parent_pid, &namespace.snapshot())?;
        let delegated = resolve_child_delegations(&state, parent_pid, requested)?;

        state.processes.insert(pid, namespace.clone());
        for projection in delegated {
            namespace.replace_mount(
                &projection.target,
                projection.export.file_tree(),
                projection.access.kernel(),
            );
            state
                .grants
                .get_mut(&projection.grant_id)
                .expect("validated delegated grant remains retained")
                .projections
                .push(Projection {
                    pid,
                    namespace: namespace.clone(),
                    namespace_path: projection.target,
                    access: projection.access,
                });
            audit(
                &mut state,
                "pass",
                &projection.grant_id,
                "service-manager".to_string(),
                vec![pid.0],
            );
        }
        Ok(())
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
        ensure!(
            !state.grants.iter().any(|(id, existing)| {
                id != grant_id
                    && existing.public.active
                    && existing.projections.iter().any(|projection| {
                        projection.pid == pid
                            && namespace_paths_strictly_overlap(
                                &namespace_path,
                                &projection.namespace_path,
                            )
                    })
            }),
            "Host Mount namespace path overlaps an active Process projection"
        );
        for (id, grant) in &mut state.grants {
            if id != grant_id {
                grant.projections.retain(|projection| {
                    projection.pid != pid || projection.namespace_path != namespace_path
                });
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
            namespace_path,
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
                projection.namespace.unmount(&projection.namespace_path);
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
        let mut request = request;
        validate_request(&request)?;
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
        pid: u64,
        mut binding: ToolExecutionBinding,
    ) -> Result<ToolExecutionBinding> {
        let pid = Pid(pid);
        let carried_host_mount_authority = binding.has_adapter();
        let requested_namespace_cwd = binding.namespace_cwd.clone();
        let state = self.state.lock().unwrap();
        let projections = state
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
                    .map(|projection| HostMountToolProjection {
                        namespace_path: PathBuf::from(&projection.namespace_path),
                        access: projection.access,
                        export: grant.export.clone(),
                    })
            })
            .collect::<Vec<_>>();
        drop(state);

        binding.clear_adapter();
        if !projections.is_empty() {
            binding.set_adapter(
                self.adapter
                    .tool_execution_adapter(&projections, &requested_namespace_cwd)?,
            );
        }
        // Fail closed only if reconciliation removed cached Host Mount authority.
        ensure!(
            !carried_host_mount_authority || binding.has_adapter(),
            "Tool Process has no active Host Mount"
        );
        Ok(binding)
    }
}

fn validate_request(request: &HostMountRequest) -> Result<()> {
    ensure!(
        !request.label.trim().is_empty(),
        "Host Mount label is empty"
    );
    validate_mount_namespace_path(&request.namespace_path)?;
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

fn validate_mount_namespace_path(path: &str) -> Result<()> {
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

fn namespace_paths_strictly_overlap(left: &str, right: &str) -> bool {
    left != right
        && (std::path::Path::new(left).starts_with(right)
            || std::path::Path::new(right).starts_with(left))
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
