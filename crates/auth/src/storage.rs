use crate::token_data::{ChatgptTokenData, parse_jwt_expiration};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, PathBuf};

const AUTH_STORAGE_FILE_NAME: &str = "auth.json";

/// Refresh an access token this many seconds before its JWT `exp`, so a request is
/// never dispatched with credentials that expire in flight.
pub const ACCESS_TOKEN_REFRESH_WINDOW_SECS: i64 = 5 * 60;

/// Refresh an access token without a readable expiry once it is this old.
const OPAQUE_ACCESS_TOKEN_MAX_AGE_SECS: i64 = 8 * 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredChatgptAuth {
    pub tokens: ChatgptTokenData,
    pub account_id: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh_at: Option<DateTime<Utc>>,
}

pub(crate) fn is_issued_client_id(client_id: &str) -> bool {
    let trimmed = client_id.trim();
    !trimmed.is_empty() && trimmed != crate::chatgpt::DYNAMIC_CLIENT_ID
}

impl StoredChatgptAuth {
    /// Whether this credential carries a real OAuth `client_id` issued by SIWC
    /// (excluding empty values and the pre-registration placeholder).
    pub fn has_issued_client_id(&self) -> bool {
        is_issued_client_id(&self.client_id)
    }

    pub fn from_tokens(
        tokens: ChatgptTokenData,
        client_id: String,
        scopes: Vec<String>,
    ) -> io::Result<Self> {
        let client_id = client_id.trim().to_string();
        if !is_issued_client_id(&client_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ChatGPT auth requires an issued OAuth client_id",
            ));
        }
        let account_id = tokens
            .id_token
            .effective_account_id()
            .map(ToString::to_string)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ChatGPT id token did not include a subject or account identity",
                )
            })?;

        Ok(Self {
            email: tokens.id_token.email.clone(),
            plan_type: tokens.id_token.plan_type.clone(),
            user_id: tokens.id_token.user_id.clone(),
            access_token_expires_at: parse_jwt_expiration(&tokens.access_token).ok().flatten(),
            tokens,
            account_id,
            client_id,
            scopes,
            last_refresh_at: Some(Utc::now()),
        })
    }

    /// Effective access-token expiry from stored metadata and the JWT `exp` claim.
    pub fn effective_access_token_expires_at(&self) -> Option<DateTime<Utc>> {
        let jwt_exp = parse_jwt_expiration(&self.tokens.access_token)
            .ok()
            .flatten();
        match (self.access_token_expires_at, jwt_exp) {
            (Some(stored), Some(claim)) => Some(stored.min(claim)),
            (stored, claim) => stored.or(claim),
        }
    }

    pub fn is_access_token_expired(&self, now: DateTime<Utc>) -> bool {
        self.effective_access_token_expires_at()
            .is_some_and(|exp| exp <= now)
    }

    /// Whether a request at `now` must refresh before dispatch: the access token is
    /// expired or expires within [`ACCESS_TOKEN_REFRESH_WINDOW_SECS`]. An opaque token
    /// without a JWT `exp` falls back to the age of the last refresh.
    pub fn should_refresh(&self, now: DateTime<Utc>) -> bool {
        match self.effective_access_token_expires_at() {
            Some(exp) => exp <= now + chrono::Duration::seconds(ACCESS_TOKEN_REFRESH_WINDOW_SECS),
            None => self.last_refresh_at.is_some_and(|refreshed| {
                refreshed + chrono::Duration::seconds(OPAQUE_ACCESS_TOKEN_MAX_AGE_SECS) <= now
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredGoogleAuth {
    pub refresh_token: String,
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh_at: Option<DateTime<Utc>>,
}

impl StoredGoogleAuth {
    /// Whether a request at `now` must refresh before dispatch: the access token is
    /// expired or expires within [`ACCESS_TOKEN_REFRESH_WINDOW_SECS`], or lacks an expiry.
    pub fn should_refresh(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(exp) => exp <= now + chrono::Duration::seconds(ACCESS_TOKEN_REFRESH_WINDOW_SECS),
            None => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthStore {
    #[serde(default)]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext_agent_host_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_chatgpt_auth"
    )]
    pub chatgpt: Option<StoredChatgptAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google: Option<StoredGoogleAuth>,
}

/// Treat pre-SIWC ChatGPT grants (missing an issued `client_id`) as logged out so
/// existing auth stores remain readable without manual file deletion while `google`
/// and `ext_agent_host_id` stay intact.
fn deserialize_chatgpt_auth<'de, D>(deserializer: D) -> Result<Option<StoredChatgptAuth>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<StoredChatgptAuth>::deserialize(deserializer)?
        .filter(StoredChatgptAuth::has_issued_client_id))
}

#[derive(Debug, Clone)]
pub struct AuthStorage {
    root_dir: PathBuf,
}

impl AuthStorage {
    pub fn new(path: PathBuf) -> io::Result<Self> {
        Ok(Self {
            root_dir: normalize_auth_storage_root_dir(path)?,
        })
    }

    pub fn path(&self) -> io::Result<PathBuf> {
        self.resolve_path()
    }

    pub fn load(&self) -> io::Result<AuthStore> {
        let path = self.resolve_path()?;
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AuthStore {
                version: 1,
                ext_agent_host_id: None,
                chatgpt: None,
                google: None,
            }),
            Err(error) => Err(error),
        }
    }

    /// Return the persistent installation-level `ext_agent_host_id` (`urn:uuid:<uuid>`),
    /// generating and saving one atomically if absent.
    pub fn ensure_ext_agent_host_id(&self) -> io::Result<String> {
        let _guard = self.lock("auth.json.lock")?;
        let mut store = self.load()?;
        if let Some(existing) = store
            .ext_agent_host_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            return Ok(existing.to_string());
        }
        let generated = new_ext_agent_host_id();
        store.version = 1;
        store.ext_agent_host_id = Some(generated.clone());
        self.save_locked(&store)?;
        Ok(generated)
    }

    /// Validate an existing managed-auth file for offline installation adoption.
    /// Expired access tokens remain valid migration input; no refresh is attempted.
    pub fn validate_for_migration(&self) -> io::Result<()> {
        let path = self.resolve_path()?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(io::Error::other("managed auth is not a regular file"));
        }
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported or invalid managed auth schema",
            )
        };
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path)?).map_err(|_| invalid())?;
        validate_fields(
            &value,
            &["version", "ext_agent_host_id", "chatgpt", "google"],
        )?;
        if value.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
            return Err(invalid());
        }
        if let Some(auth) = value.get("chatgpt").filter(|value| !value.is_null()) {
            validate_fields(
                auth,
                &[
                    "tokens",
                    "account_id",
                    "client_id",
                    "scopes",
                    "email",
                    "plan_type",
                    "user_id",
                    "access_token_expires_at",
                    "last_refresh_at",
                ],
            )?;
            validate_fields(
                &auth["tokens"],
                &["id_token", "access_token", "refresh_token"],
            )?;
        }
        if let Some(auth) = value.get("google").filter(|value| !value.is_null()) {
            validate_fields(
                auth,
                &[
                    "refresh_token",
                    "access_token",
                    "expires_at",
                    "client_id",
                    "client_secret",
                    "token_uri",
                    "email",
                    "last_refresh_at",
                ],
            )?;
        }
        let store: AuthStore = serde_json::from_value(value).map_err(|_| invalid())?;
        if let Some(auth) = store.chatgpt
            && (auth.account_id.is_empty()
                || !auth.has_issued_client_id()
                || auth.tokens.id_token.effective_account_id() != Some(auth.account_id.as_str())
                || auth.tokens.access_token.is_empty()
                || auth.tokens.refresh_token.is_empty())
        {
            return Err(invalid());
        }
        if let Some(auth) = store.google
            && (auth.access_token.is_empty() || auth.refresh_token.is_empty())
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub(crate) fn lock(&self, name: &str) -> io::Result<fs::File> {
        let root = self.resolve_root_dir()?;
        fs::create_dir_all(&root)?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let lock = options.open(root.join(name))?;
        if !lock.metadata()?.is_file() {
            return Err(io::Error::other("auth lock is not a regular file"));
        }
        lock.lock()?;
        Ok(lock)
    }

    pub(crate) fn save_if_unchanged(
        &self,
        expected: &AuthStore,
        store: &AuthStore,
    ) -> io::Result<()> {
        let _guard = self.lock("auth.json.lock")?;
        if self.load()? != *expected {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "auth state changed during refresh",
            ));
        }
        self.save_locked(store)
    }

    pub fn save(&self, store: &AuthStore) -> io::Result<()> {
        let _guard = self.lock("auth.json.lock")?;
        self.save_locked(store)
    }

    /// Atomically load, mutate, and persist the auth store under `auth.json.lock`.
    pub fn update(&self, mutate: impl FnOnce(&mut AuthStore)) -> io::Result<AuthStore> {
        let _guard = self.lock("auth.json.lock")?;
        let mut store = self.load()?;
        store.version = 1;
        mutate(&mut store);
        self.save_locked(&store)?;
        Ok(store)
    }

    fn save_locked(&self, store: &AuthStore) -> io::Result<()> {
        let root_dir = self.resolve_root_dir()?;
        fs::create_dir_all(&root_dir)?;
        let root_dir = fs::canonicalize(&root_dir)?;
        let path = root_dir.join(AUTH_STORAGE_FILE_NAME);
        ensure_path_within_root(&path, &root_dir)?;

        let raw = serde_json::to_vec_pretty(store).map_err(io::Error::other)?;
        let mut staged = tempfile::NamedTempFile::new_in(&root_dir)?;
        staged.write_all(&raw)?;
        staged.write_all(b"\n")?;
        staged.as_file().sync_all()?;
        staged.persist(&path).map_err(|error| error.error)?;
        fs::File::open(&root_dir)?.sync_all()?;

        Ok(())
    }

    pub fn clear_chatgpt(&self) -> io::Result<()> {
        let _guard = self.lock("auth.json.lock")?;
        let mut store = self.load()?;
        store.chatgpt = None;
        self.save_locked(&store)
    }

    pub fn clear_google(&self) -> io::Result<()> {
        let _guard = self.lock("auth.json.lock")?;
        let mut store = self.load()?;
        store.google = None;
        self.save_locked(&store)
    }

    fn resolve_path(&self) -> io::Result<PathBuf> {
        let root_dir = self.resolve_root_dir()?;
        let path = root_dir.join(AUTH_STORAGE_FILE_NAME);
        ensure_path_within_root(&path, &root_dir)?;
        Ok(path)
    }

    fn resolve_root_dir(&self) -> io::Result<PathBuf> {
        match fs::canonicalize(&self.root_dir) {
            Ok(path) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(self.root_dir.clone()),
            Err(error) => Err(error),
        }
    }
}

fn new_ext_agent_host_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "urn:uuid:{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

fn validate_fields(value: &serde_json::Value, allowed: &[&str]) -> io::Result<()> {
    if value
        .as_object()
        .is_some_and(|fields| fields.keys().all(|key| allowed.contains(&key.as_str())))
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported managed auth fields",
        ))
    }
}

fn normalize_auth_storage_root_dir(path: PathBuf) -> io::Result<PathBuf> {
    if path.file_name() != Some(OsStr::new(AUTH_STORAGE_FILE_NAME)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ChatGPT auth storage path must end with auth.json",
        ));
    }

    let root_dir = path.parent().map(PathBuf::from).unwrap_or_default();

    if root_dir
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ChatGPT auth storage path cannot contain parent directory traversal",
        ));
    }

    let root_dir = if root_dir.is_absolute() {
        root_dir
    } else {
        std::env::current_dir()?.join(root_dir)
    };

    Ok(root_dir)
}

fn ensure_path_within_root(path: &std::path::Path, root_dir: &std::path::Path) -> io::Result<()> {
    if path.starts_with(root_dir) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ChatGPT auth storage path escaped the configured storage root",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthStorage, AuthStore, StoredChatgptAuth};
    use crate::token_data::{ChatgptIdTokenInfo, ChatgptTokenData};
    use base64::Engine;
    use serde_json::json;
    use std::fs;
    use std::io::ErrorKind;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn build_jwt(payload: serde_json::Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
        format!("{header}.{payload}.sig")
    }

    #[test]
    fn migration_validation_is_read_only_and_rejects_unknown_schema_without_secret_errors() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("auth.json");
        let storage = AuthStorage::new(path.clone()).unwrap();
        for bytes in [r#"{"version":1}"#, r#"{"version":1,"chatgpt":null}"#] {
            fs::write(&path, bytes).unwrap();
            storage.validate_for_migration().unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
        }
        for bytes in [
            r#"{"version":2}"#,
            r#"{"version":1,"future":"secret"}"#,
            r#"{"version":1,"chatgpt":"secret"}"#,
            "secret",
        ] {
            fs::write(&path, bytes).unwrap();
            let error = storage.validate_for_migration().unwrap_err();
            assert!(!error.to_string().contains("secret"));
            assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn storage_round_trip() {
        let temp_dir = TempDir::new().expect("temp dir");
        let storage = AuthStorage::new(temp_dir.path().join("auth.json")).expect("storage");
        let id_token = build_jwt(json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "pro",
                "chatgpt_user_id": "user_123",
                "chatgpt_account_id": "acct_123"
            }
        }));
        let access_token = build_jwt(json!({"exp": 4_102_444_800_i64}));
        let auth = StoredChatgptAuth::from_tokens(
            ChatgptTokenData {
                id_token: ChatgptIdTokenInfo {
                    email: Some("user@example.com".to_string()),
                    plan_type: Some("pro".to_string()),
                    user_id: Some("user_123".to_string()),
                    account_id: Some("acct_123".to_string()),
                    subject: None,
                    nonce: None,
                    raw_jwt: id_token,
                },
                access_token,
                refresh_token: "refresh".to_string(),
            },
            "oaiapp_123".to_string(),
            vec![
                "openid".to_string(),
                "chatgpt.tokens.use.direct".to_string(),
            ],
        )
        .expect("stored auth");
        storage
            .save(&AuthStore {
                version: 1,
                ext_agent_host_id: Some("urn:uuid:11111111-1111-4111-8111-111111111111".into()),
                chatgpt: Some(auth.clone()),
                google: None,
            })
            .expect("save");

        storage
            .validate_for_migration()
            .expect("offline validation");
        let loaded = storage.load().expect("load");
        assert_eq!(loaded.chatgpt, Some(auth));
    }

    #[test]
    fn storage_rejects_parent_directory_traversal() {
        let error = AuthStorage::new(PathBuf::from("../auth.json")).expect_err("invalid path");
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn stored_auth_accepts_opaque_access_tokens() {
        let id_token = build_jwt(serde_json::json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct_123"
            }
        }));

        let auth = StoredChatgptAuth::from_tokens(
            ChatgptTokenData {
                id_token: ChatgptIdTokenInfo {
                    email: Some("user@example.com".to_string()),
                    plan_type: None,
                    user_id: None,
                    account_id: Some("acct_123".to_string()),
                    subject: None,
                    nonce: None,
                    raw_jwt: id_token,
                },
                access_token: "opaque-access-token".to_string(),
                refresh_token: "refresh".to_string(),
            },
            "oaiapp_123".to_string(),
            vec!["chatgpt.tokens.use.direct".to_string()],
        )
        .expect("stored auth");

        assert_eq!(auth.access_token_expires_at, None);
    }

    #[test]
    fn should_refresh_uses_safety_window_and_opaque_age_fallback() {
        let now = chrono::Utc::now();
        let id_token = ChatgptIdTokenInfo {
            account_id: Some("acct_123".to_string()),
            raw_jwt: build_jwt(json!({})),
            ..Default::default()
        };
        let auth = |access_token: String| {
            StoredChatgptAuth::from_tokens(
                ChatgptTokenData {
                    id_token: id_token.clone(),
                    access_token,
                    refresh_token: "refresh".to_string(),
                },
                "oaiapp_123".to_string(),
                vec!["chatgpt.tokens.use.direct".to_string()],
            )
            .unwrap()
        };
        let window = super::ACCESS_TOKEN_REFRESH_WINDOW_SECS;
        let exp = |offset: i64| build_jwt(json!({"exp": now.timestamp() + offset}));
        assert!(auth(exp(-1)).should_refresh(now));
        assert!(auth(exp(window - 10)).should_refresh(now));
        assert!(!auth(exp(window + 60)).should_refresh(now));

        let mut opaque = auth("opaque".to_string());
        assert!(!opaque.should_refresh(now));
        opaque.last_refresh_at = Some(now - chrono::Duration::days(9));
        assert!(opaque.should_refresh(now));

        // Even when stored expiry metadata is absent, JWT `exp` triggers refresh.
        let mut missing_stored_exp = auth(exp(window - 10));
        missing_stored_exp.access_token_expires_at = None;
        assert!(missing_stored_exp.should_refresh(now));

        // Stored expiry earlier than JWT `exp` also triggers refresh.
        let mut earlier_stored_exp = auth(exp(window + 3600));
        earlier_stored_exp.access_token_expires_at = Some(now + chrono::Duration::seconds(30));
        assert!(earlier_stored_exp.should_refresh(now));
    }

    #[test]
    fn ext_agent_host_id_is_persistent_across_clear_chatgpt() {
        let temp_dir = TempDir::new().expect("temp dir");
        let storage = AuthStorage::new(temp_dir.path().join("auth.json")).expect("storage");
        let first = storage.ensure_ext_agent_host_id().expect("host id");
        assert!(first.starts_with("urn:uuid:"));
        assert_eq!(first.len(), "urn:uuid:".len() + 36);
        assert_eq!(storage.ensure_ext_agent_host_id().unwrap(), first);

        storage.clear_chatgpt().unwrap();
        assert_eq!(
            storage.load().unwrap().ext_agent_host_id.as_deref(),
            Some(first.as_str())
        );
    }

    #[test]
    fn legacy_chatgpt_auth_without_client_id_is_invalidated_while_preserving_google_and_allowing_reauth()
     {
        let temp_dir = TempDir::new().expect("temp dir");
        let path = temp_dir.path().join("auth.json");
        let storage = AuthStorage::new(path.clone()).expect("storage");
        let id_token = build_jwt(json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct_legacy"
            }
        }));
        let legacy_json = json!({
            "version": 1,
            "chatgpt": {
                "tokens": {
                    "id_token": id_token,
                    "access_token": "legacy-access",
                    "refresh_token": "legacy-refresh"
                },
                "account_id": "acct_legacy",
                "email": "user@example.com"
            },
            "google": {
                "refresh_token": "g-refresh",
                "access_token": "g-access"
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&legacy_json).unwrap()).unwrap();

        let loaded = storage.load().expect("legacy store should still load");
        assert_eq!(loaded.chatgpt, None);
        assert!(loaded.google.is_some());
        storage
            .validate_for_migration()
            .expect("legacy chatgpt entry should validate without blocking migration");

        let host_id = storage
            .ensure_ext_agent_host_id()
            .expect("ensure host id for reauthentication");
        assert!(host_id.starts_with("urn:uuid:"));
        assert!(storage.load().unwrap().google.is_some());

        storage.clear_chatgpt().expect("clear obsolete chatgpt");
        assert!(storage.load().unwrap().google.is_some());
    }
}
