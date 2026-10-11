//! Host operations run off the input loop. Only correlated replies settle authority.
use super::*;

#[cfg(test)]
#[path = "project_grant_tests.rs"]
mod grant_tests;

pub(super) fn start_discovery(
    app: &mut FileBackedApp,
    shell: &alan_shell::Shell,
    owner: String,
    jobs: &mut tokio::task::JoinSet<()>,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    if app.project_host_pending {
        return;
    }
    app.project_host_pending = true;
    app.notice =
        Some("looking up current project authority; /help or /quit remain available".into());
    let cwd = app.namespace_cwd.clone();
    let shell = shell.clone();
    let tx = tx.clone();
    jobs.spawn(async move {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            discover_grant(&shell, &owner, &cwd),
        )
        .await
        .unwrap_or_else(|_| Err("Host authority lookup timed out".into()));
        let _ = tx
            .send(FileBackedEvent::ProjectGrantLocated { owner, cwd, result })
            .await;
    });
}

pub(super) fn finish_discovery(
    app: &mut FileBackedApp,
    owner: String,
    current_owner: Option<String>,
    cwd: std::path::PathBuf,
    blocked: bool,
    result: Result<Option<String>, String>,
) -> Option<(String, String, String)> {
    app.project_host_pending = false;
    if current_owner.as_deref() != Some(&owner)
        || app.namespace_cwd != cwd
        || !app.project_boundary_available(blocked)
        || app.retained_project_grant().is_some()
    {
        app.push_error("project context changed during lookup; no revocation sent".into());
        return None;
    }
    match result {
        Ok(Some(grant_id)) => {
            let (id, command) = project_dispatch::project_selector("/");
            app.stage_project_control(owner.clone(), id.clone(), None, Some(grant_id));
            Some((owner, id, command))
        }
        Ok(None) => {
            app.notice = Some("no active project grant matches current directory".into());
            None
        }
        Err(error) => {
            app.push_error(format!(
                "project authority lookup failed; no revocation sent: {error}"
            ));
            None
        }
    }
}

#[derive(serde::Deserialize)]
struct GrantRecord {
    id: String,
    namespace_path: String,
    active: bool,
}

#[derive(serde::Deserialize)]
struct RequestRecord {
    id: String,
    namespace_path: String,
    requesting_pid: u64,
}

async fn record<T: serde::de::DeserializeOwned>(
    shell: &alan_shell::Shell,
    path: &str,
) -> Result<T, String> {
    let text = action_detail_io::reference::document_with_budget(shell, path, 4096).await?;
    serde_json::from_str(&text).map_err(|_| format!("invalid Host record at {path}"))
}

async fn discover_grant(
    shell: &alan_shell::Shell,
    owner: &str,
    cwd: &std::path::Path,
) -> Result<Option<String>, String> {
    let pid = owner
        .strip_prefix("/agent/")
        .and_then(|p| p.parse::<u64>().ok())
        .filter(|p| *p != 0)
        .ok_or("Root identity unavailable")?;
    if !cwd.is_absolute()
        || cwd
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("invalid observed cwd".into());
    }
    if !cwd.starts_with("/mnt") {
        return Ok(None);
    }
    let ids = shell
        .ls_bounded("/mnt/host-mount/grants", 1024, 65536)
        .await
        .map_err(|e| format!("Host grants unavailable: {e:?}"))?;
    let mut selected: Option<(String, usize)> = None;
    let mut ambiguous = false;
    for id in ids {
        if id.is_empty() || id.contains('/') || matches!(id.as_str(), "." | "..") {
            return Err("invalid Host grant ID".into());
        }
        let grant: GrantRecord =
            record(shell, &format!("/mnt/host-mount/grants/{id}/record")).await?;
        if grant.id != id {
            return Err("Host grant identity mismatch".into());
        }
        if !grant.active {
            continue;
        }
        let path = std::path::Path::new(&grant.namespace_path);
        if !path.starts_with("/mnt")
            || path == std::path::Path::new("/mnt")
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("invalid Host grant namespace path".into());
        }
        if !cwd.starts_with(path) {
            continue;
        }
        let request: RequestRecord =
            record(shell, &format!("/mnt/host-mount/requests/{id}/request")).await?;
        if request.id != id || request.namespace_path != grant.namespace_path {
            return Err("Host grant/request identity mismatch".into());
        }
        if request.requesting_pid != pid {
            continue;
        }
        let depth = path.components().count();
        match &selected {
            Some((other, n)) if *n == depth && other != &id => {
                ambiguous = true;
            }
            Some((_, n)) if *n >= depth => {}
            _ => {
                selected = Some((id, depth));
                ambiguous = false;
            }
        }
    }
    if ambiguous {
        return Err("ambiguous current project authority".into());
    }
    Ok(selected.map(|(id, _)| id))
}

pub(super) fn start(
    app: &mut FileBackedApp,
    handler: &ProjectControlHandler,
    command: ProjectControl,
    owner: String,
    jobs: &mut tokio::task::JoinSet<()>,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    if app.project_host_pending {
        return;
    }
    let retrying_mount = app.uncertain_project_mount.is_some();
    if matches!(command, ProjectControl::Mount { .. }) {
        if let Some((prior_owner, prior_command)) = &app.uncertain_project_mount {
            if prior_owner != &owner || prior_command != &command {
                app.push_error("Root changed; unknown project operation remains fenced".into());
                return;
            }
        } else {
            app.uncertain_project_mount = Some((owner.clone(), command.clone()));
        }
    }
    app.project_host_pending = true;
    if matches!(command, ProjectControl::Mount { .. }) && !retrying_mount {
        app.project_selection = None;
        app.composer.set_text("");
        app.input_intent = alan_agent_protocol::InputIntent::Agent;
        app.refresh_completion();
    }
    app.notice = Some("project operation pending; /help or /quit remain available".into());
    let handler = handler.clone();
    let tx = tx.clone();
    jobs.spawn(async move {
        // Do not cancel a mutation on a timer and pretend its effect was rejected.
        let result = handler(command.clone()).await.map_err(|e| format!("{e:#}"));
        let _ = tx
            .send(FileBackedEvent::ProjectHostCompleted {
                owner,
                command,
                result,
            })
            .await;
    });
}

pub(super) fn finish(
    app: &mut FileBackedApp,
    owner: String,
    current_owner: Option<String>,
    command: ProjectControl,
    result: Result<ProjectControlResult, String>,
) -> Option<(String, String, String)> {
    if matches!(command, ProjectControl::Mount { .. })
        && !app
            .uncertain_project_mount
            .as_ref()
            .is_some_and(|(prior_owner, prior_command)| {
                prior_owner == &owner && prior_command == &command
            })
    {
        app.push_error("stale project response ignored; current operation retained".into());
        return None;
    }
    app.project_host_pending = false;
    match (command, result) {
        (
            ProjectControl::Mount { .. },
            Ok(ProjectControlResult::Mounted {
                receipt,
                completion_root,
            }),
        ) => {
            if receipt.grant_id.trim().is_empty()
                || !receipt.namespace_path.starts_with("/mnt/")
                || std::path::Path::new(&receipt.namespace_path)
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                app.push_error(
                    "invalid mount receipt; no cwd control or unsafe grant cleanup attempted"
                        .into(),
                );
                return None;
            }
            app.uncertain_project_mount = None;
            let (id, control) = project_dispatch::project_selector(&receipt.namespace_path);
            app.stage_project_control(
                owner.clone(),
                id.clone(),
                Some((receipt, completion_root)),
                None,
            );
            if current_owner.as_deref() != Some(&owner) {
                app.fence_project_control();
                return None;
            }
            Some((owner, id, control))
        }
        (ProjectControl::Mount { .. }, Ok(ProjectControlResult::MountRejected { message })) => {
            app.uncertain_project_mount = None;
            app.push_error(format!("project selection rejected: {message}"));
            None
        }
        (ProjectControl::Mount { .. }, Ok(ProjectControlResult::MountUncertain { message })) => {
            app.push_error(format!(
                "project outcome unknown; /project retries this selection: {message}"
            ));
            None
        }
        (ProjectControl::Revoke { grant_id }, Ok(ProjectControlResult::Revoked)) => {
            if app.project_cleanup.as_deref() == Some(&grant_id) {
                app.finish_project_cleanup(&grant_id, true);
            } else if app.retained_project_grant().as_deref() == Some(&grant_id) {
                app.project_revoked();
            }
            app.notice = Some(
                "project grant revoked; use /project before /continue for project work".into(),
            );
            None
        }
        (_, Err(error)) => {
            app.push_error(format!(
                "project operation failed; effects may be uncertain: {error}"
            ));
            None
        }
        _ => {
            app.push_error("unexpected Host project response; authority unchanged".into());
            None
        }
    }
}

pub(super) fn write_cwd(
    app: &mut FileBackedApp,
    shell: &alan_shell::Shell,
    owner: String,
    id: String,
    command: String,
    jobs: &mut tokio::task::JoinSet<()>,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
) {
    app.notice = Some("project cwd selection pending".into());
    let shell = shell.clone();
    let tx = tx.clone();
    jobs.spawn(async move {
        let current = || async {
            current_root_agent_pid(&shell)
                .await
                .ok()
                .flatten()
                .is_some_and(|pid| owner == format!("/agent/{pid}"))
        };
        let result = if current().await {
            write_machine_ctl(&shell, &owner, &command)
                .await
                .map_err(|e| format!("{e:#}"))
        } else {
            Err("Root changed before directory control; grant retained".into())
        };
        let owner_current = current().await;
        let _ = tx
            .send(FileBackedEvent::ProjectCwdWritten {
                owner,
                id,
                result,
                owner_current,
            })
            .await;
    });
}

pub(super) async fn grant_active(shell: &alan_shell::Shell, grant_id: &str) -> Option<bool> {
    if grant_id.is_empty() || grant_id.contains('/') || matches!(grant_id, "." | "..") {
        return None;
    }
    let path = format!("/mnt/host-mount/grants/{grant_id}/record");
    let text = action_detail_io::reference::document_with_budget(shell, &path, 4096)
        .await
        .ok()?;
    let record: serde_json::Value = serde_json::from_str(&text).ok()?;
    (record["id"].as_str() == Some(grant_id))
        .then(|| record["active"].as_bool())
        .flatten()
}

pub(super) fn observe_grant(app: &mut FileBackedApp, grant_id: &str, active: Option<bool>) {
    if active == Some(false)
        && !app.project_host_pending
        && app.pending_project_control.is_none()
        && app.project.as_ref().is_some_and(|p| p.grant_id == grant_id)
    {
        app.project_revoked();
        app.notice = Some("project authorization revoked; /project selects a directory".into());
        app.refresh_completion();
    }
}
