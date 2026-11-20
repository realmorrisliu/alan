use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};

static TEMP_DRAFT_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDraftFile {
    path: PathBuf,
}

impl TempDraftFile {
    fn create(initial: &str) -> Result<Self> {
        let base_dir = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..64 {
            let seq = TEMP_DRAFT_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = base_dir.join(format!("alan-prompt-{pid}-{seq}.md"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(initial.as_bytes())
                        .context("write draft to temporary editor file")?;
                    file.flush()
                        .context("flush draft to temporary editor file")?;
                    return Ok(Self { path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(err).context("create temporary file for external editor");
                }
            }
        }
        bail!("failed to allocate unique temporary file for external editor");
    }
}

impl Drop for TempDraftFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn resolve_editor_spec() -> String {
    for key in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(key)
            && !value.trim().is_empty()
        {
            return value;
        }
    }
    "vi".to_string()
}

pub(super) fn parse_editor_command(spec: &str) -> Result<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut in_token = false;

    for ch in spec.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            in_token = true;
            continue;
        }
        match quote {
            Some(q) if ch == q => {
                quote = None;
            }
            Some(_) => {
                if ch == '\\' && quote == Some('"') {
                    escaped = true;
                } else {
                    current.push(ch);
                }
                in_token = true;
            }
            None if ch == '\\' => {
                escaped = true;
                in_token = true;
            }
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                in_token = true;
            }
            None if ch.is_whitespace() => {
                if in_token {
                    parts.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            None => {
                current.push(ch);
                in_token = true;
            }
        }
    }
    if quote.is_some() {
        bail!("unterminated quote in editor command");
    }
    if escaped {
        current.push('\\');
    }
    if in_token {
        parts.push(current);
    }
    if parts.is_empty() {
        bail!("editor command is empty");
    }
    Ok(parts)
}

pub(super) fn edit_text_with_editor(initial: &str, editor_spec: &str) -> Result<Option<String>> {
    let parts = parse_editor_command(editor_spec)?;
    let (program, args) = parts
        .split_first()
        .context("editor command missing executable")?;
    let draft_file = TempDraftFile::create(initial)?;
    let status = Command::new(program)
        .args(args)
        .arg(&draft_file.path)
        .status()
        .with_context(|| format!("launch external editor `{program}`"))?;
    if !status.success() {
        bail!("external editor `{program}` exited with {status}");
    }
    let mut updated = std::fs::read_to_string(&draft_file.path)
        .context("read updated draft from temporary editor file")?;
    if !initial.ends_with('\n') && updated.ends_with('\n') {
        updated.pop();
        if updated.ends_with('\r') {
            updated.pop();
        }
    }
    Ok((updated != initial).then_some(updated))
}

pub(super) fn edit_text_in_external_editor(initial: &str) -> Result<Option<String>> {
    edit_text_with_editor(initial, &resolve_editor_spec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_editor_commands_with_flags() {
        assert_eq!(
            parse_editor_command("nvim -u NONE").unwrap(),
            vec!["nvim", "-u", "NONE"]
        );
        assert_eq!(
            parse_editor_command("\"/Applications/My Editor/bin\" --wait").unwrap(),
            vec!["/Applications/My Editor/bin", "--wait"]
        );
        assert!(parse_editor_command("\"unclosed").is_err());
    }

    #[test]
    fn round_trips_draft_through_external_editor_command() {
        let updated = edit_text_with_editor("old", "sh -c 'printf edited > \"$1\"' _").unwrap();
        assert_eq!(updated.as_deref(), Some("edited"));
    }
}
