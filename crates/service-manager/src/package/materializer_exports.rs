//! Export-kind materialization and verification helpers across Skill, Tool, MCP, and Provider packages.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use alan_agent_engine::runtime::tool_packages::{ToolExecutionHints, ToolPackageManifest};
use alan_agent_engine::skills::{
    SkillScope, SkillTypedDependency, name_to_id, parse_skill_metadata,
};
use anyhow::{Context, Result, bail, ensure};

use super::manifest::ParsedPackageManifest;
use super::types::{
    McpPackageExport, McpTransportSpec, PackageExport, PackageExportKind, PackageSnapshot,
    ProviderPackageExport, ToolPackageExport, validate_package_id,
};

pub(super) struct ComputedExports {
    pub(super) export_kind: PackageExportKind,
    pub(super) manifest_version: Option<String>,
    pub(super) description: Option<String>,
    pub(super) exports: Vec<PackageExport>,
    pub(super) tool_exports: Vec<ToolPackageExport>,
    pub(super) mcp_exports: Vec<McpPackageExport>,
    pub(super) provider_export: Option<ProviderPackageExport>,
}

/// Computes exports and generated/executable `content/` entries in memory for one validated package.
pub(super) fn build_content_exports(
    snapshot: &PackageSnapshot,
    parsed_manifest: Option<ParsedPackageManifest>,
    content_files: &mut BTreeMap<String, (Vec<u8>, bool)>,
    generated_files: &mut BTreeSet<String>,
    max_source_file_bytes: usize,
) -> Result<ComputedExports> {
    let manifest_version = parsed_manifest.as_ref().and_then(|m| m.version.clone());
    let description = parsed_manifest.as_ref().and_then(|m| m.description.clone());
    let kind = parsed_manifest
        .as_ref()
        .map_or(PackageExportKind::Skill, |m| m.export_kind);

    match kind {
        PackageExportKind::Skill => {
            let exports = materialize_skill_exports_in_map(
                snapshot,
                content_files,
                generated_files,
                max_source_file_bytes,
            )?;
            if parsed_manifest.is_some() && exports.is_empty() {
                bail!("skill manifest requires at least one SKILL.md or skills/*.md export");
            }
            Ok(ComputedExports {
                export_kind: PackageExportKind::Skill,
                manifest_version,
                description,
                exports,
                tool_exports: Vec::new(),
                mcp_exports: Vec::new(),
                provider_export: None,
            })
        }
        PackageExportKind::Tool => {
            let mut tools = parsed_manifest
                .expect("tool kind requires parsed manifest")
                .tools;
            for tool in &mut tools {
                let source_rel = tool
                    .entrypoint
                    .strip_prefix("source/")
                    .context("internal error: invalid tool entrypoint prefix")?;
                let bytes = snapshot
                    .entries
                    .iter()
                    .find(|entry| entry.path == source_rel)
                    .map(|entry| entry.bytes.clone())
                    .with_context(|| format!("missing snapshot entrypoint `{source_rel}`"))?;
                let bin_rel = format!("tools/{}/bin", tool.name);
                content_files.insert(bin_rel.clone(), (bytes, true));
                generated_files.insert(bin_rel.clone());
                tool.entrypoint = bin_rel;

                let runtime_manifest = ToolPackageManifest {
                    version: ToolPackageManifest::schema_version(),
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                    capability: tool.capability,
                    capability_is_argument_dependent: tool.capability_is_argument_dependent,
                    timeout_secs: tool.timeout_secs,
                    execution: ToolExecutionHints {
                        arguments: "json_first_arg".to_string(),
                        result: "stdout_json".to_string(),
                    },
                };
                runtime_manifest.validate_for_name(&tool.name)?;
                let encoded = serde_json::to_vec_pretty(&runtime_manifest)?;
                ensure!(
                    encoded.len() <= max_source_file_bytes,
                    "generated Tool manifest exceeds file size limit"
                );
                content_files.insert(tool.manifest_path.clone(), (encoded, false));
                generated_files.insert(tool.manifest_path.clone());
            }
            Ok(ComputedExports {
                export_kind: PackageExportKind::Tool,
                manifest_version,
                description,
                exports: Vec::new(),
                tool_exports: tools,
                mcp_exports: Vec::new(),
                provider_export: None,
            })
        }
        PackageExportKind::Mcp => {
            let mut mcps = parsed_manifest
                .expect("mcp kind requires parsed manifest")
                .mcps;
            for mcp in &mut mcps {
                if let McpTransportSpec::Stdio { command, .. } = &mut mcp.transport {
                    let source_rel = command
                        .strip_prefix("source/")
                        .context("internal error: invalid stdio MCP command prefix")?;
                    let bytes = snapshot
                        .entries
                        .iter()
                        .find(|entry| entry.path == source_rel)
                        .map(|entry| entry.bytes.clone())
                        .with_context(|| format!("missing snapshot MCP binary `{source_rel}`"))?;
                    let bin_rel = format!("mcp/{}/bin", mcp.server_id);
                    content_files.insert(bin_rel.clone(), (bytes, true));
                    generated_files.insert(bin_rel.clone());
                    *command = bin_rel;
                }
                let encoded = serde_json::to_vec_pretty(mcp)?;
                ensure!(
                    encoded.len() <= max_source_file_bytes,
                    "generated MCP server descriptor exceeds file size limit"
                );
                content_files.insert(mcp.descriptor_path.clone(), (encoded, false));
                generated_files.insert(mcp.descriptor_path.clone());
            }
            Ok(ComputedExports {
                export_kind: PackageExportKind::Mcp,
                manifest_version,
                description,
                exports: Vec::new(),
                tool_exports: Vec::new(),
                mcp_exports: mcps,
                provider_export: None,
            })
        }
        PackageExportKind::Provider => {
            let provider = parsed_manifest
                .expect("provider kind requires parsed manifest")
                .provider
                .context("provider manifest missing `[provider]` export")?;
            let encoded = serde_json::to_vec_pretty(&provider)?;
            ensure!(
                encoded.len() <= max_source_file_bytes,
                "generated provider adapter descriptor exceeds file size limit"
            );
            content_files.insert(provider.descriptor_path.clone(), (encoded, false));
            generated_files.insert(provider.descriptor_path.clone());
            Ok(ComputedExports {
                export_kind: PackageExportKind::Provider,
                manifest_version,
                description,
                exports: Vec::new(),
                tool_exports: Vec::new(),
                mcp_exports: Vec::new(),
                provider_export: Some(provider),
            })
        }
    }
}

/// Validates materialized `content/` exports against manifest metadata during revision verification.
pub(super) fn verify_materialized_exports(
    content_root: &Path,
    export_kind: PackageExportKind,
    exports: &[PackageExport],
    tool_exports: &[ToolPackageExport],
    mcp_exports: &[McpPackageExport],
    provider_export: Option<&ProviderPackageExport>,
) -> Result<()> {
    match export_kind {
        PackageExportKind::Skill => {
            ensure!(
                tool_exports.is_empty() && mcp_exports.is_empty() && provider_export.is_none(),
                "skill materialization contains non-skill exports"
            );
            for export in exports {
                validate_package_id(&export.skill_id)?;
                let export_root = content_root.join(&export.root);
                ensure!(
                    export.root == format!("skills/{}", export.skill_id)
                        && export_root.join("SKILL.md").is_file(),
                    "materialization manifest contains an invalid Skill export"
                );
            }
        }
        PackageExportKind::Tool => {
            ensure!(
                exports.is_empty()
                    && !tool_exports.is_empty()
                    && mcp_exports.is_empty()
                    && provider_export.is_none(),
                "tool materialization has inconsistent export set"
            );
            for tool in tool_exports {
                ensure!(
                    tool.entrypoint == format!("tools/{}/bin", tool.name)
                        && content_root.join(&tool.entrypoint).is_file(),
                    "materialized tool `{}` binary is missing",
                    tool.name
                );
                ensure!(
                    tool.manifest_path == format!("tools/{}/manifest.json", tool.name)
                        && content_root.join(&tool.manifest_path).is_file(),
                    "materialized tool `{}` manifest is missing",
                    tool.name
                );
                let manifest: ToolPackageManifest =
                    serde_json::from_slice(&fs::read(content_root.join(&tool.manifest_path))?)?;
                manifest.validate_for_name(&tool.name)?;
            }
        }
        PackageExportKind::Mcp => {
            ensure!(
                exports.is_empty()
                    && tool_exports.is_empty()
                    && !mcp_exports.is_empty()
                    && provider_export.is_none(),
                "mcp materialization has inconsistent export set"
            );
            for mcp in mcp_exports {
                ensure!(
                    mcp.descriptor_path == format!("mcp/{}/server.json", mcp.server_id)
                        && content_root.join(&mcp.descriptor_path).is_file(),
                    "materialized mcp `{}` descriptor is missing",
                    mcp.server_id
                );
                let decoded: McpPackageExport =
                    serde_json::from_slice(&fs::read(content_root.join(&mcp.descriptor_path))?)?;
                ensure!(&decoded == mcp, "materialized mcp descriptor mismatch");
                if let McpTransportSpec::Stdio { command, .. } = &mcp.transport {
                    ensure!(
                        command == &format!("mcp/{}/bin", mcp.server_id)
                            && content_root.join(command).is_file(),
                        "materialized mcp `{}` stdio binary is missing",
                        mcp.server_id
                    );
                }
            }
        }
        PackageExportKind::Provider => {
            ensure!(
                exports.is_empty() && tool_exports.is_empty() && mcp_exports.is_empty(),
                "provider materialization contains non-provider exports"
            );
            let provider =
                provider_export.context("provider materialization missing provider export")?;
            ensure!(
                provider.descriptor_path == "provider/adapter.json"
                    && content_root.join(&provider.descriptor_path).is_file(),
                "materialized provider descriptor is missing"
            );
            let decoded: ProviderPackageExport =
                serde_json::from_slice(&fs::read(content_root.join(&provider.descriptor_path))?)?;
            ensure!(
                &decoded == provider,
                "materialized provider descriptor mismatch"
            );
        }
    }
    Ok(())
}

fn materialize_skill_exports_in_map(
    snapshot: &PackageSnapshot,
    content_files: &mut BTreeMap<String, (Vec<u8>, bool)>,
    generated_files: &mut BTreeSet<String>,
    max_source_file_bytes: usize,
) -> Result<Vec<PackageExport>> {
    let mut exports = Vec::new();
    let mut skill_ids = BTreeSet::new();
    for native_root in native_skill_roots(snapshot)? {
        let skill_md_path = native_root.join("SKILL.md");
        let skill_document = snapshot
            .entries
            .iter()
            .find(|entry| Path::new(&entry.path) == skill_md_path)
            .map(|entry| entry.bytes.as_slice())
            .context("native skill root missing SKILL.md")?;
        let skill_id = skill_id_for_native_root(snapshot, &native_root)?;
        let declared_dependencies = parse_skill_dependencies(&skill_id, skill_document)?;
        let dependencies = merge_dependencies(
            declared_dependencies,
            detect_availability_dependencies(skill_document),
        );
        ensure!(
            skill_ids.insert(skill_id.clone()),
            "duplicate Skill id `{skill_id}`"
        );
        for entry in &snapshot.entries {
            let Ok(rel) = Path::new(&entry.path).strip_prefix(&native_root) else {
                continue;
            };
            let rel_str = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            content_files.insert(
                format!("skills/{skill_id}/{rel_str}"),
                (entry.bytes.clone(), entry.executable),
            );
        }
        exports.push(PackageExport {
            skill_id: skill_id.clone(),
            root: format!("skills/{skill_id}"),
            dependencies,
        });
    }

    if !snapshot
        .entries
        .iter()
        .any(|entry| entry.path == "SKILL.md")
    {
        for entry in &snapshot.entries {
            let path = Path::new(&entry.path);
            if path.components().count() != 2
                || path.parent() != Some(Path::new("skills"))
                || path.extension().and_then(|value| value.to_str()) != Some("md")
            {
                continue;
            }
            let skill_name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .context("command Skill name is not UTF-8")?;
            let skill_id = name_to_id(skill_name);
            validate_package_id(&skill_id)?;
            ensure!(
                skill_ids.insert(skill_id.clone()),
                "duplicate Skill id `{skill_id}`"
            );
            let body =
                std::str::from_utf8(&entry.bytes).context("command-style Skill is not UTF-8")?;
            let dependencies = detect_availability_dependencies(&entry.bytes);
            let document = command_skill_document(&skill_id, body, &dependencies);
            ensure!(
                document.len() <= max_source_file_bytes,
                "generated command-style Skill exceeds descriptor file size limit"
            );
            let rel = format!("skills/{skill_id}/SKILL.md");
            content_files.insert(rel.clone(), (document.into_bytes(), false));
            generated_files.insert(rel);
            exports.push(PackageExport {
                skill_id: skill_id.clone(),
                root: format!("skills/{skill_id}"),
                dependencies,
            });
        }
    }
    exports.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
    Ok(exports)
}

fn parse_skill_dependencies(skill_id: &str, document: &[u8]) -> Result<Vec<SkillTypedDependency>> {
    let text = std::str::from_utf8(document).context("SKILL.md is not UTF-8")?;
    let virtual_path = Path::new(skill_id).join("SKILL.md");
    let metadata = parse_skill_metadata(text, &virtual_path, SkillScope::Installed)
        .map_err(anyhow::Error::from)
        .context("validate native SKILL.md")?;
    ensure!(
        metadata.id == skill_id,
        "native Skill runtime identity is not deterministic"
    );
    Ok(alan_agent_engine::skills::skill_declared_dependencies(
        &metadata,
    ))
}

fn detect_availability_dependencies(document: &[u8]) -> Vec<SkillTypedDependency> {
    let text = String::from_utf8_lossy(document);
    let mut dependencies = BTreeMap::new();
    for (needle, name) in [
        ("WebSearch", "web-search"),
        ("WebFetch", "web-fetch"),
        ("TeamCreate", "team-orchestration"),
        ("TaskCreate", "team-orchestration"),
    ] {
        if text.contains(needle) {
            dependencies.insert(
                format!("runtime_capability:{name}"),
                SkillTypedDependency::RuntimeCapability {
                    name: name.to_string(),
                    description: Some("Required by imported command vocabulary.".to_string()),
                },
            );
        }
    }
    dependencies.into_values().collect()
}

fn merge_dependencies(
    left: Vec<SkillTypedDependency>,
    right: Vec<SkillTypedDependency>,
) -> Vec<SkillTypedDependency> {
    let mut dependencies = BTreeMap::new();
    for dependency in left.into_iter().chain(right) {
        dependencies
            .entry(dependency.identity_key())
            .or_insert(dependency);
    }
    dependencies.into_values().collect()
}

fn command_skill_document(
    skill_id: &str,
    body: &str,
    dependencies: &[SkillTypedDependency],
) -> String {
    let yaml_skill_id = serde_yaml::to_string(skill_id)
        .expect("validated Skill id serialization must succeed")
        .trim_start_matches("---\n")
        .trim_end()
        .to_string();
    let mut document = format!(
        "---\nname: {yaml_skill_id}\ndescription: Imported command-style Skill `{skill_id}`.\n"
    );
    if !dependencies.is_empty() {
        document.push_str("compatibility:\n  dependencies:\n");
        for dependency in dependencies {
            let yaml = serde_yaml::to_string(dependency)
                .expect("typed dependency serialization must succeed");
            let yaml = yaml.trim_start_matches("---\n");
            let mut lines = yaml.lines();
            if let Some(first) = lines.next() {
                document.push_str(&format!("    - {first}\n"));
            }
            for line in lines {
                document.push_str(&format!("      {line}\n"));
            }
        }
    }
    document.push_str("---\n\n# Alan adapter\n\nThis Skill was imported by Package Service materializer `alan-skill-v1`. Treat foreign command placeholders as instruction context; unavailable capabilities remain explicit dependencies.\n\n");
    document.push_str(body);
    document
}

fn native_skill_roots(snapshot: &PackageSnapshot) -> Result<Vec<PathBuf>> {
    if snapshot
        .entries
        .iter()
        .any(|entry| entry.path == "SKILL.md")
    {
        return Ok(vec![PathBuf::new()]);
    }
    let mut roots = snapshot
        .entries
        .iter()
        .filter(|entry| entry.path.ends_with("/SKILL.md"))
        .map(|entry| {
            Path::new(&entry.path)
                .parent()
                .expect("nested SKILL.md has a parent")
                .to_path_buf()
        })
        .filter(|root| !has_non_export_component(root))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    for (index, root) in roots.iter().enumerate() {
        ensure!(
            roots
                .iter()
                .skip(index + 1)
                .all(|other| !other.starts_with(root)),
            "overlapping native Skill roots are ambiguous"
        );
    }
    Ok(roots)
}

fn skill_id_for_native_root(snapshot: &PackageSnapshot, root: &Path) -> Result<String> {
    let source = if root.as_os_str().is_empty() {
        snapshot.source_name.as_str()
    } else {
        root.file_name()
            .and_then(|name| name.to_str())
            .context("native Skill directory is not UTF-8")?
    };
    let skill_id = name_to_id(source);
    validate_package_id(&skill_id).context("native Skill directory has no canonical identity")?;
    Ok(skill_id)
}

fn has_non_export_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::Normal(value)
                if matches!(
                    value.to_str(),
                    Some(
                        "agents"
                            | "assets"
                            | "bin"
                            | "evals"
                            | "eval-viewer"
                            | "references"
                            | "scripts"
                    )
                )
        )
    })
}
