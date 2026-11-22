//! Host-local `/issue` Tier 2 submission adapter backed by authenticated `gh` CLI.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use alan_tui::{IssueControl, IssueControlHandler, IssueSubmitResult};
use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const GH_AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const GH_CREATE_TIMEOUT: Duration = Duration::from_secs(30);

/// Build the TUI `IssueControlHandler` bound to Host `gh` CLI availability.
pub(super) fn build_issue_control_handler() -> IssueControlHandler {
    Arc::new(|command| {
        Box::pin(async move {
            match command {
                IssueControl::Submit { repo, title, body } => {
                    submit_via_gh_binary(OsStr::new("gh"), &repo, &title, &body).await
                }
            }
        })
    })
}

pub(super) async fn submit_via_gh_binary(
    gh_bin: &OsStr,
    repo: &str,
    title: &str,
    body: &str,
) -> Result<IssueSubmitResult> {
    anyhow::ensure!(is_valid_repo_slug(repo), "invalid repository slug `{repo}`");

    let Some(resolved_bin) = resolve_executable(gh_bin) else {
        return Ok(IssueSubmitResult::GhUnavailable {
            reason: "`gh` CLI not found on PATH".to_string(),
        });
    };

    let auth_status = tokio::time::timeout(
        GH_AUTH_TIMEOUT,
        Command::new(&resolved_bin)
            .args(["auth", "status"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await;

    match auth_status {
        Ok(Ok(out)) if out.status.success() => {}
        Ok(Ok(_)) => {
            return Ok(IssueSubmitResult::GhUnavailable {
                reason: "`gh` CLI is not authenticated (`gh auth login` required)".to_string(),
            });
        }
        Ok(Err(err)) => {
            return Ok(IssueSubmitResult::GhUnavailable {
                reason: format!("failed to run `gh auth status`: {err}"),
            });
        }
        Err(_) => {
            return Ok(IssueSubmitResult::GhUnavailable {
                reason: "`gh auth status` timed out".to_string(),
            });
        }
    }

    let mut child = Command::new(&resolved_bin)
        .arg("issue")
        .arg("create")
        .arg(format!("--repo={repo}"))
        .arg(format!("--title={title}"))
        .arg("--body-file")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("spawn `gh issue create`")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .await
            .context("write issue body to `gh issue create` stdin")?;
        drop(stdin);
    }

    let output = tokio::time::timeout(GH_CREATE_TIMEOUT, child.wait_with_output())
        .await
        .context("`gh issue create` timed out")?
        .context("wait for `gh issue create`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = [stderr.trim(), stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("non-zero exit status");
        let clean: String = detail
            .chars()
            .filter(|c| !c.is_control() || *c == ' ')
            .take(200)
            .collect();
        anyhow::bail!("`gh issue create` failed: {clean}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = parse_created_issue_url(&stdout)
        .context("`gh issue create` succeeded without returning an issue URL")?;
    let number = parse_issue_number_from_url(&url);
    Ok(IssueSubmitResult::CreatedGhCli { number, url })
}

fn parse_created_issue_url(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("https://") || line.starts_with("http://"))
        .map(ToOwned::to_owned)
}

fn parse_issue_number_from_url(url: &str) -> Option<u64> {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|seg| seg.parse::<u64>().ok())
}

fn is_valid_repo_slug(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    is_valid_slug_part(owner) && is_valid_slug_part(name)
}

fn is_valid_slug_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && !part.starts_with('-')
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn resolve_executable(bin: &OsStr) -> Option<OsString> {
    let candidate = Path::new(bin);
    if candidate.components().count() > 1 {
        return is_executable_file(candidate).then(|| bin.to_os_string());
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(candidate))
        .find(|p| is_executable_file(p))
        .map(PathBuf::into_os_string)
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[tokio::test]
    async fn missing_and_unauthenticated_gh_return_unavailable_without_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-gh");
        let res = submit_via_gh_binary(missing.as_os_str(), "realmorrisliu/alan", "t", "b")
            .await
            .unwrap();
        assert!(matches!(res, IssueSubmitResult::GhUnavailable { .. }));

        let unauth = write_script(
            dir.path(),
            "gh-unauth",
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ]; then exit 1; fi\nexit 99\n",
        );
        let res = submit_via_gh_binary(unauth.as_os_str(), "realmorrisliu/alan", "t", "b")
            .await
            .unwrap();
        assert!(matches!(res, IssueSubmitResult::GhUnavailable { .. }));
    }

    #[tokio::test]
    async fn authenticated_gh_invokes_issue_create_with_stdin_body_and_parses_url() {
        let dir = tempfile::tempdir().unwrap();
        let args_log = dir.path().join("args.txt");
        let body_log = dir.path().join("body.txt");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ]; then exit 0; fi\nprintf '%s\\n' \"$@\" > \"{}\"\ncat > \"{}\"\necho \"https://github.com/realmorrisliu/alan/issues/1234\"\n",
            args_log.display(),
            body_log.display()
        );
        let gh = write_script(dir.path(), "gh-ok", &script);
        let res = submit_via_gh_binary(
            gh.as_os_str(),
            "realmorrisliu/alan",
            "Crash title",
            "Redacted body",
        )
        .await
        .unwrap();
        assert_eq!(
            res,
            IssueSubmitResult::CreatedGhCli {
                number: Some(1234),
                url: "https://github.com/realmorrisliu/alan/issues/1234".to_string(),
            }
        );
        let recorded_args = std::fs::read_to_string(&args_log).unwrap();
        assert!(recorded_args.contains("--repo=realmorrisliu/alan"));
        assert!(recorded_args.contains("--title=Crash title"));
        assert!(recorded_args.contains("--body-file"));
        assert_eq!(std::fs::read_to_string(&body_log).unwrap(), "Redacted body");
    }

    #[tokio::test]
    async fn authenticated_gh_surfaces_create_failure() {
        let dir = tempfile::tempdir().unwrap();
        let gh = write_script(
            dir.path(),
            "gh-fail",
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ]; then exit 0; fi\necho 'network unreachable' >&2\nexit 1\n",
        );
        let err = submit_via_gh_binary(gh.as_os_str(), "realmorrisliu/alan", "t", "b")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("network unreachable"));
    }
}
