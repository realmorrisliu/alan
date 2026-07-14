use std::{
    any::Any,
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use alan_kernel::{Access, Credentials, Namespace};
use anyhow::{Result, ensure};

use crate::skills::{
    SkillCompatibility, SkillTypedDependency, validate_canonical_skill_id,
    validate_skill_compatibility,
};

pub const AGENT_DEFINITION_DESCRIPTOR: &str = "agent-definition";
pub const MEMORY_STORE_DESCRIPTOR: &str = "memory-store";

/// Host backing paths handed to the current Agent Runtime service adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRuntimeStoreBindings {
    pub rollouts: PathBuf,
    pub checkpoints: PathBuf,
    pub cache: PathBuf,
    pub tmp: PathBuf,
    pub metadata: PathBuf,
}

/// A Host-authorized directory projected into one Process namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostMountGrant {
    pub namespace_path: String,
    pub host_path: PathBuf,
    pub access: Access,
}

impl HostMountGrant {
    pub fn new(
        namespace_path: impl Into<String>,
        host_path: impl Into<PathBuf>,
        access: Access,
    ) -> Result<Self> {
        let namespace_path = normalize_namespace_path(&namespace_path.into())?;
        let host_path = host_path.into();
        ensure!(host_path.is_absolute(), "Host Mount path must be absolute");
        ensure!(
            !host_path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir)),
            "Host Mount path must not contain relative components"
        );
        Ok(Self {
            namespace_path,
            host_path,
            access,
        })
    }

    pub(crate) fn resolve_host_path(&self, namespace_path: &str) -> Option<PathBuf> {
        let mount = namespace_components(&self.namespace_path).ok()?;
        let requested = namespace_components(namespace_path).ok()?;
        if !requested.starts_with(&mount) {
            return None;
        }
        let relative = requested[mount.len()..].iter().collect::<PathBuf>();
        Some(self.host_path.join(relative))
    }

    pub(crate) fn resolve_namespace_path(&self, host_path: &Path) -> Option<PathBuf> {
        let requested = dunce::canonicalize(host_path)
            .unwrap_or_else(|_| dunce::simplified(host_path).to_path_buf());
        let root = dunce::canonicalize(&self.host_path)
            .unwrap_or_else(|_| dunce::simplified(&self.host_path).to_path_buf());
        let suffix = requested.strip_prefix(root).ok()?;
        Some(Path::new(&self.namespace_path).join(suffix))
    }
}

/// A named Process descriptor referencing a path in the Process namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessDescriptor {
    pub path: String,
}

/// Provenance tier for an immutable package reference passed to a Process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessPackageKind {
    Preinstalled,
    Installed,
}

/// One Skill root selected from an immutable distribution package revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessPackageSkillReference {
    pub skill_id: String,
    /// Normalized path relative to the package projection root.
    pub path: String,
    pub dependencies: Vec<SkillTypedDependency>,
}

impl ProcessPackageSkillReference {
    pub fn new(
        skill_id: impl Into<String>,
        path: impl Into<String>,
        dependencies: Vec<SkillTypedDependency>,
    ) -> Result<Self> {
        let skill_id = skill_id.into();
        validate_canonical_skill_id(&skill_id).map_err(anyhow::Error::msg)?;
        let path = normalize_relative_path(&path.into())?;
        ensure!(!path.is_empty(), "package Skill path must not be empty");
        validate_skill_compatibility(&SkillCompatibility {
            dependencies: dependencies.clone(),
            ..SkillCompatibility::default()
        })
        .map_err(anyhow::Error::from)?;
        Ok(Self {
            skill_id,
            path,
            dependencies,
        })
    }
}

/// Explicit immutable package authority passed at Process creation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessPackageReference {
    pub package_id: String,
    pub revision: String,
    pub kind: ProcessPackageKind,
    pub namespace_path: String,
    pub skills: Vec<ProcessPackageSkillReference>,
}

impl ProcessPackageReference {
    pub fn new(
        package_id: impl Into<String>,
        revision: impl Into<String>,
        kind: ProcessPackageKind,
        namespace_path: impl Into<String>,
        skills: Vec<ProcessPackageSkillReference>,
    ) -> Result<Self> {
        let package_id = package_id.into();
        validate_package_id(&package_id)?;
        let revision = revision.into();
        ensure!(
            revision.len() == 64
                && revision
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "package revision must be a lowercase SHA-256 fingerprint"
        );
        let namespace_path = normalize_namespace_path(&namespace_path.into())?;
        ensure!(
            namespace_path == format!("/lib/pkg/{package_id}"),
            "package reference must be projected at /lib/pkg/<package-id>"
        );
        let mut seen = std::collections::BTreeSet::new();
        ensure!(
            skills
                .iter()
                .all(|skill| seen.insert(skill.skill_id.clone())),
            "package reference contains duplicate Skill ids"
        );
        Ok(Self {
            package_id,
            revision,
            kind,
            namespace_path,
            skills,
        })
    }
}

impl ProcessDescriptor {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        Ok(Self {
            path: normalize_namespace_path(&path.into())?,
        })
    }
}

/// Authority passed when creating a Process. It contains no workspace identity.
#[derive(Clone)]
pub struct ProcessLaunchContext {
    pub namespace: Namespace,
    pub host_mounts: Vec<HostMountGrant>,
    pub descriptors: BTreeMap<String, ProcessDescriptor>,
    pub package_references: Vec<ProcessPackageReference>,
    pub credentials: Credentials,
    pub cwd: String,
    retained_authorities: Vec<Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for ProcessLaunchContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessLaunchContext")
            .field("namespace", &self.namespace.describe())
            .field("host_mounts", &self.host_mounts)
            .field("descriptors", &self.descriptors)
            .field("package_references", &self.package_references)
            .field("credentials", &self.credentials)
            .field("cwd", &self.cwd)
            .field("retained_authorities", &self.retained_authorities.len())
            .finish()
    }
}

impl ProcessLaunchContext {
    pub fn new(
        namespace: Namespace,
        credentials: Credentials,
        cwd: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            namespace,
            host_mounts: Vec::new(),
            descriptors: BTreeMap::new(),
            package_references: Vec::new(),
            credentials,
            cwd: normalize_namespace_path(&cwd.into())?,
            retained_authorities: Vec::new(),
        })
    }

    pub fn root() -> Self {
        Self::new(Namespace::new(), Credentials::user("root-agent"), "/")
            .expect("root Process Launch Context is valid")
    }

    pub fn with_host_mount(mut self, grant: HostMountGrant) -> Self {
        self.host_mounts.push(grant);
        self
    }

    pub fn with_descriptor(
        mut self,
        name: impl Into<String>,
        descriptor: ProcessDescriptor,
    ) -> Self {
        self.descriptors.insert(name.into(), descriptor);
        self
    }

    pub fn descriptor(&self, name: &str) -> Option<&ProcessDescriptor> {
        self.descriptors.get(name)
    }

    pub fn with_package_reference(mut self, reference: ProcessPackageReference) -> Self {
        self.package_references.push(reference);
        self
    }

    pub fn add_package_reference(&mut self, reference: ProcessPackageReference) {
        self.package_references.push(reference);
    }

    /// Retain an opaque owner-issued authority for the lifetime of this Process context.
    pub fn retain_authority<T>(&mut self, authority: Arc<T>)
    where
        T: Any + Send + Sync,
    {
        self.retained_authorities.push(authority);
    }

    /// Resolve an Alan OS path only through an explicit Host Mount grant.
    pub fn host_path(&self, namespace_path: &str) -> Option<PathBuf> {
        self.host_mounts
            .iter()
            .filter_map(|grant| {
                grant.resolve_host_path(namespace_path).map(|path| {
                    (
                        namespace_components(&grant.namespace_path)
                            .unwrap_or_default()
                            .len(),
                        path,
                    )
                })
            })
            .max_by_key(|(prefix_len, _)| *prefix_len)
            .map(|(_, path)| path)
    }

    pub fn host_cwd(&self) -> Option<PathBuf> {
        self.host_path(&self.cwd)
    }

    /// Resolve a Host path to its Alan OS path only through an explicit Host Mount.
    pub fn namespace_path(&self, host_path: &Path) -> Option<String> {
        let requested = dunce::canonicalize(host_path)
            .unwrap_or_else(|_| dunce::simplified(host_path).to_path_buf());
        self.host_mounts
            .iter()
            .filter_map(|grant| {
                let root = dunce::canonicalize(&grant.host_path)
                    .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf());
                let suffix = requested.strip_prefix(&root).ok()?;
                Some((
                    root.components().count(),
                    if suffix.as_os_str().is_empty() {
                        grant.namespace_path.clone()
                    } else {
                        format!(
                            "{}/{}",
                            grant.namespace_path.trim_end_matches('/'),
                            suffix.to_string_lossy()
                        )
                    },
                ))
            })
            .max_by_key(|(prefix_len, _)| *prefix_len)
            .map(|(_, path)| path)
    }

    pub fn child(&self) -> Self {
        Self {
            namespace: self.namespace.child(),
            host_mounts: self.host_mounts.clone(),
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials: self.credentials.clone(),
            cwd: self.cwd.clone(),
            retained_authorities: self.retained_authorities.clone(),
        }
    }

    /// Rebind inherited Process authority to a concrete live namespace and credentials.
    pub fn rebound(&self, namespace: Namespace, credentials: Credentials) -> Self {
        Self {
            namespace,
            host_mounts: self.host_mounts.clone(),
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials,
            cwd: self.cwd.clone(),
            retained_authorities: self.retained_authorities.clone(),
        }
    }
}

fn validate_package_id(value: &str) -> Result<()> {
    ensure!(!value.is_empty() && value.len() <= 64, "invalid package id");
    ensure!(
        value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        }),
        "invalid package id `{value}`"
    );
    Ok(())
}

fn normalize_relative_path(path: &str) -> Result<String> {
    ensure!(
        !path.starts_with('/'),
        "package Skill path must be relative"
    );
    let components = path
        .split('/')
        .filter(|component| !component.is_empty())
        .map(|component| {
            ensure!(
                component != "." && component != "..",
                "invalid package Skill path: {path}"
            );
            Ok(component)
        })
        .collect::<Result<Vec<_>>>()?;
    let normalized = components.join("/");
    ensure!(normalized == path, "package Skill path must be normalized");
    Ok(normalized)
}

pub(crate) fn normalize_namespace_path(path: &str) -> Result<String> {
    let components = namespace_components(path)?;
    if components.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", components.join("/")))
    }
}

fn namespace_components(path: &str) -> Result<Vec<&str>> {
    ensure!(
        path.starts_with('/'),
        "Alan OS path must be absolute: {path}"
    );
    path.split('/')
        .filter(|component| !component.is_empty())
        .map(|component| {
            ensure!(
                component != "." && component != "..",
                "invalid Alan OS path: {path}"
            );
            Ok(component)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_paths_resolve_only_through_explicit_mounts() {
        let context = ProcessLaunchContext::root().with_host_mount(
            HostMountGrant::new("/mnt/source", "/host/source", Access::ReadWrite).unwrap(),
        );

        assert_eq!(
            context.host_path("/mnt/source/src/lib.rs"),
            Some(Path::new("/host/source/src/lib.rs").to_path_buf())
        );
        assert_eq!(context.host_path("/host/source/src/lib.rs"), None);
    }

    #[test]
    fn child_inherits_a_snapshot_without_gaining_mounts() {
        let context = ProcessLaunchContext::root().with_host_mount(
            HostMountGrant::new("/mnt/source", "/host/source", Access::ReadOnly).unwrap(),
        );
        let child = context.child();

        assert_eq!(child.host_mounts, context.host_mounts);
        assert_eq!(child.namespace.describe(), context.namespace.describe());
        assert_eq!(child.cwd, "/");
    }

    #[test]
    fn descriptors_are_namespace_paths() {
        assert!(ProcessDescriptor::new("host/agent").is_err());
        assert!(ProcessDescriptor::new("/mnt/../agent").is_err());
        assert_eq!(
            ProcessDescriptor::new("/agent//definition").unwrap().path,
            "/agent/definition"
        );
    }
}
