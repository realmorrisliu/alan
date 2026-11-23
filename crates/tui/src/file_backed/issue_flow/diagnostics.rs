//! Strictly redacted session diagnostics and bounded GitHub URL builder for `/issue`.

use alan_agent_protocol::UiNoticeKind;

use super::{DEFAULT_ISSUE_REPO, FileBackedApp};
use crate::file_backed::model::safe;
use crate::history::{HistoryCell, ToolStatus};

const MAX_DIAGNOSTIC_SUMMARY_CHARS: usize = 240;
const MAX_RECENT_FAILURES: usize = 6;
const MAX_DEFAULT_TITLE_CHARS: usize = 72;
pub(super) const MAX_FALLBACK_URL_BYTES: usize = 2048;
const MAX_TITLE_URL_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_backed) struct RedactedDiagnostics {
    pub alan_version: String,
    pub os_arch: String,
    pub active_profile: Option<String>,
    pub model_label: String,
    pub trust_posture: String,
    pub latest_summary: String,
    pub recent_failures: Vec<String>,
}

impl RedactedDiagnostics {
    pub(in crate::file_backed) fn from_app(app: &FileBackedApp) -> Self {
        let active_profile = app
            .model
            .known()
            .and_then(|snapshot| snapshot.active.as_ref().or(snapshot.selected_next.as_ref()))
            .map(|binding| safe(&binding.profile))
            .map(|profile| profile.trim().to_string())
            .filter(|profile| !profile.is_empty());

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

        let recent_failures = collect_recent_failures(app);

        Self {
            alan_version: env!("CARGO_PKG_VERSION").to_string(),
            os_arch: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
            active_profile,
            model_label: redact_diagnostic_text(&model_label),
            trust_posture: redact_diagnostic_text(&trust_posture),
            latest_summary,
            recent_failures,
        }
    }

    pub(in crate::file_backed) fn has_real_failure(&self) -> bool {
        (self.latest_summary != "none" && !self.latest_summary.trim().is_empty())
            || !self.recent_failures.is_empty()
    }

    pub(in crate::file_backed) fn default_title(&self) -> String {
        if !self.has_real_failure() {
            return String::new();
        }
        let source = if self.latest_summary != "none" && !self.latest_summary.trim().is_empty() {
            self.latest_summary.as_str()
        } else {
            self.recent_failures
                .last()
                .map(String::as_str)
                .unwrap_or("runtime failure")
        };
        let stripped = strip_internal_error_wrappers(source);
        let single = sanitize_single_line(stripped);
        if single.is_empty() {
            return String::new();
        }
        truncate_chars(&single, MAX_DEFAULT_TITLE_CHARS)
    }

    pub(in crate::file_backed) fn default_summary(&self) -> String {
        if !self.has_real_failure() {
            return String::new();
        }
        let mut lines = Vec::new();
        let highlights: Vec<&str> = if !self.recent_failures.is_empty() {
            self.recent_failures
                .iter()
                .rev()
                .take(2)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        } else {
            vec![self.latest_summary.as_str()]
        };
        for item in highlights {
            let cleaned = sanitize_single_line(strip_internal_error_wrappers(item));
            if !cleaned.is_empty() {
                lines.push(format!("- Observed failure: {cleaned}"));
            }
        }
        if lines.is_empty() {
            return String::new();
        }
        lines.push(format!(
            "- Context: Alan v{} ({}), model `{}`, posture `{}`",
            self.alan_version, self.os_arch, self.model_label, self.trust_posture
        ));
        lines.join("\n")
    }

    pub(in crate::file_backed) fn format_issue_body_with_summary(
        &self,
        summary: &str,
    ) -> (String, usize) {
        let prefix = "### Summary\n";
        let summary_trimmed = summary.trim();
        let (summary_block, insert_offset) = if summary_trimmed.is_empty() {
            (format!("{prefix}\n\n"), prefix.len())
        } else {
            (
                format!("{prefix}{summary_trimmed}\n\n"),
                prefix.len() + summary_trimmed.len(),
            )
        };
        let mut body = format!(
            "{summary_block}### Redacted Diagnostics\n- Alan version: {}\n- OS/Arch: {}\n- Active model: {}\n- Permission posture: {}\n- Latest error/notice: {}\n",
            self.alan_version,
            self.os_arch,
            self.model_label,
            self.trust_posture,
            self.latest_summary
        );
        let include_timeline = self.recent_failures.len() > 1
            || self
                .recent_failures
                .first()
                .is_some_and(|first| first != &self.latest_summary);
        if include_timeline && !self.recent_failures.is_empty() {
            body.push_str("- Recent failures (redacted):\n");
            for item in &self.recent_failures {
                body.push_str(&format!("  - {item}\n"));
            }
        }
        (body, insert_offset)
    }

    pub(in crate::file_backed) fn to_draft_prompt_context(&self) -> String {
        let mut out = format!(
            "Alan version: {}\nOS/Arch: {}\nActive model: {}\nPermission posture: {}\nLatest error/notice: {}\n",
            self.alan_version,
            self.os_arch,
            self.model_label,
            self.trust_posture,
            self.latest_summary
        );
        if !self.recent_failures.is_empty() {
            out.push_str("Recent failures (chronological, redacted):\n");
            for (idx, item) in self.recent_failures.iter().enumerate() {
                out.push_str(&format!("{}. {item}\n", idx + 1));
            }
        }
        out
    }
}

fn collect_recent_failures(app: &FileBackedApp) -> Vec<String> {
    let mut entries = Vec::new();
    let mut push_unique = |raw: &str| {
        let redacted = redact_diagnostic_text(raw);
        if !redacted.is_empty()
            && redacted != "none"
            && entries.last().map(String::as_str) != Some(redacted.as_str())
        {
            entries.push(redacted);
        }
    };

    for cell in &app.transcript {
        match cell {
            HistoryCell::Error(msg) => push_unique(msg),
            HistoryCell::Tool {
                title,
                status: ToolStatus::Failed,
                preview,
                ..
            } => {
                // Only emit allowlisted tool category and static failure metadata so source
                // snippets (for example `edit_file` search strings or inline `Bash` scripts)
                // can never enter automatic diagnostics or AI drafting prompts.
                let summary = allowlisted_tool_failure_summary(title, preview.as_deref());
                push_unique(&summary);
            }
            _ => {}
        }
    }
    if let Some(notice) = app
        .notice
        .as_ref()
        .filter(|n| (n.kind != UiNoticeKind::None || n.runtime_owned) && !n.trim().is_empty())
    {
        push_unique(notice);
    }
    if let Some(diag) = app
        .last_diagnostic_notice
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        push_unique(diag);
    }
    if let Some(err) = app
        .expected_terminal_error
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        push_unique(err);
    }
    if entries.is_empty() && app.last_input_failed {
        entries.push("last input failed".to_string());
    }
    if entries.len() > MAX_RECENT_FAILURES {
        entries.drain(..entries.len() - MAX_RECENT_FAILURES);
    }
    entries
}

fn allowlisted_tool_kind(title: &str) -> &'static str {
    match title.split_whitespace().next().unwrap_or("") {
        "Bash" | "bash" => "bash",
        "Read" | "read_file" => "read_file",
        "Write" | "write_file" => "write_file",
        "Edit" | "edit_file" => "edit_file",
        "Grep" | "grep" => "grep",
        "Glob" | "glob" => "glob",
        "List" | "list_dir" => "list_dir",
        "cd" => "cd",
        _ => "tool",
    }
}

fn allowlisted_tool_failure_reason(preview: &str) -> Option<&'static str> {
    let lower = preview.to_ascii_lowercase();
    const REASONS: &[(&str, &str)] = &[
        (
            "absolute path outside host_mount",
            "sandbox path guard: absolute path outside host_mount",
        ),
        (
            "outside host_mount",
            "sandbox path guard: path outside host_mount",
        ),
        ("protected subpath", "sandbox path guard: protected subpath"),
        (
            "sensitive read-deny",
            "sandbox path guard: sensitive read-deny path",
        ),
        (
            "network access is confined",
            "sandbox guard: network confined",
        ),
        ("search text not found", "search text not found"),
        ("timed out", "timed out"),
        ("timeout", "timed out"),
        ("permission denied", "permission denied"),
        ("operation not permitted", "permission denied"),
        ("rejected", "rejected"),
        ("cancelled", "cancelled"),
    ];
    REASONS
        .iter()
        .find_map(|(needle, static_label)| lower.contains(needle).then_some(*static_label))
}

fn parse_tool_exit_code(preview: &str) -> Option<i32> {
    let lower = preview.to_ascii_lowercase();
    for prefix in ["exit ", "exit_code: ", "exit_code="] {
        if let Some(pos) = lower.find(prefix) {
            let rest = lower[pos + prefix.len()..].trim_start();
            let digits: String = rest
                .chars()
                .enumerate()
                .take_while(|&(i, c)| c.is_ascii_digit() || (i == 0 && c == '-'))
                .map(|(_, c)| c)
                .collect();
            if let Ok(code) = digits.parse::<i32>() {
                return Some(code);
            }
        }
    }
    None
}

fn allowlisted_tool_failure_summary(title: &str, preview: Option<&str>) -> String {
    let tool_kind = allowlisted_tool_kind(title);
    let reason = preview.and_then(allowlisted_tool_failure_reason);
    let exit_code = preview.and_then(parse_tool_exit_code);
    match (reason, exit_code) {
        (Some(r), Some(code)) => format!("tool {tool_kind} failed ({r}, exit {code})"),
        (Some(r), None) => format!("tool {tool_kind} failed ({r})"),
        (None, Some(code)) => format!("tool {tool_kind} failed (exit {code})"),
        (None, None) => format!("tool {tool_kind} failed"),
    }
}

fn strip_internal_error_wrappers(mut raw: &str) -> &str {
    const PREFIXES: &[&str] = &[
        "Error handling submission:",
        "Namespace LLM request failed:",
        "llmfs generation failed:",
        "Error:",
    ];
    loop {
        let trimmed = raw.trim();
        let Some(next) = PREFIXES
            .iter()
            .find_map(|prefix| trimmed.strip_prefix(prefix))
        else {
            return trimmed;
        };
        raw = next;
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
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
    labels: &[String],
) -> String {
    let target = if is_valid_repo_slug(repo) {
        repo
    } else {
        DEFAULT_ISSUE_REPO
    };
    let bounded_title = truncate_utf8_bytes(title.trim(), MAX_TITLE_URL_BYTES);
    let valid_labels: Vec<&str> = labels
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let labels_param = if valid_labels.is_empty() {
        String::new()
    } else {
        format!("&labels={}", percent_encode(&valid_labels.join(",")))
    };
    let prefix = format!(
        "https://github.com/{target}/issues/new?title={}{labels_param}&body=",
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

/// Strictly redact tokens, credentials, env assignments, and user home/mount paths across multiple lines.
pub(in crate::file_backed) fn redact_diagnostic_multiline(raw: &str, max_chars: usize) -> String {
    let normalized = sanitize_multiline(raw);
    let mut lines = Vec::new();
    let mut total_chars = 0usize;
    for line in normalized.lines() {
        let redacted = redact_diagnostic_text(line);
        if redacted.trim().is_empty() {
            continue;
        }
        let line_chars = redacted.chars().count();
        if total_chars + line_chars + 1 > max_chars && !lines.is_empty() {
            break;
        }
        total_chars += line_chars + usize::from(!lines.is_empty());
        lines.push(redacted);
    }
    truncate_chars(&lines.join("\n"), max_chars)
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
    let mut current = tok.to_string();
    while let Some(pos) = current.find("/mnt/project-request-") {
        let rest = &current[pos + "/mnt/project-request-".len()..];
        let suffix = rest.find('/').map_or("", |i| &rest[i..]);
        current = format!("{}<mounted-project>{suffix}", &current[..pos]);
    }
    for prefix in ["/Users/", "/home/"] {
        while let Some(pos) = current.find(prefix) {
            let after = &current[pos + prefix.len()..];
            let suffix = after.find('/').map_or("", |i| &after[i..]);
            current = format!("{}~{suffix}", &current[..pos]);
        }
    }
    current
}

fn is_sensitive_key_name(key: &str) -> bool {
    if key.contains('/') || key.contains('\\') {
        return false;
    }
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
