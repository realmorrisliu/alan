use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::connection_profile::{
    ConnectionProfile, ConnectionStoreBindings, ConnectionsFile, sanitize_identifier,
    validate_profile_settings,
};
use crate::flat_fs::{FlatFileService, FlatServiceFs};
use crate::runtime::LlmClientFactory;
use alan_agent_engine::{Config, LlmClient};
use alan_ap::{ErrorCode, FileServer};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

pub(crate) mod process_binding;

const FILES: &[(&str, bool)] = &[
    ("metadata", false),
    ("profiles", false),
    ("default", false),
    ("selection", false),
    ("status", false),
    ("validation", false),
    ("ctl", true),
    ("native-requests", false),
    ("native-responses", true),
];
const MAX_PENDING_NATIVE_REQUESTS: usize = 32;
const MAX_NATIVE_RESPONSES: usize = 64;
const MAX_IDENTIFIER_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeConnectionAction {
    BrowserLogin,
    DeviceLogin,
    SecretEntry,
    Logout,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConnectionRequest {
    pub id: String,
    pub profile_id: String,
    pub action: NativeConnectionAction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConnectionResponse {
    pub request_id: String,
    pub opaque_credential_ref: Option<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum ConnectionCommand {
    ReplaceMetadata {
        expected: String,
        connections: ConnectionsFile,
    },
    AddProfile {
        profile_id: String,
        profile: ConnectionProfile,
    },
    SetDefault {
        profile_id: String,
    },
    ClearDefault,
    RemoveProfile {
        profile_id: String,
    },
    Select {
        pid: u64,
        profile_id: String,
    },
    RequestNative {
        request: NativeConnectionRequest,
    },
}

#[derive(Clone)]
struct State {
    connections: ConnectionsFile,
    selections: BTreeMap<u64, String>,
    requests: BTreeMap<String, NativeConnectionRequest>,
    responses: BTreeMap<String, NativeConnectionResponse>,
    response_order: VecDeque<String>,
    native_status: BTreeMap<String, String>,
    validation: BTreeMap<String, String>,
}

impl State {
    fn select_generation(&mut self, pid: u64, profile_id: &str) -> Result<()> {
        ensure!(pid > 0, "Process PID must be positive");
        validate_id(profile_id)?;
        let profile = self
            .connections
            .profiles
            .get(profile_id)
            .context("unknown profile")?;
        ensure!(
            profile.provider.supports_generation(),
            "evaluation-only profile cannot be selected for generation"
        );
        self.selections.insert(pid, profile_id.into());
        Ok(())
    }

    fn replace_connections(&mut self, connections: ConnectionsFile) {
        let unchanged = connections
            .profiles
            .iter()
            .filter_map(|(id, profile)| {
                let same_credential = profile.credential_id.as_ref().is_none_or(|credential| {
                    self.connections.credentials.get(credential)
                        == connections.credentials.get(credential)
                });
                (self.connections.profiles.get(id) == Some(profile) && same_credential)
                    .then_some(id.clone())
            })
            .collect::<BTreeSet<_>>();
        self.connections = connections;
        let installed = self
            .connections
            .profiles
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        self.selections.retain(|_, profile| {
            self.connections
                .profiles
                .get(profile)
                .is_some_and(|profile| profile.provider.supports_generation())
        });
        self.requests
            .retain(|_, request| unchanged.contains(&request.profile_id));
        self.native_status
            .retain(|profile, _| unchanged.contains(profile));
        self.validation
            .retain(|profile, _| unchanged.contains(profile));
        for profile in installed {
            self.validation
                .entry(profile)
                .or_insert_with(|| "unavailable".into());
        }
    }
}

type ModelCatalogPublication = Arc<OnceLock<Option<Arc<alan_agent_engine::ModelCatalog>>>>;

struct CallableRegistry {
    llmfs: Arc<alan_llmfs::LlmFs>,
    factory: Arc<dyn LlmClientFactory>,
    base_config: Config,
    bootstrap: Option<(String, LlmClient)>,
    published_profiles: BTreeMap<String, ConnectionProfile>,
    published_catalogs: BTreeMap<String, ModelCatalogPublication>,
    published_accounts: BTreeMap<String, String>,
    published_fallbacks: BTreeSet<String>,
    published_default: Option<String>,
}

/// Channel-scoped Connection metadata authority. Secret bytes never enter it.
pub struct ConnectionService {
    channel_id: String,
    metadata_path: PathBuf,
    state: Mutex<State>,
    callables: tokio::sync::Mutex<Option<CallableRegistry>>,
}

struct ConnectionLlmProvider {
    client: LlmClient,
}

#[async_trait::async_trait]
impl LlmProvider for ConnectionLlmProvider {
    fn supports_generation(&self) -> bool {
        self.client.supports_generation()
    }
    fn supports_choice_evaluation(&self) -> bool {
        self.client.supports_choice_evaluation()
    }
    async fn evaluate_choice(
        &mut self,
        request: alan_llm::ChoiceEvaluationRequest,
    ) -> Result<alan_llm::ChoiceEvaluationResponse> {
        self.client.evaluate_choice(request).await
    }

    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.client.generate(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.client.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.client.generate_stream(request).await
    }

    fn provider_name(&self) -> &'static str {
        self.client.provider_name()
    }
}

impl ConnectionService {
    pub fn open(
        channel_id: impl Into<String>,
        bindings: &ConnectionStoreBindings,
    ) -> Result<Arc<Self>> {
        let channel_id = channel_id.into();
        ensure!(
            matches!(channel_id.as_str(), "stable" | "dev" | "test"),
            "invalid Connection Service channel"
        );
        let (connections, _) = ConnectionsFile::load_from_path(&bindings.metadata_path)?;
        let validation = connections
            .profiles
            .keys()
            .map(|profile_id| (profile_id.clone(), "unavailable".to_string()))
            .collect();
        Ok(Arc::new(Self {
            channel_id,
            metadata_path: bindings.metadata_path.clone(),
            state: Mutex::new(State {
                connections,
                selections: BTreeMap::new(),
                requests: BTreeMap::new(),
                responses: BTreeMap::new(),
                response_order: VecDeque::new(),
                native_status: BTreeMap::new(),
                validation,
            }),
            callables: tokio::sync::Mutex::new(None),
        }))
    }

    pub fn ephemeral(channel_id: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            channel_id: channel_id.into(),
            metadata_path: std::env::temp_dir().join(format!(
                "alan-connections-{}.toml",
                uuid::Uuid::new_v4().simple()
            )),
            state: Mutex::new(State {
                connections: ConnectionsFile::default(),
                selections: BTreeMap::new(),
                requests: BTreeMap::new(),
                responses: BTreeMap::new(),
                response_order: VecDeque::new(),
                native_status: BTreeMap::new(),
                validation: BTreeMap::new(),
            }),
            callables: tokio::sync::Mutex::new(None),
        })
    }

    pub fn file_server(self: &Arc<Self>) -> Arc<dyn FileServer> {
        Arc::new(FlatServiceFs::new(self.clone()))
    }

    /// Attach the callable LLM registry owned by this Connection Service.
    ///
    /// The Host factory resolves credentials behind its adapter boundary. Only
    /// ready profile identifiers and provider clients enter the Alan OS tree.
    pub async fn attach_callable_registry(
        &self,
        llmfs: Arc<alan_llmfs::LlmFs>,
        factory: Arc<dyn LlmClientFactory>,
        base_config: Config,
        bootstrap: Option<(String, LlmClient)>,
    ) -> Result<()> {
        let mut callables = self.callables.lock().await;
        ensure!(callables.is_none(), "callable registry is already attached");
        *callables = Some(CallableRegistry {
            llmfs,
            factory,
            base_config,
            bootstrap,
            published_profiles: BTreeMap::new(),
            published_catalogs: BTreeMap::new(),
            published_accounts: BTreeMap::new(),
            published_fallbacks: BTreeSet::new(),
            published_default: None,
        });
        drop(callables);
        self.refresh_callables().await;
        Ok(())
    }

    /// Refresh shared metadata and its callable projection at an operation boundary.
    pub async fn refresh(&self) -> Result<()> {
        {
            let mut state = self.state.lock().unwrap();
            let (connections, _) = ConnectionsFile::load_from_path(&self.metadata_path)?;
            if connections != state.connections {
                state.replace_connections(connections);
            }
        }
        self.refresh_callables().await;
        Ok(())
    }

    /// Capture a refreshed Process binding while registry replacement is excluded.
    pub async fn capture_connection(&self, profile_id: &str) -> Result<alan_llmfs::LlmFs> {
        self.refresh().await?;
        let callables = self.callables.lock().await;
        let registry = callables
            .as_ref()
            .context("callable registry is not attached")?;
        Ok(registry.llmfs.connection_snapshot(profile_id))
    }

    pub fn selected_profile(&self, pid: u64) -> Option<String> {
        let state = self.state.lock().unwrap();
        state
            .selections
            .get(&pid)
            .cloned()
            .or_else(|| state.connections.default_profile.clone())
    }

    pub fn default_profile(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .connections
            .default_profile
            .clone()
    }

    pub fn metadata(&self) -> ConnectionsFile {
        self.state.lock().unwrap().connections.clone()
    }

    pub fn has_profile(&self, profile_id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .connections
            .profiles
            .contains_key(profile_id)
    }

    pub fn select(&self, pid: u64, profile_id: &str) -> Result<()> {
        self.state
            .lock()
            .unwrap()
            .select_generation(pid, profile_id)
    }

    pub fn release_process(&self, pid: u64) {
        self.state.lock().unwrap().selections.remove(&pid);
    }

    pub fn native_request(&self, request_id: &str) -> Option<NativeConnectionRequest> {
        self.state.lock().unwrap().requests.get(request_id).cloned()
    }

    pub async fn respond_native(&self, response: NativeConnectionResponse) -> Result<()> {
        ensure!(
            matches!(
                response.status.as_str(),
                "ready" | "logged_out" | "failed" | "unavailable"
            ),
            "unsupported native response status"
        );
        if let Some(reference) = response.opaque_credential_ref.as_deref() {
            ensure!(
                valid_opaque_reference(reference),
                "credential response is not an opaque reference"
            );
        }
        {
            let mut state = self.state.lock().unwrap();
            let request = state
                .requests
                .remove(&response.request_id)
                .context("unknown native request")?;
            state
                .native_status
                .insert(request.profile_id, response.status.clone());
            let response_id = response.request_id.clone();
            state.responses.insert(response_id.clone(), response);
            state.response_order.push_back(response_id);
            while state.response_order.len() > MAX_NATIVE_RESPONSES {
                if let Some(expired) = state.response_order.pop_front() {
                    state.responses.remove(&expired);
                }
            }
        }
        self.refresh_callables().await;
        Ok(())
    }

    async fn apply(&self, command: ConnectionCommand) -> Result<()> {
        let (refresh, save_result) = {
            let mut committed = self.state.lock().unwrap();
            let mut state = committed.clone();
            let mut persist = false;
            let mut refresh = false;
            match command {
                ConnectionCommand::ReplaceMetadata {
                    expected,
                    connections,
                } => {
                    ensure!(
                        state.connections.fingerprint()? == expected,
                        "connection metadata changed; reload before retrying"
                    );
                    validate_connections(&connections)?;
                    state.replace_connections(connections);
                    persist = true;
                    refresh = true;
                }
                ConnectionCommand::AddProfile {
                    profile_id,
                    profile,
                } => {
                    validate_id(&profile_id)?;
                    validate_profile_settings(profile.provider, &profile.settings)?;
                    ensure!(
                        !state.connections.profiles.contains_key(&profile_id),
                        "profile already exists"
                    );
                    state
                        .connections
                        .profiles
                        .insert(profile_id.clone(), profile);
                    state
                        .validation
                        .insert(profile_id, "unavailable".to_string());
                    persist = true;
                    refresh = true;
                }
                ConnectionCommand::SetDefault { profile_id } => {
                    validate_id(&profile_id)?;
                    ensure!(
                        state.connections.profiles.contains_key(&profile_id),
                        "unknown profile"
                    );
                    ensure!(
                        state.connections.profiles[&profile_id]
                            .provider
                            .supports_generation(),
                        "evaluation-only profile cannot be the generation default"
                    );
                    state.connections.default_profile = Some(profile_id);
                    persist = true;
                    refresh = true;
                }
                ConnectionCommand::ClearDefault => {
                    state.connections.default_profile = None;
                    persist = true;
                    refresh = true;
                }
                ConnectionCommand::RemoveProfile { profile_id } => {
                    ensure!(
                        state.connections.profiles.remove(&profile_id).is_some(),
                        "unknown profile"
                    );
                    if state.connections.default_profile.as_deref() == Some(&profile_id) {
                        state.connections.default_profile = None;
                    }
                    state
                        .selections
                        .retain(|_, selected| selected != &profile_id);
                    state
                        .requests
                        .retain(|_, request| request.profile_id != profile_id);
                    state.native_status.remove(&profile_id);
                    state.validation.remove(&profile_id);
                    persist = true;
                    refresh = true;
                }
                ConnectionCommand::Select { pid, profile_id } => {
                    state.select_generation(pid, &profile_id)?;
                }
                ConnectionCommand::RequestNative { request } => {
                    validate_id(&request.id)?;
                    ensure!(
                        state.connections.profiles.contains_key(&request.profile_id),
                        "unknown profile"
                    );
                    ensure!(
                        !state.requests.contains_key(&request.id)
                            && !state.responses.contains_key(&request.id),
                        "native request already exists"
                    );
                    ensure!(
                        state.requests.len() < MAX_PENDING_NATIVE_REQUESTS,
                        "too many pending native requests"
                    );
                    state.requests.insert(request.id.clone(), request);
                    refresh = true;
                }
            }
            let save_result = if persist {
                state
                    .connections
                    .save_if_unchanged(&self.metadata_path, &committed.connections)
                    .context("persist Connection Service metadata")
            } else {
                Ok(())
            };
            let published = self.publish_saved_state(&mut committed, state, &save_result);
            (refresh && published, save_result)
        };
        if refresh {
            self.refresh_callables().await;
        }
        save_result
    }

    fn publish_saved_state(
        &self,
        committed: &mut State,
        candidate: State,
        save_result: &Result<()>,
    ) -> bool {
        if save_result.is_ok() {
            *committed = candidate;
            return true;
        }
        // A failed save may follow publication or a commit by another instance.
        let Ok((on_disk, _)) = ConnectionsFile::load_from_path(&self.metadata_path) else {
            return false;
        };
        if validate_connections(&on_disk).is_err() {
            return false;
        }
        if on_disk == candidate.connections {
            *committed = candidate;
        } else {
            committed.replace_connections(on_disk);
        }
        true
    }

    async fn refresh_callables(&self) {
        let mut callables = self.callables.lock().await;
        let (connections, pending_profiles, native_status) = {
            let state = self.state.lock().unwrap();
            (
                state.connections.clone(),
                state
                    .requests
                    .values()
                    .map(|request| request.profile_id.clone())
                    .collect::<BTreeSet<_>>(),
                state.native_status.clone(),
            )
        };

        let mut validation = BTreeMap::new();
        let mut ready_profiles = BTreeMap::new();
        for (profile_id, profile) in &connections.profiles {
            let status = if pending_profiles.contains(profile_id) {
                "pending"
            } else {
                match native_status.get(profile_id).map(String::as_str) {
                    Some("logged_out") => "logged_out",
                    Some("failed" | "unavailable") => "unavailable",
                    _ => {
                        ready_profiles.insert(profile_id.clone(), profile.clone());
                        "unavailable"
                    }
                }
            };
            validation.insert(profile_id.clone(), status.to_string());
        }

        let Some(registry) = callables.as_mut() else {
            self.state.lock().unwrap().validation = validation;
            return;
        };

        let stale_profiles = registry
            .published_profiles
            .iter()
            .filter_map(|(profile_id, published)| {
                (ready_profiles.get(profile_id) != Some(published)).then_some(profile_id.clone())
            })
            .collect::<Vec<_>>();
        for profile_id in stale_profiles {
            registry.llmfs.unregister_connection(&profile_id).await;
            registry.published_profiles.remove(&profile_id);
            registry.published_catalogs.remove(&profile_id);
            registry.published_accounts.remove(&profile_id);
        }

        let stale_fallbacks = registry
            .published_fallbacks
            .iter()
            .filter(|name| connections.profiles.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in stale_fallbacks {
            registry.llmfs.unregister_connection(&name).await;
            registry.published_fallbacks.remove(&name);
        }

        let mut catalog_requests = Vec::new();
        for (profile_id, profile) in ready_profiles {
            let already_published = registry.published_profiles.get(&profile_id) == Some(&profile);
            let needs_catalog = profile.provider == alan_agent_engine::LlmProvider::Chatgpt
                && !registry.published_catalogs.contains_key(&profile_id);
            if !already_published {
                let client = match registry.bootstrap.take() {
                    Some((name, client)) if name == profile_id => client,
                    Some(bootstrap) => {
                        registry.bootstrap = Some(bootstrap);
                        match registry.factory.create(
                            &registry.base_config,
                            Some(&profile_id),
                            &connections,
                        ) {
                            Ok(client) => client,
                            Err(_) => continue,
                        }
                    }
                    None => match registry.factory.create(
                        &registry.base_config,
                        Some(&profile_id),
                        &connections,
                    ) {
                        Ok(client) => client,
                        Err(_) => continue,
                    },
                };
                if let Some(account) = client.account_identity() {
                    registry
                        .published_accounts
                        .insert(profile_id.clone(), account.into());
                }
                let provider = Box::new(ConnectionLlmProvider { client });
                if profile.provider == alan_agent_engine::LlmProvider::TypesafeEvaluation {
                    registry.llmfs.register_connection_profile(
                        &profile_id,
                        alan_llmfs::ConnectionProfile::new(
                            profile.provider.as_str(),
                            profile.settings.get("model").cloned().unwrap_or_default(),
                            profile.credential_id.clone().unwrap_or_default(),
                        ),
                        provider,
                    );
                } else {
                    registry.llmfs.register_connection(&profile_id, provider);
                }
                registry
                    .published_profiles
                    .insert(profile_id.clone(), profile);
            }
            if needs_catalog {
                let publication = Arc::new(OnceLock::new());
                registry
                    .published_catalogs
                    .insert(profile_id.clone(), publication.clone());
                catalog_requests.push((profile_id.clone(), publication));
            }
            validation.insert(profile_id, "ready".to_string());
        }

        let default_profile = connections
            .default_profile
            .as_ref()
            .filter(|profile_id| {
                registry
                    .published_profiles
                    .contains_key(profile_id.as_str())
            })
            .cloned();
        if registry.published_default != default_profile {
            if registry.published_fallbacks.remove("default")
                || registry.published_default.take().is_some()
            {
                registry.llmfs.unregister_connection("default").await;
            }
            if let Some(profile_id) = default_profile {
                registry
                    .llmfs
                    .register_connection_alias("default", &profile_id)
                    .expect("published default profile is callable");
                registry.published_default = Some(profile_id);
            }
        }

        if let Some((name, _)) = registry.bootstrap.as_ref()
            && !connections.profiles.contains_key(name)
            && registry.published_default.is_none()
        {
            let (name, client) = registry.bootstrap.take().expect("bootstrap exists");
            registry
                .llmfs
                .register_connection(&name, Box::new(ConnectionLlmProvider { client }));
            registry.published_fallbacks.insert(name);
        }
        self.state.lock().unwrap().validation = validation;
        drop(callables);
        for (profile_id, publication) in catalog_requests {
            self.discover_catalog(&profile_id, &publication).await;
        }
    }

    async fn discover_catalog(&self, profile_id: &str, publication: &ModelCatalogPublication) {
        let discovery = async {
            let (client, base_catalog) = {
                let callables = self.callables.lock().await;
                let registry = callables
                    .as_ref()
                    .context("callable registry unavailable")?;
                let connections = self.metadata();
                ensure!(
                    registry
                        .published_catalogs
                        .get(profile_id)
                        .is_some_and(|current| Arc::ptr_eq(current, publication)),
                    "catalog publication replaced"
                );
                ensure!(
                    registry.published_profiles.get(profile_id)
                        == connections.profiles.get(profile_id)
                        && registry.published_profiles.contains_key(profile_id),
                    "profile publication changed"
                );
                let client = registry.factory.create(
                    &registry.base_config,
                    Some(profile_id),
                    &connections,
                )?;
                let account = registry
                    .published_accounts
                    .get(profile_id)
                    .context("managed catalog account unavailable")?;
                ensure!(
                    client.account_identity() == Some(account.as_str()),
                    "provider account changed since callable publication"
                );
                (
                    client,
                    registry.base_config.resolved_model_catalog().clone(),
                )
            };
            // Never hold the global callable lock across provider IO. The publication
            // slot is detached on replacement, so late results cannot authorize it.
            let models = client
                .model_catalog()
                .await?
                .context("model catalog unavailable")?;
            base_catalog.with_chatgpt_models(models).map(Arc::new)
        }
        .await;
        // A settled failure is cached too. Only explicit selection retries it.
        let _ = publication.set(discovery.ok());
    }
}

#[async_trait::async_trait]
impl FlatFileService for ConnectionService {
    fn files(&self) -> &'static [(&'static str, bool)] {
        FILES
    }

    async fn read(&self, name: &str) -> Result<Vec<u8>, ErrorCode> {
        self.refresh().await.map_err(|_| ErrorCode::Io)?;
        let state = self.state.lock().unwrap();
        let text = match name {
            "metadata" => serde_json::to_string(&state.connections),
            "profiles" => serde_json::to_string(&state.connections.profiles),
            "default" => Ok(format!(
                "{}\n",
                state.connections.default_profile.as_deref().unwrap_or("")
            )),
            "selection" => serde_json::to_string(&state.selections),
            "status" => Ok(format!(
                "channel={} profiles={} ready={} pending_native={} unavailable={}\n",
                self.channel_id,
                state.connections.profiles.len(),
                state
                    .validation
                    .values()
                    .filter(|status| status.as_str() == "ready")
                    .count(),
                state.requests.len(),
                state
                    .validation
                    .values()
                    .filter(|status| status.as_str() != "ready")
                    .count()
            )),
            "validation" => serde_json::to_string(&state.validation),
            "ctl" => Ok("write one Connection Service command JSON document\n".to_string()),
            "native-requests" => serde_json::to_string(&state.requests),
            "native-responses" => serde_json::to_string(&state.responses),
            _ => return Err(ErrorCode::NotFound),
        }
        .map_err(|_| ErrorCode::Io)?;
        Ok(text.into_bytes())
    }

    async fn commit(&self, name: &str, bytes: &[u8]) -> Result<(), ErrorCode> {
        match name {
            "ctl" => {
                let command = serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
                self.apply(command).await.map_err(|_| ErrorCode::BadRequest)
            }
            "native-responses" => {
                let response = serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
                self.respond_native(response)
                    .await
                    .map_err(|_| ErrorCode::BadRequest)
            }
            _ => Err(ErrorCode::NoAccess),
        }
    }
}

fn validate_id(id: &str) -> Result<()> {
    ensure!(
        id.len() <= MAX_IDENTIFIER_BYTES && sanitize_identifier(id).as_deref() == Some(id),
        "invalid Connection Service identifier"
    );
    Ok(())
}

fn validate_connections(connections: &ConnectionsFile) -> Result<()> {
    if let Some(default) = connections.default_profile.as_deref() {
        validate_id(default)?;
        ensure!(
            connections.profiles.contains_key(default),
            "unknown default profile"
        );
        ensure!(
            connections.profiles[default].provider.supports_generation(),
            "evaluation-only profile cannot be the generation default"
        );
    }
    for (id, profile) in &connections.profiles {
        validate_id(id)?;
        validate_profile_settings(profile.provider, &profile.settings)?;
    }
    for (id, credential) in &connections.credentials {
        validate_id(id)?;
        ensure!(
            matches!(
                credential.backend.as_str(),
                "host_managed_auth" | "host_credential_store" | "ambient"
            ),
            "credential backend must be an opaque Host adapter reference"
        );
    }
    Ok(())
}

fn valid_opaque_reference(reference: &str) -> bool {
    reference.starts_with("host-")
        && reference.len() <= 128
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
}

#[cfg(test)]
mod tests;
