//! Project binding, ledger status, and workflow inbox CLI (`alan project ...`).

use std::path::{Path, PathBuf};

use alan_agent_engine::{
    InboxDigestEntry, InboxDigestStatus, ProjectBinding, WorkstreamCounts, bind_project_dir,
    list_inbox_digests, list_unread_inbox_digests, load_project_ledger,
    mark_all_inbox_digests_digested, mark_inbox_digest_digested, resolve_bound_project,
};
use alan_os_host::SystemStorePaths;
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use serde::Serialize;

/// Top-level arguments for `alan project`.
#[derive(Debug, Args)]
pub struct ProjectArgs {
    /// Project subcommand to run.
    #[command(subcommand)]
    pub action: ProjectAction,
}

/// Supported `alan project` subcommands.
#[derive(Debug, Subcommand)]
pub enum ProjectAction {
    /// Explicitly bind a directory to a System Store project key (never infers from Git remotes)
    Bind {
        /// Target directory path to bind (defaults to current working directory)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
        /// Optional saved Host Mount grant reference (`grant_ref`)
        #[arg(long = "grant", value_name = "GRANT_ID")]
        grant_ref: Option<String>,
        /// Emit structured JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
    /// Show the bound project key, active/idle/archived workstream counts, and unread inbox count
    Status {
        /// Target directory path to inspect (defaults to current working directory)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
        /// Emit structured JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
    /// List workflow inbox digests or acknowledge entries as digested
    Inbox {
        /// Target directory path to inspect (defaults to current working directory)
        #[arg(long, value_name = "PATH")]
        path: Option<PathBuf>,
        /// Mark a single unread digest as digested by `run_id`
        #[arg(long, value_name = "RUN_ID", conflicts_with = "ack_all")]
        ack: Option<String>,
        /// Mark all unread digests for the bound project as digested
        #[arg(long = "ack-all", conflicts_with = "ack")]
        ack_all: bool,
        /// Include already-digested entries when listing
        #[arg(long)]
        all: bool,
        /// Emit structured JSON instead of plain text
        #[arg(long)]
        json: bool,
    },
}

/// Structured output for `alan project status --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectStatusReport {
    pub project_key: String,
    pub canonical_root_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_ref: Option<String>,
    pub counts: WorkstreamCounts,
    pub active_non_archived: usize,
    pub unread_inbox_count: usize,
}

/// Structured output for `alan project inbox --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectInboxReport {
    pub project_key: String,
    pub unread_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_count: Option<usize>,
    pub entries: Vec<InboxDigestEntry>,
}

/// Entrypoint for `alan project ...` against the current System Store.
pub fn run(args: ProjectArgs) -> Result<()> {
    alan_os_host::installation::validate_current_invocation()?;
    let _installation_access = alan_os_host::installation::InstallationPaths::detect()?.access()?;
    let system = SystemStorePaths::detect()?;
    let projects_root = system.projects()?;
    let cwd = std::env::current_dir().context("resolve current working directory")?;
    let out = run_with_projects_root(&projects_root, &cwd, args)?;
    print!("{out}");
    Ok(())
}

/// Execute `alan project` against an explicit `projects_root` and working directory.
pub fn run_with_projects_root(
    projects_root: &Path,
    cwd: &Path,
    args: ProjectArgs,
) -> Result<String> {
    match args.action {
        ProjectAction::Bind {
            path,
            grant_ref,
            json,
        } => {
            let target = resolve_target_dir(cwd, path.as_deref())?;
            if let Some(ref grant) = grant_ref {
                super::workflow::validate_mount_grant_id(grant.trim())?;
            }
            let binding = bind_project_dir(projects_root, &target, grant_ref)?;
            format_bind_output(&binding, json)
        }
        ProjectAction::Status { path, json } => {
            let target = resolve_target_dir(cwd, path.as_deref())?;
            let binding = require_bound_project(projects_root, &target)?;
            let ledger = load_project_ledger(projects_root, &binding.project_key)?;
            let counts = ledger.counts();
            let unread = list_unread_inbox_digests(projects_root, &binding.project_key)?;
            let report = ProjectStatusReport {
                project_key: binding.project_key,
                canonical_root_path: binding.canonical_root_path,
                grant_ref: binding.grant_ref,
                counts,
                active_non_archived: counts.active_non_archived(),
                unread_inbox_count: unread.len(),
            };
            format_status_output(&report, json)
        }
        ProjectAction::Inbox {
            path,
            ack,
            ack_all,
            all,
            json,
        } => {
            let target = resolve_target_dir(cwd, path.as_deref())?;
            let binding = require_bound_project(projects_root, &target)?;
            if let Some(run_id) = ack {
                let entry =
                    mark_inbox_digest_digested(projects_root, &binding.project_key, run_id.trim())?;
                let unread_count =
                    list_unread_inbox_digests(projects_root, &binding.project_key)?.len();
                let report = ProjectInboxReport {
                    project_key: binding.project_key,
                    unread_count,
                    acknowledged_count: Some(1),
                    entries: vec![entry],
                };
                format_inbox_ack_output(&report, json)
            } else if ack_all {
                let entries = mark_all_inbox_digests_digested(projects_root, &binding.project_key)?;
                let report = ProjectInboxReport {
                    project_key: binding.project_key,
                    unread_count: 0,
                    acknowledged_count: Some(entries.len()),
                    entries,
                };
                format_inbox_ack_output(&report, json)
            } else {
                let entries = if all {
                    list_inbox_digests(projects_root, &binding.project_key)?
                } else {
                    list_unread_inbox_digests(projects_root, &binding.project_key)?
                };
                let unread_count = if all {
                    entries
                        .iter()
                        .filter(|entry| entry.digested_at.is_none())
                        .count()
                } else {
                    entries.len()
                };
                let report = ProjectInboxReport {
                    project_key: binding.project_key,
                    unread_count,
                    acknowledged_count: None,
                    entries,
                };
                format_inbox_list_output(&report, all, json)
            }
        }
    }
}

fn resolve_target_dir(cwd: &Path, path: Option<&Path>) -> Result<PathBuf> {
    let candidate = match path {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => cwd.join(path),
        None => cwd.to_path_buf(),
    };
    let meta = std::fs::metadata(&candidate)
        .with_context(|| format!("inspect project directory {}", candidate.display()))?;
    ensure!(
        meta.is_dir(),
        "project path is not a directory: {}",
        candidate.display()
    );
    Ok(candidate)
}

fn require_bound_project(projects_root: &Path, target: &Path) -> Result<ProjectBinding> {
    resolve_bound_project(projects_root, target)?.with_context(|| {
        format!(
            "directory `{}` is not explicitly bound to an Alan project; run `alan project bind` to bind it",
            target.display()
        )
    })
}

fn format_bind_output(binding: &ProjectBinding, json: bool) -> Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(binding)?));
    }
    Ok(format!(
        "bound project `{}` -> {}\n  grant_ref: {}\n  created_at: {}\n",
        binding.project_key,
        binding.canonical_root_path.display(),
        binding.grant_ref.as_deref().unwrap_or("none"),
        binding.created_at
    ))
}

fn format_status_output(report: &ProjectStatusReport, json: bool) -> Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(report)?));
    }
    Ok(format!(
        "project_key: {}\nroot: {}\ngrant_ref: {}\nworkstreams: {} active ({} running, {} idle, {} paused_error), {} archived\nunread_inbox: {}\n",
        report.project_key,
        report.canonical_root_path.display(),
        report.grant_ref.as_deref().unwrap_or("none"),
        report.active_non_archived,
        report.counts.running,
        report.counts.idle,
        report.counts.paused_error,
        report.counts.archived,
        report.unread_inbox_count
    ))
}

fn format_inbox_ack_output(report: &ProjectInboxReport, json: bool) -> Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(report)?));
    }
    let acked = report.acknowledged_count.unwrap_or(report.entries.len());
    if acked == 0 {
        return Ok(format!(
            "No unread inbox digests to acknowledge for project `{}`.\n",
            report.project_key
        ));
    }
    let mut out = format!(
        "Acknowledged {} inbox digest(s) for project `{}` ({} unread remaining):\n",
        acked, report.project_key, report.unread_count
    );
    for entry in &report.entries {
        out.push_str(&format!(
            "  {}  workflow={}  digested_at={}\n",
            entry.run_id,
            entry.workflow_id,
            entry.digested_at.as_deref().unwrap_or("-")
        ));
    }
    Ok(out)
}

fn format_inbox_list_output(
    report: &ProjectInboxReport,
    include_all: bool,
    json: bool,
) -> Result<String> {
    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(report)?));
    }
    if report.entries.is_empty() {
        return Ok(if include_all {
            format!("No inbox digests for project `{}`.\n", report.project_key)
        } else {
            format!(
                "No unread inbox digests for project `{}`.\n",
                report.project_key
            )
        });
    }
    let mut out = format!(
        "Project `{}` inbox ({} unread):\n",
        report.project_key, report.unread_count
    );
    for entry in &report.entries {
        let state = match entry.digested_at.as_deref() {
            Some(ts) => format!("digested:{ts}"),
            None => "unread".to_string(),
        };
        let status = match entry.status {
            InboxDigestStatus::Ok => "ok",
            InboxDigestStatus::Warning => "warning",
            InboxDigestStatus::PausedError => "paused_error",
        };
        out.push_str(&format!(
            "  {}  workflow={}  status={}  {}  created={}\n    summary: {}\n    evidence: {}\n",
            entry.run_id,
            entry.workflow_id,
            status,
            state,
            entry.created_at,
            entry.summary,
            entry.evidence_ref
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::{
        WorkstreamPersistence, WorkstreamRecord, complete_workstream, upsert_workstream,
        write_inbox_digest,
    };

    fn init_fake_git_repo(dir: &Path, remote_url: &str) {
        let git_dir = dir.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(
            git_dir.join("config"),
            format!("[remote \"origin\"]\n\turl = {remote_url}\n"),
        )
        .unwrap();
    }

    #[test]
    fn project_bind_status_and_foreign_checkout_rejection() {
        let temp = tempfile::tempdir().unwrap();
        let projects_root = temp.path().join("System Store/services/projects");
        let primary = temp.path().join("work/alan");
        let foreign = temp.path().join("other/alan");
        std::fs::create_dir_all(&primary).unwrap();
        std::fs::create_dir_all(&foreign).unwrap();
        init_fake_git_repo(&primary, "git@github.com:example/alan.git");
        init_fake_git_repo(&foreign, "git@github.com:example/alan.git");

        // Unbound directory status fails cleanly.
        assert!(
            run_with_projects_root(
                &projects_root,
                &primary,
                ProjectArgs {
                    action: ProjectAction::Status {
                        path: None,
                        json: true,
                    },
                },
            )
            .is_err()
        );

        // Explicit bind succeeds and returns opaque project_key.
        let bind_out = run_with_projects_root(
            &projects_root,
            &primary,
            ProjectArgs {
                action: ProjectAction::Bind {
                    path: None,
                    grant_ref: Some("alan-repo".to_string()),
                    json: true,
                },
            },
        )
        .unwrap();
        let binding: serde_json::Value = serde_json::from_str(&bind_out).unwrap();
        let project_key = binding["project_key"].as_str().unwrap().to_string();
        assert_eq!(binding["grant_ref"], "alan-repo");

        // Foreign checkout sharing Git remote & basename NEVER auto-binds.
        assert!(
            run_with_projects_root(
                &projects_root,
                &foreign,
                ProjectArgs {
                    action: ProjectAction::Status {
                        path: None,
                        json: true,
                    },
                },
            )
            .is_err()
        );

        // Add workstreams + unread digest and verify status counts.
        let active = WorkstreamRecord::new(
            "api-work",
            "Implement endpoints",
            WorkstreamPersistence::Persistent,
        );
        upsert_workstream(&projects_root, &project_key, active).unwrap();

        let idle = WorkstreamRecord::new(
            "ui-polish",
            "Polish status bar",
            WorkstreamPersistence::Persistent,
        );
        upsert_workstream(&projects_root, &project_key, idle).unwrap();
        complete_workstream(&projects_root, &project_key, "ui-polish", None, None).unwrap();

        let one_off =
            WorkstreamRecord::new("fix-typo", "Fix README typo", WorkstreamPersistence::OneOff);
        upsert_workstream(&projects_root, &project_key, one_off).unwrap();
        complete_workstream(&projects_root, &project_key, "fix-typo", None, None).unwrap();

        write_inbox_digest(
            &projects_root,
            &project_key,
            InboxDigestEntry::new(
                "wf-1",
                "daily-check",
                None,
                InboxDigestStatus::Ok,
                "All checks passed",
                "/tmp/runs.ndjson",
            ),
        )
        .unwrap();

        let status_out = run_with_projects_root(
            &projects_root,
            &primary,
            ProjectArgs {
                action: ProjectAction::Status {
                    path: None,
                    json: true,
                },
            },
        )
        .unwrap();
        let status: serde_json::Value = serde_json::from_str(&status_out).unwrap();
        assert_eq!(status["project_key"], project_key);
        assert_eq!(status["counts"]["running"], 1);
        assert_eq!(status["counts"]["idle"], 1);
        assert_eq!(status["counts"]["archived"], 1);
        assert_eq!(status["active_non_archived"], 2);
        assert_eq!(status["unread_inbox_count"], 1);
    }

    #[test]
    fn project_inbox_list_and_ack_transitions() {
        let temp = tempfile::tempdir().unwrap();
        let projects_root = temp.path().join("System Store/services/projects");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let binding = bind_project_dir(&projects_root, &repo, None).unwrap();
        let key = &binding.project_key;

        for (run_id, workflow_id) in [("wf-run-1", "nightly"), ("wf-run-2", "weekly")] {
            write_inbox_digest(
                &projects_root,
                key,
                InboxDigestEntry::new(
                    run_id,
                    workflow_id,
                    None,
                    InboxDigestStatus::Ok,
                    format!("Finished {run_id}"),
                    "/tmp/runs.ndjson",
                ),
            )
            .unwrap();
        }

        // List unread shows both.
        let list_out = run_with_projects_root(
            &projects_root,
            &repo,
            ProjectArgs {
                action: ProjectAction::Inbox {
                    path: None,
                    ack: None,
                    ack_all: false,
                    all: false,
                    json: true,
                },
            },
        )
        .unwrap();
        let list_json: serde_json::Value = serde_json::from_str(&list_out).unwrap();
        assert_eq!(list_json["unread_count"], 2);
        assert_eq!(list_json["entries"].as_array().unwrap().len(), 2);

        // Ack single run_id sets digested_at and decrements unread count.
        let ack_one_out = run_with_projects_root(
            &projects_root,
            &repo,
            ProjectArgs {
                action: ProjectAction::Inbox {
                    path: None,
                    ack: Some("wf-run-1".to_string()),
                    ack_all: false,
                    all: false,
                    json: true,
                },
            },
        )
        .unwrap();
        let ack_one_json: serde_json::Value = serde_json::from_str(&ack_one_out).unwrap();
        assert_eq!(ack_one_json["unread_count"], 1);
        assert_eq!(ack_one_json["acknowledged_count"], 1);
        assert!(ack_one_json["entries"][0]["digested_at"].is_string());

        // Ack all clears remaining unread digests.
        let ack_all_out = run_with_projects_root(
            &projects_root,
            &repo,
            ProjectArgs {
                action: ProjectAction::Inbox {
                    path: None,
                    ack: None,
                    ack_all: true,
                    all: false,
                    json: true,
                },
            },
        )
        .unwrap();
        let ack_all_json: serde_json::Value = serde_json::from_str(&ack_all_out).unwrap();
        assert_eq!(ack_all_json["unread_count"], 0);
        assert_eq!(ack_all_json["acknowledged_count"], 1);
        assert_eq!(ack_all_json["entries"][0]["run_id"], "wf-run-2");
        assert!(ack_all_json["entries"][0]["digested_at"].is_string());

        // Default list is now empty, while --all shows both digested entries.
        let unread_after = run_with_projects_root(
            &projects_root,
            &repo,
            ProjectArgs {
                action: ProjectAction::Inbox {
                    path: None,
                    ack: None,
                    ack_all: false,
                    all: false,
                    json: true,
                },
            },
        )
        .unwrap();
        let unread_after_json: serde_json::Value = serde_json::from_str(&unread_after).unwrap();
        assert_eq!(unread_after_json["unread_count"], 0);
        assert_eq!(unread_after_json["entries"].as_array().unwrap().len(), 0);

        let all_after = run_with_projects_root(
            &projects_root,
            &repo,
            ProjectArgs {
                action: ProjectAction::Inbox {
                    path: None,
                    ack: None,
                    ack_all: false,
                    all: true,
                    json: true,
                },
            },
        )
        .unwrap();
        let all_after_json: serde_json::Value = serde_json::from_str(&all_after).unwrap();
        assert_eq!(all_after_json["unread_count"], 0);
        assert_eq!(all_after_json["entries"].as_array().unwrap().len(), 2);
    }
}
