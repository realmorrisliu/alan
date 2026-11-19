//! `AgentRuntimeService` lifecycle hooks for `/agent/history` and Root rollout switching.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use alan_agent_engine::{
    AgentRuntimeStoreBindings, ProjectSummaryEdit, delete_project_summary, edit_project_summary,
    inspect_project_summary,
};
use alan_ap::ErrorCode;
use anyhow::Result;
use serde::Deserialize;

use super::{
    HistoryDiscoveryService, HistoryProjectScope, current_rollout_id_from_state,
    fingerprint_project_boundary, path_prefix_matches,
};
use crate::agent_runtime::root_recovery::RootRolloutState;

/// Action requested for the next Root Agent launch after `/agent/history/ctl`.
#[derive(Debug, Clone)]
pub(in crate::agent_runtime) enum PendingRootRolloutSwitch {
    Resume(PathBuf),
    Fresh,
}

impl crate::agent_runtime::AgentRuntimeService {
    pub(in crate::agent_runtime) async fn ensure_history_service_bound(self: &Arc<Self>) {
        self.history_bound
            .get_or_init(|| async {
                let service_for_ctx = Arc::clone(self);
                let service_for_switch = Arc::clone(self);
                let service_for_mem = Arc::clone(self);
                let history_service = Arc::new(
                    HistoryDiscoveryService::new(
                        Arc::new(move || {
                            let service = Arc::clone(&service_for_ctx);
                            Box::pin(async move { service.current_history_context().await })
                        }),
                        Arc::new(move |next_path| {
                            let service = Arc::clone(&service_for_switch);
                            Box::pin(async move { service.switch_root_rollout(next_path).await })
                        }),
                    )
                    .with_memory_store_provider(Arc::new(move || {
                        let template = service_for_mem
                            .last_root_template
                            .lock()
                            .expect("last root template mutex poisoned");
                        template
                            .as_ref()
                            .and_then(|t| t.process.memory_store_backing.clone())
                    })),
                );
                let fs = Arc::new(crate::flat_fs::FlatServiceFs::new(history_service));
                self.agent_root.bind_history_server(fs).await;
            })
            .await;
    }

    pub(in crate::agent_runtime) async fn resolve_active_project_scope(
        &self,
        pid: Option<u64>,
        fallback_cwd: &Path,
    ) -> HistoryProjectScope {
        let mut scope = HistoryProjectScope::from_cwd(Some(&fallback_cwd.to_string_lossy()));
        let Some(pid) = pid else {
            return scope;
        };
        let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(
            self.host_mount.file_server_for_process(pid),
        ));
        let Ok(grant_ids) = shell.ls("/grants").await else {
            return scope;
        };
        for grant_id in grant_ids {
            let Some(grant) = self.host_mount.grant_record(&grant_id) else {
                continue;
            };
            if !grant.active {
                continue;
            }
            if scope
                .project_cwd
                .as_deref()
                .is_some_and(|cwd| path_prefix_matches(cwd, &grant.namespace_path))
                || grant_id == grant.id
            {
                let fp = fingerprint_project_boundary(&format!(
                    "{}:{}",
                    grant.label, grant.namespace_path
                ));
                scope = scope
                    .with_fingerprint(Some(fp))
                    .with_label(Some(grant.label));
                break;
            }
        }
        scope
    }

    pub(in crate::agent_runtime) async fn current_history_context(
        &self,
    ) -> (
        Option<AgentRuntimeStoreBindings>,
        HistoryProjectScope,
        Option<String>,
        bool,
    ) {
        let (stores, cwd, ephemeral) = {
            let template = self
                .last_root_template
                .lock()
                .expect("last root template mutex poisoned");
            match template.as_ref() {
                Some(t) => (
                    t.process.store_bindings.clone(),
                    t.process.namespace_cwd.clone(),
                    t.ephemeral_rollout,
                ),
                None => (None, PathBuf::from("/"), false),
            }
        };
        let root_pid = *self
            .active_root_pid
            .lock()
            .expect("active root pid mutex poisoned");
        let scope = self.resolve_active_project_scope(root_pid, &cwd).await;
        let current_id = {
            let state = self
                .current_root_rollout
                .lock()
                .expect("current Root rollout mutex poisoned");
            current_rollout_id_from_state(&state)
        };
        (stores, scope, current_id, ephemeral)
    }

    pub(in crate::agent_runtime) async fn prepare_root_template_recovery(
        &self,
        template: &mut crate::agent_runtime::RootAgentTemplate,
    ) -> Result<()> {
        *self
            .last_root_template
            .lock()
            .expect("last root template mutex poisoned") = Some(template.clone());
        let pending_switch = self
            .next_root_rollout
            .lock()
            .expect("next root rollout mutex poisoned")
            .take();
        let active_pid = *self
            .active_root_pid
            .lock()
            .expect("active root pid mutex poisoned");
        let scope = self
            .resolve_active_project_scope(active_pid, &template.process.namespace_cwd)
            .await;

        let mut current = self
            .current_root_rollout
            .lock()
            .expect("current Root rollout mutex poisoned");
        let mut resume_previous = template.resume_persisted_rollout && !template.ephemeral_rollout;
        let mut resume_id = if !template.ephemeral_rollout {
            template.resume_rollout_id.as_deref()
        } else {
            None
        };
        match pending_switch {
            Some(PendingRootRolloutSwitch::Resume(path)) => {
                *current = RootRolloutState::Durable(path);
                template.process.recovery_rollout_path = None;
                resume_id = None;
                resume_previous = false;
            }
            Some(PendingRootRolloutSwitch::Fresh) => {
                *current = RootRolloutState::NotStarted;
                template.process.recovery_rollout_path = None;
                resume_id = None;
                resume_previous = false;
            }
            None => {}
        }

        crate::agent_runtime::root_recovery::select_with_rollout_id(
            &mut template.process,
            resume_previous,
            resume_id,
            Some(&scope),
            &mut current,
        )
    }

    pub(in crate::agent_runtime) async fn switch_root_rollout(
        &self,
        target_path: Option<PathBuf>,
    ) -> Result<()> {
        {
            let mut next = self
                .next_root_rollout
                .lock()
                .expect("next root rollout mutex poisoned");
            *next = Some(match target_path {
                Some(path) => PendingRootRolloutSwitch::Resume(path),
                None => PendingRootRolloutSwitch::Fresh,
            });
        }
        let handle = {
            let pid = *self
                .active_root_pid
                .lock()
                .expect("active root pid mutex poisoned");
            pid.and_then(|pid| {
                self.root_runtime_handles
                    .lock()
                    .expect("Root runtime handles mutex poisoned")
                    .get(&pid)
                    .cloned()
            })
        };
        if let Some(handle) = handle {
            let _ = handle.shutdown().await;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum SummaryCtlCommand {
    Edit {
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        goal: Option<String>,
        #[serde(default)]
        progress: Option<String>,
        #[serde(default)]
        verification: Option<String>,
        #[serde(default)]
        expected_revision: Option<u64>,
    },
    Delete {
        #[serde(default)]
        expected_revision: Option<u64>,
    },
}

pub(super) fn read_project_summary_bytes(
    memory_dir: Option<&Path>,
    project_cwd: Option<&str>,
    ephemeral: bool,
) -> std::result::Result<Vec<u8>, ErrorCode> {
    if ephemeral {
        return serde_json::to_vec(&alan_agent_engine::ProjectContinuitySummaryView {
            exists: false,
            deleted: false,
            project_key: alan_agent_engine::derive_project_key(project_cwd),
            project_cwd: project_cwd.map(str::to_string),
            summary: None,
            markdown_recall_active: false,
            retention_note: "Project continuity summaries are disabled in ephemeral rollout mode."
                .to_string(),
        })
        .map_err(|_| ErrorCode::Io);
    }
    let Some(memory_dir) = memory_dir else {
        return Err(ErrorCode::NotFound);
    };
    let view = inspect_project_summary(memory_dir, project_cwd).map_err(map_summary_err)?;
    serde_json::to_vec(&view).map_err(|_| ErrorCode::Io)
}

pub(super) fn commit_project_summary_ctl(
    memory_dir: Option<&Path>,
    project_cwd: Option<&str>,
    ephemeral: bool,
    bytes: &[u8],
) -> std::result::Result<(), ErrorCode> {
    if ephemeral {
        return Err(ErrorCode::NoAccess);
    }
    let Some(memory_dir) = memory_dir else {
        return Err(ErrorCode::NotFound);
    };
    let text = std::str::from_utf8(bytes).map_err(|_| ErrorCode::BadRequest)?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ErrorCode::BadRequest);
    }
    let cmd = if trimmed.starts_with('{') {
        serde_json::from_str::<SummaryCtlCommand>(trimmed).map_err(|_| ErrorCode::BadRequest)?
    } else if trimmed == "delete" {
        SummaryCtlCommand::Delete {
            expected_revision: None,
        }
    } else if let Some(rest) = trimmed.strip_prefix("edit ") {
        SummaryCtlCommand::Edit {
            text: Some(rest.trim().to_string()),
            goal: None,
            progress: None,
            verification: None,
            expected_revision: None,
        }
    } else {
        return Err(ErrorCode::BadRequest);
    };

    match cmd {
        SummaryCtlCommand::Edit {
            text,
            goal,
            progress,
            verification,
            expected_revision,
        } => edit_project_summary(
            memory_dir,
            project_cwd,
            expected_revision,
            ProjectSummaryEdit {
                goal,
                progress,
                verification,
                raw_text: text,
            },
        )
        .map(|_| ())
        .map_err(map_summary_err),
        SummaryCtlCommand::Delete { expected_revision } => {
            delete_project_summary(memory_dir, project_cwd, expected_revision)
                .map(|_| ())
                .map_err(map_summary_err)
        }
    }
}

fn map_summary_err(err: anyhow::Error) -> ErrorCode {
    let msg = err.to_string();
    if msg.contains("revision conflict") || msg.contains("requires non-empty content") {
        ErrorCode::BadRequest
    } else if msg.contains("symlink") || msg.contains("escapes") {
        ErrorCode::NoAccess
    } else {
        ErrorCode::Io
    }
}
