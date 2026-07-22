use std::path::PathBuf;

use alan_ap::InProcessTransport;
use anyhow::{Result, ensure};

use crate::skills::{
    SkillCompatibility, SkillTypedDependency, validate_canonical_skill_id,
    validate_skill_compatibility,
};

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
    fn descriptors_are_namespace_paths() {
        assert!(ProcessDescriptor::new("host/agent").is_err());
        assert!(ProcessDescriptor::new("/mnt/../agent").is_err());
        assert_eq!(
            ProcessDescriptor::new("/agent//definition").unwrap().path,
            "/agent/definition"
        );
    }
}
