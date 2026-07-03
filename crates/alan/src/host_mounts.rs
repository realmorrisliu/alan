//! Host mount declaration and projection helpers.
//!
//! This is the composition-root layer for host directories: a declaration is
//! applied to the Alan OS namespace and projected into `SandboxSpec` here, while
//! `alan-kernel` remains host-path agnostic.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use alan_agent_engine::tools::{NetworkPosture, SandboxSpec};
use alan_ap::InProcessTransport;
use alan_hostfs::{HostDirAccess, HostDirFs};
use alan_kernel::{Access, Namespace};
use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostMountDeclaration {
    pub namespace_path: String,
    pub host_path: PathBuf,
    pub access: Access,
}

impl HostMountDeclaration {
    pub fn new(namespace_path: impl Into<String>, host_path: PathBuf, access: Access) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            host_path,
            access,
        }
    }

    fn hostfs_access(&self) -> HostDirAccess {
        match self.access {
            Access::ReadOnly => HostDirAccess::ReadOnly,
            Access::ReadWrite => HostDirAccess::ReadWrite,
        }
    }
}

pub fn apply_host_mount_declarations(
    namespace: &mut Namespace,
    declarations: &[HostMountDeclaration],
) -> Result<()> {
    validate_non_overlapping_declarations(declarations)?;
    for declaration in declarations {
        let hostfs = HostDirFs::new(&declaration.host_path, declaration.hostfs_access())
            .with_context(|| {
                format!(
                    "failed to mount host directory {} at {}",
                    declaration.host_path.display(),
                    declaration.namespace_path
                )
            })?;
        namespace.mount(
            &declaration.namespace_path,
            InProcessTransport::new(Arc::new(hostfs)),
            declaration.access,
        );
    }
    Ok(())
}

pub fn sandbox_spec_from_host_mounts(
    workspace_root: PathBuf,
    declarations: &[HostMountDeclaration],
) -> Result<SandboxSpec> {
    validate_non_overlapping_declarations(declarations)?;
    let mut effective_writable_roots = vec![canonical_host_path_or_original(&workspace_root)];
    let mut spec = SandboxSpec {
        writable_roots: vec![workspace_root],
        read_denylist: Vec::new(),
        network: NetworkPosture::Deny,
    };
    for declaration in declarations {
        if declaration.access != Access::ReadWrite {
            continue;
        }
        let host_path = canonical_host_path(&declaration.host_path).with_context(|| {
            format!(
                "failed to project writable host mount {}",
                declaration.host_path.display()
            )
        })?;
        if !spec.writable_roots.contains(&host_path) {
            spec.writable_roots.push(host_path.clone());
        }
        if !effective_writable_roots.contains(&host_path) {
            effective_writable_roots.push(host_path);
        }
    }
    validate_read_only_mounts_not_covered_by_writable_roots(
        &effective_writable_roots,
        declarations,
    )?;
    Ok(spec)
}

fn canonical_host_path(path: &Path) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(path)?)
}

fn canonical_host_path_or_original(path: &Path) -> PathBuf {
    canonical_host_path(path).unwrap_or_else(|_| path.to_path_buf())
}

fn validate_read_only_mounts_not_covered_by_writable_roots(
    writable_roots: &[PathBuf],
    declarations: &[HostMountDeclaration],
) -> Result<()> {
    for declaration in declarations {
        if declaration.access != Access::ReadOnly {
            continue;
        }
        let host_path = canonical_host_path(&declaration.host_path).with_context(|| {
            format!(
                "failed to project read-only host mount {}",
                declaration.host_path.display()
            )
        })?;
        for writable_root in writable_roots {
            if host_paths_overlap(&host_path, writable_root) {
                anyhow::bail!(
                    "read-only host mount {} overlaps writable root {}",
                    host_path.display(),
                    writable_root.display()
                );
            }
        }
    }
    Ok(())
}

fn validate_non_overlapping_declarations(declarations: &[HostMountDeclaration]) -> Result<()> {
    let paths = declarations
        .iter()
        .map(|declaration| {
            normalized_namespace_components(&declaration.namespace_path)
                .map(|components| (&declaration.namespace_path, components))
        })
        .collect::<Result<Vec<_>>>()?;

    for (index, (left_path, left_components)) in paths.iter().enumerate() {
        for (right_path, right_components) in paths.iter().skip(index + 1) {
            if namespace_paths_overlap(left_components, right_components) {
                anyhow::bail!(
                    "overlapping host mount declarations are not supported: {} and {}",
                    left_path,
                    right_path
                );
            }
        }
    }
    Ok(())
}

fn normalized_namespace_components(path: &str) -> Result<Vec<&str>> {
    anyhow::ensure!(
        path.starts_with('/'),
        "host mount namespace path must be absolute: {}",
        path
    );
    path.split('/')
        .filter(|component| !component.is_empty())
        .map(|component| {
            anyhow::ensure!(
                component != "." && component != "..",
                "host mount namespace path contains invalid component: {}",
                path
            );
            Ok(component)
        })
        .collect()
}

fn namespace_paths_overlap(left: &[&str], right: &[&str]) -> bool {
    left.len() <= right.len() && right.starts_with(left)
        || right.len() <= left.len() && left.starts_with(right)
}

fn host_paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_ap::{Fid, FileKind, OpenMode, Request, Response};
    use alan_kernel::MountFs;

    #[tokio::test]
    async fn writable_host_mount_is_reachable_and_projected() {
        let workspace = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("notes.txt"), "hello").unwrap();
        let declaration =
            HostMountDeclaration::new("/mnt/project", host.path().to_path_buf(), Access::ReadWrite);
        let mut namespace = Namespace::new();
        apply_host_mount_declarations(&mut namespace, std::slice::from_ref(&declaration)).unwrap();
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));

        assert_file_bytes(&root, Fid(1), &["mnt", "project", "notes.txt"], b"hello").await;
        create_file_bytes(
            &root,
            Fid(2),
            Fid(3),
            &["mnt", "project"],
            "created.txt",
            b"created through mount",
        )
        .await;
        assert_eq!(
            std::fs::read(host.path().join("created.txt")).unwrap(),
            b"created through mount"
        );

        let spec =
            sandbox_spec_from_host_mounts(workspace.path().to_path_buf(), &[declaration]).unwrap();
        assert_eq!(spec.writable_roots[0], workspace.path());
        assert!(
            spec.writable_roots
                .contains(&std::fs::canonicalize(host.path()).unwrap())
        );
    }

    #[tokio::test]
    async fn read_only_host_mount_is_reachable_but_not_projected_as_writable() {
        let workspace = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("manual.txt"), "read me").unwrap();
        let declaration =
            HostMountDeclaration::new("/mnt/docs", host.path().to_path_buf(), Access::ReadOnly);
        let mut namespace = Namespace::new();
        apply_host_mount_declarations(&mut namespace, std::slice::from_ref(&declaration)).unwrap();
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));

        assert_file_bytes(&root, Fid(1), &["mnt", "docs", "manual.txt"], b"read me").await;
        let opened = root
            .call(Request::Open {
                fid: Fid(1),
                mode: OpenMode::Write,
            })
            .await;
        assert_eq!(opened.unwrap_err(), alan_ap::ErrorCode::NoAccess);

        let spec =
            sandbox_spec_from_host_mounts(workspace.path().to_path_buf(), &[declaration]).unwrap();
        let host_path = std::fs::canonicalize(host.path()).unwrap();
        assert_eq!(spec.writable_roots, vec![workspace.path().to_path_buf()]);
        assert!(!spec.writable_roots.contains(&host_path));
    }

    #[test]
    fn overlapping_declarations_are_rejected_before_projection() {
        let workspace = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir().unwrap();
        let declarations = vec![
            HostMountDeclaration::new("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            HostMountDeclaration::new("/mnt/project", host.path().to_path_buf(), Access::ReadOnly),
        ];

        let err = sandbox_spec_from_host_mounts(workspace.path().to_path_buf(), &declarations)
            .unwrap_err();
        assert!(err.to_string().contains("overlapping host mount"));
        let mut namespace = Namespace::new();
        let err = apply_host_mount_declarations(&mut namespace, &declarations).unwrap_err();
        assert!(err.to_string().contains("overlapping host mount"));
    }

    #[test]
    fn nested_declarations_are_rejected_before_projection() {
        let workspace = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir().unwrap();
        let declarations = vec![
            HostMountDeclaration::new("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            HostMountDeclaration::new(
                "/mnt/project/docs",
                host.path().to_path_buf(),
                Access::ReadOnly,
            ),
        ];

        let err = sandbox_spec_from_host_mounts(workspace.path().to_path_buf(), &declarations)
            .unwrap_err();
        assert!(err.to_string().contains("overlapping host mount"));
    }

    #[test]
    fn read_only_mount_inside_workspace_writable_root_is_rejected() {
        let workspace = tempfile::tempdir().unwrap();
        let docs = workspace.path().join("docs");
        std::fs::create_dir(&docs).unwrap();
        let declarations = vec![HostMountDeclaration::new(
            "/mnt/docs",
            docs,
            Access::ReadOnly,
        )];

        let err = sandbox_spec_from_host_mounts(workspace.path().to_path_buf(), &declarations)
            .unwrap_err();
        assert!(err.to_string().contains("read-only host mount"));
    }

    #[test]
    fn read_only_mount_inside_writable_host_mount_is_rejected() {
        let workspace = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir().unwrap();
        let docs = host.path().join("docs");
        std::fs::create_dir(&docs).unwrap();
        let declarations = vec![
            HostMountDeclaration::new("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            HostMountDeclaration::new("/mnt/docs", docs, Access::ReadOnly),
        ];

        let err = sandbox_spec_from_host_mounts(workspace.path().to_path_buf(), &declarations)
            .unwrap_err();
        assert!(err.to_string().contains("read-only host mount"));
    }

    #[test]
    fn empty_declarations_project_only_workspace_seed() {
        let workspace = PathBuf::from("/workspace");
        let spec = sandbox_spec_from_host_mounts(workspace.clone(), &[]).unwrap();
        assert_eq!(spec.writable_roots, vec![workspace]);
        assert!(spec.read_denylist.is_empty());
        assert_eq!(spec.network, NetworkPosture::Deny);
    }

    async fn assert_file_bytes(
        root: &InProcessTransport,
        fid: Fid,
        path: &[&str],
        expected: &[u8],
    ) {
        let response = root
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: path.iter().map(|name| (*name).to_string()).collect(),
            })
            .await
            .unwrap();
        let Response::Walk { qid } = response else {
            panic!("unexpected walk response");
        };
        assert_eq!(qid.kind, FileKind::File);
        root.call(Request::Open {
            fid,
            mode: OpenMode::Read,
        })
        .await
        .unwrap();
        let response = root
            .call(Request::Read {
                fid,
                offset: 0,
                count: 1024,
            })
            .await
            .unwrap();
        let Response::Read { data } = response else {
            panic!("unexpected read response");
        };
        assert_eq!(data, expected);
    }

    async fn create_file_bytes(
        root: &InProcessTransport,
        dir_fid: Fid,
        file_fid: Fid,
        dir_path: &[&str],
        name: &str,
        bytes: &[u8],
    ) {
        root.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: dir_fid,
            names: dir_path.iter().map(|name| (*name).to_string()).collect(),
        })
        .await
        .unwrap();
        let response = root
            .call(Request::Create {
                fid: dir_fid,
                newfid: file_fid,
                name: name.to_string(),
                kind: FileKind::File,
            })
            .await
            .unwrap();
        let Response::Create { qid } = response else {
            panic!("unexpected create response");
        };
        assert_eq!(qid.kind, FileKind::File);
        root.call(Request::Open {
            fid: file_fid,
            mode: OpenMode::Write,
        })
        .await
        .unwrap();
        root.call(Request::Write {
            fid: file_fid,
            offset: 0,
            data: bytes.to_vec(),
        })
        .await
        .unwrap();
        root.call(Request::Clunk { fid: file_fid }).await.unwrap();
        root.call(Request::Clunk { fid: dir_fid }).await.unwrap();
    }
}
