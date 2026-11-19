use std::path::PathBuf;

use alan_os_host::{SavedGitPushRule, SavedProjectGrantRecord, SavedProjectGrantStore};
use alan_service_manager::HostMountAccess;
use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};

/// Explicit runtime directory of a live Alan instance selected by its native caller.
pub const INSTANCE_RUNTIME_DIR_ENV: &str = "ALAN_INSTANCE_RUNTIME_DIR";

pub fn explicit_instance_paths() -> Result<alan_os_host::HostEndpointPaths> {
    let runtime = std::env::var_os(INSTANCE_RUNTIME_DIR_ENV)
        .context("select a live Alan instance with ALAN_INSTANCE_RUNTIME_DIR")?;
    alan_os_host::HostEndpointPaths::from_runtime_dir(&PathBuf::from(runtime))
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum HostGrantAccessArg {
    ReadOnly,
    ReadWrite,
}

impl From<HostGrantAccessArg> for HostMountAccess {
    fn from(value: HostGrantAccessArg) -> Self {
        match value {
            HostGrantAccessArg::ReadOnly => Self::ReadOnly,
            HostGrantAccessArg::ReadWrite => Self::ReadWrite,
        }
    }
}

/// Manage explicit System Store project delegation grants (`alan host grant ...`).
#[derive(Subcommand, Debug)]
pub enum HostGrantAction {
    /// Save an explicit standing project delegation grant in System Store
    Save {
        /// Stable grant identifier (`[a-zA-Z0-9_-]`)
        id: String,
        /// Existing Host directory path (symlinks rejected)
        host_path: PathBuf,
        /// Access level (`read-only` or `read-write`)
        #[arg(long, value_enum, default_value_t = HostGrantAccessArg::ReadWrite)]
        access: HostGrantAccessArg,
        /// Optional human-readable grant label
        #[arg(long)]
        label: Option<String>,
        /// Optional remote name for scoped non-destructive `git push` delegation
        #[arg(long = "git-push-remote")]
        git_push_remote: Option<String>,
        /// Optional branch name or trailing-`*` prefix allowed for ordinary `git push`
        #[arg(long = "git-push-branch")]
        git_push_branches: Vec<String>,
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// List saved System Store project delegation grants
    List {
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Revoke a saved project delegation grant (and revoke on any selected live instance)
    Revoke {
        /// Grant identifier to revoke
        id: String,
        /// Emit structured JSON
        #[arg(long)]
        json: bool,
    },
}

/// Execute `alan host grant ...` against the System Store (and live instance if selected).
pub async fn run_host_grant_action(action: HostGrantAction) -> Result<()> {
    let store = SavedProjectGrantStore::system()?;
    run_host_grant_action_with_store(&store, action, std::env::var_os(INSTANCE_RUNTIME_DIR_ENV))
        .await
}

pub(crate) async fn run_host_grant_action_with_store(
    store: &SavedProjectGrantStore,
    action: HostGrantAction,
    live_runtime_dir: Option<std::ffi::OsString>,
) -> Result<()> {
    match action {
        HostGrantAction::Save {
            id,
            host_path,
            access,
            label,
            git_push_remote,
            git_push_branches,
            json,
        } => {
            let mut git_push_rules = Vec::new();
            if !git_push_branches.is_empty() {
                git_push_rules.push(SavedGitPushRule {
                    remote: git_push_remote,
                    branches: git_push_branches,
                });
            } else if git_push_remote.is_some() {
                anyhow::bail!("`--git-push-remote` requires at least one `--git-push-branch`");
            }
            let record = store.save_grant(
                &id,
                &host_path,
                access.into(),
                label.as_deref(),
                git_push_rules,
            )?;
            print_grant_record(&record, "saved", json)?;
        }
        HostGrantAction::List { json } => {
            let grants = store.list_grants()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&grants)?);
            } else if grants.is_empty() {
                println!("No saved project grants.");
            } else {
                for grant in &grants {
                    let status = if grant.revoked { "revoked" } else { "active" };
                    println!(
                        "{} [{}] rev={} {} -> {} ({:?}, dev={}, ino={})",
                        grant.id,
                        status,
                        grant.revision,
                        grant.namespace_path,
                        grant.canonical_path.display(),
                        grant.access,
                        grant.dev,
                        grant.ino
                    );
                }
            }
        }
        HostGrantAction::Revoke { id, json } => {
            let record = store.revoke_grant(&id)?;
            if let Some(runtime_dir) = live_runtime_dir {
                let paths =
                    alan_os_host::HostEndpointPaths::from_runtime_dir(&PathBuf::from(runtime_dir))?;
                if let Err(error) = alan_os_host::HostCommandPlane::new(paths)
                    .revoke_host_mount(&id)
                    .await
                {
                    tracing::debug!(%error, grant_id = %id, "live host mount revoke skipped or unavailable");
                }
            }
            print_grant_record(&record, "revoked", json)?;
        }
    }
    Ok(())
}

fn print_grant_record(record: &SavedProjectGrantRecord, verb: &str, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(record)?);
    } else {
        println!(
            "{verb} grant `{}` -> {} (namespace={}, rev={}, dev={}, ino={})",
            record.id,
            record.canonical_path.display(),
            record.namespace_path,
            record.revision,
            record.dev,
            record.ino
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_grant_cli_round_trip_save_list_and_revoke() {
        let store_dir = tempfile::tempdir().unwrap();
        let store = SavedProjectGrantStore::from_path(store_dir.path().join("grants.json"));
        let project = tempfile::tempdir().unwrap();

        run_host_grant_action_with_store(
            &store,
            HostGrantAction::Save {
                id: "my-repo".into(),
                host_path: project.path().to_path_buf(),
                access: HostGrantAccessArg::ReadWrite,
                label: Some("My Repo".into()),
                git_push_remote: Some("origin".into()),
                git_push_branches: vec!["feat/*".into()],
                json: false,
            },
            None,
        )
        .await
        .unwrap();

        let listed = store.list_grants().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "my-repo");
        assert!(!listed[0].revoked);
        assert_eq!(listed[0].git_push_rules.len(), 1);

        run_host_grant_action_with_store(&store, HostGrantAction::List { json: true }, None)
            .await
            .unwrap();

        run_host_grant_action_with_store(
            &store,
            HostGrantAction::Revoke {
                id: "my-repo".into(),
                json: false,
            },
            None,
        )
        .await
        .unwrap();

        assert!(store.get_grant("my-repo").unwrap().revoked);
        assert!(store.revalidate_grant("my-repo").is_err());
    }
}
