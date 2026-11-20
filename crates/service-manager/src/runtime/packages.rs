//! Process package seeding and explicit Process projection across `skill`, `tool`, `mcp`, and `provider`.

use std::sync::Arc;
use std::time::Duration;

use alan_agent_engine::runtime::mcp::{
    McpHttpTransportConfig, McpServerConfig, McpServerSession, McpStdioTransportConfig,
    McpTransportKind,
};
use alan_agent_engine::runtime::tool_packages::{
    PackagedTool, ToolExecutionHints, ToolPackageManifest,
};
use alan_agent_engine::tools::{Sandbox, SandboxSpec, Tool};
use alan_agent_engine::{
    ProcessPackageKind, ProcessPackageReference, ProcessPackageSkillReference,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, LiveNamespace, Namespace};
use anyhow::{Context, Result, bail, ensure};

use crate::process_launch::ProcessLaunchContext;
use crate::{
    McpPackageExport, McpTransportSpec, PackageExportKind, PackageReferenceLease, PackageService,
    PackageSnapshot, ToolPackageExport,
};

pub(crate) fn seed_preinstalled_packages(package_service: &Arc<PackageService>) -> Result<()> {
    package_service.retire_preinstalled("alan-shell-control")?;
    for source in alan_agent_engine::skills::preinstalled_skill_package_sources() {
        let snapshot = PackageSnapshot::from_preinstalled(&source).with_context(|| {
            format!(
                "snapshot first-party package `{}` for Package Service",
                source.package_id
            )
        })?;
        package_service.seed_preinstalled(source.package_id, snapshot)?;
    }
    Ok(())
}

pub(crate) fn namespace_with_package_references(
    mut base: Namespace,
    launch_context: &ProcessLaunchContext,
) -> Result<LiveNamespace> {
    for reference in &launch_context.package_references {
        base.mount(
            &reference.namespace_path,
            reference.handle(),
            Access::ReadOnly,
        );
    }
    Ok(LiveNamespace::new(base))
}

pub(crate) fn overlaps_package_namespace(path: &str) -> bool {
    namespace_paths_overlap(path, "/lib/pkg")
}

fn namespace_paths_overlap(left: &str, right: &str) -> bool {
    left == "/"
        || right == "/"
        || left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

struct AcquiredPackageProjection {
    lease: Arc<PackageReferenceLease>,
    namespace_root: String,
    handle: InProcessTransport,
    kind: ProcessPackageKind,
    mcp_exports: Vec<McpPackageExport>,
}

/// Synchronously projects `skill`, `tool`, and `provider` packages for boot/spawn contexts.
pub(crate) fn project_package_reference(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
) -> Result<()> {
    let acquired = project_package_sync_inner(package_service, launch_context, package_id)?;
    if !acquired.mcp_exports.is_empty() {
        bail!(
            "MCP package `{package_id}` requires async projection (`project_package_reference_async`)"
        );
    }
    launch_context.retain_authority(acquired.lease);
    Ok(())
}

/// Explicitly resolves `package_id` from `package_service` and projects its exports (Skills into
/// `/lib/pkg/<id>`, packaged Tools into `/bin/<name>` + `/lib/exec/<name>/manifest`, and MCP tools
/// into `/bin/mcp_<server>_<tool>` + `/lib/exec/.../manifest`) onto `launch_context` only.
pub(crate) async fn project_package_reference_async(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
) -> Result<()> {
    let acquired = project_package_sync_inner(package_service, launch_context, package_id)?;
    for mcp_export in &acquired.mcp_exports {
        let scratch = crate::temporary_store::TemporaryStore::new("mcp-discovery")?;
        let scratch_dir = scratch.path().to_path_buf();
        let transport = match &mcp_export.transport {
            McpTransportSpec::Stdio {
                command,
                args,
                env,
                allow_network,
            } => McpTransportKind::Stdio(McpStdioTransportConfig {
                executable: acquired.lease.content_file_path(command)?,
                args: args.clone(),
                env: env.clone(),
                cwd: scratch_dir.clone(),
                scratch_dir: scratch_dir.clone(),
                allow_network: *allow_network,
            }),
            McpTransportSpec::StreamableHttp { url, headers } => {
                McpTransportKind::StreamableHttp(McpHttpTransportConfig {
                    url: url.clone(),
                    headers: headers.clone(),
                })
            }
        };
        let server_config = McpServerConfig {
            server_id: mcp_export.server_id.clone(),
            description: mcp_export.description.clone(),
            transport,
            capability: mcp_export.capability,
            timeout: Duration::from_secs(mcp_export.timeout_secs.max(1) as u64),
        };
        let discovery_sandbox = Sandbox::from_spec(SandboxSpec::seed(scratch_dir));
        let session = Arc::new(McpServerSession::new(server_config));
        let discovered = session
            .discover_tools(&discovery_sandbox, "launch-scope")
            .await
            .with_context(|| {
                format!(
                    "discover MCP tools for `{}` in package `{package_id}`",
                    mcp_export.server_id
                )
            })?;
        for projected in discovered {
            let manifest = projected.manifest()?;
            mount_projected_tool(launch_context, projected, &manifest)?;
        }
        launch_context.retain_authority(Arc::new(scratch));
        launch_context.retain_authority(session);
    }

    if !acquired.mcp_exports.is_empty() {
        launch_context.add_package_reference(ProcessPackageReference::new(
            package_id,
            &acquired.lease.record().revision,
            acquired.kind,
            &acquired.namespace_root,
            Vec::new(),
            acquired.handle,
        )?);
    }
    launch_context.retain_authority(acquired.lease);
    Ok(())
}

fn project_package_sync_inner(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
) -> Result<AcquiredPackageProjection> {
    let lease = package_service.acquire(package_id)?;
    let record = lease.record().clone();
    let namespace_root = format!("/lib/pkg/{package_id}");
    let handle = InProcessTransport::new(lease.file_server()?);
    launch_context
        .namespace
        .mount(&namespace_root, handle.clone(), Access::ReadOnly);
    let kind = match record.kind {
        crate::PackageKind::Preinstalled => ProcessPackageKind::Preinstalled,
        crate::PackageKind::Installed => ProcessPackageKind::Installed,
    };

    let (skills, mcp_exports) = match record.export_kind {
        PackageExportKind::Skill => {
            let skills = record
                .exports
                .iter()
                .map(|export| {
                    ProcessPackageSkillReference::new(
                        &export.skill_id,
                        &export.root,
                        export.dependencies.clone(),
                        lease.skill_descriptor(&export.root)?,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            (skills, Vec::new())
        }
        PackageExportKind::Tool => {
            for tool_export in &record.tool_exports {
                project_packaged_tool(launch_context, &lease, tool_export)?;
            }
            (Vec::new(), Vec::new())
        }
        PackageExportKind::Mcp => {
            return Ok(AcquiredPackageProjection {
                lease,
                namespace_root,
                handle,
                kind,
                mcp_exports: record.mcp_exports,
            });
        }
        PackageExportKind::Provider => (Vec::new(), Vec::new()),
    };

    launch_context.add_package_reference(ProcessPackageReference::new(
        package_id,
        &record.revision,
        kind,
        &namespace_root,
        skills,
        handle.clone(),
    )?);
    Ok(AcquiredPackageProjection {
        lease,
        namespace_root,
        handle,
        kind,
        mcp_exports,
    })
}

fn project_packaged_tool(
    launch_context: &mut ProcessLaunchContext,
    lease: &PackageReferenceLease,
    tool_export: &ToolPackageExport,
) -> Result<()> {
    let executable = lease.content_file_path(&tool_export.entrypoint)?;
    let manifest = ToolPackageManifest {
        version: ToolPackageManifest::schema_version(),
        name: tool_export.name.clone(),
        description: tool_export.description.clone(),
        parameters: tool_export.parameters.clone(),
        capability: tool_export.capability,
        capability_is_argument_dependent: tool_export.capability_is_argument_dependent,
        timeout_secs: tool_export.timeout_secs,
        execution: ToolExecutionHints {
            arguments: "json_first_arg".to_string(),
            result: "stdout_json".to_string(),
        },
    };
    let tool = Arc::new(PackagedTool::new(
        manifest.clone(),
        executable,
        tool_export.allow_network,
    )?);
    mount_projected_tool(launch_context, tool, &manifest)
}

fn mount_projected_tool<T: Tool + 'static>(
    launch_context: &mut ProcessLaunchContext,
    tool: Arc<T>,
    manifest: &ToolPackageManifest,
) -> Result<()> {
    let name = tool.name().to_string();
    let bin_path = format!("/bin/{name}");
    let exec_dir = format!("/lib/exec/{name}");
    ensure!(
        !launch_context
            .namespace
            .describe()
            .iter()
            .any(|(path, _)| path == &bin_path || path == &exec_dir),
        "projected packaged Tool `{name}` collides with an existing executable mount"
    );
    ensure!(
        !launch_context
            .packaged_tools
            .iter()
            .any(|existing| existing.name() == name),
        "projected packaged Tool `{name}` collides with another packaged Tool"
    );
    let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
    launch_context.namespace.mount(
        &bin_path,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    launch_context.namespace.mount(
        &exec_dir,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "manifest",
            manifest_bytes,
        ))),
        Access::ReadOnly,
    );
    launch_context.add_packaged_tool(tool);
    Ok(())
}

pub(crate) fn validate_package_reference_mounts(
    launch_context: &ProcessLaunchContext,
) -> Result<()> {
    for reference in &launch_context.package_references {
        let exact_mounts = launch_context.namespace.union_at(&reference.namespace_path);
        ensure!(
            exact_mounts.len() == 1 && exact_mounts[0].access == Access::ReadOnly,
            "package reference {} requires one exact read-only Process namespace mount",
            reference.namespace_path
        );
    }
    Ok(())
}
