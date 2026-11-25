//! Public data model types and command contracts for Package Service (Quartermaster).

use std::collections::BTreeMap;

use alan_agent_engine::runtime::tool_packages::{TOOL_ARGUMENTS_JSON_FIRST_ARG, ToolSecretRef};
use alan_agent_engine::skills::SkillTypedDependency;
use alan_agent_engine::tools::ToolCapability;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Materializer schema tag written on pre-existing skill packages.
pub(crate) const LEGACY_SKILL_MATERIALIZER_VERSION: &str = "alan-skill-v1";
/// Materializer schema tag written on five-kind Quartermaster hub packages.
pub(crate) const MATERIALIZER_VERSION: &str = "alan-package-v2";

/// Package ownership classification between product-preinstalled and operator-installed packages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    /// Embedded product-owned package managed exclusively by Alan bootstrap/updates.
    Preinstalled,
    /// Operator-installed package managed via Quartermaster `/bin/q` or `alan q`.
    Installed,
}

/// Functional extension category exported by a Quartermaster package.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PackageExportKind {
    /// Knowledge and workflow skills (`SKILL.md` / `skills/*.md`).
    #[default]
    Skill,
    /// Model Context Protocol (MCP) server exports (`stdio` or `streamable_http`).
    Mcp,
    /// Confined packaged Tool executable exports projected into `/bin`.
    Tool,
    /// Protocol-adapter extension provider package bound by Connection profiles.
    Provider,
    /// Confined external `aP` wire-protocol file-server plugin package.
    Fileserver,
}

impl PackageExportKind {
    /// Stable lowercase wire label used in CLI output and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Mcp => "mcp",
            Self::Tool => "tool",
            Self::Provider => "provider",
            Self::Fileserver => "fileserver",
        }
    }
}

/// Installation lifecycle state of one catalog entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageState {
    /// Active installed package available for resolution and new leases.
    Installed,
    /// Marked for removal once existing immutable reference leases drop to zero.
    Retiring,
}

/// Exported Skill descriptor inside an installed package revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageExport {
    /// Canonical Skill identifier.
    pub skill_id: String,
    /// Content-relative directory root containing `SKILL.md`.
    pub root: String,
    /// Typed availability dependencies declared or inferred for the Skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<SkillTypedDependency>,
}

/// Exported packaged Tool executable projected into `/bin/<name>` when referenced by a Process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPackageExport {
    /// Canonical Tool command name (`1..=64` `[a-z0-9][a-z0-9_-]*`).
    pub name: String,
    /// Tool description surfaced to models in `ToolPackageManifest`.
    pub description: String,
    /// Relative path inside materialized `content/` to the immutable executable file.
    pub entrypoint: String,
    /// JSON Schema Draft 7 parameters object.
    pub parameters: serde_json::Value,
    /// Explicit governance capability (defaults to [`ToolCapability::Unknown`]).
    pub capability: ToolCapability,
    /// Whether capability classification depends on runtime tool arguments.
    #[serde(default)]
    pub capability_is_argument_dependent: bool,
    /// Per-invocation execution timeout in seconds (`1..=300`).
    pub timeout_secs: usize,
    /// Whether outbound network is allowed inside the OS sandbox.
    #[serde(default)]
    pub allow_network: bool,
    /// Declared outbound network domain allowlist (`allow_network = true` requires non-empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_domains: Vec<String>,
    /// Named Host Store secrets projected into the confined subprocess environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_secrets: Vec<ToolSecretRef>,
    /// Optional Host Store ADC JSON credential projected as an ephemeral `0600` file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_adc: Option<ToolSecretRef>,
    /// Argument passing convention (`"json_first_arg"` or `"argv"`).
    #[serde(default = "default_tool_arguments_hint")]
    pub arguments: String,
    /// Relative path inside materialized `content/` to the JSON `ToolPackageManifest`.
    pub manifest_path: String,
}

fn default_tool_arguments_hint() -> String {
    TOOL_ARGUMENTS_JSON_FIRST_ARG.to_string()
}

/// Transport definition for one exported MCP server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpTransportSpec {
    /// Confined subprocess speaking newline-delimited JSON-RPC 2.0 over stdio.
    Stdio {
        /// Content-relative executable path inside `content/`.
        command: String,
        /// Fixed argument list passed to the server binary.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Non-secret static environment variables injected after env sanitization.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, String>,
        /// Whether network access is enabled inside the confined subprocess sandbox.
        #[serde(default)]
        allow_network: bool,
    },
    /// Streamable HTTP endpoint speaking JSON-RPC 2.0 via POST + JSON/SSE.
    StreamableHttp {
        /// HTTP or HTTPS server URL (without embedded userinfo credentials).
        url: String,
        /// Non-secret static request headers.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
    },
}

/// Exported MCP server descriptor inside an installed package revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpPackageExport {
    /// Canonical server identifier (`mcp_<server>_<tool>` naming segment).
    pub server_id: String,
    /// Optional server description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Stdio or Streamable HTTP transport specification.
    pub transport: McpTransportSpec,
    /// Conservative Tool governance capability assigned to projected tools from this server.
    pub capability: ToolCapability,
    /// Per-RPC timeout in seconds (`1..=300`).
    pub timeout_secs: usize,
    /// Content-relative normalized JSON descriptor path (`mcp/<server_id>/server.json`).
    pub descriptor_path: String,
}

/// Credential delivery convention supported by an extension provider adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthMode {
    /// `Authorization: Bearer <secret>` credential from Host `SecretStore`.
    BearerApiKey,
    /// Named HTTP header (such as `x-api-key`) populated from Host `SecretStore`.
    HeaderApiKey,
    /// Unauthenticated local endpoint.
    None,
}

/// Exported LLM protocol-adapter descriptor registered by a `provider` package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderPackageExport {
    /// Extension provider identifier (for example `anthropic_messages`).
    pub provider_id: String,
    /// Underlying wire protocol engine supported by `crates/llm`.
    pub protocol: String,
    /// Operator-facing provider display label.
    pub display_name: String,
    /// Default HTTPS or localhost HTTP API base URL.
    pub default_base_url: String,
    /// Default model identifier suggested when configuring Connection profiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Supported/advertised model identifiers for connection defaults and selection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// Host credential header delivery mode (never stores secret values).
    pub auth_mode: ProviderAuthMode,
    /// Header name when `auth_mode == ProviderAuthMode::HeaderApiKey`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_header: Option<String>,
    /// Non-secret static protocol headers (for example `anthropic-version`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_headers: BTreeMap<String, String>,
    /// Content-relative normalized JSON descriptor path (`provider/adapter.json`).
    pub descriptor_path: String,
}

/// Wire transport definition for one exported `fileserver` package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileserverTransportSpec {
    /// Confined subprocess speaking newline-delimited aP wire frames over stdin/stdout.
    Stdio {
        /// Content-relative executable path inside `content/`.
        command: String,
        /// Fixed argument list passed to the server binary.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Non-secret static environment variables injected after env sanitization.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, String>,
    },
    /// Confined subprocess speaking newline-delimited aP wire frames over a Unix domain socket.
    UnixSocket {
        /// Socket filename created inside the confined runtime socket directory.
        socket_name: String,
    },
}

/// Exported external `aP` file-server descriptor inside an installed package revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileserverPackageExport {
    /// Canonical `/srv/<service_id>` and `/mnt/<service_id>` identifier (`1..=64` `[a-z0-9][a-z0-9_-]*`).
    pub service_id: String,
    /// Optional server description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Content-relative path inside `content/` to the immutable executable (`fileservers/<id>/bin`).
    pub entrypoint: String,
    /// Wire transport specification (`stdio` or `unix_socket`).
    pub transport: FileserverTransportSpec,
    /// Whether outbound network is expected inside the confined sandbox when explicitly granted.
    #[serde(default)]
    pub allow_network: bool,
    /// Declared outbound network domain expectations (must be empty when `allow_network == false`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_domains: Vec<String>,
    /// Per-RPC and startup timeout bound in seconds (`1..=300`).
    pub timeout_secs: usize,
    /// Maximum aP wire frame size in bytes (`1024..=alan_ap::MAX_WIRE_FRAME_BYTES`).
    pub max_message_bytes: usize,
    /// Content-relative normalized JSON descriptor path (`fileservers/<service_id>/server.json`).
    pub descriptor_path: String,
}

/// Durable catalog entry describing one installed or preinstalled Quartermaster package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRecord {
    /// Unique canonical package identifier (`1..=64` `[a-z0-9]+(?:-[a-z0-9]+)*`).
    pub id: String,
    /// Deterministic SHA-256 content revision digest.
    pub revision: String,
    /// Ownership tier (`Preinstalled` or `Installed`).
    pub kind: PackageKind,
    /// Lifecycle state (`Installed` or `Retiring`).
    pub state: PackageState,
    /// Materializer version used for this revision.
    pub materializer_version: String,
    /// SHA-256 tree digest of the materialized `content/` directory.
    pub materialized_fingerprint: String,
    /// Exported Skill entries (non-empty for Skill packages).
    pub exports: Vec<PackageExport>,
    /// Active lease count held by running Processes.
    pub reference_count: u64,
    /// UTC creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// UTC last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Single functional export kind (`Skill`, `Mcp`, `Tool`, `Provider`, or `Fileserver`).
    #[serde(default)]
    pub export_kind: PackageExportKind,
    /// Optional semantic package version declared in `alan-package.toml` / `package.toml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_version: Option<String>,
    /// Optional package description declared in `alan-package.toml` / `package.toml`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Exported packaged Tool descriptors (`export_kind == PackageExportKind::Tool`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_exports: Vec<ToolPackageExport>,
    /// Exported MCP server descriptors (`export_kind == PackageExportKind::Mcp`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_exports: Vec<McpPackageExport>,
    /// Exported provider adapter descriptor (`export_kind == PackageExportKind::Provider`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_export: Option<ProviderPackageExport>,
    /// Exported external file-server plugin descriptor (`export_kind == PackageExportKind::Fileserver`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fileserver_export: Option<FileserverPackageExport>,
}

/// Complete serialized Package Service catalog (`catalog.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCatalog {
    /// Monotonically incremented catalog mutation counter.
    pub generation: u64,
    /// Map of package ID -> durable record.
    pub packages: BTreeMap<String, PackageRecord>,
}

/// Single file captured in a bounded namespace or preinstalled package snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSnapshotEntry {
    /// Normalized slash-separated relative file path.
    pub path: String,
    /// Exact raw bytes of the file.
    pub bytes: Vec<u8>,
    /// Whether the source file had Unix executable permissions.
    #[serde(default)]
    pub executable: bool,
}

/// Immutable source snapshot submitted to Package Service `Install`, `Upgrade`, or `Sync`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSnapshot {
    /// Source leaf used only for portable root Skill identity derivation.
    pub source_name: String,
    /// Regular files captured within bounded size and count limits.
    pub entries: Vec<PackageSnapshotEntry>,
}

/// One target package entry passed inside [`PackageCommand::Sync`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSyncEntry {
    /// Canonical package identifier.
    pub package_id: String,
    /// Immutable source snapshot for this package.
    pub snapshot: PackageSnapshot,
}

/// JSON control command written to `/mnt/package/ctl`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackageCommand {
    /// Install a new operator package from a bounded snapshot.
    Install {
        /// Caller-supplied correlation identifier echoed in `/mnt/package/result`.
        request_id: String,
        /// Canonical package identifier.
        package_id: String,
        /// Source snapshot tree.
        snapshot: PackageSnapshot,
    },
    /// Query the current package catalog.
    List {
        /// Caller-supplied correlation identifier.
        request_id: String,
    },
    /// Upgrade an existing installed operator package to a new snapshot.
    Upgrade {
        /// Caller-supplied correlation identifier.
        request_id: String,
        /// Canonical package identifier.
        package_id: String,
        /// New source snapshot tree.
        snapshot: PackageSnapshot,
    },
    /// Uninstall an operator package (retiring immediately if referenced by active Processes).
    Uninstall {
        /// Caller-supplied correlation identifier.
        request_id: String,
        /// Canonical package identifier.
        package_id: String,
    },
    /// Atomically install/upgrade a set of packages (or verify catalog integrity when empty).
    Sync {
        /// Caller-supplied correlation identifier.
        request_id: String,
        /// Optional ordered list of target packages to upsert atomically.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        packages: Vec<PackageSyncEntry>,
        /// When true and `packages` is non-empty, retire/remove operator packages absent from `packages`.
        #[serde(default)]
        prune: bool,
    },
}

impl PackageCommand {
    /// Returns the correlation `request_id` for this command.
    pub(crate) fn request_id(&self) -> &str {
        match self {
            Self::Install { request_id, .. }
            | Self::List { request_id }
            | Self::Upgrade { request_id, .. }
            | Self::Uninstall { request_id, .. }
            | Self::Sync { request_id, .. } => request_id,
        }
    }

    /// Returns the verb string stored on [`PackageCommandResult::action`].
    pub(crate) fn action(&self) -> &'static str {
        match self {
            Self::Install { .. } => "install",
            Self::List { .. } => "list",
            Self::Upgrade { .. } => "upgrade",
            Self::Uninstall { .. } => "uninstall",
            Self::Sync { .. } => "sync",
        }
    }
}

/// Terminal command response stored under `/mnt/package/result`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageCommandResult {
    /// Request identifier echoed from [`PackageCommand`].
    pub request_id: String,
    /// True if the command succeeded without error.
    pub success: bool,
    /// Executed command verb (`install`, `list`, `upgrade`, `uninstall`, `sync`).
    pub action: String,
    /// Bounded human-readable outcome message.
    pub message: String,
    /// Resulting package record for single-package mutations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageRecord>,
    /// Resulting catalog for `list` and `sync` operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<PackageCatalog>,
}

/// Validates a canonical Quartermaster package identifier (`1..=64` `[a-z0-9]+(?:-[a-z0-9]+)*`).
pub fn validate_package_id(value: &str) -> Result<()> {
    ensure!(!value.is_empty() && value.len() <= 64, "invalid package id");
    ensure!(
        value.split('-').all(|part| !part.is_empty()
            && part
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())),
        "invalid package id `{value}`"
    );
    Ok(())
}
