use std::path::{Component, Path, PathBuf};

use alan_os_host::{SavedProjectGrantRecord, SavedProjectGrantStore};
use alan_service_manager::HostMountAccess;
use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedOneShotCwd {
    pub(super) namespace_path: PathBuf,
    pub(super) host_path: PathBuf,
}

pub(super) fn resolve_oneshot_grant_cwd(
    store: Option<&SavedProjectGrantStore>,
    grants: &[String],
    explicit_cwd: Option<&Path>,
    ambient_cwd: Option<&Path>,
) -> Result<Option<ResolvedOneShotCwd>> {
    if grants.is_empty() {
        if let Some(cwd) = explicit_cwd {
            bail!("{}", ungranted_cwd_error_message(store, cwd));
        }
        return Ok(None);
    }

    let store = store.context("Host Mount saved grant store is unavailable")?;
    let mut active = Vec::with_capacity(grants.len());
    for grant_id in grants {
        active.push(store.revalidate_grant(grant_id)?);
    }

    if let Some(cwd) = explicit_cwd {
        if let Some(resolved) = resolve_against_grants(&active, cwd)? {
            return Ok(Some(resolved));
        }
        bail!("{}", ungranted_cwd_error_message(Some(store), cwd));
    }

    if let Some(ambient) = ambient_cwd
        && let Some(resolved) = resolve_host_against_grants(&active, ambient)
    {
        return Ok(Some(resolved));
    }

    let selected = active
        .iter()
        .find(|grant| grant.access == HostMountAccess::ReadWrite)
        .or_else(|| active.first())
        .context("no activated Host Mount grant available")?;
    Ok(Some(ResolvedOneShotCwd {
        namespace_path: PathBuf::from(&selected.namespace_path),
        host_path: selected.canonical_path.clone(),
    }))
}

fn resolve_against_grants(
    grants: &[SavedProjectGrantRecord],
    requested: &Path,
) -> Result<Option<ResolvedOneShotCwd>> {
    if let Some(normalized) = normalize_absolute_namespace_path(requested)
        && normalized.starts_with("/mnt/grants")
        && let Some(grant) = grants
            .iter()
            .filter(|grant| normalized.starts_with(&grant.namespace_path))
            .max_by_key(|grant| Path::new(&grant.namespace_path).components().count())
    {
        let suffix = normalized
            .strip_prefix(&grant.namespace_path)
            .expect("matched namespace prefix");
        let candidate_host = grant.canonical_path.join(suffix);
        let canonical_host = std::fs::canonicalize(&candidate_host).with_context(|| {
            format!(
                "requested `--cwd` directory `{}` does not exist under grant `{}`",
                requested.display(),
                grant.id
            )
        })?;
        anyhow::ensure!(
            canonical_host.is_dir(),
            "requested `--cwd` `{}` is not a directory",
            requested.display()
        );
        anyhow::ensure!(
            canonical_host.starts_with(&grant.canonical_path),
            "requested `--cwd` `{}` escapes grant `{}`",
            requested.display(),
            grant.id
        );
        let verified_suffix = canonical_host
            .strip_prefix(&grant.canonical_path)
            .expect("verified host containment");
        return Ok(Some(ResolvedOneShotCwd {
            namespace_path: Path::new(&grant.namespace_path).join(verified_suffix),
            host_path: canonical_host,
        }));
    }

    Ok(resolve_host_against_grants(grants, requested))
}

fn resolve_host_against_grants(
    grants: &[SavedProjectGrantRecord],
    host_dir: &Path,
) -> Option<ResolvedOneShotCwd> {
    let canonical = std::fs::canonicalize(host_dir).ok()?;
    if !canonical.is_dir() {
        return None;
    }
    let matched = grants
        .iter()
        .filter(|grant| canonical.starts_with(&grant.canonical_path))
        .max_by_key(|grant| grant.canonical_path.components().count())?;
    let suffix = canonical
        .strip_prefix(&matched.canonical_path)
        .expect("matched canonical prefix");
    Some(ResolvedOneShotCwd {
        namespace_path: Path::new(&matched.namespace_path).join(suffix),
        host_path: canonical,
    })
}

fn normalize_absolute_namespace_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(part) => normalized.push(part),
            Component::Prefix(_) => return None,
        }
    }
    Some(normalized)
}

fn ungranted_cwd_error_message(store: Option<&SavedProjectGrantStore>, cwd: &Path) -> String {
    let display_path = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    if let Some(store) = store
        && let Ok(saved) = store.list_grants()
    {
        let active_saved: Vec<_> = saved.into_iter().filter(|g| !g.revoked).collect();
        if let Some(existing) = resolve_host_against_grants(&active_saved, cwd)
            && let Some(grant) = active_saved
                .iter()
                .find(|g| existing.namespace_path.starts_with(&g.namespace_path))
        {
            return format!(
                "Directory `{}` is not mounted in non-interactive mode. Pass the saved grant with `--grant {}` (or run `alan` interactively in a TTY).",
                display_path.display(),
                grant.id
            );
        }
    }

    format!(
        "Directory `{}` is not mounted by an explicit grant in non-interactive mode. Save a grant with `alan host grant save <id> {}` and pass `--grant <id>` (or run `alan` interactively in a TTY).",
        display_path.display(),
        display_path.display()
    )
}
