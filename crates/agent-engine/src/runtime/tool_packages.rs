//! Packaged Tool manifest definitions and confined executable runner.

use std::path::{Path, PathBuf};

use alan_agent_protocol::ToolCapability;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::tools::{Sandbox, SandboxSpec, Tool, ToolContext, ToolResult};

const TOOL_MANIFEST_VERSION: u16 = 1;
const MAX_TOOL_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_TOOL_TIMEOUT_SECS: usize = 300;
const MINIMAL_SAFE_PATH: &str = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin";

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
                arguments: "json_first_arg".to_string(),
                result: "stdout_json".to_string(),
            },
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
        if self.execution.arguments != "json_first_arg" || self.execution.result != "stdout_json" {
            bail!("Tool manifest '{mounted_name}' has unsupported execution hints");
        }
        Ok(())
    }
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
        "Tool name `{name}` collides with a reserved Alan OS executable"
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
        ensure!(
            executable.is_file(),
            "packaged Tool `{}` executable does not exist on disk",
            manifest.name
        );
        Ok(Self {
            manifest,
            executable,
            allow_network,
        })
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
        let name = self.manifest.name.clone();
        let arg_json = match serde_json::to_string(&arguments) {
            Ok(encoded) => encoded,
            Err(error) => {
                return Box::pin(async move {
                    Err(anyhow::anyhow!("failed to encode Tool arguments: {error}"))
                });
            }
        };
        let (sandbox, cwd) = resolve_execution_sandbox_and_cwd(ctx);

        Box::pin(async move {
            let (sandbox, cwd) = (sandbox?, cwd?);
            let mut command = sandbox
                .build_confined_program_command(&executable, &[arg_json], &cwd, allow_network)
                .with_context(|| format!("failed to confine packaged Tool `{name}`"))?;
            configure_sanitized_subprocess(&mut command, &ctx_env_home(&cwd), &name);

            let mut child = command
                .spawn()
                .with_context(|| format!("failed to spawn packaged Tool `{name}`"))?;
            let stdout_pipe = child.stdout.take();
            let stderr_pipe = child.stderr.take();
            let stdout_task = tokio::spawn(read_limited_pipe(stdout_pipe, MAX_TOOL_OUTPUT_BYTES));
            let stderr_task = tokio::spawn(read_limited_pipe(stderr_pipe, MAX_TOOL_OUTPUT_BYTES));

            let status = child
                .wait()
                .await
                .with_context(|| format!("failed waiting on packaged Tool `{name}`"))?;
            let stdout = stdout_task.await.unwrap_or_default();
            let stderr = stderr_task.await.unwrap_or_default();

            if !status.success() {
                let err_detail = String::from_utf8_lossy(&stderr);
                let out_detail = String::from_utf8_lossy(&stdout);
                let detail = if !err_detail.trim().is_empty() {
                    err_detail.trim().to_string()
                } else {
                    out_detail.trim().to_string()
                };
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
            let trimmed = trimmed.trim();
            if trimmed.is_empty() {
                return Ok(serde_json::json!({ "success": true }));
            }
            match serde_json::from_str::<serde_json::Value>(trimmed) {
                Ok(value) => Ok(value),
                Err(_) => Ok(serde_json::json!({
                    "success": true,
                    "output": trimmed,
                })),
            }
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ToolContext, ToolResult};

    struct ExampleTool;

    impl Tool for ExampleTool {
        fn name(&self) -> &str {
            "example"
        }
        fn description(&self) -> &str {
            "Example Tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(&self, _: serde_json::Value, _: &ToolContext) -> ToolResult {
            Box::pin(async { Ok(serde_json::Value::Null) })
        }
    }

    struct ArgumentDependentTool;

    impl Tool for ArgumentDependentTool {
        fn name(&self) -> &str {
            "dynamic"
        }
        fn description(&self) -> &str {
            "Dynamic Tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(&self, _: serde_json::Value, _: &ToolContext) -> ToolResult {
            Box::pin(async { Ok(serde_json::Value::Null) })
        }
        fn capability_is_argument_dependent(&self) -> bool {
            true
        }
    }

    #[test]
    fn manifest_round_trips_and_validates_against_mount_name() {
        let manifest = ToolPackageManifest::from_tool(&ExampleTool, 30).unwrap();
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let decoded: ToolPackageManifest = serde_json::from_slice(&bytes).unwrap();
        decoded.validate_for_name("example").unwrap();
        assert!(decoded.validate_for_name("other").is_err());
    }

    #[test]
    fn manifest_accepts_zero_as_the_no_timeout_sentinel() {
        ToolPackageManifest::from_tool(&ExampleTool, 0).unwrap();
    }

    #[test]
    fn argument_dependent_manifest_never_reuses_null_argument_capability() {
        let manifest = ToolPackageManifest::from_tool(&ArgumentDependentTool, 30).unwrap();
        assert!(manifest.capability_is_argument_dependent);
    }

    #[test]
    fn schema_validation_rejects_invalid_and_non_object_schemas() {
        assert!(validate_tool_parameters_schema(&serde_json::json!("not-an-object")).is_err());
        assert!(
            validate_tool_parameters_schema(&serde_json::json!({
                "type": "object",
                "properties": { "q": { "type": "string" } }
            }))
            .is_ok()
        );
    }
}
