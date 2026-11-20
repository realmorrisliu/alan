//! Four-kind Quartermaster package manifest parsing (`alan-package.toml` / `package.toml`) and validation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use alan_agent_engine::runtime::mcp::{
    canonical_mcp_tool_name, validate_http_endpoint_url, validate_safe_http_headers,
    validate_safe_stdio_env,
};
use alan_agent_engine::runtime::tool_packages::{
    validate_canonical_tool_name, validate_packaged_tool_timeout, validate_tool_parameters_schema,
};
use alan_agent_engine::tools::ToolCapability;
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use super::types::{
    McpPackageExport, McpTransportSpec, PackageExportKind, PackageSnapshot, ProviderAuthMode,
    ProviderPackageExport, ToolPackageExport, validate_package_id,
};

const PRIMARY_MANIFEST_FILE: &str = "alan-package.toml";
const LEGACY_MANIFEST_FILE: &str = "package.toml";
const SUPPORTED_SCHEMA_VERSION: u16 = 1;
const DEFAULT_TIMEOUT_SECS: usize = 30;
const MAX_ARGS_COUNT: usize = 64;
const FIRST_PARTY_PROVIDER_IDS: &[&str] = &[
    "magpie",
    "openai_responses",
    "chatgpt",
    "google_gemini_generate_content",
    "openrouter",
];
const SUPPORTED_PROVIDER_PROTOCOLS: &[&str] = &[
    "anthropic_messages",
    "openai_chat_completions",
    "openai_chat_completions_compatible",
    "openai_responses",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPackageManifest {
    #[serde(default = "default_schema_version")]
    schema_version: u16,
    kind: PackageExportKind,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tool: Option<RawToolManifest>,
    #[serde(default)]
    tools: Vec<RawToolManifest>,
    #[serde(default)]
    mcp: Option<RawMcpManifest>,
    #[serde(default)]
    mcps: Vec<RawMcpManifest>,
    #[serde(default)]
    provider: Option<RawProviderManifest>,
}

fn default_schema_version() -> u16 {
    SUPPORTED_SCHEMA_VERSION
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawToolManifest {
    name: String,
    description: String,
    entrypoint: String,
    parameters: toml::Value,
    #[serde(default = "default_unknown_capability")]
    capability: ToolCapability,
    #[serde(default)]
    capability_is_argument_dependent: bool,
    #[serde(default = "default_timeout_secs")]
    timeout_secs: usize,
    #[serde(default)]
    allow_network: bool,
    #[serde(default = "default_arguments_hint")]
    arguments: String,
    #[serde(default = "default_result_hint")]
    result: String,
}

fn default_unknown_capability() -> ToolCapability {
    ToolCapability::Unknown
}

fn default_timeout_secs() -> usize {
    DEFAULT_TIMEOUT_SECS
}

fn default_arguments_hint() -> String {
    "json_first_arg".to_string()
}

fn default_result_hint() -> String {
    "stdout_json".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMcpManifest {
    id: String,
    #[serde(default)]
    description: Option<String>,
    transport: RawMcpTransport,
    #[serde(default)]
    capability: Option<ToolCapability>,
    #[serde(default = "default_timeout_secs")]
    timeout_secs: usize,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum RawMcpTransport {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        allow_network: bool,
    },
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProviderManifest {
    id: String,
    protocol: String,
    display_name: String,
    default_base_url: String,
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    auth_mode: Option<ProviderAuthMode>,
    #[serde(default)]
    api_key_header: Option<String>,
    #[serde(default)]
    default_headers: BTreeMap<String, String>,
    #[serde(default)]
    adapter_descriptor: Option<String>,
}

/// Validated extension exports derived from `alan-package.toml` or `package.toml`.
#[derive(Debug, Clone)]
pub(super) struct ParsedPackageManifest {
    pub(super) export_kind: PackageExportKind,
    pub(super) version: Option<String>,
    pub(super) description: Option<String>,
    pub(super) tools: Vec<ToolPackageExport>,
    pub(super) mcps: Vec<McpPackageExport>,
    pub(super) provider: Option<ProviderPackageExport>,
}

/// Parses an optional root `alan-package.toml` or `package.toml` inside `snapshot`.
///
/// Returns `Ok(None)` for legacy manifestless Skill packages.
pub(super) fn parse_package_manifest(
    snapshot: &PackageSnapshot,
) -> Result<Option<ParsedPackageManifest>> {
    let primary = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == PRIMARY_MANIFEST_FILE);
    let legacy = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == LEGACY_MANIFEST_FILE);

    let manifest_entry = match (primary, legacy) {
        (Some(_), Some(_)) => {
            bail!(
                "package snapshot must not contain both `{PRIMARY_MANIFEST_FILE}` and `{LEGACY_MANIFEST_FILE}`"
            );
        }
        (Some(entry), None) | (None, Some(entry)) => entry,
        (None, None) => return Ok(None),
    };

    let raw_text = std::str::from_utf8(&manifest_entry.bytes)
        .with_context(|| format!("`{}` is not valid UTF-8", manifest_entry.path))?;
    let raw: RawPackageManifest = toml::from_str(raw_text)
        .with_context(|| format!("invalid package manifest `{}`", manifest_entry.path))?;

    ensure!(
        raw.schema_version == SUPPORTED_SCHEMA_VERSION,
        "unsupported package manifest schema_version {} (expected {SUPPORTED_SCHEMA_VERSION})",
        raw.schema_version
    );

    let version = normalize_optional_text(raw.version, "manifest version", 64)?;
    let description = normalize_optional_text(raw.description, "manifest description", 1024)?;

    let snapshot_files: BTreeSet<&str> = snapshot
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();

    match raw.kind {
        PackageExportKind::Skill => {
            ensure!(
                raw.tool.is_none()
                    && raw.tools.is_empty()
                    && raw.mcp.is_none()
                    && raw.mcps.is_empty()
                    && raw.provider.is_none(),
                "skill manifest cannot declare tool, mcp, or provider exports"
            );
            Ok(Some(ParsedPackageManifest {
                export_kind: PackageExportKind::Skill,
                version,
                description,
                tools: Vec::new(),
                mcps: Vec::new(),
                provider: None,
            }))
        }
        PackageExportKind::Tool => {
            ensure!(
                raw.mcp.is_none() && raw.mcps.is_empty() && raw.provider.is_none(),
                "tool manifest cannot declare mcp or provider exports (single export kind per package)"
            );
            ensure_no_skill_exports_in_non_skill_package(snapshot)?;
            let raw_tools = collect_single_or_many(raw.tool, raw.tools, "tool")?;
            let tools = validate_tool_exports(raw_tools, &snapshot_files)?;
            Ok(Some(ParsedPackageManifest {
                export_kind: PackageExportKind::Tool,
                version,
                description,
                tools,
                mcps: Vec::new(),
                provider: None,
            }))
        }
        PackageExportKind::Mcp => {
            ensure!(
                raw.tool.is_none() && raw.tools.is_empty() && raw.provider.is_none(),
                "mcp manifest cannot declare tool or provider exports (single export kind per package)"
            );
            ensure_no_skill_exports_in_non_skill_package(snapshot)?;
            let raw_mcps = collect_single_or_many(raw.mcp, raw.mcps, "mcp")?;
            let mcps = validate_mcp_exports(raw_mcps, &snapshot_files)?;
            Ok(Some(ParsedPackageManifest {
                export_kind: PackageExportKind::Mcp,
                version,
                description,
                tools: Vec::new(),
                mcps,
                provider: None,
            }))
        }
        PackageExportKind::Provider => {
            ensure!(
                raw.tool.is_none()
                    && raw.tools.is_empty()
                    && raw.mcp.is_none()
                    && raw.mcps.is_empty(),
                "provider manifest cannot declare tool or mcp exports (single export kind per package)"
            );
            ensure_no_skill_exports_in_non_skill_package(snapshot)?;
            let raw_provider = raw
                .provider
                .context("provider manifest must declare a `[provider]` section")?;
            let provider = validate_provider_export(raw_provider, &snapshot_files)?;
            Ok(Some(ParsedPackageManifest {
                export_kind: PackageExportKind::Provider,
                version,
                description,
                tools: Vec::new(),
                mcps: Vec::new(),
                provider: Some(provider),
            }))
        }
    }
}

fn collect_single_or_many<T>(single: Option<T>, many: Vec<T>, section: &str) -> Result<Vec<T>> {
    match (single, many.is_empty()) {
        (Some(_), false) => {
            bail!("manifest cannot declare both `[{section}]` and `[[{section}s]]`")
        }
        (Some(item), true) => Ok(vec![item]),
        (None, false) => Ok(many),
        (None, true) => bail!("manifest kind `{section}` must declare at least one export"),
    }
}

fn ensure_no_skill_exports_in_non_skill_package(snapshot: &PackageSnapshot) -> Result<()> {
    let has_skill = snapshot.entries.iter().any(|entry| {
        entry.path == "SKILL.md"
            || entry.path.ends_with("/SKILL.md")
            || (entry.path.starts_with("skills/") && entry.path.ends_with(".md"))
    });
    ensure!(
        !has_skill,
        "non-skill package cannot mix Skill exports (single export kind per package)"
    );
    Ok(())
}

fn validate_tool_exports(
    raw_tools: Vec<RawToolManifest>,
    snapshot_files: &BTreeSet<&str>,
) -> Result<Vec<ToolPackageExport>> {
    let mut seen_names = BTreeSet::new();
    let mut exports = Vec::with_capacity(raw_tools.len());
    for raw in raw_tools {
        validate_canonical_tool_name(&raw.name)?;
        ensure!(
            seen_names.insert(raw.name.clone()),
            "duplicate tool export name `{}`",
            raw.name
        );
        let description = raw.description.trim().to_string();
        ensure!(
            !description.is_empty() && description.len() <= 1024,
            "tool `{}` description must be 1..=1024 characters",
            raw.name
        );
        let entrypoint = normalize_relative_snapshot_path(&raw.entrypoint, "tool entrypoint")?;
        ensure!(
            snapshot_files.contains(entrypoint.as_str()),
            "tool `{}` entrypoint `{entrypoint}` does not exist in package snapshot",
            raw.name
        );
        let parameters = parse_parameters_schema(&raw.parameters)
            .with_context(|| format!("invalid parameters schema for tool `{}`", raw.name))?;
        validate_tool_parameters_schema(&parameters)?;
        let timeout_secs = validate_packaged_tool_timeout(raw.timeout_secs)?;
        ensure!(
            raw.arguments == "json_first_arg" && raw.result == "stdout_json",
            "tool `{}` has unsupported execution hints (expected json_first_arg / stdout_json)",
            raw.name
        );
        if raw.allow_network
            && !matches!(
                raw.capability,
                ToolCapability::Network | ToolCapability::Unknown
            )
        {
            bail!(
                "tool `{}` cannot enable allow_network with `{:?}` capability",
                raw.name,
                raw.capability
            );
        }
        exports.push(ToolPackageExport {
            name: raw.name.clone(),
            description,
            entrypoint: format!("source/{entrypoint}"),
            parameters,
            capability: raw.capability,
            capability_is_argument_dependent: raw.capability_is_argument_dependent,
            timeout_secs,
            allow_network: raw.allow_network,
            manifest_path: format!("tools/{}/manifest.json", raw.name),
        });
    }
    exports.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(exports)
}

fn validate_mcp_exports(
    raw_mcps: Vec<RawMcpManifest>,
    snapshot_files: &BTreeSet<&str>,
) -> Result<Vec<McpPackageExport>> {
    let mut seen_ids = BTreeSet::new();
    let mut exports = Vec::with_capacity(raw_mcps.len());
    for raw in raw_mcps {
        validate_package_id(&raw.id)
            .with_context(|| format!("invalid MCP server id `{}`", raw.id))?;
        ensure!(
            seen_ids.insert(raw.id.clone()),
            "duplicate MCP server id `{}`",
            raw.id
        );
        // Ensure the server id can produce canonical `mcp_<server>_<tool>` names cleanly.
        let _ = canonical_mcp_tool_name(&raw.id, "probe")?;
        let timeout_secs = validate_packaged_tool_timeout(raw.timeout_secs)?;
        let description = normalize_optional_text(raw.description, "mcp description", 512)?;

        let (transport, capability) = match raw.transport {
            RawMcpTransport::Stdio {
                command,
                args,
                env,
                allow_network,
            } => {
                let rel_cmd = normalize_relative_snapshot_path(&command, "mcp stdio command")?;
                ensure!(
                    snapshot_files.contains(rel_cmd.as_str()),
                    "mcp server `{}` stdio command `{rel_cmd}` does not exist in package snapshot",
                    raw.id
                );
                ensure!(
                    args.len() <= MAX_ARGS_COUNT,
                    "mcp server `{}` has too many stdio args (max {MAX_ARGS_COUNT})",
                    raw.id
                );
                ensure!(
                    args.iter().all(|arg| !arg.contains('\0')),
                    "mcp server `{}` args contain null bytes",
                    raw.id
                );
                validate_safe_stdio_env(&env)?;
                let capability = raw.capability.unwrap_or(ToolCapability::Unknown);
                if allow_network
                    && !matches!(
                        capability,
                        ToolCapability::Network | ToolCapability::Unknown
                    )
                {
                    bail!(
                        "mcp server `{}` cannot allow network access with `{capability:?}` capability",
                        raw.id
                    );
                }
                (
                    McpTransportSpec::Stdio {
                        command: format!("source/{rel_cmd}"),
                        args,
                        env,
                        allow_network,
                    },
                    capability,
                )
            }
            RawMcpTransport::StreamableHttp { url, headers } => {
                let _ = validate_http_endpoint_url(&url, "MCP server")?;
                validate_safe_http_headers(&headers, "MCP server")?;
                let capability = raw.capability.unwrap_or(ToolCapability::Network);
                ensure!(
                    matches!(
                        capability,
                        ToolCapability::Network | ToolCapability::Unknown
                    ),
                    "streamable_http MCP server `{}` must declare `network` or `unknown` capability",
                    raw.id
                );
                (
                    McpTransportSpec::StreamableHttp { url, headers },
                    capability,
                )
            }
        };

        exports.push(McpPackageExport {
            server_id: raw.id.clone(),
            description,
            transport,
            capability,
            timeout_secs,
            descriptor_path: format!("mcp/{}/server.json", raw.id),
        });
    }
    exports.sort_by(|left, right| left.server_id.cmp(&right.server_id));
    Ok(exports)
}

fn validate_provider_export(
    raw: RawProviderManifest,
    snapshot_files: &BTreeSet<&str>,
) -> Result<ProviderPackageExport> {
    validate_provider_id(&raw.id)?;
    ensure!(
        !FIRST_PARTY_PROVIDER_IDS.contains(&raw.id.as_str()),
        "provider package id `{}` conflicts with a built-in first-party provider",
        raw.id
    );
    ensure!(
        SUPPORTED_PROVIDER_PROTOCOLS.contains(&raw.protocol.as_str()),
        "provider `{}` requests unsupported protocol `{}`",
        raw.id,
        raw.protocol
    );
    let display_name = raw.display_name.trim().to_string();
    ensure!(
        !display_name.is_empty() && display_name.len() <= 128,
        "provider `{}` display_name must be 1..=128 characters",
        raw.id
    );
    let _ = validate_http_endpoint_url(&raw.default_base_url, "provider base")?;
    validate_safe_http_headers(&raw.default_headers, "provider")?;

    if let Some(adapter_rel) = raw.adapter_descriptor.as_deref() {
        let normalized = normalize_relative_snapshot_path(adapter_rel, "adapter descriptor")?;
        ensure!(
            snapshot_files.contains(normalized.as_str()),
            "provider `{}` adapter_descriptor `{normalized}` does not exist in package snapshot",
            raw.id
        );
    }

    let mut models = Vec::with_capacity(raw.models.len());
    let mut seen_models = BTreeSet::new();
    for model in raw.models {
        let trimmed = model.trim().to_string();
        ensure!(
            !trimmed.is_empty() && trimmed.len() <= 128,
            "provider `{}` model name must be 1..=128 characters",
            raw.id
        );
        if seen_models.insert(trimmed.clone()) {
            models.push(trimmed);
        }
    }
    let default_model = normalize_optional_text(raw.default_model, "provider default_model", 128)?
        .or_else(|| models.first().cloned());

    let api_key_header = normalize_optional_text(raw.api_key_header, "api_key_header", 64)?;
    if let Some(header) = api_key_header.as_deref() {
        ensure!(
            !header.contains(['\r', '\n', ':'])
                && header
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
            "invalid provider api_key_header name `{header}`"
        );
    }

    let auth_mode = raw.auth_mode.unwrap_or_else(|| {
        if api_key_header.is_some() {
            ProviderAuthMode::HeaderApiKey
        } else {
            ProviderAuthMode::BearerApiKey
        }
    });
    match auth_mode {
        ProviderAuthMode::HeaderApiKey => {
            ensure!(
                api_key_header.is_some(),
                "provider `{}` with `header_api_key` auth_mode must set `api_key_header`",
                raw.id
            );
        }
        ProviderAuthMode::BearerApiKey | ProviderAuthMode::None => {
            ensure!(
                api_key_header.is_none(),
                "provider `{}` cannot combine `api_key_header` with `{auth_mode:?}` auth_mode",
                raw.id
            );
        }
    }

    Ok(ProviderPackageExport {
        provider_id: raw.id,
        protocol: raw.protocol,
        display_name,
        default_base_url: raw.default_base_url,
        default_model,
        models,
        auth_mode,
        api_key_header,
        default_headers: raw.default_headers,
        descriptor_path: "provider/adapter.json".to_string(),
    })
}

fn validate_provider_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.len() <= 64,
        "provider id `{id}` must be 1..=64 characters"
    );
    let mut bytes = id.bytes();
    let Some(first) = bytes.next() else {
        bail!("provider id must not be empty");
    };
    ensure!(
        first.is_ascii_lowercase() || first.is_ascii_digit(),
        "provider id `{id}` must start with ASCII lowercase letter or digit"
    );
    ensure!(
        bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')),
        "provider id `{id}` contains unsupported characters"
    );
    Ok(())
}

fn parse_parameters_schema(value: &toml::Value) -> Result<serde_json::Value> {
    match value {
        toml::Value::String(raw) => serde_json::from_str::<serde_json::Value>(raw)
            .context("parameters JSON string is not valid JSON"),
        toml::Value::Table(_) => {
            serde_json::to_value(value).context("failed converting parameters table to JSON")
        }
        _ => bail!("tool `parameters` must be a TOML table or JSON schema string"),
    }
}

fn normalize_relative_snapshot_path(raw: &str, label: &str) -> Result<String> {
    let trimmed = raw.trim();
    ensure!(!trimmed.is_empty(), "{label} must not be empty");
    let path = Path::new(trimmed);
    ensure!(!path.is_absolute(), "{label} `{trimmed}` must be relative");
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(seg) => {
                let text = seg
                    .to_str()
                    .with_context(|| format!("{label} path is not UTF-8"))?;
                parts.push(text.to_string());
            }
            Component::CurDir => {}
            _ => bail!("{label} `{trimmed}` contains invalid path traversal"),
        }
    }
    ensure!(!parts.is_empty(), "{label} must name a file");
    Ok(parts.join("/"))
}

fn normalize_optional_text(
    value: Option<String>,
    label: &str,
    max_len: usize,
) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim().to_string();
    ensure!(!trimmed.is_empty(), "{label} must not be blank");
    ensure!(
        trimmed.len() <= max_len,
        "{label} exceeds maximum length ({max_len} chars)"
    );
    Ok(Some(trimmed))
}
