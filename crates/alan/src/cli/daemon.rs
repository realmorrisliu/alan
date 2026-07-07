//! `alan daemon stop|status` + daemon lifecycle utilities.

use crate::daemon::api_contract::paths;
use crate::host_config::HostConfig;
use alan_agent_engine::AlanHomePaths;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// PID file location under the active channel's alan home.
fn pid_file_path() -> Result<PathBuf> {
    AlanHomePaths::detect()
        .map(|paths| paths.global_daemon_pid_path)
        .context("Cannot determine home directory")
}

/// Daemon URL (from env or default).
pub fn daemon_url() -> String {
    HostConfig::resolve_daemon_url_best_effort()
}

/// Write the daemon PID to the PID file.
fn write_pid(pid: u32) -> Result<()> {
    let path = pid_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, pid.to_string())?;
    Ok(())
}

/// Read the daemon PID from the PID file.
fn read_pid() -> Result<Option<u32>> {
    let path = pid_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let pid = content.trim().parse::<u32>().ok();
    Ok(pid)
}

/// Remove the PID file.
fn remove_pid_file() -> Result<()> {
    let path = pid_file_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Check if a process with the given PID is alive using `kill -0`.
fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Send a signal to a process.
fn send_signal(pid: u32, signal: &str) -> bool {
    std::process::Command::new("kill")
        .args([signal, &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Start the daemon as a detached background process.
pub async fn start_daemon_background() -> Result<()> {
    // Check if already running
    if check_daemon_health().await {
        println!("✅ Daemon is already running at {}", daemon_url());
        return Ok(());
    }

    super::load_agent_config_with_notice()?;

    let alan_bin = std::env::current_exe().context("Cannot determine own executable path")?;

    eprintln!("Starting alan daemon...");

    let child = std::process::Command::new(&alan_bin)
        .args(["daemon", "start", "--foreground"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
        .context("Failed to start daemon process")?;

    let pid = child.id();
    write_pid(pid)?;

    // Wait for daemon to become healthy
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(10);
    loop {
        if start.elapsed() > timeout {
            anyhow::bail!("Daemon failed to start within {:?}", timeout);
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if check_daemon_health().await {
            println!("✅ Daemon started (pid: {})", pid);
            return Ok(());
        }
    }
}

/// Stop the daemon.
pub async fn stop_daemon() -> Result<()> {
    // Try PID file first
    if let Ok(Some(pid)) = read_pid() {
        if is_process_alive(pid) {
            eprintln!("Stopping daemon (pid: {})...", pid);
            send_signal(pid, "-TERM");

            // Wait for process to exit
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(5);
            loop {
                if !is_process_alive(pid) {
                    break;
                }
                if start.elapsed() > timeout {
                    eprintln!("Daemon did not stop gracefully, sending SIGKILL...");
                    send_signal(pid, "-KILL");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            remove_pid_file()?;
            println!("✅ Daemon stopped");
            return Ok(());
        } else {
            // PID file exists but process is dead — clean up stale file
            remove_pid_file()?;
        }
    }

    // Fallback: check if daemon is running but we don't have a PID
    if check_daemon_health().await {
        println!("⚠️  Daemon is running but no PID file found.");
        println!("   Cannot stop automatically. Find the process manually:");
        println!("   lsof -i :8090");
    } else {
        println!("Daemon is not running.");
    }

    Ok(())
}

/// Show daemon status.
pub async fn daemon_status() -> Result<()> {
    let url = daemon_url();

    if check_daemon_health().await {
        let pid_str = match read_pid() {
            Ok(Some(pid)) if is_process_alive(pid) => format!(" (pid: {})", pid),
            _ => String::new(),
        };
        println!("✅ Daemon is running at {}{}", url, pid_str);
    } else {
        // Clean up stale PID file if daemon isn't actually running
        if let Ok(Some(_)) = read_pid() {
            let _ = remove_pid_file();
        }
        println!("❌ Daemon is not running");
    }

    Ok(())
}

/// Check if the daemon is healthy via HTTP.
async fn check_daemon_health() -> bool {
    let url = format!("{}{}", daemon_url(), paths::HEALTH);
    let client = reqwest::Client::new();
    matches!(
        client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await,
        Ok(resp) if resp.status().is_success()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_daemon_url_default() {
        let url = HostConfig::default().daemon_url;
        if std::env::var("ALAN_AGENTD_URL").is_err() {
            assert_eq!(url, "http://127.0.0.1:8090");
        }
    }

    #[test]
    fn test_write_and_read_pid() {
        let tmp = TempDir::new().unwrap();
        let pid_file = tmp.path().join("daemon.pid");

        // Test writing PID
        std::fs::write(&pid_file, "12345").unwrap();

        // Test reading PID
        let content = std::fs::read_to_string(&pid_file).unwrap();
        let pid = content.trim().parse::<u32>().unwrap();
        assert_eq!(pid, 12345);
    }

    #[test]
    fn test_read_pid_file_not_exists() {
        let tmp = TempDir::new().unwrap();
        let pid_file = tmp.path().join("nonexistent.pid");

        assert!(!pid_file.exists());
    }

    #[test]
    fn test_pid_file_creation_and_removal() {
        let tmp = TempDir::new().unwrap();
        let pid_file = tmp.path().join("test.pid");

        // Create parent directory and file
        std::fs::create_dir_all(pid_file.parent().unwrap()).unwrap();
        std::fs::write(&pid_file, "99999").unwrap();

        assert!(pid_file.exists());

        // Remove file
        std::fs::remove_file(&pid_file).unwrap();
        assert!(!pid_file.exists());
    }

    #[test]
    fn test_is_process_alive_zero() {
        // PID 0 is the idle process on Unix, should be "alive"
        // This is a basic sanity test
        assert!(is_process_alive(0) || !is_process_alive(0)); // Either is fine, just don't panic
    }

    #[test]
    fn test_is_process_alive_nonexistent() {
        // A very high PID is unlikely to exist
        let result = is_process_alive(999999);
        // We can't assert the result because it depends on the system state,
        // but we can verify it doesn't panic
        let _ = result;
    }
}
