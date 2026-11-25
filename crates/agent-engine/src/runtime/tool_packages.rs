//! Packaged Tool manifest definitions and confined executable runner.

mod network_proxy;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use alan_agent_protocol::ToolCapability;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use self::network_proxy::NetworkDomainProxy;
use crate::tools::{Sandbox, SandboxSpec, Tool, ToolContext, ToolResult, ToolSecretResolver};

const TOOL_MANIFEST_VERSION: u16 = 1;
const MAX_TOOL_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_TOOL_TIMEOUT_SECS: usize = 300;
const MAX_ARGV_ARGS: usize = 256;
const MINIMAL_SAFE_PATH: &str = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin";

/// Legacy Tool argument encoding passing a single JSON object as `argv[1]`.
pub const TOOL_ARGUMENTS_JSON_FIRST_ARG: &str = "json_first_arg";
/// Thin `argv` forwarding encoding (`{"args": [...], "stdin": "..."}`) passed via direct `execve`.
pub const TOOL_ARGUMENTS_ARGV: &str = "argv";
/// Standard stdout JSON result convention.
pub const TOOL_RESULT_STDOUT_JSON: &str = "stdout_json";

const RESERVED_TOOL_SECRET_ENVS: &[&str] = &[
    "PATH",
    "HOME",
    "TMPDIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "PYTHONPATH",
    "NODE_OPTIONS",
    "RUBYOPT",
    "PERL5OPT",
    "IFS",
    "BASH_ENV",
    "ENV",
];

const RAW_SECRET_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "pk-",
    "pk_",
    "aiza",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "akia",
    "asia",
    "bearer",
    "eyj",
];

/// Named Host Store credential reference projected into a confined Tool subprocess.
///
/// Manifests declare only `{ env, credential_id }`; raw credential bytes stay in Host Store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolSecretRef {
    /// Environment variable name exposed to the confined subprocess (e.g. `GEMINI_API_KEY`).
    pub env: String,
    /// Host `SecretStore` credential identifier (e.g. `gemini-api-key`), never raw secret bytes.
    pub credential_id: String,
}

/// Short-lived `0600` credential file (such as scoped ADC JSON) unlinked on `Drop`.
pub struct EphemeralSecretFile {
    path: PathBuf,
}

impl std::fmt::Debug for EphemeralSecretFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EphemeralSecretFile")
            .field("path", &self.path)
            .finish()
    }
}

impl EphemeralSecretFile {
    /// Create a fresh `0600` file inside `dir` containing `contents`, unlinked when dropped.
    pub fn create_0600(dir: &Path, prefix: &str, contents: &[u8]) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("create ephemeral secret directory {}", dir.display()))?;
        let dir_meta = std::fs::symlink_metadata(dir)
            .with_context(|| format!("inspect ephemeral secret directory {}", dir.display()))?;
        ensure!(
            dir_meta.is_dir() && !dir_meta.file_type().is_symlink(),
            "ephemeral secret directory {} must be a real directory",
            dir.display()
        );
        let safe_prefix: String = prefix
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .collect();
        let file_name = format!(
            "{}-{}.json",
            if safe_prefix.is_empty() {
                "alan-secret"
            } else {
                safe_prefix.as_str()
            },
            uuid::Uuid::new_v4().simple()
        );
        let path = dir.join(file_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&path)
            .with_context(|| format!("create ephemeral secret file {}", path.display()))?;
        if let Err(error) = file.write_all(contents).and_then(|()| file.sync_all()) {
            let _ = std::fs::remove_file(&path);
            return Err(error)
                .with_context(|| format!("write ephemeral secret file {}", path.display()));
        }
        Ok(Self { path })
    }

    /// Path to the ephemeral `0600` credential file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for EphemeralSecretFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Namespace-owned model and execution metadata for one Tool package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolPackageManifest {
    pub version: u16,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub capability: ToolCapability,
    /// Dynamic Tools classify concrete arguments inside their execution adapter.
    #[serde(default)]
    pub capability_is_argument_dependent: bool,
    pub timeout_secs: usize,
    pub execution: ToolExecutionHints,
    /// Declared outbound network domain allowlist (`allow_network = true` requires non-empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub network_domains: Vec<String>,
    /// Named Host Store secrets projected into the confined subprocess environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_secrets: Vec<ToolSecretRef>,
    /// Optional Host Store ADC JSON credential projected as an ephemeral `0600` file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_adc: Option<ToolSecretRef>,
}

/// Argument and stdout result conventions declared in a Tool package manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolExecutionHints {
    pub arguments: String,
    pub result: String,
}

impl ToolPackageManifest {
    /// Returns the supported Tool package manifest schema version (`1`).
    pub const fn schema_version() -> u16 {
        TOOL_MANIFEST_VERSION
    }

    pub(crate) fn model_definition(&self) -> alan_llm::ToolDefinition {
        alan_llm::ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
        }
    }

    pub(crate) fn from_tool(tool: &dyn Tool, timeout_secs: usize) -> Result<Self> {
        let manifest = Self {
            version: TOOL_MANIFEST_VERSION,
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            parameters: tool.parameters_schema(),
            capability: tool.capability(&serde_json::Value::Null),
            capability_is_argument_dependent: tool.capability_is_argument_dependent(),
            timeout_secs,
            execution: ToolExecutionHints {
                arguments: TOOL_ARGUMENTS_JSON_FIRST_ARG.to_string(),
                result: TOOL_RESULT_STDOUT_JSON.to_string(),
            },
            network_domains: Vec::new(),
            required_secrets: Vec::new(),
            required_adc: None,
        };
        manifest.validate_for_name(tool.name())?;
        Ok(manifest)
    }

    /// Validate that this manifest matches `mounted_name` and adheres to the runtime contract.
    pub fn validate_for_name(&self, mounted_name: &str) -> Result<()> {
        if self.version != TOOL_MANIFEST_VERSION {
            bail!("unsupported Tool manifest version {}", self.version);
        }
        if self.name != mounted_name || self.name.is_empty() || self.name.contains('/') {
            bail!("Tool manifest name does not match mounted package '{mounted_name}'");
        }
        if self.description.trim().is_empty() || !self.parameters.is_object() {
            bail!("Tool manifest '{mounted_name}' lacks model metadata");
        }
        if !matches!(
            self.execution.arguments.as_str(),
            TOOL_ARGUMENTS_JSON_FIRST_ARG | TOOL_ARGUMENTS_ARGV
        ) || self.execution.result != TOOL_RESULT_STDOUT_JSON
        {
            bail!("Tool manifest '{mounted_name}' has unsupported execution hints");
        }
        let mut seen_domains = BTreeSet::new();
        for domain in &self.network_domains {
            let normalized = validate_network_domain(domain).with_context(|| {
                format!("Tool manifest '{mounted_name}' declares invalid network domain `{domain}`")
            })?;
            ensure!(
                seen_domains.insert(normalized),
                "Tool manifest '{mounted_name}' declares duplicate network domain `{domain}`"
            );
        }
        validate_tool_secret_refs(&self.required_secrets, self.required_adc.as_ref())
            .with_context(|| format!("Tool manifest '{mounted_name}' has invalid secret ref"))?;
        Ok(())
    }
}

/// Default JSON Schema for thin `argv` forwarding Tools (`{"args": [...], "stdin": "..."}`).
pub fn default_argv_parameters_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "args": {
                "type": "array",
                "items": { "type": "string" }
            },
            "stdin": {
                "type": "string"
            }
        },
        "required": ["args"],
        "additionalProperties": false
    })
}

/// Validates and normalizes a DNS hostname or IPv4 literal for `network_domains`.
pub fn validate_network_domain(raw: &str) -> Result<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    ensure!(
        !normalized.is_empty()
            && normalized.len() <= 253
            && !normalized.starts_with('.')
            && !normalized.ends_with('.')
            && !normalized.contains("..")
            && normalized.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }),
        "invalid network domain `{raw}`"
    );
    Ok(normalized)
}

/// Validates a single [`ToolSecretRef`], rejecting raw credentials and reserved env names.
pub fn validate_tool_secret_ref(secret_ref: &ToolSecretRef, label: &str) -> Result<()> {
    let env = secret_ref.env.as_str();
    ensure!(
        !env.is_empty() && env.len() <= 128 && env.trim() == env,
        "{label} env name `{env}` must be 1..=128 non-whitespace characters"
    );
    let mut env_bytes = env.bytes();
    let Some(first_env) = env_bytes.next() else {
        bail!("{label} env name must not be empty");
    };
    ensure!(
        first_env.is_ascii_alphabetic() || first_env == b'_',
        "{label} env name `{env}` must start with an ASCII letter or underscore"
    );
    ensure!(
        env_bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "{label} env name `{env}` contains unsupported characters"
    );
    let upper_env = env.to_ascii_uppercase();
    ensure!(
        !upper_env.starts_with("ALAN_") && !RESERVED_TOOL_SECRET_ENVS.contains(&upper_env.as_str()),
        "{label} env name `{env}` cannot override protected runtime variable"
    );

    let cred = secret_ref.credential_id.as_str();
    ensure!(
        !cred.is_empty() && cred.len() <= 64 && cred.trim() == cred,
        "{label} credential_id must be a 1..=64 char identifier (never raw secret material)"
    );
    let mut cred_bytes = cred.bytes();
    let Some(first_cred) = cred_bytes.next() else {
        bail!("{label} credential_id must not be empty");
    };
    ensure!(
        (first_cred.is_ascii_lowercase() || first_cred.is_ascii_digit())
            && cred_bytes
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_')),
        "{label} credential_id `{cred}` must be a lowercase `[a-z0-9][a-z0-9_-]*` identifier (never raw secret bytes or paths)"
    );
    ensure!(
        !RAW_SECRET_PREFIXES
            .iter()
            .any(|prefix| cred.starts_with(prefix)),
        "{label} credential_id `{cred}` looks like a raw credential token rather than a Host Store identifier"
    );
    Ok(())
}

/// Validates `required_secrets` and `required_adc` and checks for duplicate target `env` names.
pub fn validate_tool_secret_refs(
    required_secrets: &[ToolSecretRef],
    required_adc: Option<&ToolSecretRef>,
) -> Result<()> {
    let mut seen_envs = BTreeSet::new();
    for secret_ref in required_secrets {
        validate_tool_secret_ref(secret_ref, "required_secrets")?;
        ensure!(
            seen_envs.insert(secret_ref.env.clone()),
            "duplicate required_secrets env `{}`",
            secret_ref.env
        );
    }
    if let Some(adc_ref) = required_adc {
        validate_tool_secret_ref(adc_ref, "required_adc")?;
        ensure!(
            seen_envs.insert(adc_ref.env.clone()),
            "required_adc env `{}` duplicates a required_secrets entry",
            adc_ref.env
        );
    }
    Ok(())
}

/// Validates that an ADC payload is a non-empty JSON object without leaking raw bytes on error.
pub fn validate_adc_json_payload(raw: &str, credential_id: &str) -> Result<()> {
    let trimmed = raw.trim();
    ensure!(
        !trimmed.is_empty(),
        "Host Store ADC credential `{credential_id}` is empty"
    );
    let parsed: serde_json::Value = serde_json::from_str(trimmed).map_err(|_| {
        anyhow::anyhow!("Host Store ADC credential `{credential_id}` is not valid JSON")
    })?;
    ensure!(
        parsed.is_object(),
        "Host Store ADC credential `{credential_id}` must be a JSON object"
    );
    Ok(())
}

/// Validates a canonical Tool identifier (`1..=64` `[a-z0-9][a-z0-9_-]*`).
pub fn validate_canonical_tool_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && name.len() <= 64,
        "Tool name `{name}` must be 1..=64 characters"
    );
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        bail!("Tool name must not be empty");
    };
    ensure!(
        first.is_ascii_lowercase() || first.is_ascii_digit(),
        "Tool name `{name}` must start with an ASCII lowercase letter or digit"
    );
    ensure!(
        bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'),
        "Tool name `{name}` contains unsupported characters"
    );
    ensure!(
        !matches!(name, "q" | "alan-agent" | "alan-work"),
        "Tool name `{name}` collides with a reserved alan9 executable"
    );
    Ok(())
}

/// Validates that `parameters` is a JSON object compatible with JSON Schema Draft 7.
pub fn validate_tool_parameters_schema(parameters: &serde_json::Value) -> Result<()> {
    ensure!(
        parameters.is_object(),
        "Tool parameters schema must be a JSON object"
    );
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(parameters)
        .map_err(|error| anyhow::anyhow!("invalid Tool JSON Schema: {error}"))?;
    Ok(())
}

/// Validates a bounded packaged-Tool timeout (`1..=300` seconds).
pub fn validate_packaged_tool_timeout(timeout_secs: usize) -> Result<usize> {
    ensure!(
        (1..=MAX_TOOL_TIMEOUT_SECS).contains(&timeout_secs),
        "packaged Tool timeout_secs must be between 1 and {MAX_TOOL_TIMEOUT_SECS}"
    );
    Ok(timeout_secs)
}

/// Confined host-process Tool adapter executing one Quartermaster `tool` package entrypoint.
pub struct PackagedTool {
    manifest: ToolPackageManifest,
    executable: PathBuf,
    allow_network: bool,
    secret_resolver: Option<Arc<dyn ToolSecretResolver>>,
}

impl PackagedTool {
    /// Creates a new packaged Tool backed by an immutable materialized executable file on disk.
    pub fn new(
        manifest: ToolPackageManifest,
        executable: PathBuf,
        allow_network: bool,
    ) -> Result<Self> {
        validate_canonical_tool_name(&manifest.name)?;
        validate_tool_parameters_schema(&manifest.parameters)?;
        let _ = validate_packaged_tool_timeout(manifest.timeout_secs)?;
        manifest.validate_for_name(&manifest.name)?;
        if allow_network
            && !matches!(
                manifest.capability,
                ToolCapability::Network | ToolCapability::Unknown
            )
        {
            bail!(
                "packaged Tool `{}` cannot enable network access with `{:?}` capability",
                manifest.name,
                manifest.capability
            );
        }
        if allow_network && manifest.network_domains.is_empty() {
            bail!(
                "packaged Tool `{}` cannot enable network access with an empty `network_domains` list",
                manifest.name
            );
        }
        if !allow_network && !manifest.network_domains.is_empty() {
            bail!(
                "packaged Tool `{}` declares `network_domains` while `allow_network` is false",
                manifest.name
            );
        }
        ensure!(
            executable.is_file(),
            "packaged Tool `{}` executable does not exist on disk",
            manifest.name
        );
        Ok(Self {
            manifest,
            executable,
            allow_network,
            secret_resolver: None,
        })
    }

    /// Attach a Host-owned secret resolver hook for resolving `required_secrets` / `required_adc`.
    pub fn with_secret_resolver(mut self, resolver: Arc<dyn ToolSecretResolver>) -> Self {
        self.secret_resolver = Some(resolver);
        self
    }

    /// Attach an optional Host-owned secret resolver hook.
    pub fn with_optional_secret_resolver(
        mut self,
        resolver: Option<Arc<dyn ToolSecretResolver>>,
    ) -> Self {
        self.secret_resolver = resolver;
        self
    }

    /// Returns the namespace-projected `ToolPackageManifest`.
    pub fn manifest(&self) -> &ToolPackageManifest {
        &self.manifest
    }
}

impl Tool for PackagedTool {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn description(&self) -> &str {
        &self.manifest.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.manifest.parameters.clone()
    }

    fn capability(&self, _args: &serde_json::Value) -> ToolCapability {
        self.manifest.capability
    }

    fn capability_is_argument_dependent(&self) -> bool {
        self.manifest.capability_is_argument_dependent
    }

    fn timeout_secs(&self) -> usize {
        self.manifest.timeout_secs
    }

    fn execute(&self, arguments: serde_json::Value, ctx: &ToolContext) -> ToolResult {
        let executable = self.executable.clone();
        let allow_network = self.allow_network;
        let manifest = self.manifest.clone();
        let name = manifest.name.clone();
        let resolver = ctx
            .secret_resolver()
            .cloned()
            .or_else(|| self.secret_resolver.clone());
        let scratch_dir = ctx.scratch_dir.clone();
        let (sandbox, cwd) = resolve_execution_sandbox_and_cwd(ctx);

        Box::pin(async move {
            let (argv, stdin_payload) = encode_tool_invocation(&manifest, &arguments)?;
            let resolved_secrets =
                resolve_tool_secrets(&manifest, resolver.as_deref(), &scratch_dir)?;
            let (sandbox, cwd) = (sandbox?, cwd?);
            let proxy = if allow_network {
                Some(NetworkDomainProxy::start(manifest.network_domains.clone()).await?)
            } else {
                None
            };

            let mut command = match proxy.as_ref() {
                Some(proxy) => sandbox.build_confined_program_command_with_proxy(
                    &executable,
                    &argv,
                    &cwd,
                    proxy.port(),
                ),
                None => sandbox.build_confined_program_command(&executable, &argv, &cwd, false),
            }
            .with_context(|| format!("failed to confine packaged Tool `{name}`"))?;

            configure_sanitized_subprocess(&mut command, &ctx_env_home(&cwd), &name);
            if let Some(proxy) = proxy.as_ref() {
                let proxy_url = format!("http://127.0.0.1:{}", proxy.port());
                for var in [
                    "HTTP_PROXY",
                    "HTTPS_PROXY",
                    "ALL_PROXY",
                    "http_proxy",
                    "https_proxy",
                    "all_proxy",
                ] {
                    command.env(var, &proxy_url);
                }
                command.env("NO_PROXY", "").env("no_proxy", "");
            }
            for (env_name, env_value) in &resolved_secrets.env_pairs {
                command.env(env_name, env_value);
            }
            if stdin_payload.is_some() {
                command.stdin(std::process::Stdio::piped());
            }

            let mut child = command
                .spawn()
                .with_context(|| format!("failed to spawn packaged Tool `{name}`"))?;
            let stdin_task = if let Some(payload) = stdin_payload {
                let mut child_stdin = child
                    .stdin
                    .take()
                    .context("packaged Tool child missing piped stdin")?;
                Some(tokio::spawn(async move {
                    let _ = child_stdin.write_all(payload.as_bytes()).await;
                    let _ = child_stdin.shutdown().await;
                }))
            } else {
                None
            };
            let stdout_pipe = child.stdout.take();
            let stderr_pipe = child.stderr.take();
            let stdout_task = tokio::spawn(read_limited_pipe(stdout_pipe, MAX_TOOL_OUTPUT_BYTES));
            let stderr_task = tokio::spawn(read_limited_pipe(stderr_pipe, MAX_TOOL_OUTPUT_BYTES));

            let status = child
                .wait()
                .await
                .with_context(|| format!("failed waiting on packaged Tool `{name}`"))?;
            if let Some(task) = stdin_task {
                let _ = task.await;
            }
            let stdout = stdout_task.await.unwrap_or_default();
            let stderr = stderr_task.await.unwrap_or_default();
            drop(proxy);

            if !status.success() {
                let err_detail = String::from_utf8_lossy(&stderr);
                let out_detail = String::from_utf8_lossy(&stdout);
                let raw_detail = if !err_detail.trim().is_empty() {
                    err_detail.trim().to_string()
                } else {
                    out_detail.trim().to_string()
                };
                let detail = scrub_secret_values(&raw_detail, &resolved_secrets.scrub_needles);
                bail!(
                    "packaged Tool `{name}` exited with status {:?}{}",
                    status.code(),
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!(": {detail}")
                    }
                );
            }

            let trimmed = String::from_utf8_lossy(&stdout);
            let scrubbed = scrub_secret_values(trimmed.trim(), &resolved_secrets.scrub_needles);
            if scrubbed.is_empty() {
                return Ok(serde_json::json!({ "success": true }));
            }
            match serde_json::from_str::<serde_json::Value>(&scrubbed) {
                Ok(value) => Ok(value),
                Err(_) => Ok(serde_json::json!({
                    "success": true,
                    "output": scrubbed,
                })),
            }
        })
    }
}

fn encode_tool_invocation(
    manifest: &ToolPackageManifest,
    arguments: &serde_json::Value,
) -> Result<(Vec<String>, Option<String>)> {
    match manifest.execution.arguments.as_str() {
        TOOL_ARGUMENTS_JSON_FIRST_ARG => {
            let encoded =
                serde_json::to_string(arguments).context("failed to encode Tool arguments")?;
            Ok((vec![encoded], None))
        }
        TOOL_ARGUMENTS_ARGV => {
            let object = arguments
                .as_object()
                .with_context(|| format!("Tool `{}` expects a JSON object", manifest.name))?;
            let raw_args = object
                .get("args")
                .and_then(serde_json::Value::as_array)
                .with_context(|| {
                    format!(
                        "Tool `{}` requires an `args` array of strings",
                        manifest.name
                    )
                })?;
            ensure!(
                raw_args.len() <= MAX_ARGV_ARGS,
                "Tool `{}` args exceed limit ({MAX_ARGV_ARGS})",
                manifest.name
            );
            let mut argv = Vec::with_capacity(raw_args.len());
            for (index, item) in raw_args.iter().enumerate() {
                let text = item.as_str().with_context(|| {
                    format!("Tool `{}` args[{index}] must be a string", manifest.name)
                })?;
                ensure!(
                    !text.contains('\0'),
                    "Tool `{}` args[{index}] contains null bytes",
                    manifest.name
                );
                argv.push(text.to_string());
            }
            let stdin = match object.get("stdin") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(text)) => Some(text.clone()),
                Some(_) => bail!("Tool `{}` `stdin` must be a string", manifest.name),
            };
            Ok((argv, stdin))
        }
        other => bail!(
            "Tool `{}` has unsupported execution arguments mode `{other}`",
            manifest.name
        ),
    }
}

struct ResolvedToolSecrets {
    env_pairs: Vec<(String, String)>,
    scrub_needles: Vec<String>,
    _adc_file: Option<EphemeralSecretFile>,
}

fn resolve_tool_secrets(
    manifest: &ToolPackageManifest,
    resolver: Option<&dyn ToolSecretResolver>,
    scratch_dir: &Path,
) -> Result<ResolvedToolSecrets> {
    if manifest.required_secrets.is_empty() && manifest.required_adc.is_none() {
        return Ok(ResolvedToolSecrets {
            env_pairs: Vec::new(),
            scrub_needles: Vec::new(),
            _adc_file: None,
        });
    }

    let first_ref = manifest
        .required_secrets
        .first()
        .or(manifest.required_adc.as_ref())
        .expect("checked non-empty secrets");
    let resolver = resolver.with_context(|| {
        format!(
            "packaged Tool `{}` requires Host Store credential `{}` ({}), but no Host secret store is attached. Configure it with `alan q set-secret {}`.",
            manifest.name, first_ref.credential_id, first_ref.env, first_ref.credential_id
        )
    })?;

    let mut env_pairs = Vec::new();
    let mut scrub_needles = Vec::new();

    for secret_ref in &manifest.required_secrets {
        let value = resolver
            .resolve_named_secret(&secret_ref.credential_id)
            .with_context(|| {
                format!(
                    "failed resolving Host Store credential `{}` for packaged Tool `{}`",
                    secret_ref.credential_id, manifest.name
                )
            })?
            .filter(|val| !val.trim().is_empty())
            .with_context(|| {
                format!(
                    "missing required Host Store credential `{}` for env `{}` in packaged Tool `{}`. Configure it with `alan q set-secret {}`.",
                    secret_ref.credential_id, secret_ref.env, manifest.name, secret_ref.credential_id
                )
            })?;
        ensure!(
            !value.contains('\0'),
            "Host Store credential `{}` for packaged Tool `{}` contains invalid null bytes",
            secret_ref.credential_id,
            manifest.name
        );
        scrub_needles.push(value.clone());
        env_pairs.push((secret_ref.env.clone(), value));
    }

    let adc_file = if let Some(adc_ref) = &manifest.required_adc {
        let payload = resolver
            .resolve_named_secret(&adc_ref.credential_id)
            .with_context(|| {
                format!(
                    "failed resolving Host Store ADC credential `{}` for packaged Tool `{}`",
                    adc_ref.credential_id, manifest.name
                )
            })?
            .filter(|val| !val.trim().is_empty())
            .with_context(|| {
                format!(
                    "missing required Host Store ADC credential `{}` for env `{}` in packaged Tool `{}`. Configure it with `alan q set-secret {}` (or `alan connection`).",
                    adc_ref.credential_id, adc_ref.env, manifest.name, adc_ref.credential_id
                )
            })?;
        validate_adc_json_payload(&payload, &adc_ref.credential_id)?;
        collect_json_secret_needles(&payload, &mut scrub_needles);
        let tmp_dir = if !scratch_dir.as_os_str().is_empty() {
            scratch_dir.to_path_buf()
        } else {
            std::env::temp_dir()
        };
        let file = EphemeralSecretFile::create_0600(&tmp_dir, "alan-adc", payload.as_bytes())?;
        env_pairs.push((adc_ref.env.clone(), file.path().display().to_string()));
        Some(file)
    } else {
        None
    };

    Ok(ResolvedToolSecrets {
        env_pairs,
        scrub_needles,
        _adc_file: adc_file,
    })
}

fn collect_json_secret_needles(payload: &str, needles: &mut Vec<String>) {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(payload)
    else {
        return;
    };
    for (key, value) in map {
        if let Some(text) = value.as_str()
            && text.len() >= 8
            && matches!(
                key.as_str(),
                "refresh_token" | "client_secret" | "private_key" | "access_token"
            )
        {
            needles.push(text.to_string());
        }
    }
}

fn scrub_secret_values(input: &str, needles: &[String]) -> String {
    let mut output = input.to_string();
    for needle in needles {
        if needle.len() >= 4 {
            output = output.replace(needle, "[REDACTED]");
        }
    }
    output
}

fn ctx_env_home(cwd: &Path) -> PathBuf {
    cwd.to_path_buf()
}

pub(crate) fn resolve_execution_sandbox_and_cwd(
    ctx: &ToolContext,
) -> (Result<Sandbox>, Result<PathBuf>) {
    if ctx.execution_adapter().is_ok() {
        (ctx.sandbox(), ctx.cwd())
    } else {
        let scratch = ctx.scratch_dir.clone();
        let mut spec = SandboxSpec::seed(scratch.clone());
        spec.read_denylist = Vec::new();
        let sandbox = Sandbox::from_spec(spec).with_trust_posture(ctx.trust_posture());
        (Ok(sandbox), Ok(scratch))
    }
}

pub(crate) fn configure_sanitized_subprocess(
    command: &mut tokio::process::Command,
    temp_home: &Path,
    tool_name: &str,
) {
    let path_env = std::env::var_os("PATH").unwrap_or_else(|| MINIMAL_SAFE_PATH.into());
    command
        .env_clear()
        .env("PATH", path_env)
        .env("HOME", temp_home)
        .env("TMPDIR", temp_home)
        .env("ALAN_TOOL_NAME", tool_name)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        command.process_group(0);
    }
}

async fn read_limited_pipe<R: tokio::io::AsyncRead + Unpin>(
    pipe: Option<R>,
    limit: usize,
) -> Vec<u8> {
    let Some(mut reader) = pipe else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4_096];
    while buffer.len() < limit {
        let remaining = limit - buffer.len();
        let to_read = remaining.min(chunk.len());
        match reader.read(&mut chunk[..to_read]).await {
            Ok(0) | Err(_) => break,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    }
    buffer
}
