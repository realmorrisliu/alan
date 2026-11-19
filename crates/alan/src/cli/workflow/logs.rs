//! Bounded NDJSON run log persistence and secret scrubbing for scheduled workflows.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use alan_agent_engine::redact_secret_tokens;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::manifest::{WorkflowStore, atomic_write_file, ensure_real_dir, reject_if_symlink};

pub(crate) const MAX_LOG_ENTRIES_PER_WORKFLOW: usize = 200;
const MAX_LOG_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SUMMARY_CHARS: usize = 4_000;
const MAX_STDERR_CHARS: usize = 2_000;

/// Token usage captured from `alan -p --ephemeral --json` output when emitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    pub context_window: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// One bounded NDJSON run summary stored under `services/workflows/runs/<id>/runs.ndjson`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunLogEntry {
    pub run_id: String,
    pub workflow_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub status: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr_excerpt: String,
    pub ephemeral: bool,
    pub trust: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_profile: Option<String>,
    pub tool_calls: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<WorkflowRunUsage>,
}

/// Scrub secret tokens/headers/key-value assignments and bound text to `max_chars`.
pub fn scrub_and_bound_text(raw: &str, max_chars: usize) -> String {
    let base = redact_secret_tokens(raw.trim());
    let scrubbed = scrub_inline_key_value_secrets(&base);
    let cut_byte = scrubbed
        .char_indices()
        .nth(max_chars)
        .map_or(scrubbed.len(), |(idx, _)| idx);
    if cut_byte < scrubbed.len() {
        let mut bounded = scrubbed[..cut_byte].to_string();
        bounded.push('…');
        bounded
    } else {
        scrubbed
    }
}

fn scrub_inline_key_value_secrets(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;

    while cursor < text.len() {
        let rest = &text[cursor..];
        let at_word_start = cursor == 0
            || !(bytes[cursor - 1].is_ascii_alphanumeric() || bytes[cursor - 1] == b'_');
        if at_word_start {
            let key_len = rest
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(*b, b'_' | b'-'))
                .count();
            if key_len > 0 && is_sensitive_inline_key(&rest[..key_len]) {
                let after_key = &rest[key_len..];
                let ws_len = after_key
                    .bytes()
                    .take_while(|b| matches!(*b, b' ' | b'\t'))
                    .count();
                let sep_rest = &after_key[ws_len..];
                if let Some(sep) = sep_rest.as_bytes().first().copied()
                    && matches!(sep, b'=' | b':')
                {
                    let mut val_offset = key_len + ws_len + 1;
                    while val_offset < rest.len()
                        && matches!(rest.as_bytes()[val_offset], b' ' | b'\t')
                    {
                        val_offset += 1;
                    }
                    let val_slice = &rest[val_offset..];
                    if !val_slice.starts_with("[REDACTED") {
                        let val_len = if let Some(quote) = val_slice
                            .as_bytes()
                            .first()
                            .copied()
                            .filter(|q| *q == b'"' || *q == b'\'')
                        {
                            val_slice[1..]
                                .find(quote as char)
                                .map_or(val_slice.len(), |end| end + 2)
                        } else {
                            val_slice
                                .bytes()
                                .take_while(|b| {
                                    !b.is_ascii_whitespace() && !matches!(*b, b',' | b';')
                                })
                                .count()
                        };
                        if val_len > 0 {
                            out.push_str(&rest[..val_offset]);
                            out.push_str("[REDACTED]");
                            cursor += val_offset + val_len;
                            continue;
                        }
                    }
                }
            }
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        out.push(ch);
        cursor += ch.len_utf8();
    }

    out
}

fn is_sensitive_inline_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "setcookie"
            | "apikey"
            | "xapikey"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "bearertoken"
            | "clientsecret"
            | "password"
            | "passwd"
            | "passphrase"
            | "token"
            | "secret"
    ) || normalized.contains("apikey")
        || ["token", "secret", "password", "passwd", "passphrase"]
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
}

/// Sanitize and persist one NDJSON log entry while enforcing the per-workflow entry cap.
pub fn append_run_log(store: &WorkflowStore, entry: &WorkflowRunLogEntry) -> Result<PathBuf> {
    let run_dir = store.workflow_runs_dir(&entry.workflow_id)?;
    ensure_real_dir(&run_dir)?;
    let log_path = store.run_log_path(&entry.workflow_id)?;
    reject_if_symlink(&log_path)?;

    let mut sanitized = entry.clone();
    sanitized.summary = scrub_and_bound_text(&entry.summary, MAX_SUMMARY_CHARS);
    sanitized.stderr_excerpt = scrub_and_bound_text(&entry.stderr_excerpt, MAX_STDERR_CHARS);

    let mut existing = read_all_entries(&log_path)?;
    existing.push(sanitized);
    if existing.len() > MAX_LOG_ENTRIES_PER_WORKFLOW {
        let drop_count = existing.len() - MAX_LOG_ENTRIES_PER_WORKFLOW;
        existing.drain(..drop_count);
    }

    let mut encoded = String::new();
    for item in &existing {
        encoded.push_str(&serde_json::to_string(item)?);
        encoded.push('\n');
    }
    atomic_write_file(&log_path, encoded.as_bytes())?;
    Ok(log_path)
}

/// Read the newest `limit` run log entries for `id` (newest first).
pub fn read_run_logs(
    store: &WorkflowStore,
    id: &str,
    limit: usize,
) -> Result<Vec<WorkflowRunLogEntry>> {
    let log_path = store.run_log_path(id)?;
    let mut entries = read_all_entries(&log_path)?;
    entries.reverse();
    entries.truncate(limit.min(MAX_LOG_ENTRIES_PER_WORKFLOW));
    Ok(entries)
}

/// Return the most recent run log entry for `id`, if any.
pub fn latest_run_log(store: &WorkflowStore, id: &str) -> Result<Option<WorkflowRunLogEntry>> {
    let log_path = store.run_log_path(id)?;
    let mut entries = read_all_entries(&log_path)?;
    Ok(entries.pop())
}

fn read_all_entries(path: &PathBuf) -> Result<Vec<WorkflowRunLogEntry>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| format!("inspect log file {}", path.display()));
        }
    };
    ensure!(
        !meta.file_type().is_symlink() && meta.is_file(),
        "workflow log {} must be a regular non-symlink file",
        path.display()
    );
    ensure!(
        meta.len() <= MAX_LOG_FILE_BYTES,
        "workflow log {} exceeds {MAX_LOG_FILE_BYTES} bytes",
        path.display()
    );

    let raw = fs::read_to_string(path)
        .with_context(|| format!("read workflow log {}", path.display()))?;
    let mut entries = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<WorkflowRunLogEntry>(trimmed) {
            entries.push(entry);
        }
    }
    if entries.len() > MAX_LOG_ENTRIES_PER_WORKFLOW {
        let drop_count = entries.len() - MAX_LOG_ENTRIES_PER_WORKFLOW;
        entries.drain(..drop_count);
    }
    Ok(entries)
}

/// Ensure a non-empty log file path stays writable for direct tests if needed.
#[allow(dead_code, reason = "helper used by workflow log rotation")]
pub(crate) fn touch_log_file(path: &PathBuf) -> Result<()> {
    reject_if_symlink(path)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.flush()?;
    Ok(())
}
