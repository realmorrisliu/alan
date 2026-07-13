use alan_agent_engine::{
    ConnectionCredential, ConnectionProfile, ConnectionsFile, CredentialKind, InstallChannel,
    LlmProvider, default_credential_backend,
};
use alan_auth::{AuthStorage, AuthStore, ChatgptIdTokenInfo, ChatgptTokenData, StoredChatgptAuth};
use base64::Engine;
use chrono::Utc;
use serde_json::json;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn build_jwt(payload: serde_json::Value) -> String {
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.sig")
}

fn seed_chatgpt_auth(home: &Path) {
    let data_dir = home.join("Library/Application Support");
    let host_store =
        alan::HostStorePaths::from_data_dir(&data_dir, InstallChannel::Stable).unwrap();
    std::fs::create_dir_all(host_store.managed_auth.parent().unwrap()).unwrap();
    let storage = AuthStorage::new(host_store.managed_auth).unwrap();
    let id_token = build_jwt(json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_user_id": "user_123",
            "chatgpt_account_id": "acct_123"
        }
    }));
    let access_token = build_jwt(json!({"exp": 4_102_444_800_i64}));

    storage
        .save(&AuthStore {
            version: 1,
            chatgpt: Some(
                StoredChatgptAuth::from_tokens(ChatgptTokenData {
                    id_token: ChatgptIdTokenInfo {
                        email: Some("user@example.com".to_string()),
                        plan_type: Some("pro".to_string()),
                        user_id: Some("user_123".to_string()),
                        account_id: Some("acct_123".to_string()),
                        raw_jwt: id_token,
                    },
                    access_token,
                    refresh_token: "refresh_token".to_string(),
                })
                .unwrap(),
            ),
        })
        .unwrap();
}

fn seed_chatgpt_connection(home: &Path) {
    let data_dir = home.join("Library/Application Support");
    let system_store =
        alan::SystemStorePaths::from_data_dir(&data_dir, InstallChannel::Stable).unwrap();
    let mut connections = ConnectionsFile {
        version: 1,
        default_profile: Some("chatgpt-main".to_string()),
        credentials: std::collections::BTreeMap::new(),
        profiles: std::collections::BTreeMap::new(),
    };
    connections.credentials.insert(
        "chatgpt".to_string(),
        ConnectionCredential {
            kind: CredentialKind::ManagedOauth,
            provider_family: LlmProvider::Chatgpt,
            label: "ChatGPT managed login".to_string(),
            backend: default_credential_backend(CredentialKind::ManagedOauth).to_string(),
        },
    );
    connections.profiles.insert(
        "chatgpt-main".to_string(),
        ConnectionProfile {
            provider: LlmProvider::Chatgpt,
            label: None,
            credential_id: Some("chatgpt".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            source: "managed".to_string(),
            settings: std::collections::BTreeMap::from([
                (
                    "base_url".to_string(),
                    "https://chatgpt.com/backend-api/codex".to_string(),
                ),
                ("model".to_string(), "gpt-5.3-codex".to_string()),
                ("account_id".to_string(), "".to_string()),
            ]),
        },
    );
    connections
        .save_to_path(&system_store.connections_metadata().unwrap())
        .unwrap();
}

#[test]
fn connection_show_reports_managed_chatgpt_login() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    seed_chatgpt_auth(&home);
    seed_chatgpt_connection(&home);

    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["connection", "show", "chatgpt-main"])
        .env("HOME", &home)
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("profile_id: chatgpt-main"));
    assert!(stdout.contains("provider: chatgpt"));
    assert!(stdout.contains("credential: configured"));
    assert!(stdout.contains("settings_keys: account_id, base_url, model"));
    assert!(!stdout.contains("user_123"));
    assert!(!stdout.contains("user@example.com"));
}

#[test]
fn connection_logout_removes_managed_chatgpt_login() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    seed_chatgpt_auth(&home);
    seed_chatgpt_connection(&home);

    let logout = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["connection", "logout", "chatgpt-main"])
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(logout.status.success(), "{logout:?}");
    assert!(
        String::from_utf8_lossy(&logout.stdout).contains("Removed credentials for chatgpt-main.")
    );

    let status = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["connection", "show", "chatgpt-main"])
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    assert!(String::from_utf8_lossy(&status.stdout).contains("provider: chatgpt"));
}

#[test]
fn connection_default_and_current_use_system_metadata() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    seed_chatgpt_connection(&home);

    let set_default = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["connection", "default", "set", "chatgpt-main"])
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(set_default.status.success(), "{set_default:?}");

    let current = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["connection", "current"])
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(current.status.success(), "{current:?}");
    let stdout = String::from_utf8_lossy(&current.stdout);
    assert!(stdout.contains("effective_profile: chatgpt-main"));
    assert!(stdout.contains("source: default_profile"));
}
