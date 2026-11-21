use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

use alan_auth::{AuthStorage, AuthStore, ChatgptIdTokenInfo, ChatgptTokenData, StoredChatgptAuth};
use alan_os_host::{HostStorePaths, SecretStore, SystemStorePaths};
use base64::Engine;
use serde_json::json;

fn detected_data_dir(home: &Path, xdg_data: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        xdg_data.to_path_buf()
    }
}

fn alan_doctor(home: &Path, xdg_data: &Path, args: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", home)
        .env("XDG_DATA_HOME", xdg_data)
        .env_remove("ALAN_CONFIG_PATH")
        .env_remove("ALAN_INSTALL_CHANNEL")
        .env_remove("ALAN_NATIVE_CONNECTION_REQUEST_ID")
        .env_remove("ALAN_INSTANCE_RUNTIME_DIR")
        .env_remove("GOOGLE_APPLICATION_CREDENTIALS")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "args={args:?} output={output:?}");
    output
}

fn build_jwt(payload: serde_json::Value) -> String {
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.sig")
}

#[test]
fn doctor_cli_empty_stores_is_readonly_and_fast() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("xdg");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg_data).unwrap();
    let data_dir = detected_data_dir(&home, &xdg_data);
    std::fs::create_dir_all(&data_dir).unwrap();

    let started = Instant::now();
    let output = alan_doctor(&home, &xdg_data, &["doctor", "--json"]);
    let elapsed = started.elapsed();
    // Allow generous headroom for cold debug-binary process spawn on CI while still
    // catching any accidental remote HTTP calls or blocking loops.
    assert!(
        elapsed.as_secs() < 5,
        "alan doctor --json took unexpectedly long: {elapsed:?}"
    );

    let system = SystemStorePaths::from_data_dir(&data_dir).unwrap();
    let host = HostStorePaths::from_data_dir(&data_dir).unwrap();
    assert!(
        !system.root.exists(),
        "alan doctor must not create System Store"
    );
    assert!(
        !host.credentials.exists(),
        "alan doctor must not create Host Store"
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["healthy"], true);
    assert_eq!(report["version"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["environment"]["system_store"]["exists"], false);
    assert_eq!(report["environment"]["system_store"]["writable"], true);
    assert_eq!(
        report["connections"]["profiles"].as_array().unwrap().len(),
        0
    );
    assert_eq!(report["rollouts"]["rollout_count"], 0);
}

#[test]
fn doctor_cli_reports_profiles_rollouts_and_never_leaks_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("xdg");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg_data).unwrap();
    let data_dir = detected_data_dir(&home, &xdg_data);
    std::fs::create_dir_all(&data_dir).unwrap();

    // Add OpenAI profile and store a secret sentinel.
    let api_key_sentinel = "sk-live-doctor-integration-secret-value-xyz";
    let _ = alan_doctor(
        &home,
        &xdg_data,
        &[
            "connection",
            "add",
            "openai_responses",
            "--profile",
            "openai-main",
            "--setting",
            "model=gpt-5.4",
            "--default",
        ],
    );
    let _ = alan_doctor(
        &home,
        &xdg_data,
        &[
            "connection",
            "set-secret",
            "openai-main",
            "--value",
            api_key_sentinel,
        ],
    );

    // Add ChatGPT profile and seed managed OAuth with secret token sentinels.
    let _ = alan_doctor(
        &home,
        &xdg_data,
        &["connection", "add", "chatgpt", "--profile", "chatgpt-main"],
    );
    let host = HostStorePaths::from_data_dir(&data_dir).unwrap();
    std::fs::create_dir_all(host.managed_auth.parent().unwrap()).unwrap();
    let refresh_sentinel = "rt-super-secret-refresh-token-value-abc";
    let id_token = build_jwt(json!({
        "email": "doctor@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_user_id": "user_doc",
            "chatgpt_account_id": "acct_doc"
        }
    }));
    let access_token = build_jwt(json!({"exp": 4_102_444_800_i64}));
    AuthStorage::new(host.managed_auth.clone())
        .unwrap()
        .save(&AuthStore {
            version: 1,
            chatgpt: Some(
                StoredChatgptAuth::from_tokens(ChatgptTokenData {
                    id_token: ChatgptIdTokenInfo {
                        email: Some("doctor@example.com".to_string()),
                        plan_type: Some("pro".to_string()),
                        user_id: Some("user_doc".to_string()),
                        account_id: Some("acct_doc".to_string()),
                        raw_jwt: id_token.clone(),
                    },
                    access_token: access_token.clone(),
                    refresh_token: refresh_sentinel.to_string(),
                })
                .unwrap(),
            ),
            google: None,
        })
        .unwrap();

    // Verify SecretStore actually has the secret we want to ensure never leaks.
    assert_eq!(
        SecretStore::from_directory(&host.credentials)
            .unwrap()
            .load("openai-main")
            .unwrap()
            .as_deref(),
        Some(api_key_sentinel)
    );

    // Seed one rollout file.
    let system = SystemStorePaths::from_data_dir(&data_dir).unwrap();
    let rollouts_dir = system.agent_runtime().unwrap().rollouts;
    std::fs::create_dir_all(&rollouts_dir).unwrap();
    std::fs::write(rollouts_dir.join("rollout-test-1.jsonl"), "{}\n").unwrap();

    let human_out = alan_doctor(&home, &xdg_data, &["doctor"]);
    let human = String::from_utf8(human_out.stdout).unwrap();
    let json_out = alan_doctor(&home, &xdg_data, &["doctor", "--json"]);
    let json_text = String::from_utf8(json_out.stdout).unwrap();

    for forbidden in [api_key_sentinel, refresh_sentinel, &id_token, &access_token] {
        assert!(!human.contains(forbidden), "human output leaked secret");
        assert!(!json_text.contains(forbidden), "json output leaked secret");
    }

    assert!(human.contains("Alan Doctor"));
    assert!(human.contains("openai-main"));
    assert!(human.contains("gpt-5.4"));
    assert!(human.contains("chatgpt-main"));
    assert!(human.contains("rollout-test-1.jsonl"));

    let report: serde_json::Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(report["healthy"], true);
    assert_eq!(report["connections"]["default_profile"], "openai-main");
    assert_eq!(
        report["connections"]["missing_auth_profiles"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(report["rollouts"]["rollout_count"], 1);
}
