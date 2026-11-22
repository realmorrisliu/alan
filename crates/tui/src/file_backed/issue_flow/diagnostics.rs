//! Strictly redacted session diagnostics and bounded GitHub URL builder for `/issue`.

use alan_agent_protocol::UiNoticeKind;

use super::{DEFAULT_ISSUE_REPO, FileBackedApp};
use crate::file_backed::model::safe;
use crate::history::HistoryCell;

const MAX_DIAGNOSTIC_SUMMARY_CHARS: usize = 240;
pub(super) const MAX_FALLBACK_URL_BYTES: usize = 2048;
const MAX_TITLE_URL_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_backed) struct RedactedDiagnostics {
    pub alan_version: String,
    pub os_arch: String,
    pub model_label: String,
    pub trust_posture: String,
    pub latest_summary: String,
}

impl RedactedDiagnostics {
    pub(in crate::file_backed) fn from_app(app: &FileBackedApp) -> Self {
        let model_label = if let Some(snapshot) = app.model.known() {
            if let Some(binding) = snapshot.active.as_ref().or(snapshot.selected_next.as_ref()) {
                let provider = safe(&binding.provider);
                let model = safe(&binding.model);
                match binding.reasoning.effort {
                    Some(effort) => format!("{provider} / {model} ({effort})"),
                    None => format!("{provider} / {model}"),
                }
            } else if app.is_connection_unconfigured() {
                "not connected".to_string()
            } else {
                "unknown".to_string()
            }
        } else if let Some(effective) = app.effective_model.as_deref().map(safe)
            && !effective.is_empty()
        {
            effective
        } else if app.is_connection_unconfigured() {
            "not connected".to_string()
        } else {
            "unknown".to_string()
        };

        let posture = app
            .trust_posture_label
            .as_deref()
            .map(safe)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "governed".to_string());
        let sandbox = app
            .sandbox_backend
            .as_deref()
            .map(safe)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let mount_mode = app
            .project
            .as_ref()
            .map(|p| p.access.label())
            .unwrap_or("none");
        let trust_posture = format!("{posture} (sandbox: {sandbox}, mount: {mount_mode})");

        let raw_summary = app
            .expected_terminal_error
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                app.notice
                    .as_ref()
                    .filter(|n| {
                        (n.kind != UiNoticeKind::None || n.runtime_owned) && !n.trim().is_empty()
                    })
                    .map(|n| &**n)
            })
            .or_else(|| {
                app.last_diagnostic_notice
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
            })
            .or_else(|| {
                app.transcript.iter().rev().find_map(|cell| match cell {
                    HistoryCell::Error(msg) if !msg.trim().is_empty() => Some(msg.as_str()),
                    _ => None,
                })
            })
            .or_else(|| {
                app.notice
                    .as_ref()
                    .filter(|n| !n.trim().is_empty() && !n.starts_with("Commands:\n"))
                    .map(|n| &**n)
            });
        let latest_summary = raw_summary
            .map(redact_diagnostic_text)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if app.last_input_failed {
                    "last input failed".to_string()
                } else {
                    "none".to_string()
                }
            });

        Self {
            alan_version: env!("CARGO_PKG_VERSION").to_string(),
            os_arch: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
            model_label: redact_diagnostic_text(&model_label),
            trust_posture: redact_diagnostic_text(&trust_posture),
            latest_summary,
        }
    }

    pub(in crate::file_backed) fn format_issue_body(&self) -> (String, usize) {
        let prefix = "### Summary\n";
        let insert_offset = prefix.len();
        let body = format!(
            "{prefix}\n\n### Redacted Diagnostics\n- Alan version: {}\n- OS/Arch: {}\n- Active model: {}\n- Permission posture: {}\n- Latest error/notice: {}\n",
            self.alan_version,
            self.os_arch,
            self.model_label,
            self.trust_posture,
            self.latest_summary
        );
        (body, insert_offset)
    }
}

pub(in crate::file_backed) fn is_valid_repo_slug(repo: &str) -> bool {
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

pub(in crate::file_backed) fn build_prefilled_issue_url(
    repo: &str,
    title: &str,
    body: &str,
) -> String {
    let target = if is_valid_repo_slug(repo) {
        repo
    } else {
        DEFAULT_ISSUE_REPO
    };
    let bounded_title = truncate_utf8_bytes(title.trim(), MAX_TITLE_URL_BYTES);
    let prefix = format!(
        "https://github.com/{target}/issues/new?title={}&body=",
        percent_encode(&bounded_title)
    );
    let budget = MAX_FALLBACK_URL_BYTES.saturating_sub(prefix.len());
    let encoded_body = encode_bounded_query_value(body.trim(), budget);
    format!("{prefix}{encoded_body}")
}

fn encode_bounded_query_value(text: &str, max_encoded_bytes: usize) -> String {
    let suffix = percent_encode("\n...(truncated)");
    let mut encoded = String::new();
    let mut truncated = false;
    for ch in text.chars() {
        let mut utf8 = [0u8; 4];
        let piece = percent_encode(ch.encode_utf8(&mut utf8));
        if encoded.len() + piece.len() > max_encoded_bytes {
            truncated = true;
            break;
        }
        encoded.push_str(&piece);
    }
    if truncated {
        if max_encoded_bytes >= suffix.len() {
            while encoded.len() + suffix.len() > max_encoded_bytes {
                if let Some(last_pct) = encoded.rfind('%')
                    && encoded.len() - last_pct == 3
                {
                    encoded.truncate(last_pct);
                } else {
                    encoded.pop();
                }
            }
            encoded.push_str(&suffix);
        } else {
            encoded.clear();
        }
    }
    encoded
}

fn percent_encode(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(HEX[(byte >> 4) as usize] as char);
            out.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    out
}

fn truncate_utf8_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

/// Strictly redact tokens, credentials, env assignments, and user home/mount paths.
pub(in crate::file_backed) fn redact_diagnostic_text(raw: &str) -> String {
    let stripped = strip_ansi_and_control(raw);
    let mut tokens = Vec::new();
    let mut iter = stripped.split_whitespace().peekable();
    while let Some(tok) = iter.next() {
        let lower = tok.to_ascii_lowercase();
        if (lower == "bearer" || lower == "basic") && iter.peek().is_some() {
            let _ = iter.next();
            tokens.push(format!("{tok} <redacted>"));
            continue;
        }
        if let Some(key) = tok.strip_suffix(':').or_else(|| tok.strip_suffix('='))
            && is_sensitive_key_name(key)
            && let Some(next_tok) = iter.next()
        {
            if matches!(next_tok.to_ascii_lowercase().as_str(), "bearer" | "basic")
                && iter.peek().is_some()
            {
                let _ = iter.next();
            }
            tokens.push(format!("{tok} <redacted>"));
            continue;
        }
        tokens.push(redact_single_token(tok));
    }
    let joined = tokens.join(" ");
    let mut out = String::new();
    for (idx, ch) in joined.chars().enumerate() {
        if idx >= MAX_DIAGNOSTIC_SUMMARY_CHARS {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn redact_single_token(tok: &str) -> String {
    if let Some(scheme_idx) = tok.find("://")
        && let Some(at_rel) = tok[scheme_idx + 3..].find('@')
    {
        let authority_start = scheme_idx + 3;
        let slash_rel = tok[authority_start..].find('/');
        if slash_rel.is_none_or(|s| at_rel < s) {
            return format!(
                "{}<redacted>{}",
                &tok[..authority_start],
                &tok[authority_start + at_rel..]
            );
        }
    }
    for sep in ['=', ':'] {
        if let Some((key, _)) = tok.split_once(sep)
            && is_sensitive_key_name(key)
        {
            return format!("{key}{sep}<redacted>");
        }
    }
    let trimmed_punct = tok.trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';' | ')' | ']'));
    if looks_like_secret_token(trimmed_punct) {
        return "<redacted-token>".to_string();
    }
    if let Some(pos) = tok.find("/mnt/project-request-") {
        let rest = &tok[pos + "/mnt/project-request-".len()..];
        let suffix = rest.find('/').map_or("", |i| &rest[i..]);
        return format!("{}<mounted-project>{suffix}", &tok[..pos]);
    }
    for prefix in ["/Users/", "/home/"] {
        if let Some(pos) = tok.find(prefix) {
            let after = &tok[pos + prefix.len()..];
            let suffix = after.find('/').map_or("", |i| &after[i..]);
            return format!("{}~{suffix}", &tok[..pos]);
        }
    }
    tok.to_string()
}

fn is_sensitive_key_name(key: &str) -> bool {
    let upper = key
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '[' | '('))
        .to_ascii_uppercase();
    !upper.is_empty()
        && [
            "KEY",
            "TOKEN",
            "SECRET",
            "PASSWORD",
            "PASSWD",
            "CREDENTIAL",
            "COOKIE",
            "AUTHORIZATION",
        ]
        .iter()
        .any(|needle| upper.contains(needle))
}

fn looks_like_secret_token(tok: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "sk-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "AIza",
        "ya29.",
        "AKIA",
        "xoxb-",
        "xoxp-",
        "eyJ",
    ];
    PREFIXES
        .iter()
        .any(|p| tok.starts_with(p) && tok.len() >= p.len() + 6)
}

fn strip_ansi_and_control(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next.is_ascii_alphabetic() || next == '~' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '\x07' || (next == '\x1b' && chars.peek() == Some(&'\\')) {
                            if next == '\x1b' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn sanitize_single_line(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn sanitize_multiline(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .chars()
        .filter(|&c| c == '\n' || !c.is_control())
        .collect()
}
