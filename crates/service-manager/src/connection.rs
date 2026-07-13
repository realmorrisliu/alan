use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alan_agent_engine::{
    Config, ConnectionProfile, ConnectionStoreBindings, ConnectionsFile, LlmClient,
    sanitize_identifier, validate_profile_settings,
};
use alan_ap::{ErrorCode, FileServer};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::flat_fs::{FlatFileService, FlatServiceFs};
use crate::runtime::LlmClientFactory;

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

struct State {
    connections: ConnectionsFile,
    selections: BTreeMap<u64, String>,
    requests: BTreeMap<String, NativeConnectionRequest>,
    responses: BTreeMap<String, NativeConnectionResponse>,
    native_status: BTreeMap<String, String>,
    validation: BTreeMap<String, String>,
}

struct CallableRegistry {
    llmfs: Arc<alan_llmfs::LlmFs>,
    factory: Arc<dyn LlmClientFactory>,
    base_config: Config,
    bootstrap: Option<(String, LlmClient)>,
    published_profiles: BTreeMap<String, ConnectionProfile>,
    published_fallbacks: BTreeSet<String>,
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
        bootstrap_name: String,
        bootstrap_client: LlmClient,
    ) -> Result<()> {
        let mut callables = self.callables.lock().await;
        ensure!(callables.is_none(), "callable registry is already attached");
        *callables = Some(CallableRegistry {
            llmfs,
            factory,
            base_config,
            bootstrap: Some((bootstrap_name, bootstrap_client)),
            published_profiles: BTreeMap::new(),
            published_fallbacks: BTreeSet::new(),
        });
        drop(callables);
        self.refresh_callables().await;
        Ok(())
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
        ensure!(pid > 0, "Process PID must be positive");
        validate_id(profile_id)?;
        let mut state = self.state.lock().unwrap();
        ensure!(
            state.connections.profiles.contains_key(profile_id),
            "unknown profile"
        );
        state.selections.insert(pid, profile_id.to_string());
        Ok(())
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
            state
                .responses
                .insert(response.request_id.clone(), response);
        }
        self.refresh_callables().await;
        Ok(())
    }

    async fn apply(&self, command: ConnectionCommand) -> Result<()> {
        let refresh = {
            let mut state = self.state.lock().unwrap();
            let mut persist = false;
            let mut refresh = false;
            match command {
                ConnectionCommand::ReplaceMetadata { connections } => {
                    validate_connections(&connections)?;
                    state.connections = connections;
                    let installed = state
                        .connections
                        .profiles
                        .keys()
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>();
                    state
                        .selections
                        .retain(|_, profile| installed.contains(profile));
                    state
                        .requests
                        .retain(|_, request| installed.contains(&request.profile_id));
                    state
                        .native_status
                        .retain(|profile, _| installed.contains(profile));
                    state
                        .validation
                        .retain(|profile, _| installed.contains(profile));
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
                    state.connections.default_profile = Some(profile_id);
                    persist = true;
                }
                ConnectionCommand::ClearDefault => {
                    state.connections.default_profile = None;
                    persist = true;
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
                    ensure!(pid > 0, "Process PID must be positive");
                    ensure!(
                        state.connections.profiles.contains_key(&profile_id),
                        "unknown profile"
                    );
                    state.selections.insert(pid, profile_id);
                }
                ConnectionCommand::RequestNative { request } => {
                    validate_id(&request.id)?;
                    ensure!(
                        state.connections.profiles.contains_key(&request.profile_id),
                        "unknown profile"
                    );
                    ensure!(
                        !state.requests.contains_key(&request.id),
                        "native request already exists"
                    );
                    state.requests.insert(request.id.clone(), request);
                    refresh = true;
                }
            }
            if persist {
                state
                    .connections
                    .save_to_path(&self.metadata_path)
                    .context("persist Connection Service metadata")?;
            }
            refresh
        };
        if refresh {
            self.refresh_callables().await;
        }
        Ok(())
    }

    async fn refresh_callables(&self) {
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

        let mut callables = self.callables.lock().await;
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

        for (profile_id, profile) in ready_profiles {
            if registry.published_profiles.get(&profile_id) == Some(&profile) {
                validation.insert(profile_id, "ready".to_string());
                continue;
            }
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
            registry
                .llmfs
                .register_connection(&profile_id, Box::new(ConnectionLlmProvider { client }));
            registry
                .published_profiles
                .insert(profile_id.clone(), profile);
            validation.insert(profile_id, "ready".to_string());
        }

        if let Some((name, _)) = registry.bootstrap.as_ref()
            && !connections.profiles.contains_key(name)
        {
            let (name, client) = registry.bootstrap.take().expect("bootstrap exists");
            registry
                .llmfs
                .register_connection(&name, Box::new(ConnectionLlmProvider { client }));
            registry.published_fallbacks.insert(name);
        }
        drop(callables);
        self.state.lock().unwrap().validation = validation;
    }
}

#[async_trait::async_trait]
impl FlatFileService for ConnectionService {
    fn files(&self) -> &'static [(&'static str, bool)] {
        FILES
    }

    fn read(&self, name: &str) -> Result<Vec<u8>, ErrorCode> {
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
        sanitize_identifier(id).as_deref() == Some(id),
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
mod tests {
    use super::*;
    use alan_agent_engine::LlmProvider as ProviderId;
    use alan_ap::InProcessTransport;
    use alan_llm::MockLlmProvider;
    use alan_shell::Shell;
    use chrono::Utc;

    #[derive(Debug, Default)]
    struct TestLlmClientFactory {
        unavailable: Mutex<BTreeSet<String>>,
    }

    impl LlmClientFactory for TestLlmClientFactory {
        fn create(
            &self,
            _base_config: &Config,
            selected_profile: Option<&str>,
            _connections: &ConnectionsFile,
        ) -> Result<LlmClient> {
            let selected_profile = selected_profile.context("missing selected profile")?;
            ensure!(
                !self.unavailable.lock().unwrap().contains(selected_profile),
                "profile is unavailable"
            );
            Ok(LlmClient::new(MockLlmProvider::new()))
        }
    }

    fn profile() -> ConnectionProfile {
        ConnectionProfile {
            provider: ProviderId::OpenAiResponses,
            label: Some("main".to_string()),
            credential_id: Some("openai-main".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source: "managed".to_string(),
            settings: [
                (
                    "base_url".to_string(),
                    "https://api.openai.com/v1".to_string(),
                ),
                ("model".to_string(), "gpt-5.4".to_string()),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[tokio::test]
    async fn metadata_is_persistent_and_secret_bytes_never_enter_files() {
        let temp = tempfile::tempdir().unwrap();
        let bindings = ConnectionStoreBindings::new(
            temp.path().join("connections.toml"),
            temp.path().join("credentials"),
        )
        .unwrap();
        let service = ConnectionService::open("test", &bindings).unwrap();
        let shell = Shell::new(InProcessTransport::new(service.file_server()));
        let command = serde_json::json!({
            "op": "add_profile",
            "profile_id": "openai-main",
            "profile": profile(),
        });
        shell
            .write("/ctl", &serde_json::to_vec(&command).unwrap())
            .await
            .unwrap();
        assert!(bindings.metadata_path.is_file());
        shell
            .write(
                "/ctl",
                &serde_json::to_vec(&serde_json::json!({
                    "op": "request_native",
                    "request": {
                        "id": "login-1",
                        "profile_id": "openai-main",
                        "action": "secret_entry"
                    }
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        service
            .respond_native(NativeConnectionResponse {
                request_id: "login-1".to_string(),
                opaque_credential_ref: Some("host-keychain:openai-main".to_string()),
                status: "ready".to_string(),
            })
            .await
            .unwrap();
        let all = [
            shell.cat("/profiles").await.unwrap(),
            shell.cat("/native-responses").await.unwrap(),
        ]
        .concat();
        assert!(!String::from_utf8(all).unwrap().contains("sk-secret-value"));
    }

    #[tokio::test]
    async fn rejects_secret_material_instead_of_treating_it_as_reference() {
        let service = ConnectionService::ephemeral("test");
        assert!(
            service
                .respond_native(NativeConnectionResponse {
                    request_id: "r".to_string(),
                    opaque_credential_ref: Some("sk-secret-value".to_string()),
                    status: "ready".to_string(),
                })
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn callable_profiles_follow_metadata_and_native_readiness() {
        let service = ConnectionService::ephemeral("test");
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        let factory = Arc::new(TestLlmClientFactory::default());
        service
            .attach_callable_registry(
                llmfs.clone(),
                factory.clone(),
                Config::default(),
                "default".to_string(),
                LlmClient::new(MockLlmProvider::new()),
            )
            .await
            .unwrap();
        let control = Shell::new(InProcessTransport::new(service.file_server()));
        let callable = Shell::new(InProcessTransport::new(llmfs));
        assert_eq!(callable.ls("/connections").await.unwrap(), ["default"]);

        control
            .write(
                "/ctl",
                &serde_json::to_vec(&serde_json::json!({
                    "op": "add_profile",
                    "profile_id": "openai-main",
                    "profile": profile(),
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            callable
                .ls("/connections")
                .await
                .unwrap()
                .contains(&"openai-main".to_string())
        );

        control
            .write(
                "/ctl",
                &serde_json::to_vec(&serde_json::json!({
                    "op": "request_native",
                    "request": {
                        "id": "login-1",
                        "profile_id": "openai-main",
                        "action": "secret_entry"
                    }
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        let validation: BTreeMap<String, String> =
            serde_json::from_slice(&control.cat("/validation").await.unwrap()).unwrap();
        assert_eq!(
            validation.get("openai-main").map(String::as_str),
            Some("pending")
        );
        assert!(
            !callable
                .ls("/connections")
                .await
                .unwrap()
                .contains(&"openai-main".to_string())
        );

        control
            .write(
                "/native-responses",
                &serde_json::to_vec(&NativeConnectionResponse {
                    request_id: "login-1".to_string(),
                    opaque_credential_ref: Some("host-keychain:openai-main".to_string()),
                    status: "ready".to_string(),
                })
                .unwrap(),
            )
            .await
            .unwrap();
        let validation: BTreeMap<String, String> =
            serde_json::from_slice(&control.cat("/validation").await.unwrap()).unwrap();
        assert_eq!(
            validation.get("openai-main").map(String::as_str),
            Some("ready")
        );
        assert!(
            callable
                .ls("/connections")
                .await
                .unwrap()
                .contains(&"openai-main".to_string())
        );

        control
            .write(
                "/ctl",
                br#"{"op":"remove_profile","profile_id":"openai-main"}"#,
            )
            .await
            .unwrap();
        assert!(
            !callable
                .ls("/connections")
                .await
                .unwrap()
                .contains(&"openai-main".to_string())
        );

        factory
            .unavailable
            .lock()
            .unwrap()
            .insert("broken".to_string());
        control
            .write(
                "/ctl",
                &serde_json::to_vec(&serde_json::json!({
                    "op": "add_profile",
                    "profile_id": "broken",
                    "profile": profile(),
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        let validation: BTreeMap<String, String> =
            serde_json::from_slice(&control.cat("/validation").await.unwrap()).unwrap();
        assert_eq!(
            validation.get("broken").map(String::as_str),
            Some("unavailable")
        );
        assert!(
            !callable
                .ls("/connections")
                .await
                .unwrap()
                .contains(&"broken".to_string())
        );
    }
}
