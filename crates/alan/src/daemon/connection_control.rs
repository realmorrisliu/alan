use super::auth_control::AuthControlState;
use crate::registry::normalize_workspace_root_path;
use alan_agent_engine::{
    AlanHomePaths, Config, ConnectionCredential, ConnectionProfile, ConnectionsFile,
    CredentialKind, LlmProvider, ProviderDescriptor, SecretStore, default_credential_backend,
    normalize_profile_settings, provider_catalog, sanitize_identifier, validate_profile_settings,
};
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, broadcast};

const DEFAULT_CONNECTION_EVENT_BROADCAST_CAPACITY: usize = 64;
const DEFAULT_CONNECTION_EVENT_REPLAY_BUFFER_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionCredentialStatusKind {
    Missing,
    Available,
    Pending,
    Expired,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionCredentialStatusDetail {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionCredentialStatus {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    pub credential_kind: CredentialKind,
    pub status: ConnectionCredentialStatusKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ConnectionCredentialStatusDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionProfileSummary {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub provider: LlmProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    pub settings: BTreeMap<String, String>,
    pub credential_status: ConnectionCredentialStatusKind,
    pub is_default: bool,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionPinScope {
    #[default]
    Global,
    Workspace,
}

impl ConnectionPinScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Workspace => "workspace",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionSelectionSource {
    None,
    DefaultProfile,
    GlobalPin,
    WorkspacePin,
}

impl ConnectionSelectionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::DefaultProfile => "default_profile",
            Self::GlobalPin => "global_pin",
            Self::WorkspacePin => "workspace_pin",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionPinState {
    pub scope: ConnectionPinScope,
    pub config_path: PathBuf,
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionCurrentState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_pin: Option<ConnectionPinState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_pin: Option<ConnectionPinState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_profile: Option<String>,
    pub effective_source: ConnectionSelectionSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectionEvent {
    ProfileCreated {},
    ProfileUpdated {},
    ProfileDeleted {},
    ProfileActivated {},
    CredentialStatusChanged {
        status: ConnectionCredentialStatus,
    },
    LoginStarted {
        login_id: String,
        method: String,
    },
    BrowserLoginReady {
        login_id: String,
        auth_url: String,
        redirect_uri: String,
    },
    DeviceCodeReady {
        login_id: String,
        verification_url: String,
        user_code: String,
        interval_secs: u64,
    },
    LoginSucceeded {
        login_id: String,
        account_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_type: Option<String>,
    },
    LoginFailed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        login_id: Option<String>,
        message: String,
        recoverable: bool,
    },
    LogoutCompleted {
        removed: bool,
    },
    ConnectionTestSucceeded {
        resolved_model: String,
        message: String,
    },
    ConnectionTestFailed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionEventEnvelope {
    pub event_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub profile_id: String,
    pub provider: LlmProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(flatten)]
    pub event: ConnectionEvent,
}

#[derive(Debug, Clone)]
pub struct ConnectionEventReplayPage {
    pub events: Vec<ConnectionEventEnvelope>,
    pub gap: bool,
    pub oldest_event_id: Option<String>,
    pub latest_event_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionEventCursor {
    pub event_id: String,
    pub sequence: u64,
}

#[derive(Debug)]
struct ConnectionEventLog {
    next_sequence: u64,
    buffer: VecDeque<ConnectionEventEnvelope>,
    capacity: usize,
}

impl ConnectionEventLog {
    fn new(capacity: usize) -> Self {
        Self {
            next_sequence: 1,
            buffer: VecDeque::with_capacity(capacity.min(16)),
            capacity: capacity.max(1),
        }
    }

    fn append(
        &mut self,
        profile_id: &str,
        provider: LlmProvider,
        credential_id: Option<String>,
        event: ConnectionEvent,
    ) -> ConnectionEventEnvelope {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        let envelope = ConnectionEventEnvelope {
            event_id: format!("conn_evt_{sequence:016}"),
            sequence,
            timestamp_ms: now_timestamp_ms(),
            profile_id: profile_id.to_string(),
            provider,
            credential_id,
            event,
        };
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(envelope.clone());
        envelope
    }

    fn replay_cursor(&self) -> ConnectionEventCursor {
        self.buffer.back().map_or(
            ConnectionEventCursor {
                event_id: format!("conn_evt_{:016}", 0),
                sequence: 0,
            },
            |envelope| ConnectionEventCursor {
                event_id: envelope.event_id.clone(),
                sequence: envelope.sequence,
            },
        )
    }

    fn read_after(&self, after_event_id: Option<&str>, limit: usize) -> ConnectionEventReplayPage {
        let limit = limit.clamp(1, 1000);
        let oldest_event_id = self.buffer.front().map(|e| e.event_id.clone());
        let latest_event_id = self.buffer.back().map(|e| e.event_id.clone());

        let Some(after_event_id) = after_event_id else {
            return ConnectionEventReplayPage {
                events: self.buffer.iter().take(limit).cloned().collect(),
                gap: false,
                oldest_event_id,
                latest_event_id,
            };
        };

        let after_sequence = parse_connection_event_sequence(after_event_id);
        if after_sequence == Some(0) {
            return ConnectionEventReplayPage {
                events: self.buffer.iter().take(limit).cloned().collect(),
                gap: false,
                oldest_event_id,
                latest_event_id,
            };
        }

        let mut gap = false;
        let start_idx = if let Some(idx) = self
            .buffer
            .iter()
            .position(|e| e.event_id == after_event_id)
        {
            idx + 1
        } else {
            if let (Some(seq), Some(oldest)) = (after_sequence, self.buffer.front())
                && seq < oldest.sequence
            {
                gap = true;
            }

            if let (Some(seq), Some(latest)) = (after_sequence, self.buffer.back()) {
                if seq >= latest.sequence {
                    self.buffer.len()
                } else {
                    gap = true;
                    0
                }
            } else {
                gap = true;
                0
            }
        };

        ConnectionEventReplayPage {
            events: self
                .buffer
                .iter()
                .skip(start_idx)
                .take(limit)
                .cloned()
                .collect(),
            gap,
            oldest_event_id,
            latest_event_id,
        }
    }
}

#[derive(Debug, Clone)]
struct LoginProfileBinding {
    profile_id: String,
    provider: LlmProvider,
    credential_id: Option<String>,
}

#[derive(Debug)]
pub struct ConnectionControlState {
    home_paths: AlanHomePaths,
    auth_control: Arc<AuthControlState>,
    events_tx: broadcast::Sender<ConnectionEventEnvelope>,
    event_log: Arc<RwLock<ConnectionEventLog>>,
    login_bindings: Arc<Mutex<HashMap<String, LoginProfileBinding>>>,
    mutate_lock: Arc<Mutex<()>>,
}

impl ConnectionControlState {
    pub fn new(home_paths: AlanHomePaths, auth_control: Arc<AuthControlState>) -> Arc<Self> {
        let (events_tx, _) = broadcast::channel(DEFAULT_CONNECTION_EVENT_BROADCAST_CAPACITY);
        let state = Arc::new(Self {
            home_paths,
            auth_control,
            events_tx,
            event_log: Arc::new(RwLock::new(ConnectionEventLog::new(
                DEFAULT_CONNECTION_EVENT_REPLAY_BUFFER_CAPACITY,
            ))),
            login_bindings: Arc::new(Mutex::new(HashMap::new())),
            mutate_lock: Arc::new(Mutex::new(())),
        });
        if tokio::runtime::Handle::try_current().is_ok() {
            state.start_auth_event_bridge();
        }
        state
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionEventEnvelope> {
        self.events_tx.subscribe()
    }

    pub async fn replay_cursor(&self) -> ConnectionEventCursor {
        self.event_log.read().await.replay_cursor()
    }

    pub async fn read_events(
        &self,
        after_event_id: Option<&str>,
        limit: usize,
    ) -> ConnectionEventReplayPage {
        self.event_log
            .read()
            .await
            .read_after(after_event_id, limit)
    }

    pub fn catalog(&self) -> Vec<ProviderDescriptor> {
        provider_catalog().to_vec()
    }

    pub async fn list_profiles(
        &self,
    ) -> anyhow::Result<(Option<String>, Vec<ConnectionProfileSummary>)> {
        let connections = self.load_connections()?;
        let mut summaries = Vec::with_capacity(connections.profiles.len());
        for (profile_id, profile) in &connections.profiles {
            let status = self
                .credential_status_from_profile(profile_id, profile)
                .await?;
            summaries.push(ConnectionProfileSummary {
                profile_id: profile_id.clone(),
                label: profile.label.clone(),
                provider: profile.provider,
                credential_id: profile.credential_id.clone(),
                settings: normalize_profile_settings(profile.provider, &profile.settings),
                credential_status: status.status,
                is_default: connections.default_profile.as_deref() == Some(profile_id.as_str()),
                source: profile.source.clone(),
                created_at: profile.created_at,
                updated_at: profile.updated_at,
            });
        }
        summaries.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        Ok((connections.default_profile, summaries))
    }

    pub async fn get_profile(&self, profile_id: &str) -> anyhow::Result<ConnectionProfileSummary> {
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        let status = self
            .credential_status_from_profile(&profile_id, profile)
            .await?;
        Ok(ConnectionProfileSummary {
            profile_id: profile_id.clone(),
            label: profile.label.clone(),
            provider: profile.provider,
            credential_id: profile.credential_id.clone(),
            settings: normalize_profile_settings(profile.provider, &profile.settings),
            credential_status: status.status,
            is_default: connections.default_profile.as_deref() == Some(profile_id.as_str()),
            source: profile.source.clone(),
            created_at: profile.created_at,
            updated_at: profile.updated_at,
        })
    }

    pub async fn create_profile(
        &self,
        profile_id: &str,
        label: Option<String>,
        provider: LlmProvider,
        credential_id: Option<String>,
        settings: BTreeMap<String, String>,
        activate: bool,
    ) -> anyhow::Result<ConnectionProfileSummary> {
        let _guard = self.mutate_lock.lock().await;
        let profile_id = sanitize_identifier(profile_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid profile id `{profile_id}`"))?;
        let mut connections = self.load_connections()?;
        if connections.profiles.contains_key(&profile_id) {
            anyhow::bail!("Connection profile `{profile_id}` already exists");
        }
        let descriptor = ConnectionsFile::profile_descriptor(provider);
        let credential_id: Option<String> =
            if descriptor.credential_kind == CredentialKind::AmbientCloudAuth {
                None
            } else {
                let chosen = credential_id.unwrap_or_else(|| profile_id.clone());
                Some(
                    sanitize_identifier(&chosen)
                        .ok_or_else(|| anyhow::anyhow!("Invalid credential id `{chosen}`"))?,
                )
            };
        if let Some(credential_id) = credential_id.as_ref() {
            let credential_label = label
                .clone()
                .unwrap_or_else(|| descriptor.display_name.to_string());
            connections
                .credentials
                .entry(credential_id.clone())
                .or_insert_with(|| ConnectionCredential {
                    kind: descriptor.credential_kind,
                    provider_family: provider,
                    label: format!("{} credential", credential_label),
                    backend: default_credential_backend(descriptor.credential_kind).to_string(),
                });
        }
        let normalized_settings = normalize_profile_settings(provider, &settings);
        validate_profile_settings(provider, &normalized_settings)?;
        let now = Utc::now();
        connections.profiles.insert(
            profile_id.clone(),
            ConnectionProfile {
                provider,
                label,
                credential_id: credential_id.clone(),
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings: normalized_settings,
            },
        );
        if activate || connections.default_profile.is_none() {
            connections.default_profile = Some(profile_id.clone());
        }
        self.save_connections(&connections)?;
        self.append_event(
            &profile_id,
            provider,
            credential_id.clone(),
            ConnectionEvent::ProfileCreated {},
        )
        .await;
        if connections.default_profile.as_deref() == Some(profile_id.as_str()) {
            self.append_event(
                &profile_id,
                provider,
                credential_id,
                ConnectionEvent::ProfileActivated {},
            )
            .await;
        }
        self.get_profile(&profile_id).await
    }

    pub async fn update_profile(
        &self,
        profile_id: &str,
        label: Option<String>,
        credential_id: Option<String>,
        settings: Option<BTreeMap<String, String>>,
    ) -> anyhow::Result<ConnectionProfileSummary> {
        let _guard = self.mutate_lock.lock().await;
        let profile_id = validated_profile_id(profile_id)?;
        let mut connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get_mut(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        if let Some(label) = label {
            profile.label = Some(label);
        }
        if let Some(credential_id) = credential_id {
            let descriptor = ConnectionsFile::profile_descriptor(profile.provider);
            let credential_id = sanitize_identifier(&credential_id)
                .ok_or_else(|| anyhow::anyhow!("Invalid credential id `{credential_id}`"))?;
            let credential_label = profile
                .label
                .clone()
                .unwrap_or_else(|| descriptor.display_name.to_string());
            connections
                .credentials
                .entry(credential_id.clone())
                .or_insert_with(|| ConnectionCredential {
                    kind: descriptor.credential_kind,
                    provider_family: profile.provider,
                    label: format!("{} credential", credential_label),
                    backend: default_credential_backend(descriptor.credential_kind).to_string(),
                });
            profile.credential_id = Some(credential_id);
        }
        if let Some(settings) = settings {
            let normalized = normalize_profile_settings(profile.provider, &settings);
            validate_profile_settings(profile.provider, &normalized)?;
            profile.settings = normalized;
        }
        profile.updated_at = Utc::now();
        let provider = profile.provider;
        let credential_id = profile.credential_id.clone();
        self.save_connections(&connections)?;
        self.append_event(
            &profile_id,
            provider,
            credential_id,
            ConnectionEvent::ProfileUpdated {},
        )
        .await;
        self.get_profile(&profile_id).await
    }

    pub async fn delete_profile(&self, profile_id: &str) -> anyhow::Result<bool> {
        let _guard = self.mutate_lock.lock().await;
        let profile_id = validated_profile_id(profile_id)?;
        let mut connections = self.load_connections()?;
        let Some(profile) = connections.profiles.remove(&profile_id) else {
            return Ok(false);
        };
        if connections.default_profile.as_deref() == Some(profile_id.as_str()) {
            connections.default_profile = None;
        }
        self.save_connections(&connections)?;
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::ProfileDeleted {},
        )
        .await;
        Ok(true)
    }

    pub async fn activate_profile(
        &self,
        profile_id: &str,
    ) -> anyhow::Result<ConnectionProfileSummary> {
        let profile_id = validated_profile_id(profile_id)?;
        self.set_default_profile(&profile_id, None).await?;
        self.get_profile(&profile_id).await
    }

    pub fn current_selection(
        &self,
        workspace_dir: Option<&Path>,
    ) -> anyhow::Result<ConnectionCurrentState> {
        let connections = self.load_connections()?;
        let normalized_workspace_dir = workspace_dir
            .map(validated_workspace_root_path)
            .transpose()?;
        let global_pin = self.read_global_pin_state()?;
        let workspace_pin = if let Some(workspace_dir) = normalized_workspace_dir.as_deref() {
            self.read_workspace_pin_state(workspace_dir)?
        } else {
            None
        };

        let (effective_profile, effective_source) =
            if let Some(pin) = workspace_pin.as_ref().or(global_pin.as_ref()) {
                let source = if pin.scope == ConnectionPinScope::Workspace {
                    ConnectionSelectionSource::WorkspacePin
                } else {
                    ConnectionSelectionSource::GlobalPin
                };
                (Some(pin.profile_id.clone()), source)
            } else if let Some(default_profile) = connections.default_profile.clone() {
                (
                    Some(default_profile),
                    ConnectionSelectionSource::DefaultProfile,
                )
            } else {
                (None, ConnectionSelectionSource::None)
            };

        Ok(ConnectionCurrentState {
            workspace_dir: normalized_workspace_dir,
            global_pin,
            workspace_pin,
            default_profile: connections.default_profile,
            effective_profile,
            effective_source,
        })
    }

    pub async fn set_default_profile(
        &self,
        profile_id: &str,
        workspace_dir: Option<&Path>,
    ) -> anyhow::Result<ConnectionCurrentState> {
        let _guard = self.mutate_lock.lock().await;
        let profile_id = validated_profile_id(profile_id)?;
        let mut connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?
            .clone();
        connections.default_profile = Some(profile_id.clone());
        self.save_connections(&connections)?;
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::ProfileActivated {},
        )
        .await;
        self.current_selection(workspace_dir)
    }

    pub async fn clear_default_profile(
        &self,
        workspace_dir: Option<&Path>,
    ) -> anyhow::Result<ConnectionCurrentState> {
        let _guard = self.mutate_lock.lock().await;
        let mut connections = self.load_connections()?;
        connections.default_profile = None;
        self.save_connections(&connections)?;
        self.current_selection(workspace_dir)
    }

    pub async fn pin_profile(
        &self,
        profile_id: &str,
        scope: ConnectionPinScope,
        workspace_dir: Option<&Path>,
    ) -> anyhow::Result<ConnectionCurrentState> {
        let _guard = self.mutate_lock.lock().await;
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        if !connections.profiles.contains_key(&profile_id) {
            anyhow::bail!("Unknown connection profile `{profile_id}`");
        }
        match scope {
            ConnectionPinScope::Global => self.write_global_pin_setting(Some(&profile_id))?,
            ConnectionPinScope::Workspace => {
                let workspace_root = workspace_dir
                    .ok_or_else(|| {
                        anyhow::anyhow!("workspace_dir is required when scope=workspace")
                    })
                    .and_then(validated_workspace_root_path)?;
                self.write_workspace_pin_setting(&workspace_root, Some(&profile_id))?;
            }
        }
        self.current_selection(workspace_dir)
    }

    pub async fn unpin_profile(
        &self,
        scope: ConnectionPinScope,
        workspace_dir: Option<&Path>,
    ) -> anyhow::Result<ConnectionCurrentState> {
        let _guard = self.mutate_lock.lock().await;
        match scope {
            ConnectionPinScope::Global => self.write_global_pin_setting(None)?,
            ConnectionPinScope::Workspace => {
                let workspace_root = workspace_dir
                    .ok_or_else(|| {
                        anyhow::anyhow!("workspace_dir is required when scope=workspace")
                    })
                    .and_then(validated_workspace_root_path)?;
                self.write_workspace_pin_setting(&workspace_root, None)?;
            }
        }
        self.current_selection(workspace_dir)
    }

    pub async fn credential_status(
        &self,
        profile_id: &str,
    ) -> anyhow::Result<ConnectionCredentialStatus> {
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        self.credential_status_from_profile(&profile_id, profile)
            .await
    }

    pub async fn set_secret(
        &self,
        profile_id: &str,
        secret: &str,
    ) -> anyhow::Result<ConnectionCredentialStatus> {
        let _guard = self.mutate_lock.lock().await;
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        let descriptor = ConnectionsFile::profile_descriptor(profile.provider);
        if descriptor.credential_kind != CredentialKind::SecretString {
            anyhow::bail!(
                "Provider `{}` does not support secret entry",
                profile.provider.as_str()
            );
        }
        let credential_id = profile
            .credential_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Profile `{profile_id}` has no credential"))?;
        self.secret_store()?.save(credential_id, secret)?;
        let status = self.credential_status(&profile_id).await?;
        self.append_event(
            &profile_id,
            profile.provider,
            Some(credential_id.to_string()),
            ConnectionEvent::CredentialStatusChanged {
                status: status.clone(),
            },
        )
        .await;
        Ok(status)
    }

    pub async fn start_browser_login(
        &self,
        profile_id: &str,
        workspace_id: Option<String>,
        timeout: Duration,
    ) -> anyhow::Result<super::connection_api::StartBrowserLoginResponse> {
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        self.ensure_chatgpt_profile(&profile_id, profile)?;
        let start = self
            .auth_control
            .start_loopback_browser_login(workspace_id, timeout)
            .await
            .map_err(anyhow::Error::from)?;
        self.login_bindings.lock().await.insert(
            start.login_id.clone(),
            LoginProfileBinding {
                profile_id: profile_id.clone(),
                provider: profile.provider,
                credential_id: profile.credential_id.clone(),
            },
        );
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::LoginStarted {
                login_id: start.login_id.clone(),
                method: "browser".to_string(),
            },
        )
        .await;
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::BrowserLoginReady {
                login_id: start.login_id.clone(),
                auth_url: start.auth_url.clone(),
                redirect_uri: start.redirect_uri.clone(),
            },
        )
        .await;
        Ok(super::connection_api::StartBrowserLoginResponse {
            login_id: start.login_id,
            auth_url: start.auth_url,
            redirect_uri: start.redirect_uri,
            created_at: start.created_at,
            expires_at: start.expires_at,
        })
    }

    pub async fn start_device_login(
        &self,
        profile_id: &str,
        workspace_id: Option<String>,
    ) -> anyhow::Result<super::connection_api::StartDeviceLoginResponse> {
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        self.ensure_chatgpt_profile(&profile_id, profile)?;
        let start = self
            .auth_control
            .start_device_login(workspace_id)
            .await
            .map_err(anyhow::Error::from)?;
        self.login_bindings.lock().await.insert(
            start.login_id.clone(),
            LoginProfileBinding {
                profile_id: profile_id.clone(),
                provider: profile.provider,
                credential_id: profile.credential_id.clone(),
            },
        );
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::LoginStarted {
                login_id: start.login_id.clone(),
                method: "device".to_string(),
            },
        )
        .await;
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::DeviceCodeReady {
                login_id: start.login_id.clone(),
                verification_url: start.verification_url.clone(),
                user_code: start.user_code.clone(),
                interval_secs: start.interval_secs,
            },
        )
        .await;
        Ok(super::connection_api::StartDeviceLoginResponse {
            login_id: start.login_id,
            verification_url: start.verification_url,
            user_code: start.user_code,
            interval_secs: start.interval_secs,
            created_at: start.created_at,
            expires_at: start.expires_at,
        })
    }

    pub async fn complete_device_login(
        &self,
        profile_id: &str,
        login_id: &str,
    ) -> anyhow::Result<super::connection_api::ConnectionLoginSuccessResponse> {
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        self.ensure_chatgpt_profile(&profile_id, profile)?;
        let login = self
            .auth_control
            .complete_device_login(login_id)
            .await
            .map_err(anyhow::Error::from)?;
        let snapshot = self
            .auth_control
            .status()
            .await
            .map_err(anyhow::Error::from)?;
        let status = self.credential_status(&profile_id).await?;
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::CredentialStatusChanged { status },
        )
        .await;
        Ok(super::connection_api::ConnectionLoginSuccessResponse {
            account_id: login.account_id,
            email: login.email,
            plan_type: login.plan_type,
            snapshot,
        })
    }

    pub async fn logout(
        &self,
        profile_id: &str,
    ) -> anyhow::Result<super::connection_api::ConnectionLogoutResponse> {
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        self.ensure_chatgpt_profile(&profile_id, profile)?;
        let removed = self
            .auth_control
            .logout()
            .await
            .map_err(anyhow::Error::from)?;
        let snapshot = self
            .auth_control
            .status()
            .await
            .map_err(anyhow::Error::from)?;
        let status = self.credential_status(&profile_id).await?;
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::LogoutCompleted { removed },
        )
        .await;
        self.append_event(
            &profile_id,
            profile.provider,
            profile.credential_id.clone(),
            ConnectionEvent::CredentialStatusChanged { status },
        )
        .await;
        Ok(super::connection_api::ConnectionLogoutResponse { removed, snapshot })
    }

    pub async fn test_connection(&self, profile_id: &str) -> anyhow::Result<(String, String)> {
        let profile_id = validated_profile_id(profile_id)?;
        let connections = self.load_connections()?;
        let mut config = Config::default();
        let secret_store = self.secret_store()?;
        let resolved =
            connections.apply_profile_to_config(Some(&profile_id), &secret_store, &mut config)?;
        if resolved.provider == LlmProvider::Chatgpt {
            let status = self
                .auth_control
                .status()
                .await
                .map_err(anyhow::Error::from)?;
            if status.kind != alan_agent_protocol::AuthStatusKind::LoggedIn {
                anyhow::bail!("ChatGPT profile `{profile_id}` is not logged in");
            }
        }
        let _provider_config = config.to_provider_config()?;
        let resolved_model = config.effective_model().to_string();
        let message = "Connection test succeeded.".to_string();
        self.append_event(
            &profile_id,
            resolved.provider,
            resolved.credential_id.clone(),
            ConnectionEvent::ConnectionTestSucceeded {
                resolved_model: resolved_model.clone(),
                message: message.clone(),
            },
        )
        .await;
        Ok((resolved_model, message))
    }

    fn load_connections(&self) -> anyhow::Result<ConnectionsFile> {
        Ok(ConnectionsFile::load_global()?.0)
    }

    fn save_connections(&self, connections: &ConnectionsFile) -> anyhow::Result<()> {
        connections.save_global()
    }

    fn read_global_pin_state(&self) -> anyhow::Result<Option<ConnectionPinState>> {
        let Some(profile_id) = read_global_connection_profile_setting()? else {
            return Ok(None);
        };
        Ok(Some(ConnectionPinState {
            scope: ConnectionPinScope::Global,
            config_path: global_agent_config_path()?,
            profile_id,
        }))
    }

    fn read_workspace_pin_state(
        &self,
        workspace_root: &Path,
    ) -> anyhow::Result<Option<ConnectionPinState>> {
        let connections = self.load_connections()?;
        let Some(profile_id) = connections
            .workspace_pins
            .get(&workspace_pin_key(workspace_root))
            .cloned()
        else {
            return Ok(None);
        };
        Ok(Some(ConnectionPinState {
            scope: ConnectionPinScope::Workspace,
            config_path: self.home_paths.global_connections_path.clone(),
            profile_id,
        }))
    }

    fn write_global_pin_setting(&self, profile_id: Option<&str>) -> anyhow::Result<()> {
        write_global_connection_profile_setting(profile_id)
    }

    fn write_workspace_pin_setting(
        &self,
        workspace_root: &Path,
        profile_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut connections = self.load_connections()?;
        let key = workspace_pin_key(workspace_root);
        match profile_id {
            Some(profile_id) => {
                connections
                    .workspace_pins
                    .insert(key, profile_id.to_string());
            }
            None => {
                connections.workspace_pins.remove(&key);
            }
        }
        self.save_connections(&connections)
    }

    fn secret_store(&self) -> anyhow::Result<SecretStore> {
        SecretStore::detect()
    }

    fn ensure_chatgpt_profile(
        &self,
        profile_id: &str,
        profile: &ConnectionProfile,
    ) -> anyhow::Result<()> {
        let descriptor = ConnectionsFile::profile_descriptor(profile.provider);
        if profile.provider != LlmProvider::Chatgpt
            || descriptor.credential_kind != CredentialKind::ManagedOauth
        {
            anyhow::bail!(
                "Profile `{profile_id}` does not support managed ChatGPT login operations"
            );
        }
        Ok(())
    }

    async fn credential_status_from_profile(
        &self,
        profile_id: &str,
        profile: &ConnectionProfile,
    ) -> anyhow::Result<ConnectionCredentialStatus> {
        let descriptor = ConnectionsFile::profile_descriptor(profile.provider);
        match descriptor.credential_kind {
            CredentialKind::ManagedOauth => {
                let snapshot = self
                    .auth_control
                    .status()
                    .await
                    .map_err(anyhow::Error::from)?;
                let status = match snapshot.kind {
                    alan_agent_protocol::AuthStatusKind::LoggedOut => {
                        ConnectionCredentialStatusKind::Missing
                    }
                    alan_agent_protocol::AuthStatusKind::LoggedIn => {
                        ConnectionCredentialStatusKind::Available
                    }
                    alan_agent_protocol::AuthStatusKind::Pending => {
                        ConnectionCredentialStatusKind::Pending
                    }
                };
                Ok(ConnectionCredentialStatus {
                    profile_id: profile_id.to_string(),
                    credential_id: profile.credential_id.clone(),
                    credential_kind: CredentialKind::ManagedOauth,
                    status,
                    last_checked_at: Some(Utc::now()),
                    detail: Some(ConnectionCredentialStatusDetail {
                        account_email: snapshot.email,
                        account_plan: snapshot.plan_type,
                        message: None,
                    }),
                })
            }
            CredentialKind::SecretString => {
                let credential_id = profile
                    .credential_id
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Profile `{profile_id}` has no credential"))?;
                let secret = self.secret_store()?.load(credential_id)?;
                Ok(ConnectionCredentialStatus {
                    profile_id: profile_id.to_string(),
                    credential_id: Some(credential_id.clone()),
                    credential_kind: CredentialKind::SecretString,
                    status: if secret.is_some() {
                        ConnectionCredentialStatusKind::Available
                    } else {
                        ConnectionCredentialStatusKind::Missing
                    },
                    last_checked_at: Some(Utc::now()),
                    detail: None,
                })
            }
            CredentialKind::AmbientCloudAuth => Ok(ConnectionCredentialStatus {
                profile_id: profile_id.to_string(),
                credential_id: profile.credential_id.clone(),
                credential_kind: CredentialKind::AmbientCloudAuth,
                status: ConnectionCredentialStatusKind::Available,
                last_checked_at: Some(Utc::now()),
                detail: None,
            }),
        }
    }

    async fn append_event(
        &self,
        profile_id: &str,
        provider: LlmProvider,
        credential_id: Option<String>,
        event: ConnectionEvent,
    ) {
        let envelope =
            self.event_log
                .write()
                .await
                .append(profile_id, provider, credential_id, event);
        let _ = self.events_tx.send(envelope);
    }

    fn start_auth_event_bridge(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut rx = this.auth_control.subscribe();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        this.handle_auth_event(event).await;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    async fn handle_auth_event(&self, event: alan_agent_protocol::AuthEventEnvelope) {
        match event.event {
            alan_agent_protocol::AuthEvent::LoginSucceeded {
                login_id,
                account_id,
                email,
                plan_type,
            } => {
                if let Some(binding) = self.login_bindings.lock().await.remove(&login_id) {
                    self.append_event(
                        &binding.profile_id,
                        binding.provider,
                        binding.credential_id.clone(),
                        ConnectionEvent::LoginSucceeded {
                            login_id,
                            account_id,
                            email,
                            plan_type,
                        },
                    )
                    .await;
                    if let Ok(connections) = self.load_connections()
                        && let Some(profile) = connections.profiles.get(&binding.profile_id)
                        && let Ok(status) = self
                            .credential_status_from_profile(&binding.profile_id, profile)
                            .await
                    {
                        self.append_event(
                            &binding.profile_id,
                            binding.provider,
                            binding.credential_id,
                            ConnectionEvent::CredentialStatusChanged { status },
                        )
                        .await;
                    }
                }
            }
            alan_agent_protocol::AuthEvent::LoginFailed {
                login_id,
                message,
                recoverable,
            } => {
                let binding = match login_id.as_ref() {
                    Some(login_id_ref) => self.login_bindings.lock().await.remove(login_id_ref),
                    None => None,
                };
                if let Some(binding) = binding {
                    self.append_event(
                        &binding.profile_id,
                        binding.provider,
                        binding.credential_id,
                        ConnectionEvent::LoginFailed {
                            login_id,
                            message,
                            recoverable,
                        },
                    )
                    .await;
                }
            }
            _ => {}
        }
    }
}

fn detected_global_home_paths() -> anyhow::Result<AlanHomePaths> {
    AlanHomePaths::detect()
        .ok_or_else(|| anyhow::anyhow!("Could not determine alan home directory"))
}

fn normalized_global_home_paths() -> anyhow::Result<AlanHomePaths> {
    let detected = detected_global_home_paths()?;
    validate_safe_absolute_path("alan home parent directory", &detected.home_dir)?;
    let normalized = AlanHomePaths::from_home_dir(&detected.home_dir);
    if normalized != detected {
        anyhow::bail!(
            "invalid alan home layout; expected paths under {}",
            normalized.alan_home_dir.display()
        );
    }
    validate_safe_absolute_path("alan home directory", &normalized.alan_home_dir)?;
    validate_safe_absolute_path(
        "alan global agent root directory",
        &normalized.global_agent_root_dir,
    )?;
    validate_safe_absolute_path(
        "alan global agent config path",
        &normalized.global_agent_config_path,
    )?;
    Ok(normalized)
}

fn global_agent_config_path() -> anyhow::Result<PathBuf> {
    let path = normalized_global_home_paths()?.global_agent_config_path;
    validate_agent_config_path(&path)?;
    Ok(path)
}

fn read_global_agent_config_table() -> anyhow::Result<toml::Table> {
    let path = global_agent_config_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            if content.trim().is_empty() {
                return Ok(toml::Table::new());
            }
            let value: toml::Value = toml::from_str(&content)
                .with_context(|| format!("failed to parse agent config {}", path.display()))?;
            value.as_table().cloned().ok_or_else(|| {
                anyhow::anyhow!("agent config {} must be a TOML table", path.display())
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read agent config {}", path.display()))
        }
    }
}

fn validated_workspace_root_path(path: &Path) -> anyhow::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("failed to resolve workspace path {}", path.display()))?;
    if !canonical.is_dir() {
        anyhow::bail!("workspace path {} is not a directory", canonical.display());
    }
    Ok(normalize_workspace_root_path(&canonical))
}

fn validated_profile_id(profile_id: &str) -> anyhow::Result<String> {
    sanitize_identifier(profile_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid profile id `{profile_id}`"))
}

fn validate_agent_config_path(path: &Path) -> anyhow::Result<()> {
    let layout = alan_agent_engine::AgentRootLayout::new();
    if !layout.is_default_agent_config_path_shape(path) {
        anyhow::bail!(
            "invalid agent config path {}; expected .../{}",
            path.display(),
            layout.default_agent_config_suffix().display()
        );
    }
    Ok(())
}

fn validate_safe_absolute_path(label: &str, path: &Path) -> anyhow::Result<()> {
    if !path.is_absolute() {
        anyhow::bail!("{label} must be absolute: {}", path.display());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        anyhow::bail!(
            "{label} must not contain relative components: {}",
            path.display()
        );
    }
    Ok(())
}

fn read_global_connection_profile_setting() -> anyhow::Result<Option<String>> {
    let path = global_agent_config_path()?;
    let table = read_global_agent_config_table()?;
    match table.get("connection_profile") {
        Some(toml::Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        Some(toml::Value::String(_)) | None => Ok(None),
        Some(_) => anyhow::bail!("connection_profile in {} must be a string", path.display()),
    }
}

fn write_global_connection_profile_setting(profile_id: Option<&str>) -> anyhow::Result<()> {
    let path = global_agent_config_path()?;
    let mut table = read_global_agent_config_table()?;
    match profile_id {
        Some(profile_id) => {
            table.insert(
                "connection_profile".to_string(),
                toml::Value::String(profile_id.to_string()),
            );
        }
        None => {
            table.remove("connection_profile");
        }
    }

    if table.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to remove empty agent config {}", path.display())
                });
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create agent config directory {}",
                parent.display()
            )
        })?;
    }
    let rendered = toml::to_string_pretty(&table)
        .context("failed to encode agent configuration while updating connection_profile")?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("failed to open agent config {}", path.display()))?;
        file.write_all(rendered.as_bytes())
            .with_context(|| format!("failed to write agent config {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, rendered)
            .with_context(|| format!("failed to write agent config {}", path.display()))?;
    }
    Ok(())
}

fn workspace_pin_key(workspace_root: &Path) -> String {
    workspace_root.to_string_lossy().into_owned()
}

fn parse_connection_event_sequence(event_id: &str) -> Option<u64> {
    event_id
        .strip_prefix("conn_evt_")
        .and_then(|suffix| suffix.parse::<u64>().ok())
}

fn now_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
