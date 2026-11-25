//! Pure-Rust Google Drive API v3 HTTP client implementing [`GdriveBackend`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use alan_ap::ErrorCode;
use async_trait::async_trait;
use reqwest::StatusCode;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_MATCH, RANGE, RETRY_AFTER};
use serde::Deserialize;

use super::backend::{
    DEFAULT_SEARCH_LIMIT, DriveAuthSummary, DriveAuthToken, DriveFileMeta, DriveQuotaSummary,
    DriveRangeData, DriveSearchHit, DriveSearchResponse, DriveSyncOutcome, DriveTokenProvider,
    GdriveBackend, GdriveBackendError, MAX_EXPORT_BYTES, MAX_RANGE_READ_BYTES,
    NewDriveUploadRequest, WORKSPACE_FOLDER_MIME, export_mime_for_workspace, is_workspace_mime,
    valid_drive_id,
};

pub const DEFAULT_DRIVE_API_BASE_URL: &str = "https://www.googleapis.com/drive/v3";
pub const DEFAULT_DRIVE_UPLOAD_BASE_URL: &str = "https://www.googleapis.com/upload/drive/v3";

const FILE_FIELDS: &str =
    "id,name,mimeType,size,parents,modifiedTime,md5Checksum,headRevisionId,webViewLink,trashed";
const MAX_SYNC_PAGES: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Pure-Rust `reqwest` Drive API v3 client backed by a Host-owned [`DriveTokenProvider`].
#[derive(Clone)]
pub struct GoogleDriveApiBackend {
    client: reqwest::Client,
    token_provider: Arc<dyn DriveTokenProvider>,
    api_base_url: String,
    upload_base_url: String,
}

impl GoogleDriveApiBackend {
    /// Construct a Drive API v3 client targeting `https://www.googleapis.com`.
    pub fn new(token_provider: Arc<dyn DriveTokenProvider>) -> Self {
        Self::with_base_urls(
            token_provider,
            DEFAULT_DRIVE_API_BASE_URL,
            DEFAULT_DRIVE_UPLOAD_BASE_URL,
        )
    }

    /// Construct a Drive API v3 client with explicit API and upload base URLs (for testing).
    pub fn with_base_urls(
        token_provider: Arc<dyn DriveTokenProvider>,
        api_base_url: impl Into<String>,
        upload_base_url: impl Into<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            token_provider,
            api_base_url: api_base_url.into().trim_end_matches('/').to_string(),
            upload_base_url: upload_base_url.into().trim_end_matches('/').to_string(),
        }
    }

    async fn send_authed(
        &self,
        build: impl Fn(&reqwest::Client, &str) -> reqwest::RequestBuilder,
    ) -> Result<(reqwest::Response, DriveAuthToken), GdriveBackendError> {
        let mut token = self.token_provider.access_token().await?;
        for attempt in 0..2 {
            let response = build(&self.client, &token.access_token)
                .send()
                .await
                .map_err(|err| {
                    GdriveBackendError::redacted(
                        ErrorCode::Io,
                        format!("Drive HTTP request failed: {err}"),
                        Some(&token.access_token),
                    )
                })?;
            if response.status() == StatusCode::UNAUTHORIZED && attempt == 0 {
                token = self.token_provider.force_refresh().await?;
                continue;
            }
            return Ok((response, token));
        }
        Err(GdriveBackendError::new(
            ErrorCode::NoAccess,
            "Google Drive authorization failed after refresh",
        ))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
    ) -> Result<(T, Option<String>), GdriveBackendError> {
        let (response, token) = self
            .send_authed(|c, tok| c.get(url).bearer_auth(tok))
            .await?;
        let status = response.status();
        let headers = response.headers().clone();
        let etag = headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        let body = response.text().await.map_err(|err| {
            GdriveBackendError::redacted(
                ErrorCode::Io,
                format!("failed reading Drive API response: {err}"),
                Some(&token.access_token),
            )
        })?;
        if !status.is_success() {
            return Err(map_http_error(
                status,
                &headers,
                &body,
                Some(&token.access_token),
            ));
        }
        let parsed = serde_json::from_str::<T>(&body).map_err(|err| {
            GdriveBackendError::redacted(
                ErrorCode::Io,
                format!("invalid Drive API JSON response: {err}"),
                Some(&token.access_token),
            )
        })?;
        Ok((parsed, etag))
    }

    async fn fetch_quota(&self) -> Result<Option<DriveQuotaSummary>, GdriveBackendError> {
        let url = format!("{}/about?fields=storageQuota", self.api_base_url);
        let (resp, _) = self.get_json::<RawAboutResponse>(&url).await?;
        Ok(resp.storage_quota.map(Into::into))
    }
}

#[async_trait]
impl GdriveBackend for GoogleDriveApiBackend {
    fn auth_summary(&self) -> DriveAuthSummary {
        self.token_provider.auth_summary()
    }

    async fn has_file_write_scope(&self) -> bool {
        self.token_provider.auth_summary().has_file_write_scope
    }

    async fn get_file_meta(&self, file_id: &str) -> Result<DriveFileMeta, GdriveBackendError> {
        if !valid_drive_id(file_id) {
            return Err(GdriveBackendError::new(
                ErrorCode::BadRequest,
                "invalid Google Drive file id",
            ));
        }
        let url = format!(
            "{}/files/{}?fields={}&supportsAllDrives=true",
            self.api_base_url,
            percent_encode(file_id),
            percent_encode(FILE_FIELDS)
        );
        let (raw, etag) = self.get_json::<RawDriveFile>(&url).await?;
        let mut meta = raw.into_meta(etag);
        if meta.trashed {
            return Err(GdriveBackendError::new(
                ErrorCode::NotFound,
                format!("Drive file `{file_id}` is trashed"),
            ));
        }
        if meta.etag.is_none() {
            meta.etag = meta.head_revision_id.clone();
        }
        Ok(meta)
    }

    async fn list_folder(&self, folder_id: &str) -> Result<Vec<DriveFileMeta>, GdriveBackendError> {
        if !valid_drive_id(folder_id) {
            return Err(GdriveBackendError::new(
                ErrorCode::BadRequest,
                "invalid Google Drive folder id",
            ));
        }
        let q = format!(
            "'{}' in parents and trashed = false",
            escape_drive_q(folder_id)
        );
        let mut page_token: Option<String> = None;
        let mut out = Vec::new();
        for _ in 0..MAX_SYNC_PAGES {
            let mut url = format!(
                "{}/files?q={}&pageSize=100&fields={}&supportsAllDrives=true&includeItemsFromAllDrives=true",
                self.api_base_url,
                percent_encode(&q),
                percent_encode(&format!("nextPageToken,files({FILE_FIELDS})")),
            );
            if let Some(tok) = page_token.as_deref() {
                url.push_str("&pageToken=");
                url.push_str(&percent_encode(tok));
            }
            let (page, _) = self.get_json::<RawFileListResponse>(&url).await?;
            for item in page.files {
                let meta = item.into_meta(None);
                if !meta.trashed {
                    out.push(meta);
                }
            }
            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(out)
    }

    async fn read_binary_range(
        &self,
        meta: &DriveFileMeta,
        offset: u64,
        count: u32,
        expected_etag: Option<&str>,
        _expected_revision: Option<&str>,
    ) -> Result<DriveRangeData, GdriveBackendError> {
        if count == 0 {
            return Ok(DriveRangeData {
                bytes: Vec::new(),
                etag: meta.etag.clone(),
                revision_id: meta.head_revision_id.clone(),
                total_size: meta.size,
            });
        }
        if let Some(total) = meta.size
            && offset >= total
        {
            return Ok(DriveRangeData {
                bytes: Vec::new(),
                etag: meta.etag.clone(),
                revision_id: meta.head_revision_id.clone(),
                total_size: Some(total),
            });
        }

        let bounded = (count as usize).min(MAX_RANGE_READ_BYTES) as u64;
        let mut end_inclusive = offset.saturating_add(bounded.saturating_sub(1));
        if let Some(total) = meta.size
            && total > 0
        {
            end_inclusive = end_inclusive.min(total - 1);
        }
        let range_header = format!("bytes={offset}-{end_inclusive}");
        let url = format!(
            "{}/files/{}?alt=media&supportsAllDrives=true",
            self.api_base_url,
            percent_encode(&meta.id)
        );
        let if_match_value = expected_etag.map(ToOwned::to_owned);
        let (response, token) = self
            .send_authed(|client, tok| {
                let mut req = client
                    .get(&url)
                    .bearer_auth(tok)
                    .header(RANGE, &range_header);
                if let Some(etag) = if_match_value.as_deref() {
                    req = req.header(IF_MATCH, etag);
                }
                req
            })
            .await?;

        let status = response.status();
        let headers = response.headers().clone();
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(DriveRangeData {
                bytes: Vec::new(),
                etag: meta.etag.clone(),
                revision_id: meta.head_revision_id.clone(),
                total_size: meta.size,
            });
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(map_http_error(
                status,
                &headers,
                &body,
                Some(&token.access_token),
            ));
        }

        let resp_etag = headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        if let (Some(exp), Some(actual)) = (expected_etag, resp_etag.as_deref())
            && exp != actual
        {
            return Err(GdriveBackendError::new(
                ErrorCode::Io,
                format!("Drive file `{}` ETag changed during range read", meta.id),
            ));
        }

        let body = response.bytes().await.map_err(|err| {
            GdriveBackendError::redacted(
                ErrorCode::Io,
                format!("failed reading Drive binary chunk: {err}"),
                Some(&token.access_token),
            )
        })?;
        let bytes = if status == StatusCode::OK {
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(body.len());
            let end = start.saturating_add(bounded as usize).min(body.len());
            body[start..end].to_vec()
        } else {
            let end = (bounded as usize).min(body.len());
            body[..end].to_vec()
        };

        Ok(DriveRangeData {
            bytes,
            etag: resp_etag.or_else(|| meta.etag.clone()),
            revision_id: meta.head_revision_id.clone(),
            total_size: meta.size,
        })
    }

    async fn export_workspace_file(
        &self,
        meta: &DriveFileMeta,
        export_mime: &str,
    ) -> Result<Vec<u8>, GdriveBackendError> {
        if export_mime_for_workspace(&meta.mime_type) != Some(export_mime) {
            return Err(GdriveBackendError::new(
                ErrorCode::Unsupported,
                format!("unsupported Workspace export MIME `{export_mime}`"),
            ));
        }
        if let Some(size) = meta.size
            && size > MAX_EXPORT_BYTES as u64
        {
            return Err(GdriveBackendError::new(
                ErrorCode::BadRequest,
                format!(
                    "Google Workspace export for `{}` exceeds 10 MB limit ({size} > {MAX_EXPORT_BYTES} bytes)",
                    meta.id
                ),
            ));
        }
        let url = format!(
            "{}/files/{}/export?mimeType={}",
            self.api_base_url,
            percent_encode(&meta.id),
            percent_encode(export_mime)
        );
        let (mut response, token) = self
            .send_authed(|c, tok| c.get(&url).bearer_auth(tok))
            .await?;
        let status = response.status();
        let headers = response.headers().clone();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(map_http_error(
                status,
                &headers,
                &body,
                Some(&token.access_token),
            ));
        }
        if let Some(len) = headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            && len > MAX_EXPORT_BYTES as u64
        {
            return Err(GdriveBackendError::new(
                ErrorCode::BadRequest,
                format!(
                    "Google Workspace export for `{}` exceeds 10 MB limit ({len} > {MAX_EXPORT_BYTES} bytes)",
                    meta.id
                ),
            ));
        }

        let mut buf = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|err| {
            GdriveBackendError::redacted(
                ErrorCode::Io,
                format!("failed streaming Drive export body: {err}"),
                Some(&token.access_token),
            )
        })? {
            if buf.len().saturating_add(chunk.len()) > MAX_EXPORT_BYTES {
                return Err(GdriveBackendError::new(
                    ErrorCode::BadRequest,
                    format!(
                        "Google Workspace export for `{}` exceeds 10 MB limit ({MAX_EXPORT_BYTES} bytes)",
                        meta.id
                    ),
                ));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<DriveSearchResponse, GdriveBackendError> {
        let bounded = limit.clamp(1, DEFAULT_SEARCH_LIMIT);
        let drive_q = build_search_query(query);
        let url = format!(
            "{}/files?q={}&pageSize={}&fields={}&supportsAllDrives=true&includeItemsFromAllDrives=true",
            self.api_base_url,
            percent_encode(&drive_q),
            bounded + 1,
            percent_encode(&format!("files({FILE_FIELDS})")),
        );
        let (resp, _) = self.get_json::<RawFileListResponse>(&url).await?;
        let mut hits: Vec<DriveSearchHit> = resp
            .files
            .into_iter()
            .map(|f| f.into_meta(None))
            .filter(|m| !m.trashed)
            .map(|m| DriveSearchHit {
                id: m.id,
                name: m.name,
                mime_type: m.mime_type,
                is_folder: m.is_folder,
                size: m.size,
                modified_time: m.modified_time,
                web_view_link: m.web_view_link,
                parents: m.parents,
            })
            .collect();
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
        start_page_token: Option<&str>,
        pinned_folders: &[String],
    ) -> Result<DriveSyncOutcome, GdriveBackendError> {
        let mut outcome = DriveSyncOutcome {
            quota: self.fetch_quota().await.ok().flatten(),
            api_requests: 1,
            ..Default::default()
        };

        if let Some(mut token) = start_page_token.map(ToOwned::to_owned) {
            for _ in 0..MAX_SYNC_PAGES {
                let url = format!(
                    "{}/changes?pageToken={}&fields={}&supportsAllDrives=true&includeItemsFromAllDrives=true",
                    self.api_base_url,
                    percent_encode(&token),
                    percent_encode(&format!(
                        "newStartPageToken,nextPageToken,changes(fileId,removed,file({FILE_FIELDS}))"
                    )),
                );
                let (page, _) = self.get_json::<RawChangesResponse>(&url).await?;
                outcome.api_requests = outcome.api_requests.saturating_add(1);
                for change in page.changes {
                    if change.removed.unwrap_or(false) {
                        outcome.removed_file_ids.push(change.file_id);
                    } else if let Some(file) = change.file {
                        let meta = file.into_meta(None);
                        if meta.trashed {
                            outcome.removed_file_ids.push(meta.id);
                        } else {
                            outcome.updated_files.push(meta);
                        }
                    }
                }
                if let Some(next) = page.next_page_token {
                    token = next;
                } else {
                    outcome.start_page_token = page.new_start_page_token.or(Some(token));
                    break;
                }
            }
        } else {
            let url = format!(
                "{}/changes/startPageToken?supportsAllDrives=true",
                self.api_base_url
            );
            let (resp, _) = self.get_json::<RawStartPageTokenResponse>(&url).await?;
            outcome.api_requests = outcome.api_requests.saturating_add(1);
            outcome.start_page_token = Some(resp.start_page_token);
        }

        let mut pinned_children = BTreeMap::new();
        for folder_id in pinned_folders {
            let children = self.list_folder(folder_id).await?;
            outcome.api_requests = outcome.api_requests.saturating_add(1);
            pinned_children.insert(folder_id.clone(), children);
        }
        outcome.pinned_children = pinned_children;
        Ok(outcome)
    }

    async fn upload_file(
        &self,
        req: &NewDriveUploadRequest,
        bytes: &[u8],
    ) -> Result<DriveFileMeta, GdriveBackendError> {
        let auth = self.token_provider.auth_summary();
        if !auth.has_file_write_scope {
            return Err(GdriveBackendError::new(
                ErrorCode::NoAccess,
                "uploads/new requires Google Drive `drive.file` OAuth scope",
            ));
        }

        let boundary = "alan_gdrivefs_multipart_boundary";
        let metadata_json = serde_json::to_string(&serde_json::json!({
            "name": req.name.trim(),
            "mimeType": req.mime_type.trim(),
            "parents": req.parent_id.as_ref().map(|p| vec![p.clone()]).unwrap_or_default(),
        }))
        .map_err(|_| GdriveBackendError::new(ErrorCode::BadRequest, "invalid upload metadata"))?;

        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
        body.extend_from_slice(metadata_json.as_bytes());
        body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Type: {}\r\n\r\n", req.mime_type.trim()).as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let url = format!(
            "{}/files?uploadType=multipart&fields={}",
            self.upload_base_url,
            percent_encode(FILE_FIELDS)
        );
        let content_type_header = format!("multipart/related; boundary={boundary}");
        let (response, token) = self
            .send_authed(|client, tok| {
                client
                    .post(&url)
                    .bearer_auth(tok)
                    .header(CONTENT_TYPE, &content_type_header)
                    .body(body.clone())
            })
            .await?;

        let status = response.status();
        let headers = response.headers().clone();
        let etag = headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned);
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(map_http_error(
                status,
                &headers,
                &text,
                Some(&token.access_token),
            ));
        }
        let raw: RawDriveFile = serde_json::from_str(&text).map_err(|err| {
            GdriveBackendError::redacted(
                ErrorCode::Io,
                format!("invalid upload response JSON: {err}"),
                Some(&token.access_token),
            )
        })?;
        Ok(raw.into_meta(etag))
    }
}

fn build_search_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.contains(" contains ")
        || trimmed.contains('=')
        || trimmed.contains(" in ")
        || trimmed.contains("!=")
    {
        format!("({trimmed}) and trashed = false")
    } else {
        let esc = escape_drive_q(trimmed);
        format!("(name contains '{esc}' or fullText contains '{esc}') and trashed = false")
    }
}

fn escape_drive_q(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('\'', "\\'")
}

fn percent_encode(input: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

fn map_http_error(
    status: StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: &str,
    secret: Option<&str>,
) -> GdriveBackendError {
    let retry_after = headers
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok());
    let reason = extract_google_error_reason(body);
    match status {
        StatusCode::NOT_FOUND => {
            GdriveBackendError::redacted(ErrorCode::NotFound, format!("HTTP 404: {body}"), secret)
        }
        StatusCode::TOO_MANY_REQUESTS => GdriveBackendError::rate_limited(
            format!("HTTP 429 rate limited: {body}"),
            retry_after,
            secret,
        ),
        StatusCode::FORBIDDEN => match reason.as_deref() {
            Some("exportSizeLimitExceeded") => GdriveBackendError::redacted(
                ErrorCode::BadRequest,
                format!("Google Workspace export exceeds 10 MB limit: {body}"),
                secret,
            ),
            Some("rateLimitExceeded" | "userRateLimitExceeded") => {
                GdriveBackendError::rate_limited(
                    format!("HTTP 403 rate limited: {body}"),
                    retry_after,
                    secret,
                )
            }
            _ => GdriveBackendError::redacted(
                ErrorCode::NoAccess,
                format!("HTTP 403 access denied: {body}"),
                secret,
            ),
        },
        StatusCode::UNAUTHORIZED => {
            GdriveBackendError::redacted(ErrorCode::NoAccess, format!("HTTP 401: {body}"), secret)
        }
        StatusCode::PRECONDITION_FAILED => GdriveBackendError::redacted(
            ErrorCode::Io,
            format!("HTTP 412 revision/ETag mismatch: {body}"),
            secret,
        ),
        StatusCode::BAD_REQUEST => {
            GdriveBackendError::redacted(ErrorCode::BadRequest, format!("HTTP 400: {body}"), secret)
        }
        _ => GdriveBackendError::redacted(ErrorCode::Io, format!("HTTP {status}: {body}"), secret),
    }
}

fn extract_google_error_reason(body: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    parsed
        .pointer("/error/errors/0/reason")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDriveFile {
    id: String,
    name: String,
    mime_type: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    parents: Vec<String>,
    #[serde(default)]
    modified_time: Option<String>,
    #[serde(default)]
    md5_checksum: Option<String>,
    #[serde(default)]
    head_revision_id: Option<String>,
    #[serde(default)]
    web_view_link: Option<String>,
    #[serde(default)]
    trashed: bool,
}

impl RawDriveFile {
    fn into_meta(self, etag: Option<String>) -> DriveFileMeta {
        let is_folder = self.mime_type == WORKSPACE_FOLDER_MIME;
        let is_workspace_doc = is_workspace_mime(&self.mime_type) && !is_folder;
        let export_mime_type = export_mime_for_workspace(&self.mime_type).map(ToOwned::to_owned);
        let size = self.size.as_deref().and_then(|s| s.parse::<u64>().ok());
        DriveFileMeta {
            id: self.id,
            name: self.name,
            mime_type: self.mime_type,
            is_folder,
            is_workspace_doc,
            export_mime_type,
            size,
            parents: self.parents,
            modified_time: self.modified_time,
            md5_checksum: self.md5_checksum,
            head_revision_id: self.head_revision_id,
            etag,
            web_view_link: self.web_view_link,
            trashed: self.trashed,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFileListResponse {
    #[serde(default)]
    next_page_token: Option<String>,
    #[serde(default)]
    files: Vec<RawDriveFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAboutResponse {
    #[serde(default)]
    storage_quota: Option<RawStorageQuota>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStorageQuota {
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    usage: Option<String>,
    #[serde(default)]
    usage_in_drive: Option<String>,
    #[serde(default)]
    usage_in_trash: Option<String>,
}

impl From<RawStorageQuota> for DriveQuotaSummary {
    fn from(raw: RawStorageQuota) -> Self {
        Self {
            limit_bytes: raw.limit.and_then(|s| s.parse().ok()),
            usage_bytes: raw.usage.and_then(|s| s.parse().ok()),
            usage_in_drive_bytes: raw.usage_in_drive.and_then(|s| s.parse().ok()),
            usage_in_trash_bytes: raw.usage_in_trash.and_then(|s| s.parse().ok()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStartPageTokenResponse {
    start_page_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChangesResponse {
    #[serde(default)]
    new_start_page_token: Option<String>,
    #[serde(default)]
    next_page_token: Option<String>,
    #[serde(default)]
    changes: Vec<RawChange>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawChange {
    file_id: String,
    #[serde(default)]
    removed: Option<bool>,
    #[serde(default)]
    file: Option<RawDriveFile>,
}
