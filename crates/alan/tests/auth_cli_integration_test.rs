use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
        .env_remove("ALAN_INSTALL_CHANNEL")
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
    let host_store = alan_os_host::HostStorePaths::from_data_dir(data_dir).unwrap();
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
    let paths = HostEndpointPaths::from_runtime_dir(&runtime_base(&runtime)).unwrap();

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
    let mock_addr = spawn_models_mock_server("sk-replacement");
    let base_setting = format!("base_url=http://{mock_addr}/v1");
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
            "--setting",
            &base_setting,
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

    // Keyless Magpie works without --credential or stored secret and succeeds against local router.
    alan_command(
        &home,
        &xdg_data,
        &runtime,
        &[
            "connection",
            "add",
            "magpie",
            "--profile",
            "magpie-local",
            "--setting",
            &base_setting,
        ],
    );
    let magpie_tested = alan_command(
        &home,
        &xdg_data,
        &runtime,
        &["connection", "test", "magpie-local"],
    );
    assert!(String::from_utf8_lossy(&magpie_tested.stdout).contains("status: success"));

    // Invalid key fails real connectivity/auth check, while piped stdin updates the secret cleanly.
    let mut pipe_child = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("XDG_RUNTIME_DIR", runtime_base(&runtime))
        .env("TMPDIR", &runtime)
        .env_remove("ALAN_INSTALL_CHANNEL")
        .args(["connection", "set-secret", "openai-main"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write as _;
        pipe_child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"sk-unauthorized\n")
            .unwrap();
    }
    let pipe_out = pipe_child.wait_with_output().unwrap();
    assert!(
        pipe_out.status.success(),
        "piped set-secret failed: {}",
        String::from_utf8_lossy(&pipe_out.stderr)
    );
    let failed_test = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("XDG_RUNTIME_DIR", runtime_base(&runtime))
        .env("TMPDIR", &runtime)
        .env_remove("ALAN_INSTALL_CHANNEL")
        .args(["connection", "test", "openai-main"])
        .output()
        .unwrap();
    assert!(!failed_test.status.success());
    assert!(
        String::from_utf8_lossy(&failed_test.stderr).contains("authentication failed"),
        "expected HTTP auth error, got: {}",
        String::from_utf8_lossy(&failed_test.stderr)
    );

    // Non-first-party Anthropic is excluded from `connection add` and `connection list` built-ins.
    let anthropic_add = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("XDG_RUNTIME_DIR", runtime_base(&runtime))
        .env("TMPDIR", &runtime)
        .env_remove("ALAN_INSTALL_CHANNEL")
        .args(["connection", "add", "anthropic"])
        .output()
        .unwrap();
    assert!(!anthropic_add.status.success());
    let listed = alan_command(&home, &xdg_data, &runtime, &["connection", "list"]);
    let listed_out = String::from_utf8_lossy(&listed.stdout);
    assert!(listed_out.contains("magpie-local | provider=magpie | credential=unset"));
    assert!(listed_out.contains("first_party_providers: magpie"));
    assert!(!listed_out.contains("anthropic_messages"));

    assert!(
        !paths.root.exists(),
        "metadata commands created a Host endpoint"
    );
    let system = SystemStorePaths::from_data_dir(&detected_data_dir(&home, &xdg_data)).unwrap();
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
        .env_remove("ALAN_INSTALL_CHANNEL")
        .env("ALAN_NATIVE_CONNECTION_REQUEST_ID", "pending-request")
        .env_remove("ALAN_INSTANCE_RUNTIME_DIR")
        .args(["connection", "logout", "chatgpt-main"])
        .output()
        .unwrap();
    assert!(!missing_target.status.success());
    assert!(
        String::from_utf8_lossy(&missing_target.stderr)
            .contains("select a live Alan instance with ALAN_INSTANCE_RUNTIME_DIR")
    );
    assert!(!paths.root.exists());
}

fn spawn_models_mock_server(expected_token: &str) -> std::net::SocketAddr {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock listener");
    let addr = listener.local_addr().expect("mock listener addr");
    let expected = format!("authorization: bearer {expected_token}");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
            let req = String::from_utf8_lossy(&buf).to_ascii_lowercase();
            let has_auth = req.lines().any(|line| line.starts_with("authorization:"));
            let authorized = !has_auth || req.lines().any(|line| line.trim() == expected);
            let response = if authorized {
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"data\":[]}"
            } else {
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            };
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}
