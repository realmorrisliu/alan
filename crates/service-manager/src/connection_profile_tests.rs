use super::*;
use tempfile::TempDir;

#[test]
fn normalize_profile_settings_applies_defaults() {
    let settings = BTreeMap::from([("model".to_string(), "gpt-5".to_string())]);
    let normalized = normalize_profile_settings(LlmProvider::OpenAiResponses, &settings);
    assert_eq!(
        normalized.get("base_url").map(String::as_str),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(normalized.get("model").map(String::as_str), Some("gpt-5"));
}

#[test]
fn first_party_provider_catalog_exposes_only_four_official_providers() {
    let first_party: Vec<LlmProvider> = first_party_provider_catalog()
        .iter()
        .map(|descriptor| descriptor.provider_id)
        .collect();
    assert_eq!(
        first_party,
        vec![
            LlmProvider::Magpie,
            LlmProvider::OpenAiResponses,
            LlmProvider::Chatgpt,
            LlmProvider::GoogleGeminiGenerateContent,
            LlmProvider::OpenRouter,
        ]
    );
    assert!(!first_party.contains(&LlmProvider::AnthropicMessages));
    assert!(!first_party.contains(&LlmProvider::OpenAiChatCompletions));
    assert!(!first_party.contains(&LlmProvider::OpenAiChatCompletionsCompatible));
    assert!(!first_party.contains(&LlmProvider::TypesafeEvaluation));

    // Anthropic remains resolvable in the protocol catalog for existing profiles / q packages.
    let anthropic = ConnectionsFile::profile_descriptor(LlmProvider::AnthropicMessages);
    assert!(!anthropic.first_party);
    let anthropic_defaults =
        normalize_profile_settings(LlmProvider::AnthropicMessages, &BTreeMap::new());
    assert_eq!(
        anthropic_defaults.get("model").map(String::as_str),
        Some("claude-sonnet-4-5")
    );
}

#[test]
fn magpie_profile_allows_missing_credential_and_projects_thinking_map() {
    let descriptor = ConnectionsFile::profile_descriptor(LlmProvider::Magpie);
    assert!(descriptor.first_party);
    assert!(descriptor.credential_optional);
    assert!(descriptor.supports_secret_entry);

    let file = ConnectionsFile {
        default_profile: Some("magpie-local".to_string()),
        profiles: BTreeMap::from([(
            "magpie-local".to_string(),
            ConnectionProfile {
                provider: LlmProvider::Magpie,
                label: Some("Local Magpie".to_string()),
                credential_id: None,
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: BTreeMap::from([(
                    "thinking_level_map".to_string(),
                    r#"{"high":"deep","none":null}"#.to_string(),
                )]),
            },
        )]),
        ..ConnectionsFile::default()
    };

    let mut config = Config::default();
    let resolved = file
        .apply_profile_metadata_to_config(Some("magpie-local"), &mut config)
        .unwrap();
    assert_eq!(resolved.provider, LlmProvider::Magpie);
    assert_eq!(resolved.credential_id, None);
    assert_eq!(config.llm_provider, LlmProvider::Magpie);
    assert_eq!(config.magpie_base_url, "http://127.0.0.1:3425/v1");
    assert_eq!(config.magpie_model, "default");
    assert_eq!(config.magpie_api_key, None);
    let map = config.magpie_thinking_level_map.as_ref().unwrap();
    assert_eq!(map.get("high").and_then(Option::as_deref), Some("deep"));
    assert_eq!(map.get("none").and_then(Option::as_deref), None);
}

#[test]
fn vertex_profile_supports_adc_and_express_mode() {
    let adc_file = ConnectionsFile {
        profiles: BTreeMap::from([(
            "vertex-adc".to_string(),
            ConnectionProfile {
                provider: LlmProvider::GoogleGeminiGenerateContent,
                label: None,
                credential_id: None,
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: BTreeMap::from([("project_id".to_string(), "my-gcp-proj".to_string())]),
            },
        )]),
        ..ConnectionsFile::default()
    };
    let mut adc_config = Config::default();
    adc_file
        .apply_profile_metadata_to_config(Some("vertex-adc"), &mut adc_config)
        .unwrap();
    assert_eq!(
        adc_config
            .google_gemini_generate_content_project_id
            .as_deref(),
        Some("my-gcp-proj")
    );
    assert_eq!(adc_config.google_gemini_generate_content_location, "global");
    assert_eq!(
        adc_config.google_gemini_generate_content_model,
        "gemini-2.5-flash"
    );

    // Express Mode: API key credential with no project_id required.
    let express_file = ConnectionsFile {
        credentials: BTreeMap::from([(
            "vertex-key".to_string(),
            ConnectionCredential {
                kind: CredentialKind::SecretString,
                provider_family: LlmProvider::GoogleGeminiGenerateContent,
                label: "Vertex Express".to_string(),
                backend: SECRET_STORE_BACKEND.to_string(),
            },
        )]),
        profiles: BTreeMap::from([(
            "vertex-express".to_string(),
            ConnectionProfile {
                provider: LlmProvider::GoogleGeminiGenerateContent,
                label: None,
                credential_id: Some("vertex-key".to_string()),
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: BTreeMap::new(),
            },
        )]),
        ..ConnectionsFile::default()
    };
    let mut express_config = Config::default();
    let resolved = express_file
        .apply_profile_metadata_to_config(Some("vertex-express"), &mut express_config)
        .unwrap();
    assert_eq!(resolved.credential_kind, CredentialKind::SecretString);
    assert_eq!(
        express_config.google_gemini_generate_content_project_id,
        None
    );

    // ADC without project_id fails validation on resolve.
    let missing_project_adc = ConnectionsFile {
        profiles: BTreeMap::from([(
            "vertex-broken".to_string(),
            ConnectionProfile {
                provider: LlmProvider::GoogleGeminiGenerateContent,
                label: None,
                credential_id: None,
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: BTreeMap::new(),
            },
        )]),
        ..ConnectionsFile::default()
    };
    assert!(
        missing_project_adc
            .resolve_profile(Some("vertex-broken"))
            .is_err()
    );
}

#[test]
fn openrouter_descriptor_exposes_default_model_and_metadata_settings() {
    let descriptor = ConnectionsFile::profile_descriptor(LlmProvider::OpenRouter);
    assert_eq!(descriptor.provider_id, LlmProvider::OpenRouter);
    assert_eq!(descriptor.credential_kind, CredentialKind::SecretString);
    assert!(descriptor.first_party);
    assert!(descriptor.supports_secret_entry);
    assert!(descriptor.required_settings.contains(&"model"));
    assert!(descriptor.optional_settings.contains(&"base_url"));
    assert!(descriptor.optional_settings.contains(&"http_referer"));
    assert!(descriptor.optional_settings.contains(&"x_title"));
    assert!(descriptor.optional_settings.contains(&"app_categories"));

    let normalized = normalize_profile_settings(LlmProvider::OpenRouter, &BTreeMap::new());
    assert_eq!(
        normalized.get("base_url").map(String::as_str),
        Some("https://openrouter.ai/api/v1")
    );
    assert_eq!(
        normalized.get("model").map(String::as_str),
        Some("moonshotai/kimi-k2.6")
    );
    validate_profile_settings(LlmProvider::OpenRouter, &normalized).unwrap();
}

#[test]
fn openrouter_settings_are_not_accepted_by_generic_compatible_provider() {
    let settings = BTreeMap::from([
        (
            "base_url".to_string(),
            "https://proxy.example/v1".to_string(),
        ),
        ("model".to_string(), "qwen3.5-plus".to_string()),
        ("http_referer".to_string(), "https://alan.local".to_string()),
    ]);

    let result = validate_profile_settings(LlmProvider::OpenAiChatCompletionsCompatible, &settings);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("http_referer"));
}

#[test]
fn saving_metadata_preserves_open_readers_and_rejects_invalid_replacement() {
    use std::io::Read;

    let directory = TempDir::new().unwrap();
    let path = directory.path().join("connections.toml");
    let original = ConnectionsFile::default();
    original.save_to_path(&path).unwrap();
    let mut existing_reader = std::fs::File::open(&path).unwrap();
    let updated = ConnectionsFile {
        default_profile: Some("updated".into()),
        ..original.clone()
    };
    updated.save_to_path(&path).unwrap();
    let mut previous_bytes = String::new();
    existing_reader.read_to_string(&mut previous_bytes).unwrap();
    assert_eq!(
        toml::from_str::<ConnectionsFile>(&previous_bytes).unwrap(),
        original
    );
    assert_eq!(ConnectionsFile::load_from_path(&path).unwrap().0, updated);
    let invalid = ConnectionsFile {
        version: CONNECTIONS_VERSION + 1,
        ..updated.clone()
    };
    assert!(invalid.save_to_path(&path).is_err());
    assert_eq!(ConnectionsFile::load_from_path(&path).unwrap().0, updated);
    let names = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        ["connections.toml", "connections.toml.lock"]
            .map(std::ffi::OsString::from)
            .into()
    );
}

#[test]
fn dev_connection_store_does_not_fall_back_to_stable_store() {
    let temp = TempDir::new().unwrap();
    let stable_path = temp.path().join("system-store/stable/connections.toml");
    let dev_path = temp.path().join("system-store/dev/connections.toml");
    let stable_connections = ConnectionsFile {
        default_profile: Some("stable-main".to_string()),
        profiles: BTreeMap::from([(
            "stable-main".to_string(),
            ConnectionProfile {
                provider: LlmProvider::OpenAiResponses,
                label: Some("Stable".to_string()),
                credential_id: Some("stable-main".to_string()),
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: BTreeMap::new(),
            },
        )]),
        ..ConnectionsFile::default()
    };
    stable_connections
        .save_to_path(&stable_path)
        .expect("save stable connections");

    let (dev_connections, loaded_path) =
        ConnectionsFile::load_from_path(&dev_path).expect("load dev connections");

    assert_eq!(loaded_path.as_deref(), Some(dev_path.as_path()));
    assert_eq!(dev_connections.default_profile, None);
    assert!(dev_connections.profiles.is_empty());
}

#[test]
fn resolve_profile_uses_default_profile() {
    let file = ConnectionsFile {
        default_profile: Some("chatgpt-main".to_string()),
        credentials: BTreeMap::from([(
            "chatgpt".to_string(),
            ConnectionCredential {
                kind: CredentialKind::ManagedOauth,
                provider_family: LlmProvider::Chatgpt,
                label: "ChatGPT login".to_string(),
                backend: CHATGPT_AUTH_BACKEND.to_string(),
            },
        )]),
        profiles: BTreeMap::from([(
            "chatgpt-main".to_string(),
            ConnectionProfile {
                provider: LlmProvider::Chatgpt,
                label: Some("ChatGPT".to_string()),
                credential_id: Some("chatgpt".to_string()),
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: BTreeMap::new(),
            },
        )]),
        ..ConnectionsFile::default()
    };

    let resolved = file.resolve_profile(None).unwrap();
    assert_eq!(resolved.profile_id, "chatgpt-main");
    assert_eq!(
        resolved.settings.get("model").map(String::as_str),
        Some("gpt-5.4")
    );
    assert_eq!(
        resolved.settings.get("base_url").map(String::as_str),
        Some("https://api.openai.com/v1")
    );
}

#[test]
fn service_owned_profile_metadata_does_not_load_secret_material() {
    let file = ConnectionsFile {
        credentials: BTreeMap::from([(
            "openrouter-main".to_string(),
            ConnectionCredential {
                kind: CredentialKind::SecretString,
                provider_family: LlmProvider::OpenRouter,
                label: "OpenRouter credential".to_string(),
                backend: SECRET_STORE_BACKEND.to_string(),
            },
        )]),
        profiles: BTreeMap::from([(
            "openrouter-main".to_string(),
            ConnectionProfile {
                provider: LlmProvider::OpenRouter,
                label: None,
                credential_id: Some("openrouter-main".to_string()),
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: BTreeMap::new(),
            },
        )]),
        ..ConnectionsFile::default()
    };
    let mut config = Config::default();

    file.apply_profile_metadata_to_config(Some("openrouter-main"), &mut config)
        .unwrap();

    assert_eq!(config.llm_provider, LlmProvider::OpenRouter);
    assert_eq!(
        config.connection_profile.as_deref(),
        Some("openrouter-main")
    );
    assert_eq!(config.openrouter_api_key, None);
}

#[test]
fn metadata_lock_excludes_another_process() {
    const ENV: &str = "ALAN_TEST_CONNECTION_LOCK_PATH";
    if let Some(path) = std::env::var_os(ENV) {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        assert!(matches!(
            file.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        return;
    }
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("connections.toml");
    let lock = lock_metadata(&path).unwrap();
    let lock_path = temp.path().join("connections.toml.lock");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "connection_profile::tests::metadata_lock_excludes_another_process",
        ])
        .env(ENV, &lock_path)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    drop(lock);
    let released = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    released.try_lock().unwrap();
}

#[test]
fn normalize_profile_settings_and_resolve_migrate_retired_codex_base_url() {
    let legacy = BTreeMap::from([
        (
            "base_url".to_string(),
            "https://chatgpt.com/backend-api/codex/".to_string(),
        ),
        ("model".to_string(), "gpt-5.3-codex".to_string()),
    ]);
    let normalized = normalize_profile_settings(LlmProvider::Chatgpt, &legacy);
    assert_eq!(
        normalized.get("base_url").map(String::as_str),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(
        normalized.get("model").map(String::as_str),
        Some("gpt-5.3-codex")
    );

    let file = ConnectionsFile {
        default_profile: Some("chatgpt-main".to_string()),
        credentials: BTreeMap::from([(
            "chatgpt".to_string(),
            ConnectionCredential {
                kind: CredentialKind::ManagedOauth,
                provider_family: LlmProvider::Chatgpt,
                label: "ChatGPT".to_string(),
                backend: CHATGPT_AUTH_BACKEND.to_string(),
            },
        )]),
        profiles: BTreeMap::from([(
            "chatgpt-main".to_string(),
            ConnectionProfile {
                provider: LlmProvider::Chatgpt,
                label: None,
                credential_id: Some("chatgpt".to_string()),
                created_at: default_profile_timestamp(),
                updated_at: default_profile_timestamp(),
                source: default_profile_source(),
                settings: legacy,
            },
        )]),
        ..ConnectionsFile::default()
    };
    let resolved = file.resolve_profile(None).unwrap();
    assert_eq!(
        resolved.settings.get("base_url").map(String::as_str),
        Some("https://api.openai.com/v1")
    );

    let custom = BTreeMap::from([(
        "base_url".to_string(),
        "http://127.0.0.1:8080/v1".to_string(),
    )]);
    assert_eq!(
        normalize_profile_settings(LlmProvider::Chatgpt, &custom)
            .get("base_url")
            .map(String::as_str),
        Some("http://127.0.0.1:8080/v1")
    );
}
