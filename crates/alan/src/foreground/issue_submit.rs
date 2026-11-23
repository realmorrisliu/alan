//! Host-local `/issue` AI drafting and Tier 2 submission adapter backed by `gh` CLI.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use alan_tui::{IssueControl, IssueControlHandler, IssueSubmitResult};
use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const GH_AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const GH_CREATE_TIMEOUT: Duration = Duration::from_secs(30);
const ISSUE_DRAFT_TIMEOUT: Duration = Duration::from_secs(12);
const ETXTBSY_MAX_ATTEMPTS: usize = 5;
const ETXTBSY_RETRY_DELAY: Duration = Duration::from_millis(10);

const ISSUE_DRAFT_SYSTEM_PROMPT: &str = "\
You draft concise, accurate GitHub bug report titles and summaries for Alan.\n\
Input contains ONLY redacted runtime diagnostics and an optional user hint.\n\
Return ONLY one valid JSON object with exact string keys \"title\" and \"summary\":\n\
{\"title\":\"<concise <=72-char issue title>\",\"summary\":\"<concise markdown bullet summary>\"}\n\
Rules:\n\
- Ground every statement strictly in the provided diagnostics and user hint.\n\
- Summarize the observed symptom, ordered failure sequence (if multiple), and affected subsystem.\n\
- Never invent reproduction steps, file paths, or secret values.\n\
- Keep title single-line (<= 72 characters) and summary concise (2-5 markdown bullets).";

async fn spawn_with_etxtbsy_retry(
    mut build: impl FnMut() -> Command,
) -> std::io::Result<tokio::process::Child> {
    let mut attempt = 0;
    loop {
        match build().spawn() {
            Ok(child) => return Ok(child),
            Err(err)
                if err.kind() == std::io::ErrorKind::ExecutableFileBusy
                    && attempt + 1 < ETXTBSY_MAX_ATTEMPTS =>
            {
                attempt += 1;
                tokio::time::sleep(ETXTBSY_RETRY_DELAY).await;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn output_with_etxtbsy_retry(
    build: impl FnMut() -> Command,
) -> std::io::Result<std::process::Output> {
    let child = spawn_with_etxtbsy_retry(build).await?;
    child.wait_with_output().await
}

/// Build the TUI `IssueControlHandler` bound to Host one-shot AI drafting and `gh` CLI availability.
pub(super) fn build_issue_control_handler() -> IssueControlHandler {
    Arc::new(|command| {
        Box::pin(async move {
            match command {
                IssueControl::Draft {
                    repo,
                    user_hint,
                    diagnostics_context,
                    profile,
                } => {
                    draft_via_one_shot(
                        &repo,
                        user_hint.as_deref(),
                        &diagnostics_context,
                        profile.as_deref(),
                    )
                    .await
                }
                IssueControl::Submit {
                    repo,
                    title,
                    body,
                    labels,
                } => submit_via_gh_binary(OsStr::new("gh"), &repo, &title, &body, &labels).await,
            }
        })
    })
}

async fn draft_via_one_shot(
    repo: &str,
    user_hint: Option<&str>,
    diagnostics_context: &str,
    profile: Option<&str>,
) -> Result<IssueSubmitResult> {
    let user_prompt = build_draft_user_prompt(repo, user_hint, diagnostics_context);
    match alan_os_host::complete_one_shot_prompt(
        profile,
        ISSUE_DRAFT_SYSTEM_PROMPT,
        &user_prompt,
        ISSUE_DRAFT_TIMEOUT,
    )
    .await
    {
        Ok(raw) => match parse_draft_json(&raw) {
            Ok((title, summary)) => Ok(IssueSubmitResult::Drafted { title, summary }),
            Err(err) => Ok(IssueSubmitResult::DraftUnavailable {
                reason: format!("invalid AI draft format: {err:#}"),
            }),
        },
        Err(err) => Ok(IssueSubmitResult::DraftUnavailable {
            reason: format!("{err:#}"),
        }),
    }
}

fn build_draft_user_prompt(
    repo: &str,
    user_hint: Option<&str>,
    diagnostics_context: &str,
) -> String {
    let hint_section = user_hint
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(|h| format!("User hint: {h}\n"))
        .unwrap_or_default();
    format!(
        "Target repository: {repo}\n{hint_section}\nRedacted diagnostics:\n{diagnostics_context}"
    )
}

#[derive(Deserialize)]
struct DraftPayload {
    title: String,
    summary: String,
}

pub(super) fn parse_draft_json(raw: &str) -> Result<(String, String)> {
    let trimmed = raw.trim();
    let parsed: DraftPayload = serde_json::from_str(trimmed)
        .or_else(|_| {
            extract_fenced_json_block(trimmed)
                .ok_or_else(|| anyhow::anyhow!("no fenced JSON block"))
                .and_then(|inner| serde_json::from_str(inner).map_err(anyhow::Error::from))
        })
        .or_else(|_| {
            let start = trimmed
                .find('{')
                .context("AI draft response contained no JSON object")?;
            let end = trimmed
                .rfind('}')
                .filter(|&end| end >= start)
                .context("AI draft response contained unclosed JSON object")?;
            serde_json::from_str(&trimmed[start..=end]).map_err(anyhow::Error::from)
        })?;
    let title = parsed.title.trim().to_string();
    let summary = parsed.summary.trim().to_string();
    anyhow::ensure!(!title.is_empty(), "AI draft title was empty");
    anyhow::ensure!(!summary.is_empty(), "AI draft summary was empty");
    Ok((title, summary))
}

fn extract_fenced_json_block(trimmed: &str) -> Option<&str> {
    let fence_start = trimmed.find("```")?;
    let rest = &trimmed[fence_start + 3..];
    let newline_rel = rest.find('\n')?;
    let header = rest[..newline_rel].trim().trim_end_matches('\r');
    if !header.is_empty() && !header.eq_ignore_ascii_case("json") {
        return None;
    }
    let body = &rest[newline_rel + 1..];
    let fence_end = body.find("```")?;
    Some(body[..fence_end].trim())
}

pub(super) async fn submit_via_gh_binary(
    gh_bin: &OsStr,
    repo: &str,
    title: &str,
    body: &str,
    labels: &[String],
) -> Result<IssueSubmitResult> {
    anyhow::ensure!(is_valid_repo_slug(repo), "invalid repository slug `{repo}`");

    let Some(resolved_bin) = resolve_executable(gh_bin) else {
        return Ok(IssueSubmitResult::GhUnavailable {
            reason: "`gh` CLI not found on PATH".to_string(),
        });
    };

    let auth_status = tokio::time::timeout(
        GH_AUTH_TIMEOUT,
        output_with_etxtbsy_retry(|| {
            let mut cmd = Command::new(&resolved_bin);
            cmd.args(["auth", "status"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            cmd
        }),
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

    let safe_labels: Vec<&str> = labels
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|label| is_valid_issue_label(label))
        .collect();

    match run_gh_issue_create(&resolved_bin, repo, title, body, &safe_labels).await? {
        Ok(result) => Ok(result),
        Err(detail) if !safe_labels.is_empty() && is_missing_label_error(&detail) => {
            match run_gh_issue_create(&resolved_bin, repo, title, body, &[]).await? {
                Ok(result) => Ok(result),
                Err(retry_detail) => anyhow::bail!("`gh issue create` failed: {retry_detail}"),
            }
        }
        Err(detail) => anyhow::bail!("`gh issue create` failed: {detail}"),
    }
}

async fn run_gh_issue_create(
    resolved_bin: &OsStr,
    repo: &str,
    title: &str,
    body: &str,
    labels: &[&str],
) -> Result<std::result::Result<IssueSubmitResult, String>> {
    let mut child = spawn_with_etxtbsy_retry(|| {
        let mut cmd = Command::new(resolved_bin);
        cmd.arg("issue")
            .arg("create")
            .arg(format!("--repo={repo}"))
            .arg(format!("--title={title}"));
        for label in labels {
            cmd.arg(format!("--label={label}"));
        }
        cmd.arg("--body-file")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd
    })
    .await
    .context("spawn `gh issue create`")?;

    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(body.as_bytes()).await {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(err) => {
                return Err(err).context("write issue body to `gh issue create` stdin");
            }
        }
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
        return Ok(Err(clean));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = parse_created_issue_url(&stdout)
        .context("`gh issue create` succeeded without returning an issue URL")?;
    let number = parse_issue_number_from_url(&url);
    Ok(Ok(IssueSubmitResult::CreatedGhCli { number, url }))
}

fn is_valid_issue_label(label: &str) -> bool {
    !label.is_empty()
        && !label.starts_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_' | b'.' | b' '))
}

fn is_missing_label_error(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("not found") || lower.contains("could not add label")
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
        let res = submit_via_gh_binary(missing.as_os_str(), "realmorrisliu/alan", "t", "b", &[])
            .await
            .unwrap();
        assert!(matches!(res, IssueSubmitResult::GhUnavailable { .. }));

        let unauth = write_script(
            dir.path(),
            "gh-unauth",
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ]; then exit 1; fi\nexit 99\n",
        );
        let res = submit_via_gh_binary(unauth.as_os_str(), "realmorrisliu/alan", "t", "b", &[])
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
            &["alan:auto".to_string()],
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
        assert!(recorded_args.contains("--label=alan:auto"));
        assert!(recorded_args.contains("--body-file"));
        assert_eq!(std::fs::read_to_string(&body_log).unwrap(), "Redacted body");
    }

    #[tokio::test]
    async fn authenticated_gh_retries_without_label_when_repo_lacks_label() {
        let dir = tempfile::tempdir().unwrap();
        let calls_log = dir.path().join("calls.txt");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ]; then exit 0; fi\nprintf '%s\\n' \"$*\" >> \"{}\"\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    --label=*) echo \"'alan:auto' not found\" >&2; exit 1 ;;\n  esac\ndone\necho \"https://github.com/realmorrisliu/alan/issues/1235\"\n",
            calls_log.display(),
        );
        let gh = write_script(dir.path(), "gh-missing-label", &script);
        let res = submit_via_gh_binary(
            gh.as_os_str(),
            "realmorrisliu/alan",
            "Missing label fallback",
            "Redacted body",
            &["alan:auto".to_string()],
        )
        .await
        .unwrap();
        assert_eq!(
            res,
            IssueSubmitResult::CreatedGhCli {
                number: Some(1235),
                url: "https://github.com/realmorrisliu/alan/issues/1235".to_string(),
            }
        );
        let recorded = std::fs::read_to_string(&calls_log).unwrap();
        assert_eq!(recorded.lines().count(), 2);
        assert!(
            recorded
                .lines()
                .next()
                .unwrap()
                .contains("--label=alan:auto")
        );
        assert!(!recorded.lines().nth(1).unwrap().contains("--label="));
    }

    #[tokio::test]
    async fn authenticated_gh_surfaces_create_failure() {
        let dir = tempfile::tempdir().unwrap();
        let gh = write_script(
            dir.path(),
            "gh-fail",
            "#!/bin/sh\nif [ \"$1\" = \"auth\" ]; then exit 0; fi\necho 'network unreachable' >&2\nexit 1\n",
        );
        // Include a large body too so EPIPE on fast-exiting child does not mask stderr.
        let large_body = "x".repeat(256 * 1024);
        let err = submit_via_gh_binary(gh.as_os_str(), "realmorrisliu/alan", "t", &large_body, &[])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("network unreachable"), "{err:#}");
    }

    #[test]
    fn parse_draft_json_handles_plain_fenced_and_invalid_responses() {
        let plain = r#"{"title":"Gemini 400 on functionResponse","summary":"- Turn failed"}"#;
        assert_eq!(
            parse_draft_json(plain).unwrap(),
            (
                "Gemini 400 on functionResponse".to_string(),
                "- Turn failed".to_string()
            )
        );

        let fenced = "Here is the draft:\n```json\n{\"title\":\"Seatbelt false positive\",\"summary\":\"- Fails on /a\"}\n```";
        assert_eq!(
            parse_draft_json(fenced).unwrap(),
            (
                "Seatbelt false positive".to_string(),
                "- Fails on /a".to_string()
            )
        );

        let fenced_with_braces = "Note: {context}\n```json\n{\"title\":\"Seatbelt false positive\",\"summary\":\"- Fails on /a\"}\n```\nDone.";
        assert_eq!(
            parse_draft_json(fenced_with_braces).unwrap(),
            (
                "Seatbelt false positive".to_string(),
                "- Fails on /a".to_string()
            )
        );

        let embedded = "Notes:\n{\"title\":\"Embedded title\",\"summary\":\"- Summary\"}\nThanks";
        assert_eq!(
            parse_draft_json(embedded).unwrap(),
            ("Embedded title".to_string(), "- Summary".to_string())
        );

        assert!(parse_draft_json("{\"title\":\"\",\"summary\":\"- x\"}").is_err());
        assert!(parse_draft_json("not json").is_err());
    }
}
