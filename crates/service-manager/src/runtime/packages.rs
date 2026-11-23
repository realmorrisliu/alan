//! Process package seeding and explicit Process projection across `skill`, `mcp`, `tool`,
//! `provider`, and `fileserver` packages.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alan_agent_engine::TrustPosture;
use alan_agent_engine::runtime::mcp::{
    McpHttpTransportConfig, McpServerConfig, McpServerSession, McpStdioTransportConfig,
    McpTransportKind,
};
use alan_agent_engine::runtime::tool_packages::{
    PackagedTool, ToolExecutionHints, ToolPackageManifest,
};
use alan_agent_engine::tools::{ReifiedMountAccess, Sandbox, SandboxHostMount, SandboxSpec, Tool};
use alan_agent_engine::{
    ProcessPackageKind, ProcessPackageReference, ProcessPackageSkillReference,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, LiveNamespace, Namespace, SrvFs};
use anyhow::{Context, Result, bail, ensure};

use crate::fileserver_service::{FileserverLaunchGrant, FileserverManager};
use crate::local_entry::LocalEntryService;
use crate::process_launch::ProcessLaunchContext;
use crate::{
    FileserverMountAccess, McpPackageExport, McpTransportSpec, PackageExportKind,
    PackageReferenceLease, PackageService, PackageSnapshot, RootBindingMode, RootPackageBinding,
    ToolPackageExport,
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
    for (name, manifest_bytes) in &launch_context.packaged_tool_manifests {
        base.mount(
            &format!("/bin/{name}"),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
        base.mount(
            &format!("/lib/exec/{name}"),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "manifest",
                manifest_bytes.clone(),
            ))),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PackageProjectionMode {
    SkillsOnly,
    Full,
}

struct AcquiredPackageProjection {
    lease: Arc<PackageReferenceLease>,
    namespace_root: String,
    handle: InProcessTransport,
    kind: ProcessPackageKind,
    skills: Vec<ProcessPackageSkillReference>,
    mcp_exports: Vec<McpPackageExport>,
}

/// Synchronously projects read-only Skills (including Companion Skills) onto `launch_context`
/// without projecting packaged Tools or starting MCP/Fileserver runtimes.
pub(crate) fn project_package_skills_only(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
) -> Result<()> {
    let acquired = project_package_sync_inner(
        package_service,
        launch_context,
        package_id,
        PackageProjectionMode::SkillsOnly,
    )?;
    launch_context.retain_authority(acquired.lease);
    Ok(())
}

/// Synchronously projects `skill`, `tool`, `provider`, and Companion Skills for boot/spawn contexts.
pub(crate) fn project_package_reference(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
) -> Result<()> {
    let acquired = project_package_sync_inner(
        package_service,
        launch_context,
        package_id,
        PackageProjectionMode::Full,
    )?;
    if !acquired.mcp_exports.is_empty() {
        bail!(
            "MCP package `{package_id}` requires async projection (`project_package_reference_async`)"
        );
    }
    launch_context.retain_authority(acquired.lease);
    Ok(())
}

/// Explicitly resolves `package_id` from `package_service` and projects its exports (Skills and
/// Companion Skills into `/lib/pkg/<id>`, packaged Tools into `/bin/<name>` + `/lib/exec/<name>/manifest`,
/// and MCP tools into `/bin/mcp_<server>_<tool>` + `/lib/exec/.../manifest`) onto `launch_context`.
pub(crate) async fn project_package_reference_async(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
) -> Result<()> {
    let acquired = project_package_sync_inner(
        package_service,
        launch_context,
        package_id,
        PackageProjectionMode::Full,
    )?;
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
            acquired.skills,
            acquired.handle,
        )?);
    }
    launch_context.retain_authority(acquired.lease);
    Ok(())
}

fn collect_skill_references(
    record: &crate::PackageRecord,
    lease: &PackageReferenceLease,
) -> Result<Vec<ProcessPackageSkillReference>> {
    record
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
        .collect()
}

fn project_package_sync_inner(
    package_service: &Arc<PackageService>,
    launch_context: &mut ProcessLaunchContext,
    package_id: &str,
    mode: PackageProjectionMode,
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
    let skills = collect_skill_references(&record, &lease)?;

    let mcp_exports = match (mode, record.export_kind) {
        (PackageProjectionMode::SkillsOnly, _) => Vec::new(),
        (PackageProjectionMode::Full, PackageExportKind::Skill) => Vec::new(),
        (PackageProjectionMode::Full, PackageExportKind::Tool) => {
            for tool_export in &record.tool_exports {
                project_packaged_tool(launch_context, &lease, tool_export)?;
            }
            Vec::new()
        }
        (PackageProjectionMode::Full, PackageExportKind::Mcp) => {
            return Ok(AcquiredPackageProjection {
                lease,
                namespace_root,
                handle,
                kind,
                skills,
                mcp_exports: record.mcp_exports,
            });
        }
        (
            PackageProjectionMode::Full,
            PackageExportKind::Provider | PackageExportKind::Fileserver,
        ) => Vec::new(),
    };

    launch_context.add_package_reference(ProcessPackageReference::new(
        package_id,
        &record.revision,
        kind,
        &namespace_root,
        skills.clone(),
        handle.clone(),
    )?);
    Ok(AcquiredPackageProjection {
        lease,
        namespace_root,
        handle,
        kind,
        skills,
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
            manifest_bytes.clone(),
        ))),
        Access::ReadOnly,
    );
    launch_context.add_packaged_tool(tool);
    launch_context.add_packaged_tool_manifest(name, manifest_bytes);
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

/// Build an explicit confined [`FileserverLaunchGrant`] with Host binary/config read-only
/// visibility for a packaged fileserver (`github` or generic `fileserver`).
pub fn prepare_fileserver_launch_grant(
    package_id: &str,
    service_id: &str,
    access: FileserverMountAccess,
    allow_network: bool,
    trust_posture: TrustPosture,
    alan_bin_override: Option<&Path>,
    gh_bin_override: Option<&Path>,
) -> FileserverLaunchGrant {
    let mut host_mounts = Vec::new();
    let mut runtime_env = BTreeMap::new();

    if let Some(alan_bin) = resolve_alan_bin_for_fileserver(alan_bin_override) {
        let canonical = canonical_or_owned(&alan_bin);
        runtime_env.insert("ALAN_BIN".to_string(), canonical.display().to_string());
        push_ro_parent_dir_mount(&mut host_mounts, "/host/alan-bin", &canonical);
    }

    if service_id == "github" {
        if let Some(gh_bin) = resolve_non_empty_path(gh_bin_override, "ALAN_GITHUBFS_GH_BIN") {
            let canonical = canonical_or_owned(&gh_bin);
            runtime_env.insert(
                "ALAN_GITHUBFS_GH_BIN".to_string(),
                canonical.display().to_string(),
            );
            push_ro_parent_dir_mount(&mut host_mounts, "/host/gh-bin", &canonical);
        }
        if allow_network {
            for var in ["GH_TOKEN", "GITHUB_TOKEN", "GH_HOST"] {
                if let Some(val) = std::env::var(var)
                    .ok()
                    .filter(|v| !v.is_empty() && !v.contains(['\0', '\n', '\r']))
                {
                    runtime_env.insert(var.to_string(), val);
                }
            }
            if let Some(gh_cfg_dir) = resolve_existing_gh_config_dir() {
                let canonical = canonical_or_owned(&gh_cfg_dir);
                runtime_env.insert("GH_CONFIG_DIR".to_string(), canonical.display().to_string());
                host_mounts.push(SandboxHostMount {
                    namespace_path: PathBuf::from("/host/gh-config"),
                    host_path: canonical,
                    access: ReifiedMountAccess::ReadOnly,
                });
            }
        }
    }

    FileserverLaunchGrant::new(package_id)
        .with_mount_access(access.into_kernel_access())
        .with_allow_network(allow_network)
        .with_trust_posture(trust_posture)
        .with_host_mounts(host_mounts)
        .with_runtime_env(runtime_env)
}

#[allow(
    clippy::too_many_arguments,
    reason = "Applies explicit durable and invocation Root bindings onto ProcessLaunchContext and boot namespaces in one pass"
)]
pub(crate) async fn apply_root_package_bindings(
    package_service: &Arc<PackageService>,
    fileserver_manager: &Arc<FileserverManager>,
    srvfs: &Arc<SrvFs>,
    system_namespace: &LiveNamespace,
    local_entry: &Arc<LocalEntryService>,
    launch_context: &mut ProcessLaunchContext,
    trust_posture: TrustPosture,
    durable_bindings: &[RootPackageBinding],
    explicit_bindings: &[RootPackageBinding],
) -> Result<()> {
    let mut merged: BTreeMap<String, (RootPackageBinding, bool)> = BTreeMap::new();
    for binding in durable_bindings {
        merged.insert(binding.package_id.clone(), (binding.clone(), false));
    }
    for binding in explicit_bindings {
        merged.insert(binding.package_id.clone(), (binding.clone(), true));
    }
    if merged.is_empty() {
        return Ok(());
    }

    let mut projected_ids: BTreeSet<String> = launch_context
        .package_references
        .iter()
        .map(|reference| reference.package_id.clone())
        .collect();

    for (package_id, (binding, strict)) in merged {
        if projected_ids.contains(&package_id) {
            if strict {
                bail!("package `{package_id}` is already projected into this Process");
            }
            continue;
        }
        let result = apply_single_root_binding(
            package_service,
            fileserver_manager,
            srvfs,
            system_namespace,
            local_entry,
            launch_context,
            trust_posture,
            &binding,
        )
        .await;
        match result {
            Ok(()) => {
                projected_ids.insert(package_id);
            }
            Err(error) if strict => return Err(error),
            Err(error) => {
                tracing::warn!(
                    package_id = %binding.package_id,
                    error = %error,
                    "skipping unavailable durable Root package binding"
                );
            }
        }
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "Resolves one binding mode and mounts active fileservers into system, Root, and local-entry namespaces"
)]
async fn apply_single_root_binding(
    package_service: &Arc<PackageService>,
    fileserver_manager: &Arc<FileserverManager>,
    srvfs: &Arc<SrvFs>,
    system_namespace: &LiveNamespace,
    local_entry: &Arc<LocalEntryService>,
    launch_context: &mut ProcessLaunchContext,
    trust_posture: TrustPosture,
    binding: &RootPackageBinding,
) -> Result<()> {
    let record = package_service.resolve(&binding.package_id)?;
    match binding.mode {
        RootBindingMode::SkillsOnly => {
            if !record.exports.is_empty() {
                project_package_skills_only(package_service, launch_context, &binding.package_id)?;
            }
            Ok(())
        }
        RootBindingMode::Active => match record.export_kind {
            PackageExportKind::Fileserver => {
                let fs_export = record.fileserver_export.as_ref().with_context(|| {
                    format!(
                        "fileserver package `{}` lacks a fileserver export",
                        binding.package_id
                    )
                })?;
                let activation = binding.fileserver.clone().unwrap_or_default();
                ensure!(
                    !activation.allow_network || fs_export.allow_network,
                    "fileserver package `{}` does not allow network access",
                    binding.package_id
                );
                let grant = prepare_fileserver_launch_grant(
                    &binding.package_id,
                    &fs_export.service_id,
                    activation.access,
                    activation.allow_network,
                    trust_posture,
                    None,
                    None,
                );
                let status = fileserver_manager.start(grant).await?;
                if !record.exports.is_empty()
                    && let Err(err) = project_package_skills_only(
                        package_service,
                        launch_context,
                        &binding.package_id,
                    )
                {
                    let _ = fileserver_manager.stop(&status.service_id).await;
                    return Err(err);
                }
                if let Err(err) = async {
                    fileserver_manager
                        .mount_into_live_namespace(
                            &status.service_id,
                            system_namespace,
                            status.mount_access,
                        )
                        .await?;
                    fileserver_manager
                        .mount_into_namespace(
                            &status.service_id,
                            &mut launch_context.namespace,
                            status.mount_access,
                        )
                        .await?;
                    let (transport, posted_access) = srvfs
                        .lookup(&status.service_id)
                        .await
                        .with_context(|| format!("/srv/{} is not published", status.service_id))?;
                    let effective = match (posted_access, status.mount_access) {
                        (Access::ReadWrite, Access::ReadWrite) => Access::ReadWrite,
                        _ => Access::ReadOnly,
                    };
                    local_entry.mount_service(&status.mount_path, transport, effective);
                    Ok::<_, anyhow::Error>(())
                }
                .await
                {
                    let _ = fileserver_manager.stop(&status.service_id).await;
                    return Err(err);
                }
                Ok(())
            }
            PackageExportKind::Tool | PackageExportKind::Mcp => {
                project_package_reference_async(
                    package_service,
                    launch_context,
                    &binding.package_id,
                )
                .await
            }
            PackageExportKind::Skill | PackageExportKind::Provider => {
                if !record.exports.is_empty() {
                    project_package_skills_only(
                        package_service,
                        launch_context,
                        &binding.package_id,
                    )?;
                }
                Ok(())
            }
        },
    }
}

fn canonical_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_non_empty_path(explicit: Option<&Path>, env_name: &str) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    std::env::var_os(env_name)
        .filter(|val| !val.is_empty())
        .map(PathBuf::from)
}

fn resolve_alan_bin_for_fileserver(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = resolve_non_empty_path(explicit, "ALAN_BIN") {
        return Some(path);
    }
    let current = std::env::current_exe().ok()?;
    let file_name = current.file_name().and_then(|s| s.to_str())?;
    matches!(file_name, "alan" | "alan.exe").then_some(current)
}

fn resolve_existing_gh_config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("GH_CONFIG_DIR")
        .filter(|val| !val.is_empty())
        .map(PathBuf::from)
        && dir.is_dir()
    {
        return Some(dir);
    }
    let home = std::env::var_os("HOME")
        .filter(|val| !val.is_empty())
        .map(PathBuf::from)?;
    let default_dir = home.join(".config").join("gh");
    default_dir.is_dir().then_some(default_dir)
}

fn push_ro_parent_dir_mount(
    host_mounts: &mut Vec<SandboxHostMount>,
    namespace_path: &str,
    binary_path: &Path,
) {
    let Some(parent) = binary_path.parent() else {
        return;
    };
    let parent = canonical_or_owned(parent);
    if !parent.is_dir() {
        return;
    }
    if matches!(parent.to_str(), Some("/" | "/bin" | "/usr/bin")) {
        return;
    }
    if host_mounts
        .iter()
        .any(|existing| existing.host_path == parent)
    {
        return;
    }
    host_mounts.push(SandboxHostMount {
        namespace_path: PathBuf::from(namespace_path),
        host_path: parent,
        access: ReifiedMountAccess::ReadOnly,
    });
}
