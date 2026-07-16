use super::*;
use serde_json::json;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use tempfile::TempDir;

#[test]
fn resolve_target_derives_control_dir_from_window() {
    let options = ShellTargetOptions {
        socket: None,
        control_dir: None,
        window: Some("window_test".to_string()),
        timeout_ms: 500,
    };

    let target = resolve_target(&options).unwrap();
    assert!(target.socket_path.ends_with("window_test/shell.sock"));
    assert!(target.control_dir.ends_with("window_test"));
}

#[test]
fn resolve_target_derives_window_control_dir_from_channel_namespace() {
    let options = ShellTargetOptions {
        socket: None,
        control_dir: None,
        window: Some("window_test".to_string()),
        timeout_ms: 500,
    };

    let stable = resolve_target_for_channel(&options, InstallChannel::Stable).unwrap();
    let dev = resolve_target_for_channel(&options, InstallChannel::Dev).unwrap();

    assert_ne!(stable.control_dir, dev.control_dir);
    assert!(
        stable
            .control_dir
            .components()
            .any(|component| component.as_os_str().to_string_lossy() == "alan-shell-control")
    );
    assert!(
        dev.control_dir
            .components()
            .any(|component| component.as_os_str().to_string_lossy() == "alan-dev-shell-control")
    );
    assert!(
        stable
            .socket_path
            .ends_with("alan-shell-control/window_test/shell.sock")
    );
    assert!(
        dev.socket_path
            .ends_with("alan-dev-shell-control/window_test/shell.sock")
    );
}

#[test]
fn resolve_target_ignores_cross_channel_shell_environment() {
    let options = ShellTargetOptions {
        socket: None,
        control_dir: None,
        window: None,
        timeout_ms: 500,
    };
    let stable_control_dir = std::env::temp_dir()
        .join("alan-shell-control")
        .join("window_main");
    let stable_socket = stable_control_dir.join("shell.sock");

    let target = resolve_target_for_channel_with_env(
        &options,
        InstallChannel::Dev,
        Some(stable_socket),
        Some(stable_control_dir),
    )
    .unwrap();

    assert!(
        target
            .control_dir
            .ends_with("alan-dev-shell-control/window_main")
    );
    assert!(
        target
            .socket_path
            .ends_with("alan-dev-shell-control/window_main/shell.sock")
    );
}

#[test]
fn shell_cli_channel_prefers_environment_over_stable_cli_name() {
    assert_eq!(
        shell_cli_channel_from_inputs(Some("dev"), Some("alan")),
        InstallChannel::Dev
    );
}

#[test]
fn shell_cli_channel_prefers_environment_over_dev_cli_name() {
    assert_eq!(
        shell_cli_channel_from_inputs(Some("stable"), Some("alan-dev")),
        InstallChannel::Stable
    );
}

#[test]
fn resolve_target_accepts_unscoped_custom_shell_environment() {
    let options = ShellTargetOptions {
        socket: None,
        control_dir: None,
        window: None,
        timeout_ms: 500,
    };
    let custom_control_dir = std::env::temp_dir()
        .join("alan-ui-smoke-shell-control-123")
        .join("window_main");
    let custom_socket = custom_control_dir.join("shell.sock");

    let target = resolve_target_for_channel_with_env(
        &options,
        InstallChannel::Dev,
        Some(custom_socket.clone()),
        Some(custom_control_dir.clone()),
    )
    .unwrap();

    assert_eq!(target.control_dir, custom_control_dir);
    assert_eq!(target.socket_path, custom_socket);
}

#[test]
fn invoke_via_socket_round_trips_command() {
    let tmp = TempDir::new().unwrap();
    let socket_path = tmp.path().join("shell.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        stream.read_to_string(&mut request).unwrap();
        assert!(request.contains("\"command\":\"state\""));
        let response = json!({
            "request_id": "req-test",
            "contract_version": CONTRACT_VERSION,
            "applied": true,
            "state": {"window_id": "window_test"}
        });
        stream
            .write_all(format!("{}\n", serde_json::to_string(&response).unwrap()).as_bytes())
            .unwrap();
    });

    let target = ShellTarget {
        socket_path,
        control_dir: tmp.path().to_path_buf(),
        timeout: Duration::from_secs(1),
    };
    let response = invoke_via_socket(
        &target,
        &ShellControlCommand {
            request_id: "req-test".to_string(),
            command: "state".to_string(),
            space_id: None,
            tab_id: None,
            pane_id: None,
            content_id: None,
            split_node_id: None,
            ratio: None,
            direction: None,
            spatial_direction: None,
            placement: None,
            title: None,
            cwd: None,
            text: None,
            attention: None,
            after_event_id: None,
            limit: None,
        },
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(response.contract_version, CONTRACT_VERSION);
    assert_eq!(response.state.unwrap()["window_id"], "window_test");
}

#[test]
fn invoke_falls_back_to_files_when_socket_is_unavailable() {
    let tmp = TempDir::new().unwrap();
    let socket_path = tmp.path().join("shell.sock");

    let commands_dir = tmp.path().join("commands");
    let results_dir = tmp.path().join("results");
    fs::create_dir_all(&commands_dir).unwrap();
    fs::create_dir_all(&results_dir).unwrap();

    let request_id = "req-fallback".to_string();
    let handle = std::thread::spawn({
        let request_id = request_id.clone();
        move || {
            let command_path = commands_dir.join(format!("{request_id}.json"));
            while !command_path.exists() {
                thread::sleep(Duration::from_millis(25));
            }
            let response = json!({
                "request_id": request_id,
                "contract_version": CONTRACT_VERSION,
                "applied": true,
                "focused_pane_id": "pane_9"
            });
            fs::write(
                results_dir.join("req-fallback.json"),
                serde_json::to_vec_pretty(&response).unwrap(),
            )
            .unwrap();
        }
    });

    let response = invoke(
        &ShellTargetOptions {
            socket: Some(socket_path),
            control_dir: Some(tmp.path().to_path_buf()),
            window: None,
            timeout_ms: 500,
        },
        ShellControlCommand {
            request_id,
            command: "pane.focus".to_string(),
            space_id: None,
            tab_id: None,
            pane_id: Some("pane_9".to_string()),
            content_id: None,
            split_node_id: None,
            ratio: None,
            direction: None,
            spatial_direction: None,
            placement: None,
            title: None,
            cwd: None,
            text: None,
            attention: None,
            after_event_id: None,
            limit: None,
        },
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(response.focused_pane_id.as_deref(), Some("pane_9"));
}

#[test]
fn invoke_via_files_round_trips_command() {
    let tmp = TempDir::new().unwrap();
    let commands_dir = tmp.path().join("commands");
    let results_dir = tmp.path().join("results");
    fs::create_dir_all(&commands_dir).unwrap();
    fs::create_dir_all(&results_dir).unwrap();

    let request_id = "req-files".to_string();
    let handle = std::thread::spawn({
        let request_id = request_id.clone();
        move || {
            let command_path = commands_dir.join(format!("{request_id}.json"));
            while !command_path.exists() {
                thread::sleep(Duration::from_millis(25));
            }
            let request = fs::read_to_string(&command_path).unwrap();
            let request: ShellControlCommand = serde_json::from_str(&request).unwrap();
            assert_eq!(request.command, "pane.focus");
            let response = json!({
                "request_id": request_id,
                "contract_version": CONTRACT_VERSION,
                "applied": true,
                "focused_pane_id": "pane_2"
            });
            fs::write(
                results_dir.join("req-files.json"),
                serde_json::to_vec_pretty(&response).unwrap(),
            )
            .unwrap();
        }
    });

    let target = ShellTarget {
        socket_path: tmp.path().join("shell.sock"),
        control_dir: tmp.path().to_path_buf(),
        timeout: Duration::from_secs(2),
    };
    let response = invoke_via_files(
        &target,
        &ShellControlCommand {
            request_id,
            command: "pane.focus".to_string(),
            space_id: None,
            tab_id: None,
            pane_id: Some("pane_2".to_string()),
            content_id: None,
            split_node_id: None,
            ratio: None,
            direction: None,
            spatial_direction: None,
            placement: None,
            title: None,
            cwd: None,
            text: None,
            attention: None,
            after_event_id: None,
            limit: None,
        },
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(response.focused_pane_id.as_deref(), Some("pane_2"));
}

#[test]
fn invoke_via_files_retries_until_response_file_contains_complete_json() {
    let tmp = TempDir::new().unwrap();
    let commands_dir = tmp.path().join("commands");
    let results_dir = tmp.path().join("results");
    fs::create_dir_all(&commands_dir).unwrap();
    fs::create_dir_all(&results_dir).unwrap();

    let request_id = "req-retry".to_string();
    let handle = std::thread::spawn({
        let request_id = request_id.clone();
        move || {
            let command_path = commands_dir.join(format!("{request_id}.json"));
            while !command_path.exists() {
                thread::sleep(Duration::from_millis(25));
            }

            let result_path = results_dir.join(format!("{request_id}.json"));
            fs::write(&result_path, b"{").unwrap();
            thread::sleep(Duration::from_millis(100));

            let response = json!({
                "request_id": request_id,
                "contract_version": CONTRACT_VERSION,
                "applied": true,
                "focused_pane_id": "pane_7"
            });
            fs::write(&result_path, serde_json::to_vec_pretty(&response).unwrap()).unwrap();
        }
    });

    let target = ShellTarget {
        socket_path: tmp.path().join("shell.sock"),
        control_dir: tmp.path().to_path_buf(),
        timeout: Duration::from_secs(2),
    };
    let response = invoke_via_files(
        &target,
        &ShellControlCommand {
            request_id,
            command: "pane.focus".to_string(),
            space_id: None,
            tab_id: None,
            pane_id: Some("pane_7".to_string()),
            content_id: None,
            split_node_id: None,
            ratio: None,
            direction: None,
            spatial_direction: None,
            placement: None,
            title: None,
            cwd: None,
            text: None,
            attention: None,
            after_event_id: None,
            limit: None,
        },
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(response.focused_pane_id.as_deref(), Some("pane_7"));
}

#[test]
fn invoke_does_not_fallback_after_indeterminate_socket_failure() {
    let tmp = TempDir::new().unwrap();
    let socket_path = tmp.path().join("shell.sock");
    let commands_dir = tmp.path().join("commands");
    let results_dir = tmp.path().join("results");
    fs::create_dir_all(&commands_dir).unwrap();
    fs::create_dir_all(&results_dir).unwrap();

    let listener = UnixListener::bind(&socket_path).unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        stream.read_to_string(&mut request).unwrap();
        assert!(request.contains("\"command\":\"pane.focus\""));
    });

    let error = invoke(
        &ShellTargetOptions {
            socket: Some(socket_path),
            control_dir: Some(tmp.path().to_path_buf()),
            window: None,
            timeout_ms: 250,
        },
        ShellControlCommand {
            request_id: "req-no-fallback".to_string(),
            command: "pane.focus".to_string(),
            space_id: None,
            tab_id: None,
            pane_id: Some("pane_2".to_string()),
            content_id: None,
            split_node_id: None,
            ratio: None,
            direction: None,
            spatial_direction: None,
            placement: None,
            title: None,
            cwd: None,
            text: None,
            attention: None,
            after_event_id: None,
            limit: None,
        },
    )
    .unwrap_err();

    handle.join().unwrap();
    assert!(
        error
            .to_string()
            .contains("Failed to decode alan shell response")
    );
    assert!(fs::read_dir(&commands_dir).unwrap().next().is_none());
}

#[test]
fn invoke_via_socket_round_trips_pane_move_command() {
    let tmp = TempDir::new().unwrap();
    let socket_path = tmp.path().join("shell.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = String::new();
        stream.read_to_string(&mut request).unwrap();
        assert!(request.contains("\"command\":\"pane.move\""));
        assert!(request.contains("\"pane_id\":\"pane_2\""));
        assert!(request.contains("\"tab_id\":\"tab_9\""));
        let response = json!({
            "request_id": "req-move",
            "contract_version": CONTRACT_VERSION,
            "applied": true,
            "pane_id": "pane_2",
            "tab_id": "tab_9"
        });
        stream
            .write_all(format!("{}\n", serde_json::to_string(&response).unwrap()).as_bytes())
            .unwrap();
    });

    let target = ShellTarget {
        socket_path,
        control_dir: tmp.path().to_path_buf(),
        timeout: Duration::from_secs(1),
    };
    let response = invoke_via_socket(
        &target,
        &ShellControlCommand {
            request_id: "req-move".to_string(),
            command: "pane.move".to_string(),
            space_id: None,
            tab_id: Some("tab_9".to_string()),
            pane_id: Some("pane_2".to_string()),
            content_id: None,
            split_node_id: None,
            ratio: None,
            direction: Some("vertical".to_string()),
            spatial_direction: None,
            placement: None,
            title: None,
            cwd: None,
            text: None,
            attention: None,
            after_event_id: None,
            limit: None,
        },
    )
    .unwrap();

    handle.join().unwrap();
    assert_eq!(response.tab_id.as_deref(), Some("tab_9"));
    assert_eq!(response.pane_id.as_deref(), Some("pane_2"));
}
