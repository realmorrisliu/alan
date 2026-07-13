//! Host mount declaration and projection helpers.
//!
//! This is the composition-root layer for host directories: a declaration is
//! applied to the Alan OS namespace and projected into `SandboxSpec` here, while
//! `alan-kernel` remains host-path agnostic.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use alan_agent_engine::{
    HostMountGrant,
    runtime::{
        ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator,
        MountGrantApplicatorFactory,
    },
    tools::SandboxSpec,
};
use alan_ap::InProcessTransport;
use alan_hostfs::{HostDirAccess, HostDirFs};
use alan_kernel::{Access, LiveNamespace, Namespace};
use anyhow::{Context, Result};

pub fn apply_host_mount_declarations(
    namespace: &mut Namespace,
    declarations: &[HostMountGrant],
) -> Result<()> {
    validate_non_overlapping_declarations(declarations)?;
    let writable_roots = canonical_read_write_mount_roots(declarations)?;
    validate_read_only_mounts_not_covered_by_writable_roots(&writable_roots, declarations)?;

    let staged_mounts = declarations
        .iter()
        .map(|declaration| {
            let hostfs = HostDirFs::new(&declaration.host_path, hostfs_access(declaration.access))
                .with_context(|| {
                    format!(
                        "failed to mount host directory {} at {}",
                        declaration.host_path.display(),
                        declaration.namespace_path
                    )
                })?;
            Ok((
                declaration.namespace_path.clone(),
                InProcessTransport::new(Arc::new(hostfs)),
                declaration.access,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    for (namespace_path, transport, access) in staged_mounts {
        namespace.mount(&namespace_path, transport, access);
    }
    Ok(())
}

pub fn sandbox_spec_from_host_mounts(declarations: &[HostMountGrant]) -> Result<SandboxSpec> {
    validate_non_overlapping_declarations(declarations)?;
    let writable_host_roots = canonical_read_write_mount_roots(declarations)?;
    validate_read_only_mounts_not_covered_by_writable_roots(&writable_host_roots, declarations)?;
    let spec = SandboxSpec::from_host_mounts(declarations);
    anyhow::ensure!(
        spec.writable_roots == writable_host_roots,
        "Host Mount namespace and sandbox projection disagree"
    );
    Ok(spec)
}

fn canonical_host_path(path: &Path) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(path)?)
}

fn canonical_read_write_mount_roots(declarations: &[HostMountGrant]) -> Result<Vec<PathBuf>> {
    declarations
        .iter()
        .filter(|declaration| declaration.access == Access::ReadWrite)
        .map(|declaration| {
            canonical_host_path(&declaration.host_path).with_context(|| {
                format!(
                    "failed to project writable host mount {}",
                    declaration.host_path.display()
                )
            })
        })
        .collect()
}

fn validate_read_only_mounts_not_covered_by_writable_roots(
    writable_roots: &[PathBuf],
    declarations: &[HostMountGrant],
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

fn validate_non_overlapping_declarations(declarations: &[HostMountGrant]) -> Result<()> {
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

fn hostfs_access(access: Access) -> HostDirAccess {
    match access {
        Access::ReadOnly => HostDirAccess::ReadOnly,
        Access::ReadWrite => HostDirAccess::ReadWrite,
    }
}

#[derive(Debug, Clone, Default)]
pub struct LiveNamespaceMountGrantApplicatorFactory;

impl MountGrantApplicatorFactory for LiveNamespaceMountGrantApplicatorFactory {
    fn create(&self, live_namespace: LiveNamespace) -> Arc<dyn MountGrantApplicator> {
        Arc::new(LiveNamespaceMountGrantApplicator {
            live_namespace,
            granted_paths: Mutex::new(HashSet::new()),
        })
    }
}

#[derive(Debug)]
struct LiveNamespaceMountGrantApplicator {
    live_namespace: LiveNamespace,
    granted_paths: Mutex<HashSet<String>>,
}

impl MountGrantApplicator for LiveNamespaceMountGrantApplicator {
    fn apply_mount_grant(&self, grant: &ApprovedMountGrant) -> Result<Namespace> {
        let namespace_path = canonical_namespace_path(&grant.namespace_path)?;
        let mut granted_paths = self
            .granted_paths
            .lock()
            .expect("live namespace grant path set lock poisoned");
        validate_live_mount_grant_path(&self.live_namespace, &namespace_path, &granted_paths)?;
        let access = match grant.access {
            ApprovedMountGrantAccess::ReadOnly => Access::ReadOnly,
            ApprovedMountGrantAccess::ReadWrite => Access::ReadWrite,
        };
        let declaration =
            HostMountGrant::new(namespace_path.clone(), grant.host_path.clone(), access)?;
        let hostfs = HostDirFs::new(&declaration.host_path, hostfs_access(declaration.access))
            .with_context(|| {
                format!(
                    "failed to apply approved host mount {} at {}",
                    declaration.host_path.display(),
                    declaration.namespace_path
                )
            })?;
        self.live_namespace.replace_mount(
            &declaration.namespace_path,
            InProcessTransport::new(Arc::new(hostfs)),
            declaration.access,
        );
        granted_paths.insert(namespace_path);
        Ok(self.live_namespace.snapshot())
    }
}

fn canonical_namespace_path(path: &str) -> Result<String> {
    let components = normalized_namespace_components(path)?;
    if components.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", components.join("/")))
    }
}

fn validate_live_mount_grant_path(
    live_namespace: &LiveNamespace,
    namespace_path: &str,
    granted_paths: &HashSet<String>,
) -> Result<()> {
    let requested_components = normalized_namespace_components(namespace_path)?;
    anyhow::ensure!(
        requested_components.first() == Some(&"mnt") && requested_components.len() >= 2,
        "approved host mount grants must target a user mount under /mnt: {}",
        namespace_path
    );

    for (existing_path, _) in live_namespace.describe() {
        let existing_path = canonical_namespace_path(&existing_path)?;
        if existing_path == "/mnt" {
            continue;
        }
        let existing_components = normalized_namespace_components(&existing_path)?;
        let exact_previously_granted =
            existing_path == namespace_path && granted_paths.contains(namespace_path);
        if namespace_paths_overlap(&requested_components, &existing_components)
            && !exact_previously_granted
        {
            anyhow::bail!(
                "approved host mount {} would shadow existing namespace mount {}",
                namespace_path,
                existing_path
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::tools::NetworkPosture;
    use alan_ap::{ErrorCode, Fid, FileKind, OpenMode, Request, Response};
    use alan_kernel::MountFs;

    fn grant(namespace_path: &str, host_path: PathBuf, access: Access) -> HostMountGrant {
        HostMountGrant::new(namespace_path, host_path, access).unwrap()
    }

    #[tokio::test]
    async fn writable_host_mount_is_reachable_and_projected() {
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("notes.txt"), "hello").unwrap();
        let declaration = grant("/mnt/project", host.path().to_path_buf(), Access::ReadWrite);
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

        let spec = sandbox_spec_from_host_mounts(&[declaration]).unwrap();
        assert_eq!(
            spec.writable_roots,
            vec![std::fs::canonicalize(host.path()).unwrap()]
        );
        assert_eq!(
            spec.read_denylist,
            SandboxSpec::default_sensitive_read_denylist()
        );
    }

    #[tokio::test]
    async fn read_only_host_mount_is_reachable_but_not_projected_as_writable() {
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("manual.txt"), "read me").unwrap();
        let declaration = grant("/mnt/docs", host.path().to_path_buf(), Access::ReadOnly);
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

        let spec = sandbox_spec_from_host_mounts(&[declaration]).unwrap();
        let host_path = std::fs::canonicalize(host.path()).unwrap();
        assert!(spec.writable_roots.is_empty());
        assert!(!spec.writable_roots.contains(&host_path));
    }

    #[tokio::test]
    async fn live_namespace_applicator_mounts_read_write_grant() {
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("notes.txt"), "hello").unwrap();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/mnt",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
        let live_namespace = LiveNamespace::new(namespace);
        let applicator = LiveNamespaceMountGrantApplicatorFactory.create(live_namespace.clone());

        applicator
            .apply_mount_grant(&ApprovedMountGrant::new(
                "/mnt/project",
                host.path().to_path_buf(),
                ApprovedMountGrantAccess::ReadWrite,
                "Need to edit project files",
            ))
            .unwrap();

        assert_eq!(
            live_namespace.describe(),
            vec![
                ("/mnt".to_string(), Access::ReadOnly),
                ("/mnt/project".to_string(), Access::ReadWrite),
            ]
        );
        let root = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live_namespace)));
        assert_file_bytes(&root, Fid(1), &["mnt", "project", "notes.txt"], b"hello").await;
        root.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(2),
            names: ["mnt", "project", "notes.txt"]
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        })
        .await
        .unwrap();
        root.call(Request::Open {
            fid: Fid(2),
            mode: OpenMode::Write,
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn live_namespace_applicator_mounts_read_only_grant_as_read_only() {
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("manual.txt"), "read me").unwrap();
        let live_namespace = LiveNamespace::new(Namespace::new());
        let applicator = LiveNamespaceMountGrantApplicatorFactory.create(live_namespace.clone());

        applicator
            .apply_mount_grant(&ApprovedMountGrant::new(
                "/mnt/docs",
                host.path().to_path_buf(),
                ApprovedMountGrantAccess::ReadOnly,
                "Need to inspect docs",
            ))
            .unwrap();

        assert_eq!(
            live_namespace.describe(),
            vec![("/mnt/docs".to_string(), Access::ReadOnly)]
        );
        let root = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live_namespace)));
        assert_file_bytes(&root, Fid(1), &["mnt", "docs", "manual.txt"], b"read me").await;
        assert_eq!(
            root.call(Request::Open {
                fid: Fid(1),
                mode: OpenMode::Write,
            })
            .await,
            Err(alan_ap::ErrorCode::NoAccess)
        );
    }

    #[tokio::test]
    async fn live_namespace_applicator_rejects_existing_system_mount_shadow() {
        let host = tempfile::tempdir().unwrap();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            Access::ReadWrite,
        );
        let live_namespace = LiveNamespace::new(namespace);
        let applicator = LiveNamespaceMountGrantApplicatorFactory.create(live_namespace);

        let exact = applicator
            .apply_mount_grant(&ApprovedMountGrant::new(
                "/mnt/llm",
                host.path().to_path_buf(),
                ApprovedMountGrantAccess::ReadWrite,
                "Need to inspect provider state",
            ))
            .err()
            .expect("system mount shadowing must be rejected");
        assert!(
            exact
                .to_string()
                .contains("would shadow existing namespace mount")
        );

        let nested = applicator
            .apply_mount_grant(&ApprovedMountGrant::new(
                "/mnt/llm/connections/default",
                host.path().to_path_buf(),
                ApprovedMountGrantAccess::ReadOnly,
                "Need to inspect provider state",
            ))
            .err()
            .expect("nested system mount shadowing must be rejected");
        assert!(
            nested
                .to_string()
                .contains("would shadow existing namespace mount")
        );
    }

    #[tokio::test]
    async fn live_namespace_applicator_allows_replacing_previous_user_grant() {
        let old_host = tempfile::tempdir().unwrap();
        let new_host = tempfile::tempdir().unwrap();
        std::fs::write(old_host.path().join("notes.txt"), "old").unwrap();
        std::fs::write(new_host.path().join("notes.txt"), "new").unwrap();
        let live_namespace = LiveNamespace::new(Namespace::new());
        let applicator = LiveNamespaceMountGrantApplicatorFactory.create(live_namespace.clone());

        applicator
            .apply_mount_grant(&ApprovedMountGrant::new(
                "/mnt/project",
                old_host.path().to_path_buf(),
                ApprovedMountGrantAccess::ReadWrite,
                "Need to edit project files",
            ))
            .unwrap();
        applicator
            .apply_mount_grant(&ApprovedMountGrant::new(
                "/mnt/project",
                new_host.path().to_path_buf(),
                ApprovedMountGrantAccess::ReadOnly,
                "Need to inspect project files",
            ))
            .unwrap();

        assert_eq!(
            live_namespace.describe(),
            vec![("/mnt/project".to_string(), Access::ReadOnly)]
        );
        let root = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(live_namespace)));
        assert_file_bytes(&root, Fid(10), &["mnt", "project", "notes.txt"], b"new").await;
        assert_eq!(
            root.call(Request::Open {
                fid: Fid(10),
                mode: OpenMode::Write,
            })
            .await,
            Err(alan_ap::ErrorCode::NoAccess)
        );
    }

    #[test]
    fn overlapping_declarations_are_rejected_before_projection() {
        let host = tempfile::tempdir().unwrap();
        let declarations = vec![
            grant("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            grant("/mnt/project", host.path().to_path_buf(), Access::ReadOnly),
        ];

        let err = sandbox_spec_from_host_mounts(&declarations).unwrap_err();
        assert!(err.to_string().contains("overlapping host mount"));
        let mut namespace = Namespace::new();
        let err = apply_host_mount_declarations(&mut namespace, &declarations).unwrap_err();
        assert!(err.to_string().contains("overlapping host mount"));
    }

    #[test]
    fn nested_declarations_are_rejected_before_projection() {
        let host = tempfile::tempdir().unwrap();
        let declarations = vec![
            grant("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            grant(
                "/mnt/project/docs",
                host.path().to_path_buf(),
                Access::ReadOnly,
            ),
        ];

        let err = sandbox_spec_from_host_mounts(&declarations).unwrap_err();
        assert!(err.to_string().contains("overlapping host mount"));
    }

    #[test]
    fn read_only_mount_inside_writable_host_mount_is_rejected() {
        let host = tempfile::tempdir().unwrap();
        let docs = host.path().join("docs");
        std::fs::create_dir(&docs).unwrap();
        let declarations = vec![
            grant("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            grant("/mnt/docs", docs, Access::ReadOnly),
        ];

        let err = sandbox_spec_from_host_mounts(&declarations).unwrap_err();
        assert!(err.to_string().contains("read-only host mount"));
    }

    #[test]
    fn read_only_mount_inside_writable_host_mount_is_rejected_before_apply() {
        let host = tempfile::tempdir().unwrap();
        let docs = host.path().join("docs");
        std::fs::create_dir(&docs).unwrap();
        let declarations = vec![
            grant("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            grant("/mnt/docs", docs, Access::ReadOnly),
        ];
        let mut namespace = Namespace::new();

        let err = apply_host_mount_declarations(&mut namespace, &declarations).unwrap_err();
        assert!(err.to_string().contains("read-only host mount"));
    }

    #[tokio::test]
    async fn apply_host_mount_declarations_is_all_or_nothing() {
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("notes.txt"), "hello").unwrap();
        let not_directory = tempfile::NamedTempFile::new().unwrap();
        let declarations = vec![
            grant("/mnt/project", host.path().to_path_buf(), Access::ReadWrite),
            grant(
                "/mnt/not-directory",
                not_directory.path().to_path_buf(),
                Access::ReadOnly,
            ),
        ];
        let mut namespace = Namespace::new();

        let err = apply_host_mount_declarations(&mut namespace, &declarations).unwrap_err();
        assert!(err.to_string().contains("failed to mount host directory"));

        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let walked = root
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: ["mnt", "project", "notes.txt"]
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect(),
            })
            .await;
        assert_eq!(walked.unwrap_err(), ErrorCode::NotFound);
    }

    #[test]
    fn empty_declarations_add_no_implicit_host_authority() {
        let spec = sandbox_spec_from_host_mounts(&[]).unwrap();
        assert!(spec.writable_roots.is_empty());
        assert_eq!(
            spec.read_denylist,
            SandboxSpec::default_sensitive_read_denylist()
        );
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
