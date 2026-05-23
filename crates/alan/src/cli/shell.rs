use alan_runtime::InstallChannel;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const CONTRACT_VERSION: &str = "0.1";

#[derive(Clone, Debug)]
pub struct ShellTargetOptions {
    pub socket: Option<PathBuf>,
    pub control_dir: Option<PathBuf>,
    pub window: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct ShellControlCommand {
    request_id: String,
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    space_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tab_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    split_node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spatial_direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    placement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attention: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    after_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ShellControlResponse {
    request_id: String,
    contract_version: String,
    applied: Option<bool>,
    state: Option<Value>,
    spaces: Option<Value>,
    tabs: Option<Value>,
    panes: Option<Value>,
    pane: Option<Value>,
    items: Option<Value>,
    candidates: Option<Value>,
    events: Option<Value>,
    focused_pane_id: Option<String>,
    space_id: Option<String>,
    tab_id: Option<String>,
    pane_id: Option<String>,
    content_id: Option<String>,
    split_node_id: Option<String>,
    ratio: Option<f64>,
    changed_split_ids: Option<Value>,
    affected_pane_ids: Option<Value>,
    zoomed_pane_id: Option<String>,
    source_tab_id: Option<String>,
    target_tab_id: Option<String>,
    previous_focused_pane_id: Option<String>,
    current_focused_pane_id: Option<String>,
    split_direction: Option<String>,
    spatial_direction: Option<String>,
    placement: Option<String>,
    mounted_content_instance_id: Option<String>,
    accepted_bytes: Option<u64>,
    latest_event_id: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
}

#[derive(Clone, Debug)]
struct ShellTarget {
    socket_path: PathBuf,
    control_dir: PathBuf,
    timeout: Duration,
}

#[derive(Debug)]
enum SocketInvocationFailure {
    Unavailable(anyhow::Error),
    Indeterminate(anyhow::Error),
}

impl SocketInvocationFailure {
    fn can_fallback(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }
}

impl fmt::Display for SocketInvocationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(error) | Self::Indeterminate(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SocketInvocationFailure {}

pub fn run_shell_state(options: ShellTargetOptions) -> Result<()> {
    let response = invoke(&options, build_command("state"))?;
    ensure_success(&response)?;
    print_required_json(response.state, "`state`")
}

pub fn run_shell_space_list(options: ShellTargetOptions) -> Result<()> {
    let response = invoke(&options, build_command("space.list"))?;
    ensure_success(&response)?;
    print_required_json(response.spaces, "`space list`")
}

pub fn run_shell_space_create(
    title: Option<&str>,
    cwd: Option<&str>,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("space.create");
    command.title = title.map(str::to_owned);
    command.cwd = cwd.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_space_open_alan(
    title: Option<&str>,
    cwd: Option<&str>,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("space.open_alan");
    command.title = title.map(str::to_owned);
    command.cwd = cwd.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_tab_list(space: Option<&str>, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("tab.list");
    command.space_id = space.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_required_json(response.tabs, "`tab list`")
}

pub fn run_shell_tab_open(
    space: Option<&str>,
    title: Option<&str>,
    cwd: Option<&str>,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("tab.open");
    command.space_id = space.map(str::to_owned);
    command.title = title.map(str::to_owned);
    command.cwd = cwd.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_tab_close(tab: &str, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("tab.close");
    command.tab_id = Some(tab.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_list(tab: Option<&str>, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("pane.list");
    command.tab_id = tab.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_required_json(response.panes, "`pane list`")
}

pub fn run_shell_pane_snapshot(pane: &str, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("pane.snapshot");
    command.pane_id = Some(pane.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_required_json(response.pane, "`pane snapshot`")
}

pub fn run_shell_pane_split(
    pane: &str,
    direction: &str,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("pane.split");
    command.pane_id = Some(pane.to_string());
    command.direction = Some(direction.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_close(pane: &str, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("pane.close");
    command.pane_id = Some(pane.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_lift(
    pane: &str,
    title: Option<&str>,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("pane.lift");
    command.pane_id = Some(pane.to_string());
    command.title = title.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_move(
    pane: &str,
    tab: &str,
    direction: &str,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("pane.move");
    command.pane_id = Some(pane.to_string());
    command.tab_id = Some(tab.to_string());
    command.direction = Some(direction.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_move_within_tab(
    pane: &str,
    placement: &str,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("pane.move_within_tab");
    command.pane_id = Some(pane.to_string());
    command.placement = Some(placement.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_focus(pane: &str, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("pane.focus");
    command.pane_id = Some(pane.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_spatial_focus(direction: &str, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("pane.spatial_focus");
    command.spatial_direction = Some(direction.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_resize_split(
    split_node: &str,
    ratio: f64,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("pane.resize_split");
    command.split_node_id = Some(split_node.to_string());
    command.ratio = Some(ratio);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_equalize_splits(
    tab: Option<&str>,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("pane.equalize_splits");
    command.tab_id = tab.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_zoom(pane: &str, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("pane.zoom");
    command.pane_id = Some(pane.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_unzoom(
    pane: Option<&str>,
    tab: Option<&str>,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("pane.unzoom");
    command.pane_id = pane.map(str::to_owned);
    command.tab_id = tab.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_terminal_send_text(
    pane: Option<&str>,
    content: Option<&str>,
    text: &str,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("terminal.send_text");
    command.pane_id = pane.map(str::to_owned);
    command.content_id = content.map(str::to_owned);
    command.text = Some(text.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_pane_send_text(pane: &str, text: &str, options: ShellTargetOptions) -> Result<()> {
    run_shell_terminal_send_text(Some(pane), None, text, options)
}

pub fn run_shell_attention_inbox(options: ShellTargetOptions) -> Result<()> {
    let response = invoke(&options, build_command("attention.inbox"))?;
    ensure_success(&response)?;
    print_required_json(response.items, "`attention inbox`")
}

pub fn run_shell_attention_set(
    pane: &str,
    attention: &str,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut command = build_command("attention.set");
    command.pane_id = Some(pane.to_string());
    command.attention = Some(attention.to_string());
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_pretty(&response)
}

pub fn run_shell_routing_candidates(pane: Option<&str>, options: ShellTargetOptions) -> Result<()> {
    let mut command = build_command("routing.candidates");
    command.pane_id = pane.map(str::to_owned);
    let response = invoke(&options, command)?;
    ensure_success(&response)?;
    print_required_json(response.candidates, "`routing candidates`")
}

pub fn run_shell_events(
    after_event_id: Option<&str>,
    limit: Option<u64>,
    follow: bool,
    options: ShellTargetOptions,
) -> Result<()> {
    let mut cursor = after_event_id.map(str::to_owned);
    let request_limit = limit.unwrap_or(if follow { 20 } else { 50 });

    loop {
        let mut command = build_command("events.read");
        command.after_event_id = cursor.clone();
        command.limit = Some(request_limit);
        let response = invoke(&options, command)?;
        ensure_success(&response)?;
        let rows = response.events.unwrap_or_else(|| json!([]));

        if follow {
            let Some(events) = rows.as_array() else {
                bail!("alan shell returned malformed events payload");
            };

            for event in events {
                println!(
                    "{}",
                    serde_json::to_string(event).context("Failed to render shell event")?
                );
                cursor = event
                    .get("event_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| cursor.clone());
            }

            if events.is_empty() {
                thread::sleep(Duration::from_millis(250));
            }
            continue;
        }

        let payload = json!({
            "events": rows,
            "latest_event_id": response.latest_event_id,
        });
        return print_pretty(&payload);
    }
}

fn build_command(command: &str) -> ShellControlCommand {
    ShellControlCommand {
        request_id: request_id(),
        command: command.to_string(),
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
    }
}

fn invoke(
    options: &ShellTargetOptions,
    command: ShellControlCommand,
) -> Result<ShellControlResponse> {
    let target = resolve_target(options)?;

    if target.socket_path.exists() {
        match invoke_via_socket(&target, &command) {
            Ok(response) => return Ok(response),
            Err(socket_error) if socket_error.can_fallback() && target.control_dir.exists() => {
                tracing::warn!(
                    socket = %target.socket_path.display(),
                    error = %socket_error,
                    "alan shell socket unavailable; falling back to file-backed control plane"
                );
            }
            Err(socket_error) => return Err(socket_error.into()),
        }
    }

    invoke_via_files(&target, &command)
}

fn resolve_target(options: &ShellTargetOptions) -> Result<ShellTarget> {
    resolve_target_for_channel(options, InstallChannel::detect_current())
}

fn resolve_target_for_channel(
    options: &ShellTargetOptions,
    channel: InstallChannel,
) -> Result<ShellTarget> {
    let explicit_socket = options.socket.clone();
    let explicit_control_dir = options.control_dir.clone();
    let env_socket = std::env::var_os("ALAN_SHELL_SOCKET").map(PathBuf::from);
    let env_control_dir = std::env::var_os("ALAN_SHELL_CONTROL_DIR").map(PathBuf::from);
    let window_root = options
        .window
        .as_deref()
        .map(|window| control_dir_for_window_for_channel(window, channel));

    let control_dir = explicit_control_dir
        .clone()
        .or_else(|| window_root.clone())
        .or_else(|| env_control_dir.clone())
        .or_else(|| {
            explicit_socket
                .as_ref()
                .and_then(|path| path.parent().map(PathBuf::from))
        })
        .or_else(|| {
            env_socket
                .as_ref()
                .and_then(|path| path.parent().map(PathBuf::from))
        });

    let socket_path = explicit_socket
        .or_else(|| {
            explicit_control_dir
                .as_ref()
                .map(|dir| dir.join("shell.sock"))
        })
        .or_else(|| window_root.as_ref().map(|dir| dir.join("shell.sock")))
        .or(env_socket)
        .or_else(|| control_dir.as_ref().map(|dir| dir.join("shell.sock")));

    let Some(socket_path) = socket_path else {
        bail!(
            "No alan shell target found. Pass --socket/--control-dir/--window or set ALAN_SHELL_SOCKET."
        );
    };
    let Some(control_dir) = control_dir else {
        bail!(
            "No alan shell control directory found. Pass --control-dir/--window or set ALAN_SHELL_CONTROL_DIR."
        );
    };

    Ok(ShellTarget {
        socket_path,
        control_dir,
        timeout: Duration::from_millis(options.timeout_ms.max(1)),
    })
}

fn control_dir_for_window_for_channel(window: &str, channel: InstallChannel) -> PathBuf {
    std::env::temp_dir()
        .join(channel.descriptor().shell_control_namespace)
        .join(window)
}

fn invoke_via_socket(
    target: &ShellTarget,
    command: &ShellControlCommand,
) -> std::result::Result<ShellControlResponse, SocketInvocationFailure> {
    let mut stream = UnixStream::connect(&target.socket_path).map_err(|error| {
        SocketInvocationFailure::Unavailable(anyhow::anyhow!(
            "Failed to connect to alan shell socket at {}: {error}",
            target.socket_path.display()
        ))
    })?;
    stream
        .set_read_timeout(Some(target.timeout))
        .map_err(|error| {
            SocketInvocationFailure::Indeterminate(anyhow::anyhow!(
                "Failed to configure socket read timeout: {error}"
            ))
        })?;
    stream
        .set_write_timeout(Some(target.timeout))
        .map_err(|error| {
            SocketInvocationFailure::Indeterminate(anyhow::anyhow!(
                "Failed to configure socket write timeout: {error}"
            ))
        })?;

    let mut payload = serde_json::to_vec(command).map_err(|error| {
        SocketInvocationFailure::Indeterminate(
            anyhow::Error::new(error).context("Failed to encode shell command"),
        )
    })?;
    payload.push(b'\n');
    stream.write_all(&payload).map_err(|error| {
        SocketInvocationFailure::Indeterminate(anyhow::anyhow!(
            "Failed to send shell command over socket: {error}"
        ))
    })?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| {
            SocketInvocationFailure::Indeterminate(anyhow::anyhow!(
                "Failed to close shell socket write half: {error}"
            ))
        })?;

    let mut response_bytes = Vec::new();
    stream.read_to_end(&mut response_bytes).map_err(|error| {
        SocketInvocationFailure::Indeterminate(anyhow::anyhow!(
            "Failed to read shell socket response: {error}"
        ))
    })?;
    decode_response(&response_bytes).map_err(SocketInvocationFailure::Indeterminate)
}

fn invoke_via_files(
    target: &ShellTarget,
    command: &ShellControlCommand,
) -> Result<ShellControlResponse> {
    let commands_dir = target.control_dir.join("commands");
    let results_dir = target.control_dir.join("results");
    fs::create_dir_all(&commands_dir).with_context(|| {
        format!(
            "Failed to create alan shell commands dir at {}",
            commands_dir.display()
        )
    })?;
    fs::create_dir_all(&results_dir).with_context(|| {
        format!(
            "Failed to create alan shell results dir at {}",
            results_dir.display()
        )
    })?;

    let command_path = commands_dir.join(format!("{}.json", command.request_id));
    let result_path = results_dir.join(format!("{}.json", command.request_id));
    let payload = serde_json::to_vec_pretty(command).context("Failed to encode shell command")?;
    write_bytes_atomically(&command_path, &payload).with_context(|| {
        format!(
            "Failed to write alan shell command file at {}",
            command_path.display()
        )
    })?;

    let deadline = Instant::now() + target.timeout;
    let mut last_response_error: Option<anyhow::Error> = None;
    loop {
        if result_path.exists() {
            match fs::read(&result_path) {
                Ok(bytes) => match try_decode_response(&bytes) {
                    Ok(response) => {
                        let _ = fs::remove_file(&result_path);
                        let _ = fs::remove_file(&command_path);
                        return Ok(response);
                    }
                    Err(error) => {
                        last_response_error = Some(anyhow::Error::new(error).context(format!(
                            "alan shell response file at {} is not complete yet",
                            result_path.display()
                        )));
                    }
                },
                Err(error) => {
                    last_response_error = Some(anyhow::Error::new(error).context(format!(
                        "Failed to read alan shell response file at {}",
                        result_path.display()
                    )));
                }
            }
        }

        if Instant::now() >= deadline {
            let _ = fs::remove_file(&command_path);
            if let Some(error) = last_response_error {
                bail!(
                    "Timed out waiting for alan shell response in {}: {error}",
                    result_path.display()
                );
            }
            bail!(
                "Timed out waiting for alan shell response in {}",
                result_path.display()
            );
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn decode_response(bytes: &[u8]) -> Result<ShellControlResponse> {
    try_decode_response(bytes).context("Failed to decode alan shell response")
}

fn try_decode_response(
    bytes: &[u8],
) -> std::result::Result<ShellControlResponse, serde_json::Error> {
    let trimmed = trim_json_payload(bytes);
    serde_json::from_slice(trimmed)
}

fn trim_json_payload(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().with_context(|| {
        format!(
            "alan shell control path has no parent directory: {}",
            path.display()
        )
    })?;
    let tmp_path = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("command.json"),
        std::process::id()
    ));
    let mut tmp_file = fs::File::create(&tmp_path)
        .with_context(|| format!("Failed to create temp shell file: {}", tmp_path.display()))?;
    tmp_file
        .write_all(bytes)
        .with_context(|| format!("Failed to write temp shell file: {}", tmp_path.display()))?;
    tmp_file
        .sync_all()
        .with_context(|| format!("Failed to sync temp shell file: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Failed to atomically replace shell file {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn ensure_success(response: &ShellControlResponse) -> Result<()> {
    if response.contract_version != CONTRACT_VERSION {
        bail!(
            "alan shell contract mismatch: expected {}, got {}",
            CONTRACT_VERSION,
            response.contract_version
        );
    }

    if let Some(error_code) = response.error_code.as_deref() {
        bail!(
            "alan shell returned {}{}",
            error_code,
            response
                .error_message
                .as_deref()
                .map(|message| format!(": {message}"))
                .unwrap_or_default()
        );
    }

    if response.applied == Some(false) {
        bail!("alan shell rejected the request without an explicit error");
    }

    Ok(())
}

fn print_required_json(value: Option<Value>, operation: &str) -> Result<()> {
    let value = value.with_context(|| format!("alan shell returned no payload for {operation}"))?;
    print_pretty(&value)
}

fn print_pretty<T: Serialize>(value: &T) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("Failed to render JSON output")?
    );
    Ok(())
}

fn request_id() -> String {
    format!("req-{}", Uuid::new_v4())
}

#[cfg(test)]
mod tests {
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
            dev.control_dir.components().any(
                |component| component.as_os_str().to_string_lossy() == "alan-dev-shell-control"
            )
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
            let commands_dir = commands_dir.clone();
            let results_dir = results_dir.clone();
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
            let commands_dir = commands_dir.clone();
            let results_dir = results_dir.clone();
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
            let commands_dir = commands_dir.clone();
            let results_dir = results_dir.clone();
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
}
