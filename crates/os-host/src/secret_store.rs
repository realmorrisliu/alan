//! Host credential-store adapter used while materializing callable connections.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use alan_agent_engine::{Config, LlmProvider};
use alan_service_manager::{ConnectionsFile, ResolvedConnectionProfile};
use anyhow::Context;
use serde::{Deserialize, Serialize};

const SECRET_STORE_FILE_NAME: &str = "secrets.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SecretStoreFile {
    #[serde(default)]
    secrets: BTreeMap<String, String>,
    // Retain explicit logout across legacy native-store fallback and process restarts.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    revoked: BTreeSet<String>,
}

/// Explicit Host credential-store binding. Connection Service receives only
/// metadata and opaque references; secret bytes never enter its file tree.
#[derive(Clone)]
pub struct SecretStore {
    credentials_dir: PathBuf,
    resolved_secrets: BTreeMap<String, String>,
}

impl std::fmt::Debug for SecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretStore")
            .field("credentials_dir", &self.credentials_dir)
            .field("resolved_secret_count", &self.resolved_secrets.len())
            .finish()
    }
}

impl SecretStore {
    pub fn from_directory(credentials_dir: &Path) -> anyhow::Result<Self> {
        validate_safe_absolute_path("Host credential directory", credentials_dir)?;
        Ok(Self {
            credentials_dir: credentials_dir.to_path_buf(),
            resolved_secrets: BTreeMap::new(),
        })
    }

    pub fn with_resolved_secret(
        credentials_dir: &Path,
        credential_id: &str,
        secret: String,
    ) -> anyhow::Result<Self> {
        let mut store = Self::from_directory(credentials_dir)?;
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        store
            .resolved_secrets
            .insert(credential_id.to_string(), secret);
        Ok(store)
    }

    /// Validate credential storage without changing secrets or consulting native providers.
    pub fn validate_for_migration(&self) -> anyhow::Result<()> {
        let stored = self
            .read_secret_file()
            .map_err(|_| anyhow::anyhow!("invalid migration credential store"))?;
        for (id, secret) in &stored.secrets {
            validated_identifier_component("credential id", id)?;
            anyhow::ensure!(
                !secret.trim().is_empty() && !stored.revoked.contains(id),
                "invalid stored credential state"
            );
        }
        for id in &stored.revoked {
            validated_identifier_component("credential id", id)?;
        }
        Ok(())
    }

    /// Require local material or an explicit revocation, preserving logged-out state.
    pub fn validate_credential_for_migration(&self, credential_id: &str) -> anyhow::Result<()> {
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        self.validate_for_migration()?;
        let stored = self
            .read_secret_file()
            .map_err(|_| anyhow::anyhow!("invalid migration credential store"))?;
        anyhow::ensure!(
            stored.secrets.contains_key(credential_id) || stored.revoked.contains(credential_id),
            "credential backing is missing"
        );
        Ok(())
    }

    pub fn load(&self, credential_id: &str) -> anyhow::Result<Option<String>> {
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        let secrets = self.read_secret_file()?;
        if secrets.revoked.contains(credential_id) {
            return Ok(None);
        }
        Ok(secrets
            .secrets
            .get(credential_id)
            .or_else(|| self.resolved_secrets.get(credential_id))
            .cloned())
    }

    pub fn save(&self, credential_id: &str, secret: &str) -> anyhow::Result<()> {
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        let _lock = self.lock()?;
        let mut secrets = self.read_secret_file()?;
        secrets
            .secrets
            .insert(credential_id.to_string(), secret.trim().to_string());
        secrets.revoked.remove(credential_id);
        self.write_secret_file(&secrets)
    }

    pub fn delete(&self, credential_id: &str) -> anyhow::Result<bool> {
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        let _lock = self.lock()?;
        let mut secrets = self.read_secret_file()?;
        let removed = secrets.secrets.remove(credential_id).is_some()
            || (!secrets.revoked.contains(credential_id)
                && self.resolved_secrets.contains_key(credential_id));
        let newly_revoked = secrets.revoked.insert(credential_id.to_string());
        let changed = removed || newly_revoked;
        if changed {
            self.write_secret_file(&secrets)?;
        }
        Ok(changed)
    }

    fn lock(&self) -> anyhow::Result<std::fs::File> {
        std::fs::create_dir_all(&self.credentials_dir)?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let lock = options.open(self.credentials_dir.join("secrets.toml.lock"))?;
        anyhow::ensure!(
            lock.metadata()?.is_file(),
            "secret store lock is not a regular file"
        );
        lock.lock()?;
        Ok(lock)
    }

    fn secret_file_path(&self) -> anyhow::Result<PathBuf> {
        let path = self.credentials_dir.join(SECRET_STORE_FILE_NAME);
        validate_safe_absolute_path("secret store path", &path)?;
        Ok(path)
    }

    fn read_secret_file(&self) -> anyhow::Result<SecretStoreFile> {
        let path = self.secret_file_path()?;
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content)
                .with_context(|| format!("failed to parse secret store {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(SecretStoreFile::default())
            }
            Err(error) => Err(error)
                .with_context(|| format!("failed to read secret store {}", path.display())),
        }
    }

    fn write_secret_file(&self, secret_file: &SecretStoreFile) -> anyhow::Result<()> {
        let path = self.secret_file_path()?;
        if secret_file.secrets.is_empty() && secret_file.revoked.is_empty() {
            match std::fs::remove_file(&path) {
                Ok(()) => std::fs::File::open(&self.credentials_dir)?.sync_all()?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("failed to remove secret store"),
            }
            return Ok(());
        }
        let rendered = toml::to_string_pretty(secret_file)
            .context("failed to encode secret store while saving")?;
        let mut staged = tempfile::NamedTempFile::new_in(&self.credentials_dir)?;
        staged.write_all(rendered.as_bytes())?;
        staged.as_file().sync_all()?;
        staged
            .persist(&path)
            .context("failed to replace secret store")?;
        std::fs::File::open(&self.credentials_dir)?.sync_all()?;
        Ok(())
    }
}

/// Resolve Connection Service metadata, then project Host-owned credential
/// material into the runtime provider config without exposing it to the service.
pub fn apply_profile_to_config(
    connections: &ConnectionsFile,
    profile_id: Option<&str>,
    secret_store: &SecretStore,
    config: &mut Config,
) -> anyhow::Result<ResolvedConnectionProfile> {
    let resolved = connections.apply_profile_metadata_to_config(profile_id, config)?;
    if matches!(
        resolved.provider,
        LlmProvider::Chatgpt | LlmProvider::GoogleGeminiGenerateContent
    ) {
        return Ok(resolved);
    }
    let Some(credential_id) = resolved.credential_id.as_deref() else {
        anyhow::bail!("Profile `{}` is missing a credential", resolved.profile_id);
    };
    let api_key = secret_store.load(credential_id)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Credential `{credential_id}` for profile `{}` is missing a secret",
            resolved.profile_id
        )
    })?;
    match resolved.provider {
        LlmProvider::TypesafeEvaluation => config.typesafe_api_key = Some(api_key),
        LlmProvider::OpenAiResponses => config.openai_responses_api_key = Some(api_key),
        LlmProvider::OpenAiChatCompletions => {
            config.openai_chat_completions_api_key = Some(api_key)
        }
        LlmProvider::OpenAiChatCompletionsCompatible => {
            config.openai_chat_completions_compatible_api_key = Some(api_key)
        }
        LlmProvider::OpenRouter => config.openrouter_api_key = Some(api_key),
        LlmProvider::AnthropicMessages => config.anthropic_messages_api_key = Some(api_key),
        LlmProvider::Chatgpt | LlmProvider::GoogleGeminiGenerateContent => unreachable!(),
    }
    Ok(resolved)
}

fn validated_identifier_component<'a>(label: &str, value: &'a str) -> anyhow::Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.contains("..")
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        anyhow::bail!("invalid {label} `{value}`");
    }
    Ok(trimmed)
}

fn validate_safe_absolute_path(label: &str, path: &Path) -> anyhow::Result<()> {
    if !path.is_absolute() {
        anyhow::bail!("{label} must be absolute: {}", path.display());
    }
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        anyhow::bail!(
            "{label} must not contain relative components: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use alan_service_manager::{
        ConnectionCredential, ConnectionProfile, CredentialKind, default_credential_backend,
    };

    use super::*;

    #[test]
    fn secret_store_round_trips_secret() {
        let temp = tempfile::tempdir().unwrap();
        let store = SecretStore::from_directory(temp.path()).unwrap();
        store.save("kimi", "sk-test").unwrap();
        assert_eq!(store.load("kimi").unwrap().as_deref(), Some("sk-test"));
        assert!(store.delete("kimi").unwrap());
        assert_eq!(store.load("kimi").unwrap(), None);
    }

    #[test]
    fn independent_processes_preserve_secret_updates() {
        const CHILD_ROOT: &str = "ALAN_SECRET_TEST_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let store = SecretStore::from_directory(Path::new(&root)).unwrap();
            let prefix = std::env::var("ALAN_SECRET_TEST_PREFIX").unwrap();
            for index in 0..16 {
                store
                    .save(&format!("{prefix}-{index}"), "test-secret")
                    .unwrap();
            }
            for index in 0..8 {
                assert!(store.delete(&format!("{prefix}-{index}")).unwrap());
            }
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let children = (0..4)
            .map(|index| {
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "secret_store::tests::independent_processes_preserve_secret_updates",
                    ])
                    .env(CHILD_ROOT, temp.path())
                    .env("ALAN_SECRET_TEST_PREFIX", format!("writer-{index}"))
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        let store = SecretStore::from_directory(temp.path()).unwrap();
        assert_eq!(store.read_secret_file().unwrap().secrets.len(), 32);
        for writer in 0..4 {
            for index in 8..16 {
                assert_eq!(
                    store
                        .load(&format!("writer-{writer}-{index}"))
                        .unwrap()
                        .as_deref(),
                    Some("test-secret")
                );
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(store.secret_file_path().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn explicit_updates_and_logout_override_legacy_native_secrets() {
        let temp = tempfile::tempdir().unwrap();
        let fallback =
            || SecretStore::with_resolved_secret(temp.path(), "native", "legacy".into()).unwrap();
        let reader = fallback();
        let writer = SecretStore::from_directory(temp.path()).unwrap();
        assert_eq!(reader.load("native").unwrap().as_deref(), Some("legacy"));
        writer.save("native", "replacement").unwrap();
        assert_eq!(
            reader.load("native").unwrap().as_deref(),
            Some("replacement")
        );
        assert!(writer.delete("native").unwrap());
        assert_eq!(reader.load("native").unwrap(), None);
        assert_eq!(fallback().load("native").unwrap(), None);
        writer.save("native", "renewed").unwrap();
        assert_eq!(
            fallback().load("native").unwrap().as_deref(),
            Some("renewed")
        );
        assert!(reader.delete("native").unwrap());
        assert!(!reader.delete("native").unwrap());
    }

    #[test]
    fn logout_reports_a_new_revocation_of_a_native_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let live =
            SecretStore::with_resolved_secret(temp.path(), "native", "legacy".into()).unwrap();
        let logout = SecretStore::from_directory(temp.path()).unwrap();

        assert_eq!(live.load("native").unwrap().as_deref(), Some("legacy"));
        assert!(logout.delete("native").unwrap());
        assert_eq!(live.load("native").unwrap(), None);
        assert!(!logout.delete("native").unwrap());
    }

    #[test]
    fn resolved_host_secret_is_not_exposed_by_debug_output() {
        let temp = tempfile::tempdir().unwrap();
        let store =
            SecretStore::with_resolved_secret(temp.path(), "native", "host-secret".to_string())
                .unwrap();

        assert_eq!(
            store.load("native").unwrap().as_deref(),
            Some("host-secret")
        );
        assert_eq!(store.load("missing").unwrap(), None);
        assert!(!format!("{store:?}").contains("host-secret"));
    }

    #[test]
    fn secret_store_uses_only_the_explicit_host_credentials_directory() {
        let temp = tempfile::tempdir().unwrap();
        let credentials = temp.path().join("host-store/dev/credentials");
        let store = SecretStore::from_directory(&credentials).unwrap();

        store.save("dev-profile", "sk-dev").unwrap();

        assert!(
            std::fs::read_to_string(credentials.join("secrets.toml"))
                .unwrap()
                .contains("sk-dev")
        );
        assert!(
            !temp
                .path()
                .join("host-store/stable/credentials/secrets.toml")
                .exists()
        );
    }

    #[test]
    fn host_projection_combines_profile_metadata_with_secret_material() {
        let temp = tempfile::tempdir().unwrap();
        let store = SecretStore::from_directory(temp.path()).unwrap();
        store.save("openrouter-main", "sk-or").unwrap();
        let connections = ConnectionsFile {
            credentials: BTreeMap::from([(
                "openrouter-main".to_string(),
                ConnectionCredential {
                    kind: CredentialKind::SecretString,
                    provider_family: LlmProvider::OpenRouter,
                    label: "OpenRouter credential".to_string(),
                    backend: default_credential_backend(CredentialKind::SecretString).to_string(),
                },
            )]),
            profiles: BTreeMap::from([(
                "openrouter-main".to_string(),
                ConnectionProfile {
                    provider: LlmProvider::OpenRouter,
                    label: Some("OpenRouter".to_string()),
                    credential_id: Some("openrouter-main".to_string()),
                    created_at: "2026-01-01T00:00:00Z".parse().unwrap(),
                    updated_at: "2026-01-01T00:00:00Z".parse().unwrap(),
                    source: "managed".to_string(),
                    settings: BTreeMap::from([
                        ("http_referer".to_string(), "https://alan.local".to_string()),
                        ("x_title".to_string(), "alan".to_string()),
                        (
                            "app_categories".to_string(),
                            "cli-agent,devtool".to_string(),
                        ),
                    ]),
                },
            )]),
            ..ConnectionsFile::default()
        };
        let mut config = Config::default();

        let resolved =
            apply_profile_to_config(&connections, Some("openrouter-main"), &store, &mut config)
                .unwrap();

        assert_eq!(resolved.provider, LlmProvider::OpenRouter);
        assert_eq!(config.llm_provider, LlmProvider::OpenRouter);
        assert_eq!(config.openrouter_api_key.as_deref(), Some("sk-or"));
        assert_eq!(config.openrouter_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(config.openrouter_model, "moonshotai/kimi-k2.6");
        assert_eq!(
            config.openrouter_http_referer.as_deref(),
            Some("https://alan.local")
        );
        assert_eq!(config.openrouter_x_title.as_deref(), Some("alan"));
        assert_eq!(
            config.openrouter_app_categories,
            vec!["cli-agent", "devtool"]
        );
    }
}
