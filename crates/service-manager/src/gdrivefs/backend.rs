//! Google Drive backend trait, DTOs, token redaction, and deterministic in-memory test backend.
//!
//! OAuth credentials stay strictly behind [`DriveTokenProvider`] (implemented on Host Store
//! via `alan_auth::google`) and are never serialized into any namespace file, error message,
//! or debug output. Permanent delete and empty-trash operations are intentionally absent
//! from [`GdriveBackend`] so destructive transitions remain structurally unreachable.

use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use alan_ap::ErrorCode;
use async_trait::async_trait;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Google Drive API v3 export size ceiling (10 MiB = 10,485,760 bytes).
pub const MAX_EXPORT_BYTES: usize = 10 * 1024 * 1024;
/// Maximum commit-on-clunk upload payload size accepted on `uploads/new` (10 MiB).
pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
/// Maximum commit-on-clunk control command size accepted by `ctl` (64 KiB).
pub const MAX_CTL_BYTES: usize = 64 * 1024;
/// Maximum byte span served in a single binary range read.
pub const MAX_RANGE_READ_BYTES: usize = 1024 * 1024;
/// Maximum search hits returned by `search/<query>`.
pub const DEFAULT_SEARCH_LIMIT: usize = 25;

pub const GOOGLE_DRIVE_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
pub const GOOGLE_DRIVE_FILE_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

pub const WORKSPACE_FOLDER_MIME: &str = "application/vnd.google-apps.folder";
pub const WORKSPACE_DOC_MIME: &str = "application/vnd.google-apps.document";
pub const WORKSPACE_SHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";
pub const WORKSPACE_SLIDES_MIME: &str = "application/vnd.google-apps.presentation";
pub const WORKSPACE_SCRIPT_MIME: &str = "application/vnd.google-apps.script";

/// Return the canonical Workspace export MIME type for Google Workspace native files.
pub fn export_mime_for_workspace(mime_type: &str) -> Option<&'static str> {
    match mime_type {
        WORKSPACE_DOC_MIME => Some("text/markdown"),
        WORKSPACE_SHEET_MIME => Some("text/csv"),
        WORKSPACE_SLIDES_MIME => Some("text/plain"),
        WORKSPACE_SCRIPT_MIME => Some("application/json"),
        _ => None,
    }
}

/// Whether `mime_type` denotes a Google Workspace native item (`application/vnd.google-apps.*`).
pub fn is_workspace_mime(mime_type: &str) -> bool {
    mime_type.starts_with("application/vnd.google-apps.")
}

/// Validate a Google Drive file/folder id path segment.
pub fn valid_drive_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id != "."
        && id != ".."
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Ephemeral Bearer token wrapper whose `Debug` output always redacts the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct DriveAuthToken {
    pub access_token: String,
    pub scopes: Vec<String>,
    pub email: Option<String>,
}

impl fmt::Debug for DriveAuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DriveAuthToken")
            .field("access_token", &"[REDACTED]")
            .field("scopes", &self.scopes)
            .field("email", &self.email)
            .finish()
    }
}

/// Non-secret OAuth status summary projected in `/mnt/gdrive/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveAuthSummary {
    pub authenticated: bool,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub has_file_write_scope: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// Boxed future returned by [`DriveTokenProvider`].
pub type DriveTokenFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DriveAuthToken, GdriveBackendError>> + Send + 'a>>;

/// Host-owned token provider boundary (implemented via `alan-auth` in CLI host).
pub trait DriveTokenProvider: Send + Sync {
    fn access_token(&self) -> DriveTokenFuture<'_>;
    fn force_refresh(&self) -> DriveTokenFuture<'_>;
    fn auth_summary(&self) -> DriveAuthSummary;
}

/// Metadata projected at `by-id/<file_id>/meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveFileMeta {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub is_folder: bool,
    pub is_workspace_doc: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default)]
    pub parents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5_checksum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_view_link: Option<String>,
    #[serde(default)]
    pub trashed: bool,
}

/// Storage quota summary projected in `/mnt/gdrive/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveQuotaSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_in_drive_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_in_trash_bytes: Option<u64>,
}

/// Rate-limit and request telemetry projected in `/mnt/gdrive/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveRateLimitStatus {
    pub api_requests: u64,
    pub rate_limited_responses: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_retry_after_secs: Option<u64>,
}

/// Change cursor status projected in `/mnt/gdrive/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveCursorStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_page_token: Option<String>,
    pub sync_generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
}

/// Read-only status document projected at `/mnt/gdrive/status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GdriveStatusSnapshot {
    pub mount_access: String,
    pub auth: DriveAuthSummary,
    pub cursor: DriveCursorStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<DriveQuotaSummary>,
    pub rate_limit: DriveRateLimitStatus,
    pub pinned_folders: Vec<String>,
    pub cached_files: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Single search hit entry returned by `search/<query>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveSearchHit {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub is_folder: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_view_link: Option<String>,
    #[serde(default)]
    pub parents: Vec<String>,
}

/// Bounded JSON response projected at `search/<query>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveSearchResponse {
    pub query: String,
    pub truncated: bool,
    #[serde(default)]
    pub hits: Vec<DriveSearchHit>,
}

fn default_upload_mime_type() -> String {
    "text/plain".to_string()
}

/// JSON request payload written to `uploads/new` (commit-on-clunk).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewDriveUploadRequest {
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default = "default_upload_mime_type")]
    pub mime_type: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_base64: Option<String>,
}

impl NewDriveUploadRequest {
    /// Validate upload metadata and decode payload bytes within `MAX_UPLOAD_BYTES`.
    pub fn validate_and_decode(&self) -> Result<Vec<u8>, ErrorCode> {
        let name = self.name.trim();
        if name.is_empty()
            || name.len() > 255
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\0')
        {
            return Err(ErrorCode::BadRequest);
        }
        if let Some(parent) = self.parent_id.as_deref()
            && !valid_drive_id(parent)
        {
            return Err(ErrorCode::BadRequest);
        }
        let mime = self.mime_type.trim();
        if mime.is_empty()
            || mime.len() > 255
            || mime.bytes().any(|b| matches!(b, b'\0' | b'\r' | b'\n'))
        {
            return Err(ErrorCode::BadRequest);
        }
        let bytes = match (&self.content, &self.content_base64) {
            (Some(_), Some(_)) => return Err(ErrorCode::BadRequest),
            (Some(text), None) => text.as_bytes().to_vec(),
            (None, Some(b64)) => base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|_| ErrorCode::BadRequest)?,
            (None, None) => Vec::new(),
        };
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        Ok(bytes)
    }
}

/// JSON receipt readable from `uploads/new` after a successful commit-on-clunk upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveUploadedReceipt {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_view_link: Option<String>,
}

/// Binary byte range chunk returned with revision/ETag metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveRangeData {
    pub bytes: Vec<u8>,
    pub etag: Option<String>,
    pub revision_id: Option<String>,
    pub total_size: Option<u64>,
}

/// Sync result returned by [`GdriveBackend::sync`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriveSyncOutcome {
    pub start_page_token: Option<String>,
    pub updated_files: Vec<DriveFileMeta>,
    pub removed_file_ids: Vec<String>,
    pub pinned_children: BTreeMap<String, Vec<DriveFileMeta>>,
    pub quota: Option<DriveQuotaSummary>,
    pub api_requests: u64,
}

/// Backend failure carrying an aP [`ErrorCode`] and token-redacted diagnostic text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GdriveBackendError {
    pub code: ErrorCode,
    pub message: String,
    pub retry_after_secs: Option<u64>,
}

impl GdriveBackendError {
    /// Construct an error after scrubbing any embedded Google OAuth tokens.
    pub fn new(code: ErrorCode, message: impl AsRef<str>) -> Self {
        Self {
            code,
            message: redact_google_tokens(message.as_ref(), None),
            retry_after_secs: None,
        }
    }

    pub(crate) fn redacted(
        code: ErrorCode,
        message: impl AsRef<str>,
        secret: Option<&str>,
    ) -> Self {
        Self {
            code,
            message: redact_google_tokens(message.as_ref(), secret),
            retry_after_secs: None,
        }
    }

    pub(crate) fn rate_limited(
        message: impl AsRef<str>,
        retry_after_secs: Option<u64>,
        secret: Option<&str>,
    ) -> Self {
        Self {
            code: ErrorCode::RateLimited,
            message: redact_google_tokens(message.as_ref(), secret),
            retry_after_secs,
        }
    }
}

impl fmt::Display for GdriveBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for GdriveBackendError {}

/// Scrubs Google OAuth tokens (`ya29.`, `1//`, `Bearer ...`, JSON/query token fields) from text.
pub fn redact_google_tokens(input: &str, extra_secret: Option<&str>) -> String {
    let mut scrubbed = if let Some(secret) = extra_secret.filter(|s| !s.is_empty()) {
        input.replace(secret, "[REDACTED]")
    } else {
        input.to_string()
    };

    for prefix in ["ya29.", "1//"] {
        while let Some(idx) = scrubbed.find(prefix) {
            let tail = &scrubbed[idx + prefix.len()..];
            let len = tail
                .bytes()
                .take_while(|b| {
                    !b.is_ascii_whitespace() && !matches!(b, b'"' | b'\'' | b',' | b';' | b'&')
                })
                .count();
            scrubbed.replace_range(idx..idx + prefix.len() + len, "[REDACTED]");
        }
    }

    for marker in [
        "Bearer ",
        "bearer ",
        "access_token=",
        "refresh_token=",
        "\"access_token\":\"",
        "\"refresh_token\":\"",
        "\"access_token\": \"",
        "\"refresh_token\": \"",
    ] {
        let mut search_from = 0usize;
        while let Some(rel) = scrubbed[search_from..].find(marker) {
            let token_start = search_from + rel + marker.len();
            let token_len = scrubbed[token_start..]
                .bytes()
                .take_while(|b| {
                    !b.is_ascii_whitespace() && !matches!(b, b'"' | b'\'' | b',' | b';' | b'&')
                })
                .count();
            if token_len > 0 && &scrubbed[token_start..token_start + token_len] != "[REDACTED]" {
                scrubbed.replace_range(token_start..token_start + token_len, "[REDACTED]");
                search_from = token_start + "[REDACTED]".len();
            } else {
                search_from = token_start;
            }
        }
    }
    scrubbed
}

/// Abstract Google Drive backend.
///
/// Intentionally omits `delete_file`, `trash_file`, and `empty_trash` so permanent delete
/// and empty-trash transitions remain structurally unreachable through `gdrivefs`.
#[async_trait]
pub trait GdriveBackend: Send + Sync {
    fn auth_summary(&self) -> DriveAuthSummary;
    async fn has_file_write_scope(&self) -> bool;
    async fn get_file_meta(&self, file_id: &str) -> Result<DriveFileMeta, GdriveBackendError>;
    async fn list_folder(&self, folder_id: &str) -> Result<Vec<DriveFileMeta>, GdriveBackendError>;
    async fn read_binary_range(
        &self,
        meta: &DriveFileMeta,
        offset: u64,
        count: u32,
        expected_etag: Option<&str>,
        expected_revision: Option<&str>,
    ) -> Result<DriveRangeData, GdriveBackendError>;
    async fn export_workspace_file(
        &self,
        meta: &DriveFileMeta,
        export_mime: &str,
    ) -> Result<Vec<u8>, GdriveBackendError>;
    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DriveSearchResponse, GdriveBackendError>;
    async fn sync(
        &self,
        start_page_token: Option<&str>,
        pinned_folders: &[String],
    ) -> Result<DriveSyncOutcome, GdriveBackendError>;
    async fn upload_file(
        &self,
        req: &NewDriveUploadRequest,
        bytes: &[u8],
    ) -> Result<DriveFileMeta, GdriveBackendError>;
}

#[derive(Clone)]
struct MemoryFileEntry {
    meta: DriveFileMeta,
    content: Vec<u8>,
}

struct MemoryState {
    auth: DriveAuthSummary,
    files: BTreeMap<String, MemoryFileEntry>,
    quota: Option<DriveQuotaSummary>,
    start_page_token: u64,
    next_upload_seq: u64,
    recorded_range_reads: Vec<(String, u64, u32)>,
    forced_rate_limit_secs: Option<u64>,
}

/// Deterministic in-memory [`GdriveBackend`] for unit and integration tests.
#[derive(Clone)]
pub struct MemoryGdriveBackend {
    state: Arc<Mutex<MemoryState>>,
}

impl Default for MemoryGdriveBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryGdriveBackend {
    /// Create an empty in-memory Drive backend authenticated with `drive.readonly` scope.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryState {
                auth: DriveAuthSummary {
                    authenticated: true,
                    scopes: vec![GOOGLE_DRIVE_READONLY_SCOPE.to_string()],
                    has_file_write_scope: false,
                    email: Some("drive-test@example.com".to_string()),
                },
                files: BTreeMap::new(),
                quota: Some(DriveQuotaSummary {
                    limit_bytes: Some(15 * 1024 * 1024 * 1024),
                    usage_bytes: Some(1024 * 1024),
                    usage_in_drive_bytes: Some(1024 * 1024),
                    usage_in_trash_bytes: Some(0),
                }),
                start_page_token: 1,
                next_upload_seq: 1,
                recorded_range_reads: Vec::new(),
                forced_rate_limit_secs: None,
            })),
        }
    }

    /// Configure the OAuth scopes advertised by this mock backend.
    pub fn set_scopes(&self, scopes: Vec<String>) {
        let mut state = self.state.lock().expect("memory gdrive lock poisoned");
        state.auth.has_file_write_scope = scopes.iter().any(|s| s == GOOGLE_DRIVE_FILE_SCOPE);
        state.auth.authenticated = !scopes.is_empty();
        state.auth.scopes = scopes;
    }

    /// Seed a file or folder entry with explicit bytes (used for both binary and Workspace exports).
    pub fn seed_file(&self, mut meta: DriveFileMeta, content: impl Into<Vec<u8>>) {
        let content = content.into();
        meta.is_folder = meta.mime_type == WORKSPACE_FOLDER_MIME;
        meta.is_workspace_doc = is_workspace_mime(&meta.mime_type) && !meta.is_folder;
        meta.export_mime_type = export_mime_for_workspace(&meta.mime_type).map(ToOwned::to_owned);
        if !meta.is_folder && !meta.is_workspace_doc && meta.size.is_none() {
            meta.size = Some(content.len() as u64);
        }
        let mut state = self.state.lock().expect("memory gdrive lock poisoned");
        state.start_page_token = state.start_page_token.saturating_add(1);
        state
            .files
            .insert(meta.id.clone(), MemoryFileEntry { meta, content });
    }

    /// Mutate the ETag / head_revision_id of an existing file (to simulate mid-read revision drift).
    pub fn bump_file_revision(&self, file_id: &str, new_etag: &str, new_rev: &str) {
        let mut state = self.state.lock().expect("memory gdrive lock poisoned");
        if let Some(entry) = state.files.get_mut(file_id) {
            entry.meta.etag = Some(new_etag.to_string());
            entry.meta.head_revision_id = Some(new_rev.to_string());
        }
    }

    /// Inspect `(file_id, offset, count)` tuples recorded by `read_binary_range`.
    pub fn recorded_range_reads(&self) -> Vec<(String, u64, u32)> {
        self.state
            .lock()
            .expect("memory gdrive lock poisoned")
            .recorded_range_reads
            .clone()
    }

    /// Force the next backend call to return a rate-limit error.
    pub fn set_next_rate_limit(&self, retry_after_secs: Option<u64>) {
        self.state
            .lock()
            .expect("memory gdrive lock poisoned")
            .forced_rate_limit_secs = retry_after_secs;
    }

    /// Count all seeded/uploaded files currently stored in the mock backend.
    pub fn file_count(&self) -> usize {
        self.state
            .lock()
            .expect("memory gdrive lock poisoned")
            .files
            .len()
    }
}

#[async_trait]
impl GdriveBackend for MemoryGdriveBackend {
    fn auth_summary(&self) -> DriveAuthSummary {
        self.state
            .lock()
            .expect("memory gdrive lock poisoned")
            .auth
            .clone()
    }

    async fn has_file_write_scope(&self) -> bool {
        self.state
            .lock()
            .expect("memory gdrive lock poisoned")
            .auth
            .has_file_write_scope
    }

    async fn get_file_meta(&self, file_id: &str) -> Result<DriveFileMeta, GdriveBackendError> {
        let mut state = self.state.lock().expect("memory gdrive lock poisoned");
        if let Some(secs) = state.forced_rate_limit_secs.take() {
            return Err(GdriveBackendError::rate_limited(
                "Drive API rate limit exceeded",
                Some(secs),
                None,
            ));
        }
        state
            .files
            .get(file_id)
            .filter(|entry| !entry.meta.trashed)
            .map(|entry| entry.meta.clone())
            .ok_or_else(|| GdriveBackendError::new(ErrorCode::NotFound, "Drive file not found"))
    }

    async fn list_folder(&self, folder_id: &str) -> Result<Vec<DriveFileMeta>, GdriveBackendError> {
        let state = self.state.lock().expect("memory gdrive lock poisoned");
        let mut items: Vec<DriveFileMeta> = state
            .files
            .values()
            .filter(|entry| {
                !entry.meta.trashed && entry.meta.parents.iter().any(|p| p == folder_id)
            })
            .map(|entry| entry.meta.clone())
            .collect();
        items.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(items)
    }

    async fn read_binary_range(
        &self,
        meta: &DriveFileMeta,
        offset: u64,
        count: u32,
        expected_etag: Option<&str>,
        expected_revision: Option<&str>,
    ) -> Result<DriveRangeData, GdriveBackendError> {
        let mut state = self.state.lock().expect("memory gdrive lock poisoned");
        state
            .recorded_range_reads
            .push((meta.id.clone(), offset, count));
        let entry = state
            .files
            .get(&meta.id)
            .filter(|e| !e.meta.trashed)
            .ok_or_else(|| GdriveBackendError::new(ErrorCode::NotFound, "Drive file not found"))?;
        if let Some(exp) = expected_etag
            && entry.meta.etag.as_deref() != Some(exp)
        {
            return Err(GdriveBackendError::new(
                ErrorCode::Io,
                "Drive file ETag changed during range read",
            ));
        }
        if let Some(exp_rev) = expected_revision
            && entry.meta.head_revision_id.as_deref() != Some(exp_rev)
        {
            return Err(GdriveBackendError::new(
                ErrorCode::Io,
                "Drive file revision changed during range read",
            ));
        }
        let total_size = entry.content.len() as u64;
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(entry.content.len());
        let end = start
            .saturating_add(count as usize)
            .min(entry.content.len());
        Ok(DriveRangeData {
            bytes: entry.content[start..end].to_vec(),
            etag: entry.meta.etag.clone(),
            revision_id: entry.meta.head_revision_id.clone(),
            total_size: Some(total_size),
        })
    }

    async fn export_workspace_file(
        &self,
        meta: &DriveFileMeta,
        export_mime: &str,
    ) -> Result<Vec<u8>, GdriveBackendError> {
        let state = self.state.lock().expect("memory gdrive lock poisoned");
        let entry = state
            .files
            .get(&meta.id)
            .filter(|e| !e.meta.trashed)
            .ok_or_else(|| GdriveBackendError::new(ErrorCode::NotFound, "Drive file not found"))?;
        if export_mime_for_workspace(&entry.meta.mime_type) != Some(export_mime) {
            return Err(GdriveBackendError::new(
                ErrorCode::Unsupported,
                format!("unsupported Workspace export MIME `{export_mime}`"),
            ));
        }
        if entry.content.len() > MAX_EXPORT_BYTES {
            return Err(GdriveBackendError::new(
                ErrorCode::BadRequest,
                format!(
                    "Google Workspace export for `{}` exceeds 10 MB limit ({} > {} bytes)",
                    meta.id,
                    entry.content.len(),
                    MAX_EXPORT_BYTES
                ),
            ));
        }
        Ok(entry.content.clone())
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DriveSearchResponse, GdriveBackendError> {
        let needle = query.trim().to_ascii_lowercase();
        let bounded = limit.clamp(1, DEFAULT_SEARCH_LIMIT);
        let state = self.state.lock().expect("memory gdrive lock poisoned");
        let mut hits = Vec::new();
        for entry in state.files.values() {
            if entry.meta.trashed {
                continue;
            }
            let hay_name = entry.meta.name.to_ascii_lowercase();
            let hay_body = String::from_utf8_lossy(&entry.content).to_ascii_lowercase();
            if hay_name.contains(&needle) || hay_body.contains(&needle) {
                hits.push(DriveSearchHit {
                    id: entry.meta.id.clone(),
                    name: entry.meta.name.clone(),
                    mime_type: entry.meta.mime_type.clone(),
                    is_folder: entry.meta.is_folder,
                    size: entry.meta.size,
                    modified_time: entry.meta.modified_time.clone(),
                    web_view_link: entry.meta.web_view_link.clone(),
                    parents: entry.meta.parents.clone(),
                });
            }
        }
        hits.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        let truncated = hits.len() > bounded;
        hits.truncate(bounded);
        Ok(DriveSearchResponse {
            query: query.to_string(),
            truncated,
            hits,
        })
    }

    async fn sync(
        &self,
        _start_page_token: Option<&str>,
        pinned_folders: &[String],
    ) -> Result<DriveSyncOutcome, GdriveBackendError> {
        let state = self.state.lock().expect("memory gdrive lock poisoned");
        let mut pinned_children = BTreeMap::new();
        for folder in pinned_folders {
            let children: Vec<DriveFileMeta> = state
                .files
                .values()
                .filter(|entry| {
                    !entry.meta.trashed && entry.meta.parents.iter().any(|p| p == folder)
                })
                .map(|entry| entry.meta.clone())
                .collect();
            pinned_children.insert(folder.clone(), children);
        }
        Ok(DriveSyncOutcome {
            start_page_token: Some(state.start_page_token.to_string()),
            updated_files: state
                .files
                .values()
                .filter(|e| !e.meta.trashed)
                .map(|e| e.meta.clone())
                .collect(),
            removed_file_ids: Vec::new(),
            pinned_children,
            quota: state.quota.clone(),
            api_requests: 1,
        })
    }

    async fn upload_file(
        &self,
        req: &NewDriveUploadRequest,
        bytes: &[u8],
    ) -> Result<DriveFileMeta, GdriveBackendError> {
        let mut state = self.state.lock().expect("memory gdrive lock poisoned");
        if !state.auth.has_file_write_scope {
            return Err(GdriveBackendError::new(
                ErrorCode::NoAccess,
                "uploads/new requires Google Drive `drive.file` OAuth scope",
            ));
        }
        let seq = state.next_upload_seq;
        state.next_upload_seq = state.next_upload_seq.saturating_add(1);
        let id = format!("uploaded_{seq}");
        let parents = vec![req.parent_id.clone().unwrap_or_else(|| "root".to_string())];
        let meta = DriveFileMeta {
            id: id.clone(),
            name: req.name.trim().to_string(),
            mime_type: req.mime_type.trim().to_string(),
            is_folder: false,
            is_workspace_doc: false,
            export_mime_type: None,
            size: Some(bytes.len() as u64),
            parents,
            modified_time: Some(Utc::now().to_rfc3339()),
            md5_checksum: None,
            head_revision_id: Some("rev-1".to_string()),
            etag: Some(format!("\"mem-upload-{seq}\"")),
            web_view_link: Some(format!("https://drive.google.com/file/d/{id}/view")),
            trashed: false,
        };
        state.start_page_token = state.start_page_token.saturating_add(1);
        state.files.insert(
            id,
            MemoryFileEntry {
                meta: meta.clone(),
                content: bytes.to_vec(),
            },
        );
        Ok(meta)
    }
}
