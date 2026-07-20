//! Native Host Mount export and Tool execution projection.
//!
//! Runtime grant authority and live namespace projection belong to Host Mount
//! Service. This adapter alone retains native backing paths.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use alan_agent_engine::tools::{
    ReifiedMountAccess, Sandbox, SandboxHostMount, SandboxSpec, ToolExecutionAdapter,
};
use alan_ap::InProcessTransport;
use alan_hostfs::{HostDirAccess, HostDirFs};
use alan_kernel::Access;
use alan_service_manager::{
    HostMountAccess, HostMountExport, HostMountExportAdapter, HostMountGrantRecord,
    HostMountService, HostMountToolProjection,
};
use anyhow::{Context, Result};

/// Native Host adapter. This is the only component that turns a raw Host path
/// into a hostfs tree and native Tool sandbox authority.
#[derive(Debug, Default)]
pub struct NativeHostMountExportAdapter;

struct NativeHostMountExport {
    tree: InProcessTransport,
    host_path: PathBuf,
    maximum_access: HostMountAccess,
}

impl std::fmt::Debug for NativeHostMountExport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("NativeHostMountExport")
    }
}

impl HostMountExport for NativeHostMountExport {
    fn file_tree(&self) -> InProcessTransport {
        self.tree.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone)]
struct NativeToolMount {
    namespace_path: PathBuf,
    host_path: PathBuf,
    access: HostMountAccess,
}

#[derive(Debug)]
struct NativeToolExecutionAdapter {
    mounts: Vec<NativeToolMount>,
    namespace_cwd: PathBuf,
    cwd: PathBuf,
    sandbox: Sandbox,
}

impl ToolExecutionAdapter for NativeToolExecutionAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        self.namespace_cwd.clone()
    }

    fn cwd(&self) -> Result<PathBuf> {
        Ok(self.cwd.clone())
    }

    fn resolve_path(&self, namespace_cwd: &Path, path: &Path) -> Result<PathBuf> {
        let namespace_path = normalize_tool_namespace_path(if path.is_absolute() {
            path.to_path_buf()
        } else {
            namespace_cwd.join(path)
        })?;
        let mount = longest_namespace_mount(&self.mounts, &namespace_path).with_context(|| {
            format!(
                "path {} is outside delegated Host Mounts",
                namespace_path.display()
            )
        })?;
        let suffix = namespace_path
            .strip_prefix(&mount.namespace_path)
            .expect("selected Host Mount is a namespace prefix");
        Ok(mount.host_path.join(suffix))
    }

    fn visible_path(&self, host_path: &Path) -> PathBuf {
        let host_path = dunce::canonicalize(host_path)
            .unwrap_or_else(|_| dunce::simplified(host_path).to_path_buf());
        self.mounts
            .iter()
            .filter_map(|mount| {
                host_path.strip_prefix(&mount.host_path).ok().map(|suffix| {
                    (
                        mount.host_path.components().count(),
                        mount.namespace_path.join(suffix),
                    )
                })
            })
            .max_by_key(|(prefix_len, _)| *prefix_len)
            .map_or_else(|| PathBuf::from("<unmapped-host-path>"), |(_, path)| path)
    }

    fn project_text(&self, text: &str) -> String {
        let mut projected = text.to_string();
        let mut mounts = self.mounts.iter().collect::<Vec<_>>();
        mounts.sort_by_key(|mount| std::cmp::Reverse(mount.host_path.as_os_str().len()));
        for mount in mounts {
            projected = projected.replace(
                mount.host_path.to_string_lossy().as_ref(),
                mount.namespace_path.to_string_lossy().as_ref(),
            );
        }
        projected
    }

    fn sandbox(&self) -> Result<Sandbox> {
        Ok(self.sandbox.clone())
    }
}

impl HostMountExportAdapter for NativeHostMountExportAdapter {
    fn tool_execution_adapter(
        &self,
        projections: &[HostMountToolProjection],
        requested_namespace_cwd: &Path,
    ) -> Result<Arc<dyn ToolExecutionAdapter>> {
        let mounts = projections
            .iter()
            .map(|projection| {
                let export = projection
                    .export()
                    .as_any()
                    .downcast_ref::<NativeHostMountExport>()
                    .context("Host Mount export was not produced by the native Host adapter")?;
                anyhow::ensure!(
                    export.maximum_access == HostMountAccess::ReadWrite
                        || projection.access == HostMountAccess::ReadOnly,
                    "Host Mount Tool authority cannot amplify export access"
                );
                Ok(NativeToolMount {
                    namespace_path: projection.namespace_path.clone(),
                    host_path: export.host_path.clone(),
                    access: projection.access,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        validate_native_tool_mounts(&mounts)?;
        let requested_namespace_cwd =
            normalize_tool_namespace_path(requested_namespace_cwd.to_path_buf())?;
        let selected = longest_namespace_mount(&mounts, &requested_namespace_cwd)
            .or_else(|| {
                mounts
                    .iter()
                    .find(|mount| mount.access == HostMountAccess::ReadWrite)
            })
            .or_else(|| mounts.first())
            .context("Tool Process has no active Host Mount")?;
        let namespace_cwd = if requested_namespace_cwd.starts_with(&selected.namespace_path) {
            requested_namespace_cwd
        } else {
            selected.namespace_path.clone()
        };
        let cwd = selected.host_path.join(
            namespace_cwd
                .strip_prefix(&selected.namespace_path)
                .expect("selected Host Mount owns Tool cwd"),
        );
        let sandbox_mounts = mounts
            .iter()
            .map(|mount| SandboxHostMount {
                namespace_path: mount.namespace_path.clone(),
                host_path: mount.host_path.clone(),
                access: match mount.access {
                    HostMountAccess::ReadOnly => ReifiedMountAccess::ReadOnly,
                    HostMountAccess::ReadWrite => ReifiedMountAccess::ReadWrite,
                },
            })
            .collect::<Vec<_>>();
        Ok(Arc::new(NativeToolExecutionAdapter {
            mounts,
            namespace_cwd,
            cwd,
            sandbox: Sandbox::from_spec(SandboxSpec::from_host_mounts(&sandbox_mounts)),
        }))
    }
}

/// Complete one native approval without passing its raw Host path into Alan OS.
pub fn approve_host_mount(
    service: &HostMountService,
    request_id: &str,
    host_path: &Path,
    provenance: impl Into<String>,
    actor: impl Into<String>,
) -> Result<HostMountGrantRecord> {
    let request = service
        .pending_request(request_id)
        .with_context(|| format!("unknown Host Mount request `{request_id}`"))?;
    let export = match native_export(&request.namespace_path, host_path, request.access) {
        Ok(export) => export,
        Err(error) => {
            let _ = service.fail_request(
                request_id,
                "Host adapter could not authorize or export the selected directory",
                "host-adapter",
            );
            return Err(error);
        }
    };
    service.approve_export(request_id, export, provenance, actor)
}

fn native_export(
    namespace_path: &str,
    host_path: &Path,
    access: HostMountAccess,
) -> Result<Arc<dyn HostMountExport>> {
    let kernel_access = match access {
        HostMountAccess::ReadOnly => Access::ReadOnly,
        HostMountAccess::ReadWrite => Access::ReadWrite,
    };
    let host_path = canonical_host_path(host_path)?;
    let tree = InProcessTransport::new(Arc::new(
        HostDirFs::new(&host_path, hostfs_access(kernel_access)).with_context(|| {
            format!(
                "failed to export host directory {} at {namespace_path}",
                host_path.display()
            )
        })?,
    ));
    Ok(Arc::new(NativeHostMountExport {
        tree,
        host_path,
        maximum_access: access,
    }))
}

fn normalize_tool_namespace_path(path: PathBuf) -> Result<PathBuf> {
    anyhow::ensure!(path.is_absolute(), "Tool namespace path must be absolute");
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                anyhow::ensure!(normalized.pop(), "Tool namespace path escapes root");
            }
            Component::Prefix(_) => anyhow::bail!("Tool namespace path contains a Host prefix"),
        }
    }
    Ok(normalized)
}

fn longest_namespace_mount<'a>(
    mounts: &'a [NativeToolMount],
    path: &Path,
) -> Option<&'a NativeToolMount> {
    mounts
        .iter()
        .filter(|mount| path.starts_with(&mount.namespace_path))
        .max_by_key(|mount| mount.namespace_path.components().count())
}

fn validate_native_tool_mounts(mounts: &[NativeToolMount]) -> Result<()> {
    for (index, left) in mounts.iter().enumerate() {
        for right in mounts.iter().skip(index + 1) {
            anyhow::ensure!(
                !(left.namespace_path.starts_with(&right.namespace_path)
                    || right.namespace_path.starts_with(&left.namespace_path)),
                "overlapping Host Mount projections are not supported: {} and {}",
                left.namespace_path.display(),
                right.namespace_path.display()
            );
            if left.access != right.access {
                anyhow::ensure!(
                    !host_paths_overlap(&left.host_path, &right.host_path),
                    "read-only and read-write Host Mount projections overlap native backing"
                );
            }
        }
    }
    Ok(())
}

fn canonical_host_path(path: &Path) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(path)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    use alan_agent_engine::tools::{ToolExecutionAuthority, ToolExecutionBinding};
    use alan_ap::{ErrorCode, Fid, OpenMode, Request, Response};
    use alan_kernel::{LiveNamespace, MountFs, Namespace, Pid};

    fn service() -> Arc<HostMountService> {
        HostMountService::new(Arc::new(NativeHostMountExportAdapter))
    }

    async fn request(
        service: &Arc<HostMountService>,
        pid: u64,
        namespace_path: &str,
        access: HostMountAccess,
    ) -> String {
        let transport = InProcessTransport::new(service.file_server_for_process(pid));
        transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["requests".to_string(), "clone".to_string()],
            })
            .await
            .unwrap();
        transport
            .call(Request::Open {
                fid: Fid(1),
                mode: OpenMode::ReadWrite,
            })
            .await
            .unwrap();
        let Response::Read { data } = transport
            .call(Request::Read {
                fid: Fid(1),
                offset: 0,
                count: 128,
            })
            .await
            .unwrap()
        else {
            panic!("Host Mount clone did not return a request reference");
        };
        let request_id = String::from_utf8(data).unwrap();
        let document = serde_json::to_vec(&serde_json::json!({
            "namespace_path": namespace_path,
            "access": access,
            "reason": "native Host adapter test",
        }))
        .unwrap();
        transport
            .call(Request::Write {
                fid: Fid(1),
                offset: 0,
                data: document,
            })
            .await
            .unwrap();
        transport
            .call(Request::Clunk { fid: Fid(1) })
            .await
            .unwrap();
        request_id
    }

    async fn approve(
        service: &Arc<HostMountService>,
        pid: u64,
        namespace_path: &str,
        access: HostMountAccess,
        host_path: &Path,
    ) -> HostMountGrantRecord {
        let request_id = request(service, pid, namespace_path, access).await;
        approve_host_mount(
            service,
            &request_id,
            host_path,
            "test-native-selection",
            "test",
        )
        .unwrap()
    }

    fn binding(namespace_cwd: &str) -> ToolExecutionBinding {
        ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from(namespace_cwd),
            PathBuf::from("/tmp/alan-native-host-mount-test-scratch"),
        )
    }

    #[tokio::test]
    async fn native_approval_projects_one_handle_into_namespace_and_tool_execution() {
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("notes.txt"), "hello").unwrap();
        let service = service();
        let namespace = LiveNamespace::new(Namespace::new());
        service.register_process(Pid(7), namespace.clone());

        let record = approve(
            &service,
            7,
            "/mnt/project",
            HostMountAccess::ReadWrite,
            host.path(),
        )
        .await;

        assert_eq!(record.namespace_path, "/mnt/project");
        assert!(
            !serde_json::to_string(&record)
                .unwrap()
                .contains(host.path().to_string_lossy().as_ref())
        );
        assert_eq!(
            namespace.snapshot().resolve("/mnt/project").unwrap().access,
            Access::ReadWrite
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace.snapshot())));
        assert_file_bytes(&root, Fid(10), &["mnt", "project", "notes.txt"], b"hello").await;

        let execution = service.reconcile(7, binding("/mnt/project")).unwrap();
        let adapter = execution.adapter().unwrap();
        assert_eq!(
            adapter.cwd().unwrap(),
            std::fs::canonicalize(host.path()).unwrap()
        );
        assert_eq!(
            adapter
                .resolve_path(Path::new("/mnt/project"), Path::new("notes.txt"))
                .unwrap(),
            std::fs::canonicalize(host.path())
                .unwrap()
                .join("notes.txt")
        );
        assert_eq!(
            adapter.visible_path(&host.path().join("notes.txt")),
            PathBuf::from("/mnt/project/notes.txt")
        );
        let projected = adapter.project_text(&format!(
            "failed at {}",
            std::fs::canonicalize(host.path())
                .unwrap()
                .join("notes.txt")
                .display()
        ));
        assert_eq!(projected, "failed at /mnt/project/notes.txt");
        assert!(adapter.sandbox().unwrap().is_writable(host.path()));
    }

    #[tokio::test]
    async fn read_only_handle_stays_read_only_in_namespace_and_sandbox() {
        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("manual.txt"), "read me").unwrap();
        let service = service();
        let namespace = LiveNamespace::new(Namespace::new());
        service.register_process(Pid(7), namespace.clone());

        approve(
            &service,
            7,
            "/mnt/docs",
            HostMountAccess::ReadOnly,
            host.path(),
        )
        .await;

        assert_eq!(
            namespace.snapshot().resolve("/mnt/docs").unwrap().access,
            Access::ReadOnly
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace.snapshot())));
        root.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(20),
            names: vec!["mnt".into(), "docs".into(), "manual.txt".into()],
        })
        .await
        .unwrap();
        assert_eq!(
            root.call(Request::Open {
                fid: Fid(20),
                mode: OpenMode::Write,
            })
            .await,
            Err(ErrorCode::NoAccess)
        );
        let adapter = service
            .reconcile(7, binding("/mnt/docs"))
            .unwrap()
            .adapter()
            .unwrap();
        assert!(adapter.sandbox().unwrap().is_readable(host.path()));
        assert!(!adapter.sandbox().unwrap().is_writable(host.path()));
    }

    #[tokio::test]
    async fn adapter_selects_the_mount_containing_the_logical_cwd() {
        let first = tempfile::tempdir().unwrap();
        let cwd_host = tempfile::tempdir().unwrap();
        std::fs::create_dir(cwd_host.path().join("work")).unwrap();
        let service = service();
        service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
        approve(
            &service,
            7,
            "/mnt/a",
            HostMountAccess::ReadOnly,
            first.path(),
        )
        .await;
        approve(
            &service,
            7,
            "/mnt/z",
            HostMountAccess::ReadWrite,
            cwd_host.path(),
        )
        .await;

        let execution = service.reconcile(7, binding("/mnt/z/work")).unwrap();

        assert_eq!(execution.namespace_cwd, PathBuf::from("/mnt/z/work"));
        assert_eq!(
            execution.adapter().unwrap().cwd().unwrap(),
            std::fs::canonicalize(cwd_host.path()).unwrap().join("work")
        );
    }

    #[tokio::test]
    async fn mixed_access_native_overlap_is_rejected_without_amplification() {
        let host = tempfile::tempdir().unwrap();
        let nested = host.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let service = service();
        service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
        approve(
            &service,
            7,
            "/mnt/project",
            HostMountAccess::ReadWrite,
            host.path(),
        )
        .await;
        approve(&service, 7, "/mnt/docs", HostMountAccess::ReadOnly, &nested).await;

        let error = service.reconcile(7, binding("/mnt/project")).unwrap_err();

        assert!(error.to_string().contains("overlap native backing"));
    }

    #[tokio::test]
    async fn namespace_path_resolution_cannot_escape_delegated_mounts() {
        let host = tempfile::tempdir().unwrap();
        let service = service();
        service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
        approve(
            &service,
            7,
            "/mnt/project",
            HostMountAccess::ReadWrite,
            host.path(),
        )
        .await;
        let adapter = service
            .reconcile(7, binding("/mnt/project"))
            .unwrap()
            .adapter()
            .unwrap();

        assert!(
            adapter
                .resolve_path(Path::new("/mnt/project"), Path::new("../../etc/passwd"))
                .is_err()
        );
        assert!(
            adapter
                .resolve_path(Path::new("/mnt/project"), Path::new("/mnt/other/file"))
                .is_err()
        );
    }

    #[tokio::test]
    async fn invalid_native_selection_fails_without_returning_a_grant() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let service = service();
        service.register_process(Pid(7), LiveNamespace::new(Namespace::new()));
        let request_id = request(&service, 7, "/mnt/project", HostMountAccess::ReadOnly).await;

        let error = approve_host_mount(
            &service,
            &request_id,
            file.path(),
            "test-native-selection",
            "test",
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("failed to export host directory")
        );
        assert!(service.pending_request(&request_id).is_none());
    }

    async fn assert_file_bytes(
        root: &InProcessTransport,
        fid: Fid,
        path: &[&str],
        expected: &[u8],
    ) {
        root.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: path.iter().map(|name| (*name).to_string()).collect(),
        })
        .await
        .unwrap();
        root.call(Request::Open {
            fid,
            mode: OpenMode::Read,
        })
        .await
        .unwrap();
        let Response::Read { data } = root
            .call(Request::Read {
                fid,
                offset: 0,
                count: 1024,
            })
            .await
            .unwrap()
        else {
            panic!("unexpected Host Mount read response");
        };
        assert_eq!(data, expected);
    }
}
