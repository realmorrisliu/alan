//! Session Store - persistence for `Session -> Workspace` bindings.
//!
//! Stores the mapping from session IDs to workspace paths so bindings can be
//! recovered after daemon restarts.
//!
//! Storage location: `~/.alan/sessions/<session_id>.json`

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const SESSION_BINDING_EXTENSION: &str = "json";
#[cfg(test)]
const SESSION_BINDING_TMP_EXTENSION: &str = "json.tmp";
const CORRUPTED_BINDINGS_DIR_NAME: &str = "corrupted";

#[derive(Debug)]
enum SessionBindingLoadError {
    Read(anyhow::Error),
    Corrupted(anyhow::Error),
}

/// Session binding metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBinding {
    /// Session ID
    pub session_id: String,
    /// Workspace path
    pub workspace_path: PathBuf,
    /// Creation time
    pub created_at: String,
    /// Governance configuration
    #[serde(default)]
    pub governance: alan_agent_protocol::GovernanceConfig,
    /// Selected named agent, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Bound connection profile id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Bound provider id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<alan_agent_engine::LlmProvider>,
    /// Bound resolved model.
    #[serde(default)]
    pub resolved_model: String,
    /// Effective reasoning effort bound to this session, if resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    /// Per-session streaming mode override (None = runtime default/config).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming_mode: Option<alan_agent_engine::StreamingMode>,
    /// Per-session partial stream recovery override (None = runtime default/config).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_stream_recovery_mode: Option<alan_agent_engine::PartialStreamRecoveryMode>,
    /// Rollout file path (if present)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollout_path: Option<PathBuf>,
    /// Whether this session required durable startup.
    ///
    /// Legacy bindings may omit this field; callers should fall back to the
    /// current config when it is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durability_required: Option<bool>,
    /// Actual durability state observed at runtime startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable: Option<bool>,
}

impl SessionBinding {
    /// Resolve effective durability requirement, falling back for legacy bindings.
    pub fn effective_durability_required(&self, default_required: bool) -> bool {
        self.durability_required.unwrap_or(default_required)
    }
}

/// Session store
#[derive(Debug)]
pub struct SessionStore {
    storage_dir: PathBuf,
    /// In-memory cache
    cache: std::sync::RwLock<HashMap<String, SessionBinding>>,
    /// Canonical binding paths indexed by session id.
    binding_paths: std::sync::RwLock<HashMap<String, PathBuf>>,
}

impl SessionStore {
    /// Create a new `SessionStore`
    pub fn new() -> Result<Self> {
        let storage_dir = Self::prepare_storage_dir(Self::default_storage_dir()?)?;

        let cache = std::sync::RwLock::new(HashMap::new());
        let binding_paths = std::sync::RwLock::new(HashMap::new());
        let store = Self {
            storage_dir,
            cache,
            binding_paths,
        };

        // Load all persisted sessions.
        store.load_all()?;

        Ok(store)
    }

    /// Create with an explicit storage directory.
    #[allow(dead_code)]
    pub fn with_dir(storage_dir: PathBuf) -> Result<Self> {
        let storage_dir = Self::prepare_storage_dir(storage_dir)?;
        let cache = std::sync::RwLock::new(HashMap::new());
        let binding_paths = std::sync::RwLock::new(HashMap::new());
        let store = Self {
            storage_dir,
            cache,
            binding_paths,
        };
        store.load_all()?;
        Ok(store)
    }

    /// Default storage directory
    fn default_storage_dir() -> Result<PathBuf> {
        alan_agent_engine::AlanHomePaths::detect()
            .map(|paths| {
                alan_agent_engine::workspace_sessions_dir_from_alan_dir(&paths.alan_home_dir)
            })
            .context("Cannot determine home directory")
    }

    fn prepare_storage_dir(storage_dir: PathBuf) -> Result<PathBuf> {
        std::fs::create_dir_all(&storage_dir).with_context(|| {
            format!(
                "Failed to create session store dir {}",
                storage_dir.display()
            )
        })?;
        let canonical = std::fs::canonicalize(&storage_dir).with_context(|| {
            format!(
                "Failed to canonicalize session store dir {}",
                storage_dir.display()
            )
        })?;
        if !canonical.is_dir() {
            bail!(
                "Session store path is not a directory: {}",
                canonical.display()
            );
        }
        std::fs::create_dir_all(canonical.join(CORRUPTED_BINDINGS_DIR_NAME)).with_context(
            || {
                format!(
                    "Failed to create corrupted session binding quarantine dir {}",
                    canonical.join(CORRUPTED_BINDINGS_DIR_NAME).display()
                )
            },
        )?;
        Ok(canonical)
    }

    /// Get the managed session binding path for a validated session id.
    fn session_file_path(&self, session_id: &str) -> Result<PathBuf> {
        let session_id = SafeSessionId::parse(session_id)?;
        Ok(self.storage_dir.join(session_id.binding_file_name()))
    }

    fn corrupted_bindings_dir(&self) -> PathBuf {
        self.storage_dir.join(CORRUPTED_BINDINGS_DIR_NAME)
    }

    /// Save a session binding
    pub fn save(&self, binding: SessionBinding) -> Result<()> {
        let session_id = binding.session_id.clone();

        // Persist via temp file + fsync + rename so partial writes do not
        // silently corrupt session recovery state.
        let content = serde_json::to_string_pretty(&binding)?;
        let path = self.write_binding_atomically(&session_id, &content)?;

        // Update cache.
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(session_id.clone(), binding);
        }
        if let Ok(mut binding_paths) = self.binding_paths.write() {
            binding_paths.insert(session_id.clone(), path.clone());
        }

        debug!(%session_id, path = %path.display(), "Saved session binding");
        Ok(())
    }

    /// Load a specific session
    #[allow(dead_code)]
    pub fn load(&self, session_id: &str) -> Option<SessionBinding> {
        if let Err(err) = SafeSessionId::parse(session_id) {
            warn!(
                %session_id,
                error = %err,
                "Rejected invalid session binding id"
            );
            return None;
        }

        // Check cache first.
        if let Ok(cache) = self.cache.read()
            && let Some(binding) = cache.get(session_id)
        {
            return Some(binding.clone());
        }

        if let Err(err) = self.load_all() {
            warn!(
                %session_id,
                error = %err,
                "Failed to refresh session binding cache"
            );
            return None;
        }

        self.cache
            .read()
            .ok()
            .and_then(|cache| cache.get(session_id).cloned())
    }

    /// Remove a session binding
    pub fn remove(&self, session_id: &str) -> Result<()> {
        SafeSessionId::parse(session_id)?;
        if let Some(path) = self.indexed_session_file_path(session_id)? {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err).context("Failed to remove session binding"),
            }
        }

        // Update cache.
        if let Ok(mut cache) = self.cache.write() {
            cache.remove(session_id);
        }
        if let Ok(mut binding_paths) = self.binding_paths.write() {
            binding_paths.remove(session_id);
        }

        debug!(%session_id, "Removed session binding");
        Ok(())
    }

    /// List all sessions
    pub fn list_all(&self) -> Vec<SessionBinding> {
        // Refresh cache.
        let _ = self.load_all();

        if let Ok(cache) = self.cache.read() {
            cache.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// List active sessions (binding exists and workspace path is valid)
    pub fn list_active(&self) -> Vec<SessionBinding> {
        self.list_all()
            .into_iter()
            .filter(|b| b.workspace_path.exists())
            .collect()
    }

    /// Check whether a session exists
    #[allow(dead_code)]
    pub fn exists(&self, session_id: &str) -> bool {
        if SafeSessionId::parse(session_id).is_err() {
            return false;
        }

        // Check cache first.
        if let Ok(cache) = self.cache.read()
            && cache.contains_key(session_id)
        {
            return true;
        }

        self.load_all()
            .map(|()| {
                self.cache
                    .read()
                    .is_ok_and(|cache| cache.contains_key(session_id))
            })
            .unwrap_or(false)
    }

    /// Update rollout path
    pub fn update_rollout_path(
        &self,
        session_id: &str,
        rollout_path: Option<PathBuf>,
    ) -> Result<()> {
        if let Some(mut binding) = self.load(session_id) {
            binding.rollout_path = rollout_path;
            self.save(binding)?;
        }
        Ok(())
    }

    /// Update rollout path and durability fields atomically in the binding payload.
    pub fn update_runtime_state(
        &self,
        session_id: &str,
        rollout_path: Option<PathBuf>,
        durability: alan_agent_engine::runtime::SessionDurabilityState,
    ) -> Result<()> {
        if let Some(mut binding) = self.load(session_id) {
            binding.rollout_path = rollout_path;
            binding.durability_required = Some(durability.required);
            binding.durable = Some(durability.durable);
            self.save(binding)?;
        }
        Ok(())
    }

    /// Get workspace path
    #[allow(dead_code)]
    pub fn get_workspace_path(&self, session_id: &str) -> Option<PathBuf> {
        self.load(session_id).map(|b| b.workspace_path)
    }

    /// Load all sessions into cache
    fn load_all(&self) -> Result<()> {
        let storage_dir = std::fs::canonicalize(&self.storage_dir).with_context(|| {
            format!(
                "Failed to canonicalize session store dir {}",
                self.storage_dir.display()
            )
        })?;
        let entries = std::fs::read_dir(&storage_dir)?;
        let mut bindings = HashMap::new();
        let mut binding_paths = HashMap::new();
        let mut corrupted_entries = 0usize;
        let mut unreadable_entries = 0usize;

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    unreadable_entries += 1;
                    warn!(error = %err, "Failed to enumerate session binding entry");
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some(SESSION_BINDING_EXTENSION) {
                continue;
            }

            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            match self.load_binding_from_path(&path, None) {
                Ok((binding, canonical_path)) => {
                    binding_paths.insert(binding.session_id.clone(), canonical_path);
                    bindings.insert(binding.session_id.clone(), binding);
                }
                Err(SessionBindingLoadError::Read(err)) => {
                    unreadable_entries += 1;
                    warn!(
                        session_id = %session_id,
                        path = %path.display(),
                        error = %err,
                        "Skipping unreadable session binding entry"
                    );
                }
                Err(SessionBindingLoadError::Corrupted(err)) => {
                    corrupted_entries += 1;
                    self.warn_and_quarantine_corrupted_binding(&session_id, &path, &err);
                }
            }
        }

        if let Ok(mut cache) = self.cache.write() {
            *cache = bindings;
        }
        if let Ok(mut paths) = self.binding_paths.write() {
            *paths = binding_paths;
        }

        if corrupted_entries > 0 {
            warn!(
                corrupted_entries,
                quarantine_dir = %self.corrupted_bindings_dir().display(),
                "Recovered session bindings after quarantining corrupted entries"
            );
        }

        if unreadable_entries > 0 {
            warn!(
                unreadable_entries,
                "Skipped unreadable session binding entries during recovery"
            );
        }

        info!(
            count = self.cache.read().map(|c| c.len()).unwrap_or(0),
            "Loaded session bindings"
        );
        Ok(())
    }

    /// Remove stale sessions (workspace path does not exist)
    #[allow(dead_code)]
    pub fn cleanup_stale(&self) -> usize {
        let all = self.list_all();
        let mut removed = 0;

        for binding in all {
            if !binding.workspace_path.exists() {
                if let Err(err) = self.remove(&binding.session_id) {
                    warn!(session_id = %binding.session_id, error = %err, "Failed to remove stale session");
                } else {
                    info!(session_id = %binding.session_id, "Removed stale session binding");
                    removed += 1;
                }
            }
        }

        removed
    }

    fn write_binding_atomically(&self, session_id: &str, content: &str) -> Result<PathBuf> {
        let storage_dir = std::fs::canonicalize(&self.storage_dir).with_context(|| {
            format!(
                "Failed to canonicalize session store dir {}",
                self.storage_dir.display()
            )
        })?;
        let binding_name = SafeSessionId::parse(session_id)?.binding_file_name();
        if binding_name.is_empty()
            || binding_name == "."
            || binding_name == ".."
            || binding_name.contains("..")
            || binding_name.contains('/')
            || binding_name.contains('\\')
        {
            bail!("Invalid session binding file name: {binding_name}");
        }
        let path = storage_dir.join(&binding_name);

        let tmp_name = format!("{binding_name}.tmp");
        if tmp_name.is_empty()
            || tmp_name == "."
            || tmp_name == ".."
            || tmp_name.contains("..")
            || tmp_name.contains('/')
            || tmp_name.contains('\\')
        {
            bail!("Invalid temp session binding file name: {tmp_name}");
        }
        let tmp_path = storage_dir.join(&tmp_name);

        let mut tmp_file = std::fs::File::create(&tmp_path).with_context(|| {
            format!(
                "Failed to create temp session binding file: {}",
                tmp_path.display()
            )
        })?;
        tmp_file.write_all(content.as_bytes()).with_context(|| {
            format!(
                "Failed to write temp session binding file: {}",
                tmp_path.display()
            )
        })?;
        tmp_file.sync_all().with_context(|| {
            format!(
                "Failed to fsync temp session binding file: {}",
                tmp_path.display()
            )
        })?;

        std::fs::rename(&tmp_path, &path).with_context(|| {
            format!(
                "Failed to atomically replace session binding file {} -> {}",
                tmp_path.display(),
                path.display()
            )
        })?;
        sync_directory(&storage_dir)?;

        Ok(path)
    }

    fn indexed_session_file_path(&self, session_id: &str) -> Result<Option<PathBuf>> {
        SafeSessionId::parse(session_id)?;
        if let Ok(binding_paths) = self.binding_paths.read()
            && let Some(path) = binding_paths.get(session_id)
        {
            return self.validate_indexed_session_file_path(path).map(Some);
        }

        self.load_all()?;
        if let Ok(binding_paths) = self.binding_paths.read()
            && let Some(path) = binding_paths.get(session_id)
        {
            return self.validate_indexed_session_file_path(path).map(Some);
        }

        Ok(None)
    }

    fn validate_indexed_session_file_path(&self, path: &Path) -> Result<PathBuf> {
        let parent = path.parent().with_context(|| {
            format!(
                "Indexed session binding path has no parent directory: {}",
                path.display()
            )
        })?;
        if parent != self.storage_dir.as_path() {
            bail!(
                "Indexed session binding path {} escapes storage dir {}",
                path.display(),
                self.storage_dir.display()
            );
        }
        Ok(path.to_path_buf())
    }

    fn load_binding_from_path(
        &self,
        path: &Path,
        expected_session_id: Option<&str>,
    ) -> std::result::Result<(SessionBinding, PathBuf), SessionBindingLoadError> {
        let canonical_path = std::fs::canonicalize(path).map_err(|err| {
            SessionBindingLoadError::Corrupted(anyhow::anyhow!(
                "Failed to canonicalize managed session store path {}: {}",
                path.display(),
                err
            ))
        })?;
        if !canonical_path.starts_with(&self.storage_dir) {
            return Err(SessionBindingLoadError::Corrupted(anyhow::anyhow!(
                "Session binding {} escapes storage dir {}",
                canonical_path.display(),
                self.storage_dir.display()
            )));
        }
        let canonical_parent = canonical_path.parent().ok_or_else(|| {
            SessionBindingLoadError::Corrupted(anyhow::anyhow!(
                "Managed session store path has no parent directory: {}",
                canonical_path.display()
            ))
        })?;
        if canonical_parent != self.storage_dir.as_path() {
            return Err(SessionBindingLoadError::Corrupted(anyhow::anyhow!(
                "Session binding {} is not a direct child of storage dir {}",
                canonical_path.display(),
                self.storage_dir.display()
            )));
        }

        let content = std::fs::read_to_string(&canonical_path).map_err(|err| {
            SessionBindingLoadError::Read(anyhow::anyhow!(
                "Failed to read session binding {}: {}",
                canonical_path.display(),
                err
            ))
        })?;
        let binding = serde_json::from_str::<SessionBinding>(&content).map_err(|err| {
            SessionBindingLoadError::Corrupted(anyhow::anyhow!(
                "Failed to parse session binding {}: {}",
                canonical_path.display(),
                err
            ))
        })?;
        self.validate_binding_path(&canonical_path, &binding, expected_session_id)
            .map_err(SessionBindingLoadError::Corrupted)?;
        Ok((binding, canonical_path))
    }

    fn validate_binding_path(
        &self,
        path: &Path,
        binding: &SessionBinding,
        expected_session_id: Option<&str>,
    ) -> Result<()> {
        if let Some(session_id) = expected_session_id
            && binding.session_id != session_id
        {
            bail!(
                "Session binding {} stores mismatched session id {} (expected {})",
                path.display(),
                binding.session_id,
                session_id
            );
        }

        let expected_path = self.session_file_path(&binding.session_id)?;
        if path != expected_path.as_path() {
            bail!(
                "Session binding {} does not match serialized session id {} (expected path {})",
                path.display(),
                binding.session_id,
                expected_path.display()
            );
        }

        Ok(())
    }

    fn warn_and_quarantine_corrupted_binding(
        &self,
        session_id: &str,
        path: &Path,
        error: &anyhow::Error,
    ) {
        match self.quarantine_corrupted_binding(path) {
            Ok(quarantine_path) => {
                warn!(
                    %session_id,
                    path = %path.display(),
                    quarantine_path = %quarantine_path.display(),
                    error = %error,
                    "Quarantined corrupted session binding entry"
                );
            }
            Err(quarantine_err) => {
                warn!(
                    %session_id,
                    path = %path.display(),
                    error = %error,
                    quarantine_error = %quarantine_err,
                    "Detected corrupted session binding entry but failed to quarantine it"
                );
            }
        }
    }

    fn quarantine_corrupted_binding(&self, path: &Path) -> Result<PathBuf> {
        let storage_dir = std::fs::canonicalize(&self.storage_dir).with_context(|| {
            format!(
                "Failed to canonicalize session store dir {}",
                self.storage_dir.display()
            )
        })?;
        let source_parent = path.parent().with_context(|| {
            format!(
                "Corrupted session binding path has no parent directory: {}",
                path.display()
            )
        })?;
        if source_parent != storage_dir.as_path() {
            bail!(
                "Corrupted session binding path {} escapes storage dir {}",
                path.display(),
                storage_dir.display()
            );
        }
        let source_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Corrupted session binding path is missing a valid file name")?;
        if source_name.is_empty()
            || source_name == "."
            || source_name == ".."
            || source_name.contains("..")
            || source_name.contains('/')
            || source_name.contains('\\')
        {
            bail!("Invalid corrupted session binding file name: {source_name}");
        }
        let source_path = storage_dir.join(source_name);

        let canonical_quarantine_dir = storage_dir.join(CORRUPTED_BINDINGS_DIR_NAME);
        let canonical_quarantine_parent = canonical_quarantine_dir.parent().with_context(|| {
            format!(
                "Corrupted session binding quarantine dir has no parent directory: {}",
                canonical_quarantine_dir.display()
            )
        })?;
        if canonical_quarantine_parent != storage_dir.as_path() {
            bail!(
                "Corrupted session binding quarantine dir {} is not a direct child of storage dir {}",
                canonical_quarantine_dir.display(),
                storage_dir.display()
            );
        }

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let quarantine_name = format!(
            "binding-{}-{suffix}.{}",
            digest_hex(source_path.to_string_lossy().as_bytes()),
            SESSION_BINDING_EXTENSION
        );
        if quarantine_name.is_empty()
            || quarantine_name == "."
            || quarantine_name == ".."
            || quarantine_name.contains("..")
            || quarantine_name.contains('/')
            || quarantine_name.contains('\\')
        {
            bail!("Invalid corrupted session binding quarantine file name: {quarantine_name}");
        }
        let quarantine_path = canonical_quarantine_dir.join(quarantine_name);
        let quarantine_path_parent = quarantine_path.parent().with_context(|| {
            format!(
                "Corrupted session binding quarantine path has no parent directory: {}",
                quarantine_path.display()
            )
        })?;
        if quarantine_path_parent != canonical_quarantine_dir.as_path() {
            bail!(
                "Corrupted session binding quarantine path {} escapes quarantine dir {}",
                quarantine_path.display(),
                canonical_quarantine_dir.display()
            );
        }

        std::fs::rename(&source_path, &quarantine_path).with_context(|| {
            format!(
                "Failed to quarantine corrupted session binding {} -> {}",
                source_path.display(),
                quarantine_path.display()
            )
        })?;

        sync_directory(source_parent)?;
        sync_directory(&canonical_quarantine_dir)?;

        Ok(quarantine_path)
    }
}

#[derive(Debug, Clone, Copy)]
struct SafeSessionId<'a>(&'a str);

impl<'a> SafeSessionId<'a> {
    fn parse(value: &'a str) -> Result<Self> {
        if is_safe_session_id(value) {
            Ok(Self(value))
        } else {
            bail!("Invalid session id path component: {value:?}");
        }
    }

    fn binding_file_name(self) -> String {
        format!("{}.{}", self.0, SESSION_BINDING_EXTENSION)
    }
}

fn is_safe_session_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn digest_hex(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn sync_directory(path: &Path) -> Result<()> {
    let canonical = std::fs::canonicalize(path).with_context(|| {
        format!(
            "Failed to canonicalize session store dir: {}",
            path.display()
        )
    })?;
    let dir = std::fs::File::open(&canonical).with_context(|| {
        format!(
            "Failed to open session store dir for fsync: {}",
            canonical.display()
        )
    })?;
    dir.sync_all()
        .with_context(|| format!("Failed to fsync session store dir: {}", canonical.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_session_store_new() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        // Initially empty.
        assert!(store.list_all().is_empty());
        assert!(!store.exists("test"));
    }

    #[test]
    fn test_save_and_load() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let binding = SessionBinding {
            session_id: "test-session".to_string(),
            workspace_path: PathBuf::from("/tmp/test-workspace"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig {
                profile: alan_agent_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            },
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding.clone()).unwrap();

        // Should be loadable again.
        let loaded = store.load("test-session").unwrap();
        assert_eq!(loaded.session_id, binding.session_id);
        assert_eq!(loaded.workspace_path, binding.workspace_path);
    }

    #[test]
    fn test_exists() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        assert!(!store.exists("nonexistent"));

        let binding = SessionBinding {
            session_id: "exists-session".to_string(),
            workspace_path: PathBuf::from("/tmp/test"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding).unwrap();
        assert!(store.exists("exists-session"));
    }

    #[test]
    fn test_save_and_load_streaming_mode() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let binding = SessionBinding {
            session_id: "streaming-mode-session".to_string(),
            workspace_path: PathBuf::from("/tmp/test-streaming"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: Some(alan_agent_engine::StreamingMode::Off),
            partial_stream_recovery_mode: Some(alan_agent_engine::PartialStreamRecoveryMode::Off),
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding).unwrap();
        let loaded = store.load("streaming-mode-session").unwrap();
        assert_eq!(
            loaded.streaming_mode,
            Some(alan_agent_engine::StreamingMode::Off)
        );
    }

    #[test]
    fn test_remove() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let binding = SessionBinding {
            session_id: "to-remove".to_string(),
            workspace_path: PathBuf::from("/tmp/test"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding).unwrap();
        assert!(store.exists("to-remove"));

        store.remove("to-remove").unwrap();
        assert!(!store.exists("to-remove"));
        assert!(store.load("to-remove").is_none());
    }

    #[test]
    fn rejects_unsafe_session_ids_for_direct_binding_paths() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let binding = SessionBinding {
            session_id: "../escape".to_string(),
            workspace_path: PathBuf::from("/tmp/test"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        assert!(store.save(binding).is_err());
        assert!(store.load("../escape").is_none());
        assert!(!store.exists("../escape"));
        assert!(store.remove("../escape").is_err());
    }

    #[test]
    fn test_list_all() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        // Create multiple sessions.
        for i in 0..3 {
            let binding = SessionBinding {
                session_id: format!("session-{}", i),
                workspace_path: PathBuf::from(format!("/tmp/ws-{}", i)),
                created_at: chrono::Utc::now().to_rfc3339(),
                governance: alan_agent_protocol::GovernanceConfig::default(),
                agent_name: None,
                profile_id: None,
                provider: None,
                resolved_model: String::new(),
                reasoning_effort: None,
                streaming_mode: None,
                partial_stream_recovery_mode: None,
                rollout_path: None,
                durability_required: Some(false),
                durable: None,
            };
            store.save(binding).unwrap();
        }

        let all = store.list_all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_get_workspace_path() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let workspace_path = PathBuf::from("/tmp/my-workspace");
        let binding = SessionBinding {
            session_id: "ws-test".to_string(),
            workspace_path: workspace_path.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding).unwrap();

        let retrieved = store.get_workspace_path("ws-test").unwrap();
        assert_eq!(retrieved, workspace_path);
    }

    #[test]
    fn test_update_rollout_path() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let binding = SessionBinding {
            session_id: "rollout-test".to_string(),
            workspace_path: PathBuf::from("/tmp/ws"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding).unwrap();

        let new_rollout = Some(PathBuf::from("/tmp/rollout.jsonl"));
        store
            .update_rollout_path("rollout-test", new_rollout.clone())
            .unwrap();

        let loaded = store.load("rollout-test").unwrap();
        assert_eq!(loaded.rollout_path, new_rollout);
    }

    #[test]
    fn test_update_runtime_state() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let binding = SessionBinding {
            session_id: "durability-test".to_string(),
            workspace_path: PathBuf::from("/tmp/ws"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding).unwrap();
        store
            .update_runtime_state(
                "durability-test",
                Some(PathBuf::from("/tmp/runtime.jsonl")),
                alan_agent_engine::runtime::SessionDurabilityState {
                    durable: false,
                    required: true,
                },
            )
            .unwrap();

        let loaded = store.load("durability-test").unwrap();
        assert_eq!(
            loaded.rollout_path,
            Some(PathBuf::from("/tmp/runtime.jsonl"))
        );
        assert_eq!(loaded.durability_required, Some(true));
        assert_eq!(loaded.durable, Some(false));
    }

    #[test]
    fn test_load_nonexistent() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        assert!(store.load("nonexistent").is_none());
        assert!(store.get_workspace_path("nonexistent").is_none());
    }

    #[test]
    fn test_persistence() {
        let temp = TempDir::new().unwrap();
        let storage_dir = temp.path().to_path_buf();

        // The first store instance saves data.
        {
            let store = SessionStore::with_dir(storage_dir.clone()).unwrap();
            let binding = SessionBinding {
                session_id: "persistent".to_string(),
                workspace_path: PathBuf::from("/tmp/persistent-ws"),
                created_at: chrono::Utc::now().to_rfc3339(),
                governance: alan_agent_protocol::GovernanceConfig::default(),
                agent_name: None,
                profile_id: None,
                provider: None,
                resolved_model: String::new(),
                reasoning_effort: None,
                streaming_mode: None,
                partial_stream_recovery_mode: None,
                rollout_path: None,
                durability_required: Some(false),
                durable: None,
            };
            store.save(binding).unwrap();
        }

        // The second store instance should load persisted data.
        {
            let store = SessionStore::with_dir(storage_dir).unwrap();
            let loaded = store.load("persistent").unwrap();
            assert_eq!(loaded.session_id, "persistent");
            assert_eq!(loaded.workspace_path, PathBuf::from("/tmp/persistent-ws"));
        }
    }

    #[test]
    fn test_load_legacy_binding_without_durability_required() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();

        let binding = SessionBinding {
            session_id: "legacy-binding".to_string(),
            workspace_path: PathBuf::from("/tmp/legacy-ws"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: None,
            durable: None,
        };

        store.save(binding).unwrap();

        let loaded = store.load("legacy-binding").unwrap();
        assert_eq!(loaded.durability_required, None);
        assert!(!loaded.effective_durability_required(false));
        assert!(loaded.effective_durability_required(true));
    }

    #[test]
    fn test_save_is_atomic_and_cleans_up_tmp_file() {
        let temp = TempDir::new().unwrap();
        let store = SessionStore::with_dir(temp.path().to_path_buf()).unwrap();
        let session_id = "atomic-save";
        let binding = SessionBinding {
            session_id: session_id.to_string(),
            workspace_path: PathBuf::from("/tmp/atomic-ws"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };

        store.save(binding).unwrap();

        let path = store.session_file_path(session_id).unwrap();
        let tmp_path = path.with_extension(SESSION_BINDING_TMP_EXTENSION);
        assert!(path.exists());
        assert!(!tmp_path.exists());
    }

    #[test]
    fn test_list_all_quarantines_corrupted_binding_file() {
        let temp = TempDir::new().unwrap();
        let storage_dir = temp.path().to_path_buf();
        let store = SessionStore::with_dir(storage_dir.clone()).unwrap();

        let binding = SessionBinding {
            session_id: "valid-session".to_string(),
            workspace_path: PathBuf::from("/tmp/valid-ws"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };
        store.save(binding).unwrap();

        let corrupted_path = storage_dir.join("broken.json");
        fs::write(&corrupted_path, "{ not valid json").unwrap();

        let all = store.list_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].session_id, "valid-session");
        assert!(!corrupted_path.exists());

        let quarantine_dir = storage_dir.join(CORRUPTED_BINDINGS_DIR_NAME);
        let quarantined: Vec<_> = fs::read_dir(&quarantine_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            fs::read_to_string(&quarantined[0]).unwrap(),
            "{ not valid json"
        );
    }

    #[test]
    fn test_load_quarantines_binding_with_mismatched_session_id() {
        let temp = TempDir::new().unwrap();
        let storage_dir = temp.path().to_path_buf();
        let store = SessionStore::with_dir(storage_dir.clone()).unwrap();

        let mismatched = SessionBinding {
            session_id: "other-session".to_string(),
            workspace_path: PathBuf::from("/tmp/other-ws"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };
        let mismatched_path = store.session_file_path("expected-session").unwrap();
        fs::write(
            &mismatched_path,
            serde_json::to_string_pretty(&mismatched).unwrap(),
        )
        .unwrap();

        assert!(store.load("expected-session").is_none());
        assert!(!mismatched_path.exists());

        let quarantine_dir = storage_dir.join(CORRUPTED_BINDINGS_DIR_NAME);
        let quarantined: Vec<_> = fs::read_dir(&quarantine_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(quarantined.len(), 1);
        let content = fs::read_to_string(&quarantined[0]).unwrap();
        assert!(content.contains("\"other-session\""));
    }

    #[cfg(unix)]
    #[test]
    fn test_list_all_quarantines_symlinked_binding_outside_storage_dir() {
        let temp = TempDir::new().unwrap();
        let storage_dir = temp.path().join("sessions");
        let outside_dir = temp.path().join("outside");
        fs::create_dir_all(&outside_dir).unwrap();
        let store = SessionStore::with_dir(storage_dir.clone()).unwrap();

        let outside_binding = SessionBinding {
            session_id: "outside-session".to_string(),
            workspace_path: PathBuf::from("/tmp/outside-ws"),
            created_at: chrono::Utc::now().to_rfc3339(),
            governance: alan_agent_protocol::GovernanceConfig::default(),
            agent_name: None,
            profile_id: None,
            provider: None,
            resolved_model: String::new(),
            reasoning_effort: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            rollout_path: None,
            durability_required: Some(false),
            durable: None,
        };
        let outside_path = outside_dir.join("outside.json");
        fs::write(
            &outside_path,
            serde_json::to_string_pretty(&outside_binding).unwrap(),
        )
        .unwrap();

        let symlink_path = storage_dir.join("escaped.json");
        std::os::unix::fs::symlink(&outside_path, &symlink_path).unwrap();

        assert!(store.list_all().is_empty());
        assert!(!symlink_path.exists());

        let quarantine_dir = storage_dir.join(CORRUPTED_BINDINGS_DIR_NAME);
        let quarantined: Vec<_> = fs::read_dir(&quarantine_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(quarantined.len(), 1);
        let metadata = fs::symlink_metadata(&quarantined[0]).unwrap();
        assert!(metadata.file_type().is_symlink());
        assert_eq!(fs::read_link(&quarantined[0]).unwrap(), outside_path);
    }
}
