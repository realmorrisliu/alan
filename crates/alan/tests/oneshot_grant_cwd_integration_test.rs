use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct ToolLoopResponsesServer {
    addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ToolLoopResponsesServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock responses server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking mock listener");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let step = Arc::new(AtomicUsize::new(0));
        let reqs_bg = Arc::clone(&requests);
        let stop_bg = Arc::clone(&stop);
        let step_bg = Arc::clone(&step);

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
                        let body_text = String::from_utf8_lossy(&body).to_string();
                        let is_memory_plan = body_text.contains("TURN-END MEMORY WRITE PLAN");
                        reqs_bg.lock().unwrap().push(body_text);

                        let sse = if is_memory_plan {
                            concat!(
                                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"{\\\"writes\\\":[]}\"}\n\n",
                                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_mem\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
                            )
                            .to_string()
                        } else {
                            match step_bg.fetch_add(1, Ordering::SeqCst) {
                                0 => {
                                    let args = serde_json::json!({
                                        "command": "cat marker.txt && pwd"
                                    })
                                    .to_string();
                                    format!(
                                        "data: {}\n\ndata: {}\n\n",
                                        serde_json::json!({
                                            "type": "response.output_item.done",
                                            "output_index": 0,
                                            "item": {
                                                "type": "function_call",
                                                "call_id": "call_pwd_1",
                                                "name": "bash",
                                                "arguments": args
                                            }
                                        }),
                                        serde_json::json!({
                                            "type": "response.completed",
                                            "response": {
                                                "id": "resp_1",
                                                "output": [{
                                                    "type": "function_call",
                                                    "call_id": "call_pwd_1",
                                                    "name": "bash",
                                                    "arguments": args
                                                }],
                                                "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
                                            }
                                        })
                                    )
                                }
                                1 => {
                                    let args = serde_json::json!({
                                        "command": "cat sub.txt && pwd",
                                        "cwd": "subdir"
                                    })
                                    .to_string();
                                    format!(
                                        "data: {}\n\ndata: {}\n\n",
                                        serde_json::json!({
                                            "type": "response.output_item.done",
                                            "output_index": 0,
                                            "item": {
                                                "type": "function_call",
                                                "call_id": "call_pwd_2",
                                                "name": "bash",
                                                "arguments": args
                                            }
                                        }),
                                        serde_json::json!({
                                            "type": "response.completed",
                                            "response": {
                                                "id": "resp_2",
                                                "output": [{
                                                    "type": "function_call",
                                                    "call_id": "call_pwd_2",
                                                    "name": "bash",
                                                    "arguments": args
                                                }],
                                                "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
                                            }
                                        })
                                    )
                                }
                                _ => concat!(
                                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"verified\"}\n\n",
                                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_3\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
                                )
                                .to_string(),
                            }
                        };
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

impl Drop for ToolLoopResponsesServer {
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
fn oneshot_grant_sets_namespace_cwd_for_bash_and_fails_closed_for_ungranted_cwd() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let xdg_data = tmp.path().join("xdg");
    let runtime = tmp.path().join("rt");
    let ambient = tmp.path().join("ambient");
    let repo = tmp.path().join("repo");
    let subdir = repo.join("subdir");
    let ungranted = tmp.path().join("ungranted");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg_data).unwrap();
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::create_dir_all(&ambient).unwrap();
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::create_dir_all(&ungranted).unwrap();
    std::fs::write(repo.join("marker.txt"), "root-marker\n").unwrap();
    std::fs::write(subdir.join("sub.txt"), "sub-marker\n").unwrap();

    let server = ToolLoopResponsesServer::start();
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
        vec!["host", "grant", "save", "repo-wt", repo.to_str().unwrap()],
    ] {
        let out = isolated_alan(&home, &xdg_data, &runtime)
            .args(&args)
            .output()
            .expect("setup connection and saved grant");
        assert!(
            out.status.success(),
            "setup {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // 1. --cwd without --grant fails closed before hitting the LLM, with a non-interactive hint.
    let ungranted_out = isolated_alan(&home, &xdg_data, &runtime)
        .current_dir(&ambient)
        .args([
            "--cwd",
            ungranted.to_str().unwrap(),
            "--ephemeral",
            "--json",
            "-p",
            "hello",
        ])
        .output()
        .expect("run ungranted --cwd");
    assert!(!ungranted_out.status.success());
    let ungranted_stdout = String::from_utf8_lossy(&ungranted_out.stdout);
    let ungranted_stderr = String::from_utf8_lossy(&ungranted_out.stderr);
    assert!(
        ungranted_stdout.contains("alan host grant save")
            && ungranted_stderr.contains("--grant <id>"),
        "expected non-interactive grant hint, stdout={ungranted_stdout} stderr={ungranted_stderr}"
    );
    assert!(
        server.requests.lock().unwrap().is_empty(),
        "ungranted --cwd must fail before calling LLM"
    );

    // 2. --cwd matching a saved grant without --grant also fails closed and names `--grant repo-wt`.
    let saved_unpassed = isolated_alan(&home, &xdg_data, &runtime)
        .current_dir(&ambient)
        .args([
            "--cwd",
            subdir.to_str().unwrap(),
            "--ephemeral",
            "-p",
            "hello",
        ])
        .output()
        .expect("run saved grant path without --grant");
    assert!(!saved_unpassed.status.success());
    let saved_stderr = String::from_utf8_lossy(&saved_unpassed.stderr);
    assert!(
        saved_stderr.contains("--grant repo-wt"),
        "expected saved grant hint, got: {saved_stderr}"
    );

    // 3. --grant repo-wt from an unrelated ambient directory resolves namespace_cwd to
    //    `/mnt/grants/repo-wt` and runs bash cleanly in both grant root and `cwd: "subdir"`.
    let granted_out = isolated_alan(&home, &xdg_data, &runtime)
        .current_dir(&ambient)
        .args([
            "--grant",
            "repo-wt",
            "--ephemeral",
            "--trust",
            "-p",
            "verify bash pwd",
        ])
        .output()
        .expect("run oneshot with --grant");
    assert!(
        granted_out.status.success(),
        "oneshot --grant failed: stdout={} stderr={}",
        String::from_utf8_lossy(&granted_out.stdout),
        String::from_utf8_lossy(&granted_out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&granted_out.stdout).trim(),
        "verified"
    );

    let recorded = server.requests.lock().unwrap();
    let turn_requests: Vec<&String> = recorded
        .iter()
        .filter(|body| !body.contains("TURN-END MEMORY WRITE PLAN"))
        .collect();
    assert_eq!(
        turn_requests.len(),
        3,
        "expected 3 LLM calls: {turn_requests:?}"
    );
    assert!(
        turn_requests[0].contains("/mnt/grants/repo-wt"),
        "expected namespace_cwd in initial context: {}",
        turn_requests[0]
    );
    assert!(
        turn_requests[1].contains("root-marker")
            && turn_requests[1].contains("/mnt/grants/repo-wt"),
        "expected first bash output in repo root: {}",
        turn_requests[1]
    );
    assert!(
        turn_requests[2].contains("sub-marker")
            && turn_requests[2].contains("/mnt/grants/repo-wt/subdir"),
        "expected second bash output in subdir: {}",
        turn_requests[2]
    );
    drop(recorded);

    // 4. --grant repo-wt combined with explicit --cwd <subdir> sets namespace_cwd to `/mnt/grants/repo-wt/subdir`.
    let cwd_granted = isolated_alan(&home, &xdg_data, &runtime)
        .current_dir(&ambient)
        .args([
            "--grant",
            "repo-wt",
            "--cwd",
            subdir.to_str().unwrap(),
            "--ephemeral",
            "--trust",
            "-p",
            "check subdir cwd",
        ])
        .output()
        .expect("run oneshot with --grant and --cwd");
    assert!(
        cwd_granted.status.success(),
        "oneshot --grant --cwd failed: stderr={}",
        String::from_utf8_lossy(&cwd_granted.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&cwd_granted.stdout).trim(),
        "verified"
    );
    let recorded = server.requests.lock().unwrap();
    let turn_requests: Vec<&String> = recorded
        .iter()
        .filter(|body| !body.contains("TURN-END MEMORY WRITE PLAN"))
        .collect();
    assert_eq!(turn_requests.len(), 4);
    assert!(
        turn_requests[3].contains("/mnt/grants/repo-wt/subdir"),
        "expected subdirectory namespace_cwd in context: {}",
        turn_requests[3]
    );
}
