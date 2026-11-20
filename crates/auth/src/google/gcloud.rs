//! `gcloud` CLI availability, default project lookup, and streaming ADC login.

use super::GoogleAuthError;
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

const GCLOUD_PROJECT_TIMEOUT: Duration = Duration::from_secs(5);
const GCLOUD_ADC_LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_CAPTURED_STDERR_LINES: usize = 8;

/// Return `true` if a `gcloud` executable is resolvable on `PATH`.
pub fn gcloud_is_available() -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    let bin_name = if cfg!(windows) {
        "gcloud.cmd"
    } else {
        "gcloud"
    };
    std::env::split_paths(&path_var).any(|dir| is_executable_file(&dir.join(bin_name)))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Read the active default GCP project from `gcloud config get-value project --quiet`.
pub async fn gcloud_default_project() -> Option<String> {
    if !gcloud_is_available() {
        return None;
    }
    let output = tokio::time::timeout(
        GCLOUD_PROJECT_TIMEOUT,
        Command::new("gcloud")
            .args(["config", "get-value", "project", "--quiet"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_gcloud_project_output(&stdout)
}

pub(crate) fn parse_gcloud_project_output(raw: &str) -> Option<String> {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.eq_ignore_ascii_case("(unset)"))
        .map(ToOwned::to_owned)
}

pub(crate) fn extract_google_auth_url(line: &str) -> Option<String> {
    line.split_whitespace().find_map(|token| {
        let trimmed = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | ',' | '.'
            )
        });
        trimmed
            .starts_with("https://accounts.google.com/")
            .then(|| trimmed.to_string())
    })
}

/// Run `gcloud auth application-default login --quiet` non-interactively, streaming any emitted
/// Google sign-in URL to `on_url`. Kills the child process if dropped/cancelled.
pub async fn run_gcloud_adc_login_streaming<F>(on_url: F) -> Result<(), GoogleAuthError>
where
    F: Fn(String) + Send + Sync + 'static,
{
    let mut child = Command::new("gcloud")
        .args(["auth", "application-default", "login", "--quiet"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            GoogleAuthError::GcloudFailed(format!(
                "failed to spawn `gcloud auth application-default login`: {error}"
            ))
        })?;

    let seen_urls = Arc::new(Mutex::new(HashSet::new()));
    let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(
        MAX_CAPTURED_STDERR_LINES,
    )));
    let callback = Arc::new(on_url);

    let stdout_task = child.stdout.take().map(|stdout| {
        let seen = seen_urls.clone();
        let cb = callback.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(url) = extract_google_auth_url(&line)
                    && seen.lock().await.insert(url.clone())
                {
                    cb(url);
                }
            }
        })
    });

    let stderr_task = child.stderr.take().map(|stderr| {
        let seen = seen_urls.clone();
        let cb = callback.clone();
        let tail = stderr_tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(url) = extract_google_auth_url(&line)
                    && seen.lock().await.insert(url.clone())
                {
                    cb(url);
                }
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let mut guard = tail.lock().await;
                    if guard.len() >= MAX_CAPTURED_STDERR_LINES {
                        guard.pop_front();
                    }
                    guard.push_back(trimmed.to_string());
                }
            }
        })
    });

    let status = tokio::time::timeout(GCLOUD_ADC_LOGIN_TIMEOUT, child.wait())
        .await
        .map_err(|_| {
            GoogleAuthError::GcloudFailed(
                "`gcloud auth application-default login` timed out after 300s".to_string(),
            )
        })?
        .map_err(|error| {
            GoogleAuthError::GcloudFailed(format!(
                "`gcloud auth application-default login` wait failed: {error}"
            ))
        })?;

    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }

    if !status.success() {
        let lines = stderr_tail.lock().await;
        let detail = if lines.is_empty() {
            format!("exited with status {status}")
        } else {
            format!(
                "exited with status {status}: {}",
                lines.iter().cloned().collect::<Vec<_>>().join(" | ")
            )
        };
        return Err(GoogleAuthError::GcloudFailed(format!(
            "`gcloud auth application-default login` {detail}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_google_auth_url_handles_wrapped_lines() {
        assert_eq!(
            extract_google_auth_url(
                "Opening browser: https://accounts.google.com/o/oauth2/auth?client_id=123&scope=abc."
            ),
            Some("https://accounts.google.com/o/oauth2/auth?client_id=123&scope=abc".to_string())
        );
        assert_eq!(
            extract_google_auth_url("  https://accounts.google.com/o/oauth2/auth?state=xyz  "),
            Some("https://accounts.google.com/o/oauth2/auth?state=xyz".to_string())
        );
        assert_eq!(extract_google_auth_url("no url here"), None);
    }

    #[test]
    fn parse_gcloud_project_output_skips_unset_and_blank() {
        assert_eq!(parse_gcloud_project_output(""), None);
        assert_eq!(parse_gcloud_project_output("  (unset)\n"), None);
        assert_eq!(
            parse_gcloud_project_output("\n  workus-ai-473608 \n"),
            Some("workus-ai-473608".to_string())
        );
    }
}
