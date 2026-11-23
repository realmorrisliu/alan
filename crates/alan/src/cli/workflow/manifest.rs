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
pub(crate) const DEFAULT_TRIGGER_MAX_RETRIES: u32 = 3;
pub(crate) const MAX_TRIGGER_RETRIES: u32 = 20;
pub(crate) const DEFAULT_TRIGGER_MAX_CONCURRENT: u32 = 1;
pub(crate) const MAX_TRIGGER_CONCURRENT: u32 = 16;
const MAX_ALLOWLIST_ENTRIES: usize = 64;
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

/// Control-plane allowlist governing autonomous triggers for self-iteration workflows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerAllowlist {
    /// Canonical GitHub repository slug (`owner/repo`).
    pub repo: String,
    /// GitHub actor logins explicitly allowed to trigger autonomous runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_actors: Vec<String>,
    /// Labels required on the issue or PR before dispatch (for example `["alan:auto"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_labels: Vec<String>,
    /// Self or bot actor logins always suppressed to prevent feedback loops.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_actors: Vec<String>,
    /// Maximum bounded retry attempts per item before escalating to human review.
    #[serde(default = "default_trigger_max_retries")]
    pub max_retries: u32,
    /// Maximum concurrent autonomous worker processes allowed for this trigger.
    #[serde(default = "default_trigger_max_concurrent")]
    pub max_concurrent: u32,
}

const fn default_trigger_max_retries() -> u32 {
    DEFAULT_TRIGGER_MAX_RETRIES
}

const fn default_trigger_max_concurrent() -> u32 {
    DEFAULT_TRIGGER_MAX_CONCURRENT
}

impl TriggerAllowlist {
    /// Validate repository slug, explicit actor/label lists, and bounded concurrency/retry caps.
    pub fn validate(&self) -> Result<()> {
        validate_github_repo(&self.repo)?;

        ensure!(
            !self.allowed_actors.is_empty(),
            "trigger_allowlist.allowed_actors must contain at least one explicit actor login"
        );
        validate_actor_list(&self.allowed_actors, "trigger_allowlist.allowed_actors")?;

        ensure!(
            !self.required_labels.is_empty(),
            "trigger_allowlist.required_labels must contain at least one explicit label (for example `alan:auto`)"
        );
        ensure!(
            self.required_labels.len() <= MAX_ALLOWLIST_ENTRIES,
            "trigger_allowlist.required_labels exceeds {MAX_ALLOWLIST_ENTRIES} entries"
        );
        let mut seen_labels = std::collections::BTreeSet::new();
        for label in &self.required_labels {
            validate_label_name(label)?;
            let key = label.to_ascii_lowercase();
            ensure!(
                seen_labels.insert(key),
                "duplicate label `{label}` in trigger_allowlist.required_labels"
            );
        }

        validate_actor_list(&self.ignored_actors, "trigger_allowlist.ignored_actors")?;

        ensure!(
            self.max_retries <= MAX_TRIGGER_RETRIES,
            "trigger_allowlist.max_retries must be <= {MAX_TRIGGER_RETRIES}"
        );
        ensure!(
            (1..=MAX_TRIGGER_CONCURRENT).contains(&self.max_concurrent),
            "trigger_allowlist.max_concurrent must be between 1 and {MAX_TRIGGER_CONCURRENT}"
        );
        Ok(())
    }
}

/// Fail-closed human gate preventing autonomous merge, tag push, or release publication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseGatePolicy {
    /// Require explicit human approval and merge on pull requests (`true` required).
    pub require_human_merge: bool,
    /// Forbid autonomous `v*` git tag creation or push (`true` required).
    pub forbid_autonomous_tag_push: bool,
    /// Forbid autonomous GitHub release publication or package uploads (`true` required).
    pub forbid_autonomous_publish: bool,
}

impl ReleaseGatePolicy {
    /// Construct the enforced fail-closed release gate with all three safety flags enabled.
    pub const fn enforced() -> Self {
        Self {
            require_human_merge: true,
            forbid_autonomous_tag_push: true,
            forbid_autonomous_publish: true,
        }
    }

    /// Validate that every release safety flag remains strictly enabled (`true`).
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.require_human_merge,
            "release_gate.require_human_merge must be true (autonomous PR merge is forbidden)"
        );
        ensure!(
            self.forbid_autonomous_tag_push,
            "release_gate.forbid_autonomous_tag_push must be true (autonomous git tag push is forbidden)"
        );
        ensure!(
            self.forbid_autonomous_publish,
            "release_gate.forbid_autonomous_publish must be true (autonomous release publication is forbidden)"
        );
        Ok(())
    }
}

impl Default for ReleaseGatePolicy {
    fn default() -> Self {
        Self::enforced()
    }
}

/// Declarative `fileserver` activation included on an isolated workflow run (`--fileserver`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowFileserverSpec {
    /// Installed Quartermaster `fileserver` package identifier.
    pub package_id: String,
    /// Mount access (`read_only` or `read_write`; defaults to `read_only`).
    #[serde(default)]
    pub access: alan_service_manager::FileserverMountAccess,
    /// Authorize outbound network inside OS confinement (`false` by default).
    #[serde(default)]
    pub allow_network: bool,
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
    /// Explicit Quartermaster package ids projected only for this workflow's isolated run (`--package <id>`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
    /// Explicit Quartermaster fileserver activations for this workflow's isolated run (`--fileserver ...`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fileservers: Vec<WorkflowFileserverSpec>,
    /// Explicit invocation trust posture (defaults to `"governed"`).
    #[serde(default)]
    pub trust: WorkflowTrustPosture,
    /// Pass `--json` to `alan -p --ephemeral` (defaults to `true`).
    #[serde(default = "default_true")]
    pub json: bool,
    /// Optional hard timeout in seconds for a single scheduled run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// Optional control-plane trigger allowlist for bounded self-iteration workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_allowlist: Option<TriggerAllowlist>,
    /// Optional fail-closed human release gate policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_gate: Option<ReleaseGatePolicy>,
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

    /// Validate all fields, IDs, schedule syntax, explicit mount grants, allowlists, and gates.
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

        let mut seen_packages = std::collections::BTreeSet::new();
        for pkg in &self.packages {
            alan_service_manager::validate_package_id(pkg)
                .with_context(|| format!("invalid workflow package `{pkg}`"))?;
            ensure!(
                seen_packages.insert(pkg.as_str()),
                "duplicate workflow package `{pkg}`"
            );
        }
        for fs in &self.fileservers {
            alan_service_manager::validate_package_id(&fs.package_id)
                .with_context(|| format!("invalid workflow fileserver `{}`", fs.package_id))?;
            ensure!(
                seen_packages.insert(fs.package_id.as_str()),
                "duplicate workflow package/fileserver `{}`",
                fs.package_id
            );
        }

        if let Some(timeout) = self.timeout_secs {
            ensure!(
                (1..=MAX_TIMEOUT_SECS).contains(&timeout),
                "workflow `timeout_secs` must be between 1 and {MAX_TIMEOUT_SECS}"
            );
        }

        if let Some(allowlist) = &self.trigger_allowlist {
            allowlist.validate()?;
        }
        if let Some(gate) = &self.release_gate {
            gate.validate()?;
        }
        if (self.trigger_allowlist.is_some() || self.release_gate.is_some())
            && self.trust == WorkflowTrustPosture::HostAccountTrusted
        {
            bail!(
                "workflows with `trigger_allowlist` or `release_gate` must remain OS-confined (`governed` or `confined_trusted`); `host_account_trusted` is forbidden"
            );
        }

        Ok(())
    }
}

/// Validates a canonical GitHub `owner/repo` slug.
pub(crate) fn validate_github_repo(repo: &str) -> Result<()> {
    let trimmed = repo.trim();
    ensure!(
        trimmed == repo && !trimmed.is_empty(),
        "trigger_allowlist.repo must be non-empty without surrounding whitespace"
    );
    ensure!(
        !trimmed.contains("://")
            && !trimmed.contains('\\')
            && !trimmed.chars().any(char::is_whitespace),
        "trigger_allowlist.repo `{repo}` must be a canonical `owner/repo` slug"
    );
    let (owner, name) = trimmed
        .split_once('/')
        .context("trigger_allowlist.repo must be formatted as `owner/repo`")?;
    ensure!(
        !name.contains('/'),
        "trigger_allowlist.repo `{repo}` must contain exactly one `/`"
    );
    validate_repo_segment(owner, "owner")?;
    validate_repo_segment(name, "repo")?;
    Ok(())
}

fn validate_repo_segment(segment: &str, label: &str) -> Result<()> {
    ensure!(
        !segment.is_empty() && segment.len() <= 100,
        "trigger_allowlist.repo {label} `{segment}` must be 1..=100 characters"
    );
    ensure!(
        segment != "." && segment != "..",
        "trigger_allowlist.repo {label} `{segment}` must not be `.` or `..`"
    );
    let bytes = segment.as_bytes();
    ensure!(
        bytes[0].is_ascii_alphanumeric() && bytes[bytes.len() - 1].is_ascii_alphanumeric(),
        "trigger_allowlist.repo {label} `{segment}` must start and end with an ASCII letter or digit"
    );
    ensure!(
        bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.')),
        "trigger_allowlist.repo {label} `{segment}` contains unsupported characters"
    );
    Ok(())
}

fn validate_actor_list(actors: &[String], field: &str) -> Result<()> {
    ensure!(
        actors.len() <= MAX_ALLOWLIST_ENTRIES,
        "{field} exceeds {MAX_ALLOWLIST_ENTRIES} entries"
    );
    let mut seen = std::collections::BTreeSet::new();
    for actor in actors {
        validate_actor_login(actor, field)?;
        ensure!(
            seen.insert(actor.to_ascii_lowercase()),
            "duplicate actor `{actor}` in {field}"
        );
    }
    Ok(())
}

/// Validates a GitHub actor login (`[a-zA-Z0-9_-]` plus `[bot]` suffix brackets).
pub(crate) fn validate_actor_login(actor: &str, field: &str) -> Result<()> {
    let trimmed = actor.trim();
    ensure!(
        trimmed == actor && !trimmed.is_empty() && trimmed.len() <= 64,
        "{field} entry `{actor}` must be 1..=64 characters without surrounding whitespace"
    );
    ensure!(
        trimmed.as_bytes()[0].is_ascii_alphanumeric(),
        "{field} entry `{actor}` must start with an ASCII letter or digit"
    );
    ensure!(
        trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'[' | b']')),
        "{field} entry `{actor}` contains unsupported characters"
    );
    Ok(())
}

/// Validates a GitHub label name (`1..=64` non-control chars without surrounding whitespace).
pub(crate) fn validate_label_name(label: &str) -> Result<()> {
    let trimmed = label.trim();
    ensure!(
        trimmed == label && !trimmed.is_empty() && trimmed.len() <= 64,
        "trigger_allowlist.required_labels entry `{label}` must be 1..=64 characters without surrounding whitespace"
    );
    ensure!(
        !trimmed.chars().any(char::is_control),
        "trigger_allowlist.required_labels entry `{label}` contains control characters"
    );
    Ok(())
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

/// Trim entries, drop empties, and deduplicate case-insensitively while preserving order.
pub(crate) fn normalize_unique_entries(entries: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for entry in entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_ascii_lowercase()) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

/// Normalize optional model profile string to `None` when empty or whitespace-only.
pub(crate) fn normalize_model_profile(model_profile: Option<String>) -> Option<String> {
    model_profile
        .map(|profile| profile.trim().to_string())
        .filter(|profile| !profile.is_empty())
}

/// Parse repeatable `--package` and `--fileserver` CLI inputs with duplicate checks.
pub(crate) fn parse_workflow_package_inputs(
    raw_packages: &[String],
    raw_fileservers: &[String],
) -> Result<(Vec<String>, Vec<WorkflowFileserverSpec>)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut packages = Vec::new();
    for raw in raw_packages {
        let id = raw.trim();
        alan_service_manager::validate_package_id(id)?;
        ensure!(
            seen.insert(id.to_string()),
            "duplicate `--package` / `--fileserver` selection `{id}`"
        );
        packages.push(id.to_string());
    }
    let mut fileservers = Vec::new();
    for spec in raw_fileservers {
        let parsed = crate::cli::trust::parse_fileserver_cli_spec(spec)?;
        let activation = parsed.fileserver.unwrap_or_default();
        ensure!(
            seen.insert(parsed.package_id.clone()),
            "duplicate `--package` / `--fileserver` selection `{}`",
            parsed.package_id
        );
        fileservers.push(WorkflowFileserverSpec {
            package_id: parsed.package_id,
            access: activation.access,
            allow_network: activation.allow_network,
        });
    }
    Ok((packages, fileservers))
}

/// Ensure a template workflow uses explicit grants and OS-confined trust posture.
pub(crate) fn ensure_confined_template_inputs(
    mounts: &[String],
    trust: WorkflowTrustPosture,
    kind: &str,
) -> Result<()> {
    ensure!(
        !mounts.is_empty(),
        "{kind} workflow requires at least one explicit `--grant <GRANT_ID>` binding; register paths first with `alan host grant save <id> <path>`"
    );
    ensure!(
        trust != WorkflowTrustPosture::HostAccountTrusted,
        "{kind} workflow must remain OS-confined (`governed` or `confined_trusted`); `host_account_trusted` is forbidden"
    );
    Ok(())
}
