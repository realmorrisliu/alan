//! Declarative workflow manifests and System Store persistence under `services/workflows`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use alan_os_host::SystemStorePaths;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub(crate) const SUPPORTED_MANIFEST_VERSION: u16 = 1;
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 300;
pub(crate) const MAX_TIMEOUT_SECS: u64 = 86_400;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_PROMPT_BYTES: usize = 32 * 1024;

/// Explicit invocation trust posture declared in a workflow manifest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTrustPosture {
    /// Default governed execution inside the OS boundary.
    #[default]
    Governed,
    /// Explicit `--trust`: keep OS sandbox & Host Mount bounds, bypass interactive prompts.
    #[clap(name = "confined_trusted", alias = "confined-trusted")]
    ConfinedTrusted,
    /// Explicit `--trust --unsafe-host-account`: disable OS sandbox & prompts.
    #[clap(name = "host_account_trusted", alias = "host-account-trusted")]
    HostAccountTrusted,
}

impl WorkflowTrustPosture {
    /// Canonical string identifier matching `alan_agent_engine::TrustPosture::as_str`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Governed => "governed",
            Self::ConfinedTrusted => "confined_trusted",
            Self::HostAccountTrusted => "host_account_trusted",
        }
    }

    /// CLI trust flags passed to `alan -p --ephemeral`.
    pub const fn cli_flags(self) -> &'static [&'static str] {
        match self {
            Self::Governed => &[],
            Self::ConfinedTrusted => &["--trust"],
            Self::HostAccountTrusted => &["--trust", "--unsafe-host-account"],
        }
    }
}

/// Declarative TOML workflow manifest stored in System Store (`services/workflows/manifests/<id>.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowManifest {
    /// Schema version (`1`).
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    /// Canonical workflow identifier (`[a-z0-9][a-z0-9_-]*`).
    pub id: String,
    /// Cron (5-field) or interval expression (`every 30m`, `1h`, `60s`, etc.).
    pub schedule: String,
    /// Non-empty task prompt submitted via `alan -p --ephemeral`.
    pub prompt: String,
    /// Optional Connection Service profile id (`connection_profile`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_profile: Option<String>,
    /// Explicit saved System Store Host Mount grant ids (`--grant <id>`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<String>,
    /// Explicit invocation trust posture (defaults to `"governed"`).
    #[serde(default)]
    pub trust: WorkflowTrustPosture,
    /// Pass `--json` to `alan -p --ephemeral` (defaults to `true`).
    #[serde(default = "default_true")]
    pub json: bool,
    /// Optional hard timeout in seconds for a single scheduled run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

const fn default_schema_version() -> u16 {
    SUPPORTED_MANIFEST_VERSION
}

const fn default_true() -> bool {
    true
}

impl WorkflowManifest {
    /// Effective timeout in seconds (`timeout_secs` or `DEFAULT_TIMEOUT_SECS`).
    pub fn effective_timeout_secs(&self) -> u64 {
        self.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS)
    }

    /// Validate all fields, IDs, schedule syntax, explicit mount grants, and bounds.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SUPPORTED_MANIFEST_VERSION,
            "unsupported workflow manifest schema_version {} (expected {SUPPORTED_MANIFEST_VERSION})",
            self.schema_version
        );
        validate_workflow_id(&self.id)?;
        super::schedule::parse_schedule(&self.schedule)?;

        let trimmed_prompt = self.prompt.trim();
        ensure!(
            !trimmed_prompt.is_empty(),
            "workflow prompt must not be empty"
        );
        ensure!(
            self.prompt.len() <= MAX_PROMPT_BYTES,
            "workflow prompt exceeds {MAX_PROMPT_BYTES} bytes"
        );

        if let Some(profile) = self.model_profile.as_deref() {
            let trimmed = profile.trim();
            ensure!(
                !trimmed.is_empty(),
                "workflow `model_profile` must not be blank"
            );
            ensure!(
                !trimmed.contains(['\n', '\r', '\0']),
                "workflow `model_profile` contains invalid control characters"
            );
        }

        let mut seen_mounts = std::collections::BTreeSet::new();
        for mount in &self.mounts {
            validate_mount_grant_id(mount)?;
            ensure!(
                seen_mounts.insert(mount.as_str()),
                "duplicate workflow mount grant `{mount}`"
            );
        }

        if let Some(timeout) = self.timeout_secs {
            ensure!(
                (1..=MAX_TIMEOUT_SECS).contains(&timeout),
                "workflow `timeout_secs` must be between 1 and {MAX_TIMEOUT_SECS}"
            );
        }

        Ok(())
    }
}

/// Validates a canonical workflow identifier (`1..=64` lowercase ASCII alphanumeric, `-`, `_`).
pub fn validate_workflow_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.len() <= 64,
        "workflow id `{id}` must be 1..=64 characters"
    );
    ensure!(
        !id.contains("..") && !id.contains('/') && !id.contains('\\'),
        "workflow id `{id}` must be a single path-safe identifier"
    );
    let bytes = id.as_bytes();
    ensure!(
        bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit(),
        "workflow id `{id}` must start with a lowercase ASCII letter or digit"
    );
    ensure!(
        bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit(),
        "workflow id `{id}` must end with a lowercase ASCII letter or digit"
    );
    ensure!(
        bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(*b, b'-' | b'_')),
        "workflow id `{id}` may contain only lowercase ASCII letters, digits, `-`, or `_`"
    );
    Ok(())
}

/// Validates an explicit System Store Host Mount grant id (rejects raw Host paths).
pub fn validate_mount_grant_id(grant_id: &str) -> Result<()> {
    ensure!(
        !grant_id.is_empty()
            && grant_id.len() <= 64
            && grant_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "workflow mount `{grant_id}` must be an explicit saved grant id (`[a-zA-Z0-9_-]`), never a raw host path; register paths first with `alan host grant save <id> <path>`"
    );
    Ok(())
}

/// System-Store-backed storage handle for workflow manifests and bounded run logs.
#[derive(Debug, Clone)]
pub struct WorkflowStore {
    root: PathBuf,
}

impl WorkflowStore {
    /// Open the System Store `services/workflows` root for the current Alan installation.
    pub fn from_system_store(system: &SystemStorePaths) -> Result<Self> {
        Ok(Self {
            root: system.service("workflows")?,
        })
    }

    /// Open a workflow store rooted at an explicit directory (primarily for tests).
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "used by workflow unit tests and library callers")
    )]
    pub fn from_root(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        ensure!(
            root.is_absolute(),
            "workflow store root must be absolute: {}",
            root.display()
        );
        Ok(Self { root })
    }

    /// Directory holding workflow TOML manifests (`manifests/`).
    pub fn manifests_dir(&self) -> PathBuf {
        self.root.join("manifests")
    }

    /// Directory holding per-workflow bounded NDJSON run logs (`runs/`).
    pub fn runs_dir(&self) -> PathBuf {
        self.root.join("runs")
    }

    /// Canonical manifest path for `id` (`manifests/<id>.toml`).
    pub fn manifest_path(&self, id: &str) -> Result<PathBuf> {
        validate_workflow_id(id)?;
        Ok(self.manifests_dir().join(format!("{id}.toml")))
    }

    /// Per-workflow run directory (`runs/<id>`).
    pub fn workflow_runs_dir(&self, id: &str) -> Result<PathBuf> {
        validate_workflow_id(id)?;
        Ok(self.runs_dir().join(id))
    }

    /// Bounded NDJSON log file (`runs/<id>/runs.ndjson`).
    pub fn run_log_path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.workflow_runs_dir(id)?.join("runs.ndjson"))
    }

    /// Persist a validated workflow manifest atomically (without granting implicit mounts/trust).
    pub fn save_manifest(&self, manifest: &WorkflowManifest, overwrite: bool) -> Result<PathBuf> {
        manifest.validate()?;
        ensure_real_dir(&self.root)?;
        let manifests_dir = self.manifests_dir();
        ensure_real_dir(&manifests_dir)?;

        let path = self.manifest_path(&manifest.id)?;
        reject_if_symlink(&path)?;
        if path.exists() && !overwrite {
            bail!(
                "workflow `{}` already exists at {} (pass `--force` to replace)",
                manifest.id,
                path.display()
            );
        }

        let encoded = toml::to_string_pretty(manifest).context("serialize workflow manifest")?;
        ensure!(
            encoded.len() <= MAX_MANIFEST_BYTES,
            "workflow manifest exceeds {MAX_MANIFEST_BYTES} bytes"
        );
        atomic_write_file(&path, encoded.as_bytes())?;
        Ok(path)
    }

    /// Load and validate one workflow manifest by id.
    pub fn load_manifest(&self, id: &str) -> Result<WorkflowManifest> {
        let path = self.manifest_path(id)?;
        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("workflow `{id}` not found at {}", path.display()))?;
        ensure!(
            !meta.file_type().is_symlink() && meta.is_file(),
            "workflow manifest {} must be a regular non-symlink file",
            path.display()
        );
        ensure!(
            meta.len() <= MAX_MANIFEST_BYTES as u64,
            "workflow manifest {} exceeds {MAX_MANIFEST_BYTES} bytes",
            path.display()
        );

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read workflow manifest {}", path.display()))?;
        let manifest: WorkflowManifest = toml::from_str(&raw)
            .with_context(|| format!("parse workflow manifest {}", path.display()))?;
        ensure!(
            manifest.id == id,
            "workflow manifest id `{}` does not match filename `{id}`",
            manifest.id
        );
        manifest.validate()?;
        Ok(manifest)
    }

    /// List all registered workflow manifests ordered deterministically by `id`.
    pub fn list_manifests(&self) -> Result<Vec<WorkflowManifest>> {
        let dir = self.manifests_dir();
        match fs::symlink_metadata(&dir) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink() && meta.is_dir(),
                    "workflow manifests directory {} must be a real directory",
                    dir.display()
                );
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(err).with_context(|| format!("inspect {}", dir.display()));
            }
        }

        let mut manifests = Vec::new();
        for entry in fs::read_dir(&dir)
            .with_context(|| format!("read workflow manifests directory {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            ensure!(
                !meta.file_type().is_symlink(),
                "workflow manifest entry {} must not be a symlink",
                path.display()
            );
            if !meta.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if ext != "toml" {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .with_context(|| format!("non-UTF-8 workflow filename {}", path.display()))?;
            manifests.push(self.load_manifest(id)?);
        }
        manifests.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(manifests)
    }
}

pub(crate) fn ensure_real_dir(dir: &Path) -> Result<()> {
    for ancestor in dir.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink() && meta.is_dir(),
                    "workflow path component {} must be a real non-symlink directory",
                    ancestor.display()
                );
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err).with_context(|| format!("inspect {}", ancestor.display()));
            }
        }
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .with_context(|| format!("create workflow directory {}", dir.display()))?;
    let meta = fs::symlink_metadata(dir)?;
    ensure!(
        !meta.file_type().is_symlink() && meta.is_dir(),
        "workflow directory {} must not be a symlink",
        dir.display()
    );
    Ok(())
}

pub(crate) fn reject_if_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                !meta.file_type().is_symlink(),
                "refusing to write through symlink {}",
                path.display()
            );
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("inspect {}", path.display())),
    }
}

pub(crate) fn atomic_write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    reject_if_symlink(path)?;
    let parent = path
        .parent()
        .context("target file must have a parent directory")?;
    ensure_real_dir(parent)?;
    let tmp_name = format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workflow"),
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    );
    let tmp_path = parent.join(tmp_name);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&tmp_path)
        .with_context(|| format!("create temporary file {}", tmp_path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write temporary file {}", tmp_path.display()))?;
    file.sync_all()
        .with_context(|| format!("flush temporary file {}", tmp_path.display()))?;
    drop(file);
    fs::rename(&tmp_path, path)
        .with_context(|| format!("atomically replace {}", path.display()))?;
    Ok(())
}
