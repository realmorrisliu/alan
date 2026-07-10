use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_TERMINAL_CHILD_RUN_RECORDS: usize = 256;
const MAX_CHILD_RUN_WARNINGS: usize = 32;
const MAX_CHILD_RUN_WARNING_CHARS: usize = 512;
const MAX_CHILD_RUN_STATUS_SUMMARY_CHARS: usize = 512;
const MAX_CHILD_RUN_ERROR_MESSAGE_CHARS: usize = 1_000;
const MAX_CHILD_RUN_TERMINATION_ACTOR_CHARS: usize = 120;
const MAX_CHILD_RUN_TERMINATION_REASON_CHARS: usize = 512;

/// Lifecycle state for a delegated child runtime launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildRunStatus {
    Starting,
    Running,
    Blocked,
    Completed,
    Failed,
    TimedOut,
    Terminating,
    Terminated,
    Cancelled,
}

impl ChildRunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::TimedOut | Self::Terminated | Self::Cancelled
        )
    }
}

/// Requested child termination mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildRunTerminationMode {
    Graceful,
    Forceful,
}

/// Audit metadata for an explicit child termination request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildRunTerminationRequest {
    pub actor: String,
    pub reason: String,
    pub mode: ChildRunTerminationMode,
    pub requested_at_ms: u64,
}

/// Observable lifecycle record for one delegated child runtime launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildRunRecord {
    pub id: String,
    pub parent_session_id: String,
    pub child_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_ref: Option<crate::skills::DelegatedSkillOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_target: Option<String>,
    pub status: ChildRunStatus,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_heartbeat_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_progress_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_event_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_status_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub termination: Option<ChildRunTerminationRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_capability_decision: Option<alan_agent_protocol::DelegatedCapabilityDecision>,
}

impl ChildRunRecord {
    pub fn new(
        id: String,
        parent_session_id: String,
        child_session_id: String,
        workspace_root: Option<String>,
        process_path: Option<String>,
        launch_target: Option<String>,
    ) -> Self {
        let now = now_ms();
        Self {
            id,
            parent_session_id,
            child_session_id,
            workspace_root,
            process_path,
            state_ref: None,
            launch_target,
            status: ChildRunStatus::Starting,
            created_at_ms: now,
            updated_at_ms: now,
            latest_heartbeat_at_ms: None,
            latest_progress_at_ms: None,
            latest_event_kind: None,
            latest_status_summary: None,
            warnings: Vec::new(),
            error_message: None,
            termination: None,
            delegation_capability_decision: None,
        }
    }

    /// Attach bounded delegation requirements and the derived namespace
    /// summary to this historical launch record.
    pub fn with_delegation_capability_decision(
        mut self,
        decision: alan_agent_protocol::DelegatedCapabilityDecision,
    ) -> Self {
        self.delegation_capability_decision = Some(decision);
        self
    }

    pub fn with_state_ref(mut self, state_ref: crate::skills::DelegatedSkillOutputRef) -> Self {
        self.state_ref = Some(state_ref);
        self
    }
}

/// Process-local registry for active and recently terminal child runs.
#[derive(Debug, Clone, Default)]
pub struct ChildRunRegistry {
    inner: Arc<RwLock<HashMap<String, ChildRunRecord>>>,
}

impl ChildRunRegistry {
    pub fn register(&self, record: ChildRunRecord) {
        let mut records = self.inner.write().expect("child run registry poisoned");
        records.insert(record.id.clone(), record);
    }

    pub fn list_for_parent(&self, parent_session_id: &str) -> Vec<ChildRunRecord> {
        let mut records = self
            .inner
            .read()
            .expect("child run registry poisoned")
            .values()
            .filter(|record| record.parent_session_id == parent_session_id)
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.created_at_ms);
        records
    }

    pub fn get_for_parent(
        &self,
        parent_session_id: &str,
        child_run_id: &str,
    ) -> Option<ChildRunRecord> {
        self.inner
            .read()
            .expect("child run registry poisoned")
            .get(child_run_id)
            .filter(|record| record.parent_session_id == parent_session_id)
            .cloned()
    }

    pub fn get(&self, child_run_id: &str) -> Option<ChildRunRecord> {
        self.inner
            .read()
            .expect("child run registry poisoned")
            .get(child_run_id)
            .cloned()
    }

    pub fn mark_running(&self, child_run_id: &str) {
        self.update(child_run_id, |record, now| {
            if !record.status.is_terminal() {
                record.status = ChildRunStatus::Running;
                record.latest_progress_at_ms = Some(now);
            }
        });
    }

    /// Reconcile cached lifecycle from authoritative `/proc` exit state.
    /// Delegation metadata remains record-owned; process state never flows in
    /// the opposite direction.
    pub fn reconcile_process_exit(&self, child_run_id: &str, exit_code: i32) {
        let mut records = self.inner.write().expect("child run registry poisoned");
        let Some(record) = records.get_mut(child_run_id) else {
            return;
        };
        if record.status.is_terminal() {
            return;
        }
        record.status = if matches!(record.status, ChildRunStatus::Terminating) {
            ChildRunStatus::Terminated
        } else if exit_code == 0 {
            ChildRunStatus::Completed
        } else {
            ChildRunStatus::Failed
        };
        record.latest_event_kind = Some("proc_exited".to_string());
        record.latest_status_summary = Some(format!(
            "authoritative process {} exited with code {exit_code}",
            record.process_path.as_deref().unwrap_or("/proc/<unknown>")
        ));
        record.updated_at_ms = now_ms();
    }

    pub fn set_state_ref(
        &self,
        child_run_id: &str,
        state_ref: crate::skills::DelegatedSkillOutputRef,
    ) {
        let mut records = self.inner.write().expect("child run registry poisoned");
        if let Some(record) = records.get_mut(child_run_id) {
            record.state_ref = Some(state_ref);
            record.updated_at_ms = now_ms();
        }
    }

    pub fn observe_heartbeat(&self, child_run_id: &str, summary: Option<String>) {
        self.update(child_run_id, |record, now| {
            if !record.status.is_terminal() {
                record.latest_heartbeat_at_ms = Some(now);
                if let Some(summary) = summary {
                    record.latest_status_summary = Some(truncate_text_with_suffix(
                        &summary,
                        MAX_CHILD_RUN_STATUS_SUMMARY_CHARS,
                        "...",
                    ));
                }
            }
        });
    }

    pub fn observe_progress(
        &self,
        child_run_id: &str,
        event_kind: impl Into<String>,
        summary: Option<String>,
    ) {
        self.update(child_run_id, |record, now| {
            if !record.status.is_terminal() {
                record.latest_progress_at_ms = Some(now);
                record.latest_event_kind = Some(event_kind.into());
                if let Some(summary) = summary {
                    record.latest_status_summary = Some(truncate_text_with_suffix(
                        &summary,
                        MAX_CHILD_RUN_STATUS_SUMMARY_CHARS,
                        "...",
                    ));
                }
            }
        });
    }

    pub fn observe_warning(&self, child_run_id: &str, warning: String) {
        self.update(child_run_id, |record, _| {
            push_bounded_warning(&mut record.warnings, warning);
        });
    }

    pub fn mark_terminal(
        &self,
        child_run_id: &str,
        status: ChildRunStatus,
        error_message: Option<String>,
    ) {
        let mut records = self.inner.write().expect("child run registry poisoned");
        let now = now_ms();
        if let Some(record) = records.get_mut(child_run_id) {
            record.status = status;
            record.error_message = error_message.map(|message| {
                truncate_text_with_suffix(&message, MAX_CHILD_RUN_ERROR_MESSAGE_CHARS, "...")
            });
            record.latest_progress_at_ms = Some(now);
            record.updated_at_ms = now;
        }
        prune_terminal_records(&mut records, MAX_TERMINAL_CHILD_RUN_RECORDS);
    }

    pub fn request_termination(
        &self,
        parent_session_id: &str,
        child_run_id: &str,
        actor: impl Into<String>,
        mode: ChildRunTerminationMode,
        reason: impl Into<String>,
    ) -> Result<ChildRunRecord, ChildRunRegistryError> {
        let mut records = self.inner.write().expect("child run registry poisoned");
        let Some(record) = records.get_mut(child_run_id) else {
            return Err(ChildRunRegistryError::NotFound);
        };
        if record.parent_session_id != parent_session_id {
            return Err(ChildRunRegistryError::NotFound);
        }
        if record.status.is_terminal() {
            return Err(ChildRunRegistryError::AlreadyTerminal(Box::new(
                record.clone(),
            )));
        }

        let now = now_ms();
        record.status = ChildRunStatus::Terminating;
        record.updated_at_ms = now;
        let actor = actor.into();
        let reason = reason.into();
        record.termination = Some(ChildRunTerminationRequest {
            actor: truncate_text_with_suffix(&actor, MAX_CHILD_RUN_TERMINATION_ACTOR_CHARS, "..."),
            reason: truncate_text_with_suffix(
                &reason,
                MAX_CHILD_RUN_TERMINATION_REASON_CHARS,
                "...",
            ),
            mode,
            requested_at_ms: now,
        });
        Ok(record.clone())
    }

    pub fn termination_request(&self, child_run_id: &str) -> Option<ChildRunTerminationRequest> {
        self.inner
            .read()
            .expect("child run registry poisoned")
            .get(child_run_id)
            .and_then(|record| record.termination.clone())
    }

    #[cfg(test)]
    pub fn clear(&self) {
        self.inner
            .write()
            .expect("child run registry poisoned")
            .clear();
    }

    fn update(&self, child_run_id: &str, update: impl FnOnce(&mut ChildRunRecord, u64)) {
        let mut records = self.inner.write().expect("child run registry poisoned");
        if let Some(record) = records.get_mut(child_run_id) {
            let now = now_ms();
            update(record, now);
            record.updated_at_ms = now;
        }
    }
}

fn push_bounded_warning(warnings: &mut Vec<String>, warning: String) {
    while warnings.len() >= MAX_CHILD_RUN_WARNINGS {
        warnings.remove(0);
    }
    warnings.push(truncate_text_with_suffix(
        &warning,
        MAX_CHILD_RUN_WARNING_CHARS,
        "...",
    ));
}

fn truncate_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_len))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
}

fn prune_terminal_records(
    records: &mut HashMap<String, ChildRunRecord>,
    terminal_retention_limit: usize,
) {
    let terminal_count = records
        .values()
        .filter(|record| record.status.is_terminal())
        .count();
    if terminal_count <= terminal_retention_limit {
        return;
    }

    let mut terminal_records = records
        .values()
        .filter(|record| record.status.is_terminal())
        .map(|record| {
            (
                record.updated_at_ms,
                record.created_at_ms,
                record.id.clone(),
            )
        })
        .collect::<Vec<_>>();
    terminal_records.sort();

    let remove_count = terminal_count - terminal_retention_limit;
    for (_, _, id) in terminal_records.into_iter().take(remove_count) {
        records.remove(&id);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildRunRegistryError {
    NotFound,
    AlreadyTerminal(Box<ChildRunRecord>),
}

static GLOBAL_CHILD_RUN_REGISTRY: OnceLock<ChildRunRegistry> = OnceLock::new();

pub fn global_child_run_registry() -> &'static ChildRunRegistry {
    GLOBAL_CHILD_RUN_REGISTRY.get_or_init(ChildRunRegistry::default)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record(child_run_id: &str, parent_session_id: &str) -> ChildRunRecord {
        ChildRunRecord::new(
            child_run_id.to_string(),
            parent_session_id.to_string(),
            format!("child-session-{child_run_id}"),
            Some("/tmp/workspace".to_string()),
            Some("/proc/42".to_string()),
            Some("repo-coding".to_string()),
        )
    }

    #[test]
    fn child_run_launch_record_retains_requirements_and_namespace_summary() {
        let decision = alan_agent_protocol::DelegatedCapabilityDecision {
            requirements: vec![
                alan_agent_protocol::DelegatedCapabilityRequirement::WorkspaceRead {
                    path: Some(std::path::PathBuf::from("/tmp/workspace")),
                },
            ],
            namespace: alan_agent_protocol::DelegatedNamespaceSummary {
                mounts: vec!["/agent".to_string(), "/mnt/llm".to_string()],
                bin_bindings: vec!["/bin/read_file".to_string()],
                workspace_root: Some(std::path::PathBuf::from("/tmp/workspace")),
                workspace_access: Some(alan_agent_protocol::DelegatedWorkspaceAccess::ReadOnly),
                workspace_projection: Some("host_tool_binding_compatibility".to_string()),
                llm_connection: Some("default".to_string()),
            },
            unsatisfied: Vec::new(),
            recovery: alan_agent_protocol::DelegatedCapabilityRecovery::Satisfied,
            narrowed_task: None,
        };

        let record = test_record("run-audit", "parent-audit")
            .with_delegation_capability_decision(decision.clone());

        assert_eq!(record.delegation_capability_decision, Some(decision));
    }

    #[test]
    fn registry_tracks_liveness_progress_and_terminal_state() {
        let registry = ChildRunRegistry::default();
        registry.register(test_record("run-1", "parent-1"));
        registry.register(test_record("run-2", "parent-2"));

        registry.mark_running("run-1");
        registry.observe_heartbeat("run-1", Some("alive".to_string()));
        registry.observe_progress(
            "run-1",
            "tool_call_started",
            Some("running bash".to_string()),
        );
        registry.observe_warning("run-1", "first warning".to_string());

        let parent_runs = registry.list_for_parent("parent-1");
        assert_eq!(parent_runs.len(), 1);
        assert_eq!(parent_runs[0].id, "run-1");

        let running = registry.get_for_parent("parent-1", "run-1").unwrap();
        assert_eq!(running.status, ChildRunStatus::Running);
        assert_eq!(
            running.latest_event_kind.as_deref(),
            Some("tool_call_started")
        );
        assert_eq!(
            running.latest_status_summary.as_deref(),
            Some("running bash")
        );
        assert_eq!(running.warnings, vec!["first warning"]);
        assert!(running.latest_heartbeat_at_ms.is_some());
        assert!(running.latest_progress_at_ms.is_some());

        registry.mark_terminal("run-1", ChildRunStatus::Completed, None);
        let terminal = registry.get("run-1").unwrap();
        registry.observe_progress("run-1", "text_delta", Some("late update".to_string()));
        let after_late_progress = registry.get("run-1").unwrap();
        assert_eq!(after_late_progress.status, ChildRunStatus::Completed);
        assert_eq!(
            after_late_progress.latest_event_kind,
            terminal.latest_event_kind
        );
        assert_eq!(
            after_late_progress.latest_status_summary,
            terminal.latest_status_summary
        );
    }

    #[test]
    fn registry_caps_warning_retention_per_run() {
        let registry = ChildRunRegistry::default();
        registry.register(test_record("run-1", "parent-1"));

        for index in 0..(MAX_CHILD_RUN_WARNINGS + 2) {
            registry.observe_warning(
                "run-1",
                format!(
                    "warning-{index:03}-{}",
                    "x".repeat(MAX_CHILD_RUN_WARNING_CHARS)
                ),
            );
        }

        let record = registry.get("run-1").unwrap();
        assert_eq!(record.warnings.len(), MAX_CHILD_RUN_WARNINGS);
        assert!(record.warnings[0].starts_with("warning-002-"));
        assert!(
            record
                .warnings
                .iter()
                .all(|warning| warning.chars().count() <= MAX_CHILD_RUN_WARNING_CHARS)
        );
        assert!(record.warnings.last().unwrap().ends_with("..."));
    }

    #[test]
    fn registry_bounds_child_run_string_payloads() {
        let registry = ChildRunRegistry::default();
        registry.register(test_record("run-1", "parent-1"));

        registry.observe_progress(
            "run-1",
            "warning",
            Some("s".repeat(MAX_CHILD_RUN_STATUS_SUMMARY_CHARS + 10)),
        );
        let running = registry.get("run-1").unwrap();
        assert!(
            running
                .latest_status_summary
                .as_ref()
                .unwrap()
                .chars()
                .count()
                <= MAX_CHILD_RUN_STATUS_SUMMARY_CHARS
        );
        assert!(
            running
                .latest_status_summary
                .as_ref()
                .unwrap()
                .ends_with("...")
        );

        let terminating = registry
            .request_termination(
                "parent-1",
                "run-1",
                "a".repeat(MAX_CHILD_RUN_TERMINATION_ACTOR_CHARS + 10),
                ChildRunTerminationMode::Graceful,
                "r".repeat(MAX_CHILD_RUN_TERMINATION_REASON_CHARS + 10),
            )
            .unwrap();
        let termination = terminating.termination.unwrap();
        assert!(termination.actor.chars().count() <= MAX_CHILD_RUN_TERMINATION_ACTOR_CHARS);
        assert!(termination.reason.chars().count() <= MAX_CHILD_RUN_TERMINATION_REASON_CHARS);
        assert!(termination.actor.ends_with("..."));
        assert!(termination.reason.ends_with("..."));

        registry.mark_terminal(
            "run-1",
            ChildRunStatus::Failed,
            Some("e".repeat(MAX_CHILD_RUN_ERROR_MESSAGE_CHARS + 10)),
        );
        let terminal = registry.get("run-1").unwrap();
        assert!(
            terminal.error_message.as_ref().unwrap().chars().count()
                <= MAX_CHILD_RUN_ERROR_MESSAGE_CHARS
        );
        assert!(terminal.error_message.unwrap().ends_with("..."));
    }

    #[test]
    fn registry_records_termination_and_rejects_unknown_or_terminal_runs() {
        let registry = ChildRunRegistry::default();
        registry.register(test_record("run-1", "parent-1"));

        assert_eq!(
            registry.request_termination(
                "parent-2",
                "run-1",
                "parent_runtime",
                ChildRunTerminationMode::Graceful,
                "wrong parent",
            ),
            Err(ChildRunRegistryError::NotFound)
        );
        assert_eq!(
            registry.request_termination(
                "parent-1",
                "missing",
                "parent_runtime",
                ChildRunTerminationMode::Graceful,
                "missing child",
            ),
            Err(ChildRunRegistryError::NotFound)
        );

        let terminating = registry
            .request_termination(
                "parent-1",
                "run-1",
                "parent_runtime",
                ChildRunTerminationMode::Forceful,
                "operator requested stop",
            )
            .unwrap();
        assert_eq!(terminating.status, ChildRunStatus::Terminating);
        assert_eq!(
            terminating.termination.as_ref().map(|request| (
                request.actor.as_str(),
                request.reason.as_str(),
                request.mode
            )),
            Some((
                "parent_runtime",
                "operator requested stop",
                ChildRunTerminationMode::Forceful
            ))
        );

        registry.mark_terminal("run-1", ChildRunStatus::Terminated, None);
        match registry.request_termination(
            "parent-1",
            "run-1",
            "parent_runtime",
            ChildRunTerminationMode::Graceful,
            "duplicate",
        ) {
            Err(ChildRunRegistryError::AlreadyTerminal(record)) => {
                assert_eq!(record.status, ChildRunStatus::Terminated);
                assert_eq!(
                    record
                        .termination
                        .as_ref()
                        .map(|request| request.reason.as_str()),
                    Some("operator requested stop")
                );
            }
            other => panic!("expected already-terminal error, got {other:?}"),
        }
    }

    #[test]
    fn registry_reconciles_stale_running_record_from_authoritative_process_exit() {
        let registry = ChildRunRegistry::default();
        registry.register(test_record("run-1", "parent-1"));
        registry.mark_running("run-1");

        registry.reconcile_process_exit("run-1", 17);

        let record = registry.get("run-1").unwrap();
        assert_eq!(record.status, ChildRunStatus::Failed);
        assert_eq!(record.latest_event_kind.as_deref(), Some("proc_exited"));
        assert_eq!(record.process_path.as_deref(), Some("/proc/42"));
        assert!(
            record
                .latest_status_summary
                .as_deref()
                .unwrap()
                .contains("exited with code 17")
        );
    }

    #[test]
    fn registry_prunes_old_terminal_runs_without_removing_active_runs() {
        let registry = ChildRunRegistry::default();
        let parent_session_id = "parent-1";
        for index in 0..(MAX_TERMINAL_CHILD_RUN_RECORDS + 2) {
            let id = format!("terminal-{index:03}");
            let mut record = test_record(&id, parent_session_id);
            record.created_at_ms = index as u64;
            record.updated_at_ms = index as u64;
            registry.register(record);
            registry.mark_terminal(&id, ChildRunStatus::Completed, None);
        }

        registry.register(test_record("active-run", parent_session_id));
        registry.mark_running("active-run");

        let runs = registry.list_for_parent(parent_session_id);
        let terminal_count = runs
            .iter()
            .filter(|record| record.status.is_terminal())
            .count();
        assert_eq!(terminal_count, MAX_TERMINAL_CHILD_RUN_RECORDS);
        assert!(runs.iter().any(|record| record.id == "active-run"));
        assert!(registry.get("terminal-000").is_none());
        assert!(
            registry
                .get(&format!(
                    "terminal-{:03}",
                    MAX_TERMINAL_CHILD_RUN_RECORDS + 1
                ))
                .is_some()
        );
    }
}
