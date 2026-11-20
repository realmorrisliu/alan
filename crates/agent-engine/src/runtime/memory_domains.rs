//! Domain-scoped Memory Store isolation and path resolution.
//!
//! Each [`MemoryDomain`] maps to an isolated subtree under
//! `SystemStorePaths::memory_stores()`:
//! - `default` -> `stores/default`
//! - `personal` -> `stores/personal`
//! - `continuity` -> `stores/continuity`
//! - `app:<id>` -> `stores/app/<id>`
//! - `project:<slug>` -> `stores/project/<slug>`
//!
//! Automatic runtime recall MUST stay confined to the active domain; cross-domain
//! recall happens only when a caller explicitly specifies target domains.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use super::memory_surfaces::project_summary::derive_project_key;

const MAX_DOMAIN_SLUG_LEN: usize = 64;

/// Isolated ownership domain for a Memory Store under `services/memory/stores`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryDomain {
    /// Legacy/default interactive Memory Store (`stores/default`).
    Default,
    /// Personal user preferences and identity (`stores/personal`).
    Personal,
    /// Cross-process continuity and handoff state (`stores/continuity`).
    Continuity,
    /// App-scoped isolated store (`stores/app/<id>`).
    App(String),
    /// Project-scoped isolated store (`stores/project/<slug>`).
    Project(String),
}

impl MemoryDomain {
    /// Parse a domain selector string such as `default`, `personal`, `continuity`,
    /// `app:<id>`, `app/<id>`, `project:<slug>`, or `project/<slug>`.
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        match trimmed {
            "default" => Ok(Self::Default),
            "personal" => Ok(Self::Personal),
            "continuity" => Ok(Self::Continuity),
            _ => {
                if let Some(id) = trimmed
                    .strip_prefix("app:")
                    .or_else(|| trimmed.strip_prefix("app/"))
                {
                    Self::app(id)
                } else if let Some(slug) = trimmed
                    .strip_prefix("project:")
                    .or_else(|| trimmed.strip_prefix("project/"))
                {
                    Self::project(slug)
                } else {
                    bail!(
                        "unsupported memory domain `{trimmed}`; expected `default`, `personal`, `continuity`, `app:<id>`, or `project:<slug>`"
                    )
                }
            }
        }
    }

    /// Construct a validated app domain.
    pub fn app(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        validate_domain_slug("app", &id)?;
        Ok(Self::App(id))
    }

    /// Construct a validated project domain.
    pub fn project(slug: impl Into<String>) -> Result<Self> {
        let slug = slug.into();
        validate_domain_slug("project", &slug)?;
        Ok(Self::Project(slug))
    }

    /// Derive a deterministic project domain from an Alan OS project cwd.
    pub fn project_for_cwd(project_cwd: &str) -> Self {
        Self::Project(derive_project_key(Some(project_cwd)))
    }

    /// Canonical CLI/API label for this domain.
    pub fn label(&self) -> String {
        match self {
            Self::Default => "default".to_string(),
            Self::Personal => "personal".to_string(),
            Self::Continuity => "continuity".to_string(),
            Self::App(id) => format!("app:{id}"),
            Self::Project(slug) => format!("project:{slug}"),
        }
    }

    /// Relative subtree under `SystemStorePaths::memory_stores()`.
    pub fn store_relative_path(&self) -> PathBuf {
        match self {
            Self::Default => PathBuf::from("default"),
            Self::Personal => PathBuf::from("personal"),
            Self::Continuity => PathBuf::from("continuity"),
            Self::App(id) => PathBuf::from("app").join(id),
            Self::Project(slug) => PathBuf::from("project").join(slug),
        }
    }

    /// Resolve this domain's backing directory beneath `stores_root`, rejecting
    /// symlinks and path traversal along any existing path prefix.
    pub fn resolve_store_dir(&self, stores_root: &Path) -> Result<PathBuf> {
        resolve_domain_store_dir(stores_root, self)
    }
}

impl std::fmt::Display for MemoryDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

impl std::str::FromStr for MemoryDomain {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Explicit boundary controlling which Memory Domains a recall query may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainRecallScope {
    /// Strict single-domain boundary (default for automatic turn recall).
    ActiveOnly(MemoryDomain),
    /// Caller-selected list of domains (no implicit expansion).
    Explicit(Vec<MemoryDomain>),
}

impl DomainRecallScope {
    /// Return the deduplicated ordered domains permitted under this scope.
    pub fn resolved_domains(&self) -> Vec<MemoryDomain> {
        match self {
            Self::ActiveOnly(domain) => vec![domain.clone()],
            Self::Explicit(domains) => {
                let mut seen = BTreeSet::new();
                domains
                    .iter()
                    .filter(|domain| seen.insert((*domain).clone()))
                    .cloned()
                    .collect()
            }
        }
    }

    /// Check whether `candidate` is authorized under this scope.
    pub fn allows(&self, candidate: &MemoryDomain) -> bool {
        match self {
            Self::ActiveOnly(active) => active == candidate,
            Self::Explicit(domains) => domains.iter().any(|allowed| allowed == candidate),
        }
    }
}

/// Validate a domain identifier segment (`app:<id>` or `project:<slug>`).
pub fn validate_domain_slug(kind: &str, slug: &str) -> Result<()> {
    ensure!(
        !slug.is_empty() && slug.len() <= MAX_DOMAIN_SLUG_LEN,
        "{kind} domain id must be 1..={MAX_DOMAIN_SLUG_LEN} characters: `{slug}`"
    );
    ensure!(
        slug.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric()),
        "{kind} domain id must start with an ASCII alphanumeric character: `{slug}`"
    );
    ensure!(
        slug.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "{kind} domain id `{slug}` contains invalid characters (allowed: [a-zA-Z0-9_-])"
    );
    Ok(())
}

/// Safely resolve `stores_root/<domain_subpath>` without following symlinks or escaping `stores_root`.
pub fn resolve_domain_store_dir(stores_root: &Path, domain: &MemoryDomain) -> Result<PathBuf> {
    ensure!(
        !stores_root
            .components()
            .any(|component| matches!(component, Component::ParentDir)),
        "memory stores root must not contain `..`: {}",
        stores_root.display()
    );

    let relative = domain.store_relative_path();
    let target = stores_root.join(&relative);

    if let Ok(meta) = fs::symlink_metadata(stores_root) {
        ensure!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "memory stores root is not a real directory: {}",
            stores_root.display()
        );
        let canonical_root = fs::canonicalize(stores_root).with_context(|| {
            format!("canonicalize memory stores root {}", stores_root.display())
        })?;
        let mut cursor = stores_root.to_path_buf();
        for component in relative.components() {
            cursor.push(component.as_os_str());
            match fs::symlink_metadata(&cursor) {
                Ok(meta) => {
                    ensure!(
                        meta.is_dir() && !meta.file_type().is_symlink(),
                        "memory domain path must be a real directory without symlinks: {}",
                        cursor.display()
                    );
                    let canonical_cursor = fs::canonicalize(&cursor)?;
                    ensure!(
                        canonical_cursor.starts_with(&canonical_root),
                        "memory domain path escapes stores root: {}",
                        cursor.display()
                    );
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!("inspect memory domain path {}", cursor.display())
                    });
                }
            }
        }
    }

    Ok(target)
}

/// Enumerate existing valid memory domains under `stores_root` (sorted deterministically).
pub fn list_existing_memory_domains(stores_root: &Path) -> Result<Vec<MemoryDomain>> {
    match fs::symlink_metadata(stores_root) {
        Ok(meta) => {
            ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "memory stores root is not a real directory: {}",
                stores_root.display()
            );
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("inspect memory stores root {}", stores_root.display()));
        }
    }

    let mut domains = Vec::new();
    for (name, domain) in [
        ("default", MemoryDomain::Default),
        ("personal", MemoryDomain::Personal),
        ("continuity", MemoryDomain::Continuity),
    ] {
        let dir = stores_root.join(name);
        if is_real_directory(&dir)? {
            domains.push(domain);
        }
    }

    collect_prefixed_domains(
        &stores_root.join("app"),
        "app",
        &mut domains,
        MemoryDomain::App,
    )?;
    collect_prefixed_domains(
        &stores_root.join("project"),
        "project",
        &mut domains,
        MemoryDomain::Project,
    )?;

    domains.sort();
    domains.dedup();
    Ok(domains)
}

fn collect_prefixed_domains(
    parent: &Path,
    kind: &str,
    out: &mut Vec<MemoryDomain>,
    build: impl Fn(String) -> MemoryDomain,
) -> Result<()> {
    match fs::symlink_metadata(parent) {
        Ok(meta) => {
            ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "{kind} domain directory must not be a symlink: {}",
                parent.display()
            );
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("inspect {kind} domain dir {}", parent.display()));
        }
    }

    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if validate_domain_slug(kind, name).is_ok() {
            out.push(build(name.to_string()));
        }
    }
    Ok(())
}

fn is_real_directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(meta.is_dir() && !meta.file_type().is_symlink()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("inspect directory {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn domain_parsing_and_relative_paths_validate_cleanly() {
        assert_eq!(
            MemoryDomain::parse("default").unwrap(),
            MemoryDomain::Default
        );
        assert_eq!(
            MemoryDomain::parse("personal").unwrap(),
            MemoryDomain::Personal
        );
        assert_eq!(
            MemoryDomain::parse("continuity").unwrap(),
            MemoryDomain::Continuity
        );
        assert_eq!(
            MemoryDomain::parse("app:slowpage").unwrap(),
            MemoryDomain::App("slowpage".to_string())
        );
        assert_eq!(
            MemoryDomain::parse("project/alan-repo").unwrap(),
            MemoryDomain::Project("alan-repo".to_string())
        );

        assert!(MemoryDomain::parse("app:../escape").is_err());
        assert!(MemoryDomain::parse("project:.hidden").is_err());
        assert!(MemoryDomain::parse("unknown").is_err());

        assert_eq!(
            MemoryDomain::App("slowpage".to_string()).store_relative_path(),
            PathBuf::from("app/slowpage")
        );
        assert_eq!(
            MemoryDomain::Project("alan".to_string()).store_relative_path(),
            PathBuf::from("project/alan")
        );
    }

    #[test]
    fn domain_recall_scope_enforces_strict_isolation() {
        let active = MemoryDomain::Project("alpha".to_string());
        let scope = DomainRecallScope::ActiveOnly(active.clone());
        assert!(scope.allows(&active));
        assert!(!scope.allows(&MemoryDomain::Project("beta".to_string())));
        assert!(!scope.allows(&MemoryDomain::App("slowpage".to_string())));
        assert!(!scope.allows(&MemoryDomain::Personal));
        assert_eq!(scope.resolved_domains(), vec![active]);
    }

    #[test]
    fn list_and_resolve_domains_enumerate_real_directories() {
        let temp = TempDir::new().unwrap();
        let stores = temp.path().join("stores");
        fs::create_dir_all(stores.join("personal")).unwrap();
        fs::create_dir_all(stores.join("app/slowpage")).unwrap();
        fs::create_dir_all(stores.join("project/alan")).unwrap();

        let listed = list_existing_memory_domains(&stores).unwrap();
        assert_eq!(
            listed,
            vec![
                MemoryDomain::Personal,
                MemoryDomain::App("slowpage".to_string()),
                MemoryDomain::Project("alan".to_string()),
            ]
        );
        assert_eq!(
            resolve_domain_store_dir(&stores, &MemoryDomain::App("slowpage".to_string())).unwrap(),
            stores.join("app/slowpage")
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_domain_store_dir_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let stores = temp.path().join("stores");
        let outside = temp.path().join("outside");
        fs::create_dir_all(stores.join("app")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, stores.join("app/evil")).unwrap();

        let err =
            resolve_domain_store_dir(&stores, &MemoryDomain::App("evil".to_string())).unwrap_err();
        assert!(err.to_string().contains("symlink"));
    }
}
