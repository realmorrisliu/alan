//! Lightweight non-secret disk cache for live provider model catalogs.
//!
//! Stored alongside `connections.toml` in the Connection Service System Store
//! as `catalog-cache.json`. Hydrates `PublishedCatalog` and `llmfs` on boot
//! before background live discovery completes; never stores secrets.

use crate::connection_profile::ConnectionProfile;
use alan_agent_engine::LlmProvider;
use alan_llm::{ProviderModel, ReasoningEffort};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const CATALOG_CACHE_VERSION: u32 = 1;
const CATALOG_CACHE_FILENAME: &str = "catalog-cache.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CachedModelEntry {
    slug: String,
    context_window_tokens: u32,
    #[serde(default)]
    supported_reasoning_efforts: Vec<ReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_reasoning_effort: Option<ReasoningEffort>,
}

impl From<&ProviderModel> for CachedModelEntry {
    fn from(model: &ProviderModel) -> Self {
        Self {
            slug: model.slug.clone(),
            context_window_tokens: model.context_window_tokens,
            supported_reasoning_efforts: model.supported_reasoning_efforts.clone(),
            default_reasoning_effort: model.default_reasoning_effort,
        }
    }
}

impl From<CachedModelEntry> for ProviderModel {
    fn from(entry: CachedModelEntry) -> Self {
        Self {
            slug: entry.slug,
            context_window_tokens: entry.context_window_tokens,
            supported_reasoning_efforts: entry.supported_reasoning_efforts,
            default_reasoning_effort: entry.default_reasoning_effort,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedProfileCatalog {
    provider: LlmProvider,
    profile_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_identity: Option<String>,
    fetched_at_unix: i64,
    models: Vec<CachedModelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CatalogCacheFile {
    version: u32,
    #[serde(default)]
    profiles: BTreeMap<String, CachedProfileCatalog>,
}

impl Default for CatalogCacheFile {
    fn default() -> Self {
        Self {
            version: CATALOG_CACHE_VERSION,
            profiles: BTreeMap::new(),
        }
    }
}

pub(crate) fn cache_path_for_metadata(metadata_path: &Path) -> PathBuf {
    metadata_path.with_file_name(CATALOG_CACHE_FILENAME)
}

/// Non-secret fingerprint of catalog-relevant profile fields.
///
/// Intentionally excludes `settings["model"]` so switching the default model
/// within a profile does not invalidate its provider catalog, while any profile
/// replacement (provider, label, credential, endpoint, timestamps, etc.) does.
pub(crate) fn profile_catalog_fingerprint(profile: &ConnectionProfile) -> String {
    let mut normalized = profile.clone();
    normalized.settings.remove("model");
    let encoded = serde_json::to_vec(&normalized).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&encoded);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn load_cache_file(metadata_path: &Path) -> CatalogCacheFile {
    let path = cache_path_for_metadata(metadata_path);
    let Ok(bytes) = std::fs::read(&path) else {
        return CatalogCacheFile::default();
    };
    match serde_json::from_slice::<CatalogCacheFile>(&bytes) {
        Ok(file) if file.version == CATALOG_CACHE_VERSION => file,
        _ => CatalogCacheFile::default(),
    }
}

impl CatalogCacheFile {
    pub(crate) fn lookup(
        &self,
        profile_id: &str,
        profile: &ConnectionProfile,
        account_identity: Option<&str>,
    ) -> Option<Vec<ProviderModel>> {
        let entry = self.profiles.get(profile_id)?;
        if entry.provider != profile.provider
            || entry.profile_fingerprint != profile_catalog_fingerprint(profile)
            || entry.account_identity.as_deref() != account_identity
            || entry.models.is_empty()
        {
            return None;
        }
        Some(
            entry
                .models
                .iter()
                .cloned()
                .map(ProviderModel::from)
                .collect(),
        )
    }
}

pub(crate) fn save_cached_models(
    metadata_path: &Path,
    io_lock: &Mutex<()>,
    closed: &AtomicBool,
    profile_id: &str,
    profile: &ConnectionProfile,
    account_identity: Option<&str>,
    models: &[ProviderModel],
) {
    if models.is_empty() || closed.load(Ordering::SeqCst) {
        return;
    }
    let _guard = io_lock.lock().unwrap_or_else(|e| e.into_inner());
    if closed.load(Ordering::SeqCst) {
        return;
    }
    let path = cache_path_for_metadata(metadata_path);
    let Some(parent) = path.parent() else {
        return;
    };
    if !parent.exists() && std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut cache = load_cache_file(metadata_path);
    cache.profiles.insert(
        profile_id.to_string(),
        CachedProfileCatalog {
            provider: profile.provider,
            profile_fingerprint: profile_catalog_fingerprint(profile),
            account_identity: account_identity.map(str::to_string),
            fetched_at_unix: chrono::Utc::now().timestamp(),
            models: models.iter().map(CachedModelEntry::from).collect(),
        },
    );
    let Ok(rendered) = serde_json::to_vec_pretty(&cache) else {
        return;
    };
    let Ok(mut staged) = tempfile::NamedTempFile::new_in(parent) else {
        return;
    };
    if staged.write_all(&rendered).is_err() {
        return;
    }
    if let Err(error) = staged.persist(&path) {
        tracing::debug!(%error, path = %path.display(), "failed to persist catalog cache");
    }
}
