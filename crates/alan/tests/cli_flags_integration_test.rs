use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn help_lists_ephemeral_and_cwd_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .arg("--help")
        .output()
        .expect("run alan --help");
    assert!(output.status.success(), "alan --help failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--ephemeral"),
        "missing --ephemeral:\n{stdout}"
    );
    assert!(
        stdout.contains("--no-resume-update"),
        "missing --no-resume-update:\n{stdout}"
    );
    assert!(
        stdout.contains("ALAN_NO_RESUME"),
        "missing ALAN_NO_RESUME:\n{stdout}"
    );
    assert!(stdout.contains("--cwd"), "missing --cwd:\n{stdout}");
}

#[test]
fn ephemeral_rejects_resume_and_subcommands() {
    let conflict = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--ephemeral", "--resume"])
        .output()
        .expect("run alan --ephemeral --resume");
    assert!(
        !conflict.status.success(),
        "--ephemeral --resume should fail"
    );
    let stderr = String::from_utf8_lossy(&conflict.stderr);
    assert!(
        stderr.contains("--ephemeral") && stderr.contains("--resume"),
        "unexpected stderr: {stderr}"
    );

    let subcmd = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--ephemeral", "connection", "list"])
        .output()
        .expect("run alan --ephemeral connection list");
    assert!(
        !subcmd.status.success(),
        "--ephemeral with subcommand should fail"
    );
    let sub_stderr = String::from_utf8_lossy(&subcmd.stderr);
    assert!(
        sub_stderr.contains("--ephemeral"),
        "unexpected stderr: {sub_stderr}"
    );
}

#[test]
fn cwd_rejects_missing_non_directory_and_subcommands() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("does-not-exist");
    let missing_out = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--cwd", missing.to_str().unwrap()])
        .output()
        .expect("run alan --cwd <missing>");
    assert!(!missing_out.status.success(), "--cwd missing should fail");

    let regular_file = tmp.path().join("file.txt");
    std::fs::write(&regular_file, b"hello").expect("write file");
    let file_out = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--cwd", regular_file.to_str().unwrap()])
        .output()
        .expect("run alan --cwd <file>");
    assert!(!file_out.status.success(), "--cwd file should fail");
    let file_stderr = String::from_utf8_lossy(&file_out.stderr);
    assert!(
        file_stderr.contains("not a directory"),
        "unexpected stderr: {file_stderr}"
    );

    let subcmd = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--cwd", tmp.path().to_str().unwrap(), "connection", "list"])
        .output()
        .expect("run alan --cwd <dir> connection list");
    assert!(
        !subcmd.status.success(),
        "--cwd with subcommand should fail"
    );
    let sub_stderr = String::from_utf8_lossy(&subcmd.stderr);
    assert!(
        sub_stderr.contains("--cwd"),
        "unexpected stderr: {sub_stderr}"
    );
}

#[test]
fn valid_ephemeral_and_cwd_pass_cli_validation_before_empty_stdin_check() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let xdg_data = tmp.path().join("xdg");
    let cwd = tmp.path().join("project");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(&xdg_data).expect("xdg");
    std::fs::create_dir_all(&cwd).expect("cwd");

    let out = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("TMPDIR", tmp.path())
        .env_remove("ALAN_INSTALL_CHANNEL")
        .args(["--ephemeral", "--cwd", cwd.to_str().unwrap()])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run alan --ephemeral --cwd");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("--ephemeral") && !stderr.contains("--cwd"),
        "valid --ephemeral --cwd should pass flag validation, got: {stderr}"
    );
}

#[test]
fn no_resume_update_rejects_subcommands_while_env_allows_them() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let xdg_data = tmp.path().join("xdg");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg_data).unwrap();

    let subcmd = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("TMPDIR", tmp.path())
        .env_remove("ALAN_INSTALL_CHANNEL")
        .args(["--no-resume-update", "connection", "list"])
        .output()
        .expect("run alan --no-resume-update connection list");
    assert!(
        !subcmd.status.success(),
        "--no-resume-update with subcommand should fail"
    );
    assert!(
        String::from_utf8_lossy(&subcmd.stderr).contains("--no-resume-update"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&subcmd.stderr)
    );

    let env_subcmd = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("TMPDIR", tmp.path())
        .env("ALAN_NO_RESUME", "1")
        .env_remove("ALAN_INSTALL_CHANNEL")
        .args(["connection", "list"])
        .output()
        .expect("run ALAN_NO_RESUME=1 alan connection list");
    assert!(
        env_subcmd.status.success(),
        "ambient ALAN_NO_RESUME=1 must not break subcommands: {}",
        String::from_utf8_lossy(&env_subcmd.stderr)
    );
}

fn detected_data_dir(home: &Path, xdg_data: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        xdg_data.to_path_buf()
    }
}

struct MockResponsesServer {
    addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MockResponsesServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock responses server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking mock listener");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let reqs_bg = Arc::clone(&requests);
        let stop_bg = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            while !stop_bg.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                        let mut content_length = 0usize;
                        let mut line = String::new();
                        loop {
                            line.clear();
                            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                                break;
                            }
                            let trimmed = line.trim_end_matches(['\r', '\n']);
                            if trimmed.is_empty() {
                                break;
                            }
                            if let Some((name, val)) = trimmed.split_once(':')
                                && name.eq_ignore_ascii_case("content-length")
                            {
                                content_length = val.trim().parse().unwrap_or(0);
                            }
                        }
                        let mut body = vec![0u8; content_length];
                        if content_length > 0 {
                            let _ = reader.read_exact(&mut body);
                        }
                        reqs_bg
                            .lock()
                            .unwrap()
                            .push(String::from_utf8_lossy(&body).to_string());

                        let sse = concat!(
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
                        );
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            sse.len(),
                            sse
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            requests,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for MockResponsesServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn isolated_alan(home: &Path, xdg_data: &Path, tmp: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_alan"));
    cmd.env("HOME", home)
        .env("XDG_DATA_HOME", xdg_data)
        .env("TMPDIR", tmp)
        .env("ALAN_NO_UPDATE_CHECK", "1")
        .env_remove("ALAN_INSTALL_CHANNEL")
        .env_remove("ALAN_CONFIG_PATH")
        .env_remove("ALAN_INSTANCE_RUNTIME_DIR")
        .env_remove("ALAN_NO_RESUME");
    cmd
}

#[test]
fn no_resume_update_flag_and_env_preserve_resume_pointer_and_personal_memory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let xdg_data = tmp.path().join("xdg");
    let runtime = tmp.path().join("rt");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg_data).unwrap();
    std::fs::create_dir_all(&runtime).unwrap();

    let server = MockResponsesServer::start();
    let base_setting = format!("base_url=http://{}/v1", server.addr);
    for args in [
        vec![
            "connection",
            "add",
            "openai_responses",
            "--profile",
            "mock-openai",
            "--setting",
            &base_setting,
            "--setting",
            "model=gpt-5.4",
            "--credential",
            "sk-test",
        ],
        vec![
            "connection",
            "set-secret",
            "mock-openai",
            "--value",
            "sk-test",
        ],
        vec!["connection", "default", "set", "mock-openai"],
    ] {
        let out = isolated_alan(&home, &xdg_data, &runtime)
            .args(&args)
            .output()
            .expect("configure mock connection");
        assert!(
            out.status.success(),
            "args={args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Seed durable memory (`MemoryDomain::Default` -> `services/memory/stores/default`)
    // that must still be loaded when skipping resume pointer updates.
    let system_store = detected_data_dir(&home, &xdg_data).join("Alan/System Store");
    let default_memory = system_store.join("services/memory/stores/default");
    std::fs::create_dir_all(&default_memory).unwrap();
    std::fs::write(
        default_memory.join("USER.md"),
        "# User Memory\n\nReader note style: concise-amber-1078\n",
    )
    .unwrap();

    let root_rollout_path = system_store.join("services/agent-runtime/metadata/root-rollout");

    // 1. Normal one-shot without flag sets the global resume pointer.
    let first = isolated_alan(&home, &xdg_data, &runtime)
        .args(["-p", "interactive session marker alpha"])
        .output()
        .expect("run initial alan");
    assert!(
        first.status.success(),
        "initial run failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&first.stdout).trim(), "ok");
    let selected_after_first =
        std::fs::read_to_string(&root_rollout_path).expect("root-rollout written on normal run");

    // 2. --no-resume-update leaves root-rollout untouched while loading personal memory.
    let flagged = isolated_alan(&home, &xdg_data, &runtime)
        .args(["--no-resume-update", "-p", "book passage marker beta"])
        .output()
        .expect("run --no-resume-update");
    assert!(
        flagged.status.success(),
        "--no-resume-update failed: {}",
        String::from_utf8_lossy(&flagged.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&flagged.stdout).trim(), "ok");
    assert_eq!(
        std::fs::read_to_string(&root_rollout_path).unwrap(),
        selected_after_first,
        "--no-resume-update must leave metadata/root-rollout untouched"
    );

    // 3. ALAN_NO_RESUME=1 on piped stdin leaves root-rollout untouched while loading personal memory.
    let mut child = isolated_alan(&home, &xdg_data, &runtime)
        .env("ALAN_NO_RESUME", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn piped ALAN_NO_RESUME=1");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"piped highlight marker gamma")
        .unwrap();
    let piped = child.wait_with_output().unwrap();
    assert!(
        piped.status.success(),
        "ALAN_NO_RESUME=1 failed: {}",
        String::from_utf8_lossy(&piped.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&piped.stdout).trim(), "ok");
    assert_eq!(
        std::fs::read_to_string(&root_rollout_path).unwrap(),
        selected_after_first,
        "ALAN_NO_RESUME=1 must leave metadata/root-rollout untouched"
    );

    // 4. `alan --resume` restores the first session, not the --no-resume-update / ALAN_NO_RESUME runs.
    let resumed = isolated_alan(&home, &xdg_data, &runtime)
        .args(["--resume", "-p", "resume check delta"])
        .output()
        .expect("run alan --resume");
    assert!(
        resumed.status.success(),
        "--resume failed: {}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let selected_after_resume = std::fs::read_to_string(&root_rollout_path).unwrap();
    assert_ne!(
        selected_after_resume, selected_after_first,
        "normal --resume without --no-resume-update updates pointer to new rollout"
    );

    // 5. Normal run without flag overwrites metadata/root-rollout as usual.
    let normal = isolated_alan(&home, &xdg_data, &runtime)
        .args(["-p", "normal run marker epsilon"])
        .output()
        .expect("run normal alan");
    assert!(normal.status.success());
    let selected_after_normal = std::fs::read_to_string(&root_rollout_path).unwrap();
    assert_ne!(selected_after_normal, selected_after_resume);

    let recorded = server.requests.lock().unwrap();
    let turn_requests: Vec<&String> = recorded
        .iter()
        .filter(|body| !body.contains("TURN-END MEMORY WRITE PLAN"))
        .collect();
    assert_eq!(
        turn_requests.len(),
        5,
        "expected 5 main LLM turn requests: {turn_requests:?}"
    );
    assert!(
        turn_requests[1].contains("concise-amber-1078"),
        "--no-resume-update must still load personal memory: {}",
        turn_requests[1]
    );
    assert!(
        turn_requests[2].contains("concise-amber-1078"),
        "ALAN_NO_RESUME=1 must still load personal memory: {}",
        turn_requests[2]
    );
    let resumed_req: serde_json::Value =
        serde_json::from_str(turn_requests[3]).expect("valid JSON request for resumed run");
    let resumed_user_inputs: String = resumed_req["input"]
        .as_array()
        .expect("input array on resumed request")
        .iter()
        .filter(|item| item["role"] == "user")
        .filter_map(|item| item["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        resumed_user_inputs.contains("interactive session marker alpha"),
        "--resume must restore the pre-no-resume session tape: {resumed_user_inputs}"
    );
    assert!(
        !resumed_user_inputs.contains("book passage marker beta")
            && !resumed_user_inputs.contains("piped highlight marker gamma"),
        "--resume must not restore --no-resume-update runs in tape: {resumed_user_inputs}"
    );
}
