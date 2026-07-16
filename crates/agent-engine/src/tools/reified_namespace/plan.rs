//! Pure planning model for Linux reified namespace execution.
//!
//! This module translates host-backed mount authority into a stable plan that
//! can be validated on any host before a native runner consumes it.

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use super::super::sandbox::NetworkPosture;

/// Default namespace path used only by the single-mount convenience builder.
pub const DEFAULT_PRIMARY_MOUNT_NAMESPACE_PATH: &str = "/mnt/source";

/// Default namespace path for the private scratch/tmp mount.
pub const DEFAULT_SCRATCH_TMP_NAMESPACE_PATH: &str = "/tmp";

/// Access mode for a declared host-backed mount in the reified namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReifiedMountAccess {
    ReadOnly,
    ReadWrite,
}

impl ReifiedMountAccess {
    /// Stable audit label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }

    /// Whether the mount permits writes.
    pub const fn is_writable(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

/// Source behind a namespace declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReifiedMountSource {
    /// A host path that can be bind-mounted into the native subprocess view.
    Host(PathBuf),
    /// A pure Alan OS/aP file tree that is not exposed as a native filesystem path.
    Virtual,
}

/// Mount declaration supplied by sandbox authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedMountDeclaration {
    pub namespace_path: PathBuf,
    pub source: ReifiedMountSource,
    pub access: ReifiedMountAccess,
}

impl ReifiedMountDeclaration {
    /// Declare a host-backed namespace mount.
    pub fn host(
        namespace_path: impl Into<PathBuf>,
        host_path: impl Into<PathBuf>,
        access: ReifiedMountAccess,
    ) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            source: ReifiedMountSource::Host(host_path.into()),
            access,
        }
    }

    /// Declare a virtual Alan OS mount that must be excluded from native reification.
    pub fn virtual_mount(namespace_path: impl Into<PathBuf>) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            source: ReifiedMountSource::Virtual,
            access: ReifiedMountAccess::ReadOnly,
        }
    }
}

/// Host-backed mount included in the reified subprocess view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedHostMount {
    pub namespace_path: PathBuf,
    pub host_path: PathBuf,
    pub access: ReifiedMountAccess,
}

/// Read-only host path needed to execute common programs inside the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedExecutionSubstrateMount {
    pub namespace_path: PathBuf,
    pub host_path: PathBuf,
}

impl ReifiedExecutionSubstrateMount {
    /// Declare a read-only execution substrate mount.
    pub fn new(namespace_path: impl Into<PathBuf>, host_path: impl Into<PathBuf>) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            host_path: host_path.into(),
        }
    }
}

/// Private scratch/tmp mount inside the reified view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedScratchTmpMount {
    pub namespace_path: PathBuf,
}

/// Pure plan consumed by the future Linux namespace runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedNamespacePlan {
    pub declared_host_mounts: Vec<ReifiedHostMount>,
    pub execution_substrate: Vec<ReifiedExecutionSubstrateMount>,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub scratch_tmp: ReifiedScratchTmpMount,
    pub network: NetworkPosture,
}

impl ReifiedNamespacePlan {
    /// Derive a plan from namespace mount authority and a projected host cwd.
    pub fn derive(input: ReifiedNamespacePlanInput) -> Result<Self, ReifiedNamespacePlanError> {
        let mut declared_host_mounts = Vec::new();
        let mut virtual_namespace_paths = Vec::new();
        for declaration in input.declarations {
            validate_namespace_path(&declaration.namespace_path)?;
            match declaration.source {
                ReifiedMountSource::Host(host_path) => {
                    let host_path =
                        validate_and_normalize_host_source_path(&host_path, "host mount")?;
                    declared_host_mounts.push(ReifiedHostMount {
                        namespace_path: declaration.namespace_path,
                        host_path,
                        access: declaration.access,
                    });
                }
                ReifiedMountSource::Virtual => {
                    virtual_namespace_paths.push(declaration.namespace_path);
                }
            }
        }
        let mut execution_substrate = input.execution_substrate;
        for mount in &mut execution_substrate {
            validate_namespace_path(&mount.namespace_path)?;
            mount.host_path =
                validate_and_normalize_host_source_path(&mount.host_path, "execution substrate")?;
        }
        validate_namespace_path(&input.scratch_tmp_namespace_path)?;
        validate_absolute_path(&input.cwd, "cwd")?;
        let projected_cwd = canonicalize_existing_host_path(&input.cwd);

        let mut namespace_paths = declared_host_mounts
            .iter()
            .map(|mount| mount.namespace_path.as_path())
            .collect::<Vec<_>>();
        namespace_paths.extend(virtual_namespace_paths.iter().map(|path| path.as_path()));
        namespace_paths.extend(
            execution_substrate
                .iter()
                .map(|mount| mount.namespace_path.as_path()),
        );
        namespace_paths.push(input.scratch_tmp_namespace_path.as_path());
        validate_no_overlapping_namespace_paths(&namespace_paths)?;
        validate_no_mixed_access_host_mount_overlap(&declared_host_mounts)?;
        validate_no_writable_mount_over_execution_substrate(
            &declared_host_mounts,
            &execution_substrate,
        )?;

        let cwd = translate_host_path_with_mounts(&declared_host_mounts, &projected_cwd)
            .ok_or_else(|| ReifiedNamespacePlanError::CwdOutsideView {
                cwd: input.cwd.clone(),
            })?;

        Ok(Self {
            declared_host_mounts,
            execution_substrate,
            cwd,
            argv: input.argv,
            scratch_tmp: ReifiedScratchTmpMount {
                namespace_path: input.scratch_tmp_namespace_path,
            },
            network: input.network,
        })
    }

    /// Build a plan for one explicit writable Host Mount.
    pub fn primary_mount(
        host_root: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        argv: Vec<String>,
        network: NetworkPosture,
    ) -> Result<Self, ReifiedNamespacePlanError> {
        let host_root = host_root.into();
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                DEFAULT_PRIMARY_MOUNT_NAMESPACE_PATH,
                host_root,
                ReifiedMountAccess::ReadWrite,
            )],
            cwd,
            argv,
            network,
        );
        Self::derive(input)
    }

    /// Translate a projected host path into the reified namespace view.
    pub fn translate_projected_host_path(&self, host_path: &Path) -> Option<PathBuf> {
        if !host_path.is_absolute() || contains_parent_component(host_path) {
            return None;
        }

        let projected_host_path = canonicalize_existing_host_path(host_path);
        translate_host_path_with_mounts(&self.declared_host_mounts, &projected_host_path)
    }
}

/// Input used to derive a reified namespace plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedNamespacePlanInput {
    pub declarations: Vec<ReifiedMountDeclaration>,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub network: NetworkPosture,
    pub execution_substrate: Vec<ReifiedExecutionSubstrateMount>,
    pub scratch_tmp_namespace_path: PathBuf,
}

impl ReifiedNamespacePlanInput {
    /// Create input with the default Linux execution substrate and scratch mount.
    pub fn new(
        declarations: Vec<ReifiedMountDeclaration>,
        cwd: impl Into<PathBuf>,
        argv: Vec<String>,
        network: NetworkPosture,
    ) -> Self {
        Self {
            declarations,
            cwd: cwd.into(),
            argv,
            network,
            execution_substrate: default_execution_substrate(),
            scratch_tmp_namespace_path: PathBuf::from(DEFAULT_SCRATCH_TMP_NAMESPACE_PATH),
        }
    }

    /// Override the read-only execution substrate list.
    pub fn with_execution_substrate(
        mut self,
        execution_substrate: Vec<ReifiedExecutionSubstrateMount>,
    ) -> Self {
        self.execution_substrate = execution_substrate;
        self
    }

    /// Override the scratch/tmp namespace path.
    pub fn with_scratch_tmp_namespace_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.scratch_tmp_namespace_path = path.into();
        self
    }
}

/// Stable default execution substrate. Each entry is mounted read-only by the runner.
pub fn default_execution_substrate() -> Vec<ReifiedExecutionSubstrateMount> {
    [
        ("/bin", "/bin"),
        ("/sbin", "/sbin"),
        ("/usr/bin", "/usr/bin"),
        ("/usr/sbin", "/usr/sbin"),
        ("/usr/local/bin", "/usr/local/bin"),
        ("/usr/local/sbin", "/usr/local/sbin"),
        ("/lib", "/lib"),
        ("/lib64", "/lib64"),
        ("/usr/lib", "/usr/lib"),
        ("/usr/lib64", "/usr/lib64"),
        ("/usr/local/lib", "/usr/local/lib"),
        ("/usr/local/lib64", "/usr/local/lib64"),
        ("/etc/ssl", "/etc/ssl"),
        ("/etc/hosts", "/etc/hosts"),
        ("/etc/resolv.conf", "/etc/resolv.conf"),
    ]
    .into_iter()
    .map(|(namespace_path, host_path)| {
        ReifiedExecutionSubstrateMount::new(namespace_path, host_path)
    })
    .collect()
}

/// Errors produced while deriving a pure reified namespace plan.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ReifiedNamespacePlanError {
    #[error("{kind} path must be absolute: {path:?}")]
    RelativePath { kind: &'static str, path: PathBuf },
    #[error("{kind} path must not contain '..': {path:?}")]
    ParentPath { kind: &'static str, path: PathBuf },
    #[error("namespace path must not be root")]
    RootNamespacePath,
    #[error("{kind} path must not be root")]
    RootHostSourcePath { kind: &'static str },
    #[error("namespace mount path overlaps another mount: {child:?} shadows {parent:?}")]
    NamespaceMountOverlap { parent: PathBuf, child: PathBuf },
    #[error(
        "read-only host mount {read_only_host_path:?} overlaps writable mount {writable_host_path:?}"
    )]
    ReadOnlyHostMountOverlapsWritableMount {
        read_only_host_path: PathBuf,
        writable_host_path: PathBuf,
    },
    #[error(
        "writable host mount {writable_host_path:?} overlaps execution substrate {substrate_host_path:?}"
    )]
    WritableHostMountOverlapsExecutionSubstrate {
        writable_host_path: PathBuf,
        substrate_host_path: PathBuf,
    },
    #[error("cwd is outside the reified host mount view: {cwd:?}")]
    CwdOutsideView { cwd: PathBuf },
}

fn translate_host_path_with_mounts(
    mounts: &[ReifiedHostMount],
    host_path: &Path,
) -> Option<PathBuf> {
    if !host_path.is_absolute() || contains_parent_component(host_path) {
        return None;
    }

    let mount = mounts
        .iter()
        .filter(|mount| host_path.starts_with(&mount.host_path))
        .max_by_key(|mount| mount.host_path.components().count())?;
    let suffix = host_path.strip_prefix(&mount.host_path).ok()?;
    if suffix.as_os_str().is_empty() {
        Some(mount.namespace_path.clone())
    } else {
        Some(mount.namespace_path.join(suffix))
    }
}

fn validate_no_overlapping_namespace_paths(
    paths: &[&Path],
) -> Result<(), ReifiedNamespacePlanError> {
    for (index, &left) in paths.iter().enumerate() {
        for &right in &paths[index + 1..] {
            if left == right {
                return Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                    parent: left.to_path_buf(),
                    child: right.to_path_buf(),
                });
            }
            if right.starts_with(left) {
                return Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                    parent: left.to_path_buf(),
                    child: right.to_path_buf(),
                });
            }
            if left.starts_with(right) {
                return Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                    parent: right.to_path_buf(),
                    child: left.to_path_buf(),
                });
            }
        }
    }
    Ok(())
}

fn validate_no_mixed_access_host_mount_overlap(
    mounts: &[ReifiedHostMount],
) -> Result<(), ReifiedNamespacePlanError> {
    for (index, left) in mounts.iter().enumerate() {
        for right in &mounts[index + 1..] {
            if !paths_overlap(&left.host_path, &right.host_path) {
                continue;
            }

            match (left.access.is_writable(), right.access.is_writable()) {
                (true, false) => {
                    return Err(
                        ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                            read_only_host_path: right.host_path.clone(),
                            writable_host_path: left.host_path.clone(),
                        },
                    );
                }
                (false, true) => {
                    return Err(
                        ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                            read_only_host_path: left.host_path.clone(),
                            writable_host_path: right.host_path.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_no_writable_mount_over_execution_substrate(
    mounts: &[ReifiedHostMount],
    substrate: &[ReifiedExecutionSubstrateMount],
) -> Result<(), ReifiedNamespacePlanError> {
    for mount in mounts.iter().filter(|mount| mount.access.is_writable()) {
        for substrate_mount in substrate {
            if paths_overlap(&mount.host_path, &substrate_mount.host_path) {
                return Err(
                    ReifiedNamespacePlanError::WritableHostMountOverlapsExecutionSubstrate {
                        writable_host_path: mount.host_path.clone(),
                        substrate_host_path: substrate_mount.host_path.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

pub(super) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn validate_namespace_path(path: &Path) -> Result<(), ReifiedNamespacePlanError> {
    validate_absolute_path(path, "namespace")?;
    if path == Path::new("/") {
        return Err(ReifiedNamespacePlanError::RootNamespacePath);
    }
    Ok(())
}

fn validate_and_normalize_host_source_path(
    path: &Path,
    kind: &'static str,
) -> Result<PathBuf, ReifiedNamespacePlanError> {
    validate_absolute_path(path, kind)?;
    let normalized = canonicalize_existing_host_path(path);
    if normalized == Path::new("/") {
        return Err(ReifiedNamespacePlanError::RootHostSourcePath { kind });
    }
    Ok(normalized)
}

pub(super) fn canonicalize_existing_host_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = dunce::canonicalize(path) {
        return canonical;
    }

    let mut ancestor = path.parent();
    while let Some(parent) = ancestor {
        if let Ok(canonical_parent) = dunce::canonicalize(parent)
            && let Ok(suffix) = path.strip_prefix(parent)
        {
            return canonical_parent.join(suffix);
        }
        ancestor = parent.parent();
    }

    path.to_path_buf()
}

fn validate_absolute_path(
    path: &Path,
    kind: &'static str,
) -> Result<(), ReifiedNamespacePlanError> {
    if !path.is_absolute() {
        return Err(ReifiedNamespacePlanError::RelativePath {
            kind,
            path: path.to_path_buf(),
        });
    }
    if contains_parent_component(path) {
        return Err(ReifiedNamespacePlanError::ParentPath {
            kind,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

pub(super) fn contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
