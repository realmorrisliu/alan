use std::{any::Any, collections::BTreeMap, path::PathBuf, sync::Arc};

use alan_ap::InProcessTransport;
use alan_kernel::{Credentials, LiveNamespace, Namespace};
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

/// A named Process descriptor referencing a path in the Process namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessDescriptor {
    pub path: String,
    pub file_tree: Option<crate::ProcessFileTree>,
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
    pub descriptor: crate::ProcessFileTree,
}

impl ProcessPackageSkillReference {
    pub fn new(
        skill_id: impl Into<String>,
        path: impl Into<String>,
        dependencies: Vec<SkillTypedDependency>,
        descriptor: crate::ProcessFileTree,
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
            descriptor,
        })
    }
}

/// Explicit immutable package authority passed at Process creation.
#[derive(Clone)]
pub struct ProcessPackageReference {
    pub package_id: String,
    pub revision: String,
    pub kind: ProcessPackageKind,
    pub namespace_path: String,
    pub skills: Vec<ProcessPackageSkillReference>,
    handle: InProcessTransport,
}

impl std::fmt::Debug for ProcessPackageReference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessPackageReference")
            .field("package_id", &self.package_id)
            .field("revision", &self.revision)
            .field("kind", &self.kind)
            .field("namespace_path", &self.namespace_path)
            .field("skills", &self.skills)
            .finish_non_exhaustive()
    }
}

impl ProcessPackageReference {
    pub fn new(
        package_id: impl Into<String>,
        revision: impl Into<String>,
        kind: ProcessPackageKind,
        namespace_path: impl Into<String>,
        skills: Vec<ProcessPackageSkillReference>,
        handle: InProcessTransport,
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
            handle,
        })
    }

    pub fn handle(&self) -> InProcessTransport {
        self.handle.clone()
    }
}

impl ProcessDescriptor {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        Ok(Self {
            path: normalize_namespace_path(&path.into())?,
            file_tree: None,
        })
    }

    pub fn with_file_tree(
        path: impl Into<String>,
        file_tree: crate::ProcessFileTree,
    ) -> Result<Self> {
        Ok(Self {
            path: normalize_namespace_path(&path.into())?,
            file_tree: Some(file_tree),
        })
    }
}

/// Authority passed when creating a Process. It contains no workspace identity.
#[derive(Clone)]
pub struct ProcessLaunchContext {
    pub namespace: Namespace,
    pub descriptors: BTreeMap<String, ProcessDescriptor>,
    pub package_references: Vec<ProcessPackageReference>,
    pub credentials: Credentials,
    pub cwd: String,
    live_namespace: Option<LiveNamespace>,
    retained_authorities: Vec<Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for ProcessLaunchContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessLaunchContext")
            .field("namespace", &self.namespace.describe())
            .field("descriptors", &self.descriptors)
            .field("package_references", &self.package_references)
            .field("credentials", &self.credentials)
            .field("cwd", &self.cwd)
            .field("live_namespace", &self.live_namespace.is_some())
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
            descriptors: BTreeMap::new(),
            package_references: Vec::new(),
            credentials,
            cwd: normalize_namespace_path(&cwd.into())?,
            live_namespace: None,
            retained_authorities: Vec::new(),
        })
    }

    pub fn root() -> Self {
        Self::new(Namespace::new(), Credentials::user("root-agent"), "/")
            .expect("root Process Launch Context is valid")
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

    pub fn child(&self) -> Self {
        Self {
            namespace: self.namespace_snapshot(),
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials: self.credentials.clone(),
            cwd: self.cwd.clone(),
            live_namespace: None,
            retained_authorities: self.retained_authorities.clone(),
        }
    }

    /// Snapshot the current Process namespace, including live mount and revocation changes.
    pub fn namespace_snapshot(&self) -> Namespace {
        self.live_namespace
            .as_ref()
            .map(LiveNamespace::snapshot)
            .unwrap_or_else(|| self.namespace.child())
    }

    /// Rebind inherited Process authority to a concrete live namespace and credentials.
    pub fn rebound(&self, namespace: Namespace, credentials: Credentials) -> Self {
        Self {
            namespace,
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials,
            cwd: self.cwd.clone(),
            live_namespace: None,
            retained_authorities: self.retained_authorities.clone(),
        }
    }

    /// Rebind inherited Process authority to the live namespace owned by its Process.
    pub fn rebound_live(&self, namespace: LiveNamespace, credentials: Credentials) -> Self {
        Self {
            namespace: namespace.snapshot(),
            descriptors: self.descriptors.clone(),
            package_references: self.package_references.clone(),
            credentials,
            cwd: self.cwd.clone(),
            live_namespace: Some(namespace),
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
    fn child_inherits_a_namespace_snapshot_without_host_backing() {
        let context = ProcessLaunchContext::root();
        let child = context.child();

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
