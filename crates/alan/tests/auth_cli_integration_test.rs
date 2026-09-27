use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use alan_agent_engine::InstallChannel;
use alan_auth::{AuthStorage, AuthStore, ChatgptIdTokenInfo, ChatgptTokenData, StoredChatgptAuth};
use alan_os_host::{HostEndpointPaths, SystemStorePaths};
use base64::Engine;
use serde_json::json;

fn detected_data_dir(home: &Path, xdg_data: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        xdg_data.to_path_buf()
    }
}

fn runtime_base(root: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let uid = String::from_utf8(Command::new("id").arg("-u").output().unwrap().stdout).unwrap();
        root.join(format!("alan-os-{}", uid.trim()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        root.join("runtime")
    }
}

fn alan_command(home: &Path, xdg_data: &Path, runtime: &Path, args: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", home)
        .env("XDG_DATA_HOME", xdg_data)
        .env("XDG_RUNTIME_DIR", runtime_base(runtime))
        .env("TMPDIR", runtime)
        .env("ALAN_INSTALL_CHANNEL", "dev")
        .env_remove("ALAN_NATIVE_CONNECTION_REQUEST_ID")
        .env_remove("ALAN_INSTANCE_RUNTIME_DIR")
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

fn seed_chatgpt_auth(data_dir: &Path) {
    let host_store =
        alan_os_host::HostStorePaths::from_data_dir(data_dir, InstallChannel::Dev.descriptor().id)
            .unwrap();
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

#[test]
fn connection_cli_uses_the_connection_service_without_starting_a_host_or_root() {
    let temp = tempfile::tempdir_in("/tmp").unwrap();
    let runtime = temp.path().join("runtime-root");
    let home = temp.path().join("home");
    let xdg_data = temp.path().join("data");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(&runtime_base(&runtime), "dev").unwrap();

    alan_command(
        &home,
        &xdg_data,
        &runtime,
        &["connection", "add", "chatgpt", "--profile", "chatgpt-main"],
    );
    seed_chatgpt_auth(&detected_data_dir(&home, &xdg_data));
    let show = alan_command(
        &home,
        &xdg_data,
        &runtime,
        &["connection", "show", "chatgpt-main"],
    );
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("profile_id: chatgpt-main"));
    assert!(stdout.contains("provider: chatgpt"));
    assert!(stdout.contains("credential: configured"));
    assert!(!stdout.contains("user_123"));
    assert!(!stdout.contains("user@example.com"));

    let current = alan_command(&home, &xdg_data, &runtime, &["connection", "current"]);
    assert!(String::from_utf8_lossy(&current.stdout).contains("effective_profile: chatgpt-main"));
    let logout = alan_command(
        &home,
        &xdg_data,
        &runtime,
        &["connection", "logout", "chatgpt-main"],
    );
    assert!(
        String::from_utf8_lossy(&logout.stdout).contains("Removed credentials for chatgpt-main.")
    );

    alan_command(
        &home,
        &xdg_data,
        &runtime,
        &[
            "connection",
            "add",
            "openai_responses",
            "--profile",
            "openai-main",
            "--credential",
            "original-secret",
        ],
    );
    alan_command(
        &home,
        &xdg_data,
        &runtime,
        &[
            "connection",
            "edit",
            "openai-main",
            "--credential",
            "replacement-secret",
        ],
    );
    alan_command(
        &home,
        &xdg_data,
        &runtime,
        &[
            "connection",
            "set-secret",
            "openai-main",
            "--value",
            "sk-replacement",
        ],
    );
    let tested = alan_command(
        &home,
        &xdg_data,
        &runtime,
        &["connection", "test", "openai-main"],
    );
    assert!(String::from_utf8_lossy(&tested.stdout).contains("status: success"));

    assert!(
        !paths.root.exists(),
        "metadata commands created a Host endpoint"
    );
    let system =
        SystemStorePaths::from_data_dir(&detected_data_dir(&home, &xdg_data), "dev").unwrap();
    assert!(
        system
            .connection_bindings()
            .unwrap()
            .metadata_path
            .is_file()
    );
    assert!(!system.service("agent-runtime").unwrap().exists());

    let missing_target = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("XDG_RUNTIME_DIR", runtime_base(&runtime))
        .env("TMPDIR", &runtime)
        .env("ALAN_INSTALL_CHANNEL", "dev")
        .env("ALAN_NATIVE_CONNECTION_REQUEST_ID", "pending-request")
        .env_remove("ALAN_INSTANCE_RUNTIME_DIR")
        .args(["connection", "logout", "chatgpt-main"])
        .output()
        .unwrap();
    assert!(!missing_target.status.success());
    assert!(
        String::from_utf8_lossy(&missing_target.stderr)
            .contains("native Connection requests require ALAN_INSTANCE_RUNTIME_DIR")
    );
    assert!(!paths.root.exists());
}
