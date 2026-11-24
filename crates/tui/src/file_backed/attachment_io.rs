use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use alan_agent_protocol::ContentPart;

pub(in crate::file_backed) const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::file_backed) enum PasteImageOutcome {
    Attached(Vec<ContentPart>),
    Warning(String),
    NotPureImagePaste,
}

pub(in crate::file_backed) fn attachment_hash(part: &ContentPart) -> Option<&str> {
    match part {
        ContentPart::Attachment { hash, .. } => Some(hash.as_str()),
        _ => None,
    }
}

pub(in crate::file_backed) fn push_deduped_attachment(
    parts: &mut Vec<ContentPart>,
    seen_hashes: &mut BTreeSet<String>,
    part: ContentPart,
) -> bool {
    if let Some(hash) = attachment_hash(&part)
        && !seen_hashes.insert(hash.to_string())
    {
        return false;
    }
    parts.push(part);
    true
}

pub(in crate::file_backed) fn attachment_filename(part: &ContentPart) -> &str {
    match part {
        ContentPart::Attachment { metadata, .. } => metadata
            .get("filename")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or("image"),
        _ => "image",
    }
}

pub(in crate::file_backed) fn attachment_byte_len(part: &ContentPart) -> Option<u64> {
    let ContentPart::Attachment { metadata, .. } = part else {
        return None;
    };
    if let Some(byte_len) = metadata.get("byte_len").and_then(serde_json::Value::as_u64) {
        return Some(byte_len);
    }
    let data = metadata.get("data").and_then(serde_json::Value::as_str)?;
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return None;
    }
    let padding = trimmed.bytes().rev().take(2).filter(|b| *b == b'=').count();
    Some(((trimmed.len() / 4) * 3).saturating_sub(padding) as u64)
}

pub(in crate::file_backed) fn format_byte_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub(in crate::file_backed) fn summarize_attachments(parts: &[ContentPart]) -> Option<String> {
    let images: Vec<_> = parts
        .iter()
        .filter(|part| matches!(part, ContentPart::Attachment { .. }))
        .collect();
    match images.len() {
        0 => None,
        1 => Some(format!("[1 image: {}]", attachment_filename(images[0]))),
        count => Some(format!("[{count} images]")),
    }
}

pub(in crate::file_backed) fn try_extract_pasted_image_attachments(
    text: &str,
) -> PasteImageOutcome {
    let home = std::env::var_os("HOME");
    try_extract_pasted_image_attachments_with_home(text, home.as_deref())
}

pub(super) fn try_extract_pasted_image_attachments_with_home(
    text: &str,
    home: Option<&OsStr>,
) -> PasteImageOutcome {
    if text.trim().is_empty() {
        return PasteImageOutcome::NotPureImagePaste;
    }

    let Some(paths) = parse_pure_image_paste_paths(text, home) else {
        return PasteImageOutcome::NotPureImagePaste;
    };

    let mut parts = Vec::new();
    let mut seen_hashes = BTreeSet::new();
    for path in paths {
        match load_bounded_image_with_error(&path) {
            Ok(part) => {
                push_deduped_attachment(&mut parts, &mut seen_hashes, part);
            }
            Err(warning) => return PasteImageOutcome::Warning(warning),
        }
    }
    if parts.is_empty() {
        PasteImageOutcome::NotPureImagePaste
    } else {
        PasteImageOutcome::Attached(parts)
    }
}

pub(in crate::file_backed) fn extract_prompt_image_attachments(
    prompt: &str,
    project_root: Option<&Path>,
) -> Vec<ContentPart> {
    let home = std::env::var_os("HOME");
    extract_prompt_image_attachments_with_home(prompt, project_root, home.as_deref())
}

pub(super) fn extract_prompt_image_attachments_with_home(
    prompt: &str,
    project_root: Option<&Path>,
    home: Option<&OsStr>,
) -> Vec<ContentPart> {
    let canonical_root = project_root.and_then(|root| std::fs::canonicalize(root).ok());
    let mut seen_hashes = BTreeSet::new();
    let mut seen_paths = BTreeSet::new();
    let mut parts = Vec::new();

    for (raw_token, was_quoted) in split_prompt_tokens(prompt) {
        if !was_quoted && let Some(rel) = raw_token.strip_prefix('@') {
            if let Some(root) = canonical_root.as_deref()
                && let Some(part) = resolve_project_at_image(rel, root, &mut seen_paths)
            {
                push_deduped_attachment(&mut parts, &mut seen_hashes, part);
            }
            continue;
        }

        let Some(path) = resolve_explicit_prompt_image_path(&raw_token, was_quoted, home) else {
            continue;
        };
        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        if !seen_paths.insert(canonical.clone()) {
            continue;
        }
        if let Ok(part) = load_bounded_image_with_error(&canonical) {
            push_deduped_attachment(&mut parts, &mut seen_hashes, part);
        }
    }
    parts
}

#[cfg(test)]
pub(in crate::file_backed) fn extract_at_image_attachments(
    prompt: &str,
    project_root: Option<&Path>,
) -> Vec<ContentPart> {
    let Some(root) = project_root else {
        return Vec::new();
    };
    let Ok(canonical_root) = std::fs::canonicalize(root) else {
        return Vec::new();
    };

    let mut seen_paths = BTreeSet::new();
    let mut seen_hashes = BTreeSet::new();
    let mut parts = Vec::new();
    for raw_token in prompt.split_whitespace() {
        let Some(rel) = raw_token.strip_prefix('@') else {
            continue;
        };
        if let Some(part) = resolve_project_at_image(rel, &canonical_root, &mut seen_paths) {
            push_deduped_attachment(&mut parts, &mut seen_hashes, part);
        }
    }
    parts
}

fn resolve_project_at_image(
    raw_rel: &str,
    canonical_root: &Path,
    seen_paths: &mut BTreeSet<PathBuf>,
) -> Option<ContentPart> {
    let rel = raw_rel.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
    if rel.is_empty() || mime_for_image_extension(Path::new(rel)).is_none() {
        return None;
    }
    let candidate = canonical_root.join(rel);
    let canonical_file = std::fs::canonicalize(&candidate).ok()?;
    if !canonical_file.starts_with(canonical_root)
        || !canonical_file.is_file()
        || !seen_paths.insert(canonical_file.clone())
    {
        return None;
    }
    load_bounded_image_with_error(&canonical_file).ok()
}

fn parse_pure_image_paste_paths(text: &str, home: Option<&OsStr>) -> Option<Vec<PathBuf>> {
    if let Some(tokens) = split_shell_tokens(text)
        && !tokens.is_empty()
    {
        let mut paths = Vec::with_capacity(tokens.len());
        let mut all_valid = true;
        for token in &tokens {
            if let Some(path) = parse_explicit_local_path(token, home)
                && is_existing_regular_image_file(&path)
            {
                paths.push(path);
            } else {
                all_valid = false;
                break;
            }
        }
        if all_valid {
            return Some(paths);
        }
    }

    // Fallback for unquoted paths containing spaces (one per line).
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut paths = Vec::with_capacity(lines.len());
    for line in lines {
        let candidate = strip_matching_quotes(line);
        let path = parse_explicit_local_path(candidate, home)?;
        if !is_existing_regular_image_file(&path) {
            return None;
        }
        paths.push(path);
    }
    Some(paths)
}

fn strip_matching_quotes(s: &str) -> &str {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn split_shell_tokens(text: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match quote {
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                } else {
                    cur.push(ch);
                }
            }
            Some('"') => {
                if ch == '"' {
                    quote = None;
                } else if ch == '\\' {
                    if matches!(chars.peek(), Some('"' | '\\')) {
                        cur.push(chars.next().expect("peeked char"));
                    } else {
                        cur.push('\\');
                    }
                } else {
                    cur.push(ch);
                }
            }
            Some(_) => unreachable!(),
            None => {
                if ch.is_whitespace() {
                    if in_token {
                        tokens.push(std::mem::take(&mut cur));
                        in_token = false;
                    }
                } else {
                    in_token = true;
                    match ch {
                        '\'' | '"' => quote = Some(ch),
                        '\\' => {
                            if let Some(next) = chars.next() {
                                cur.push(next);
                            } else {
                                cur.push('\\');
                            }
                        }
                        _ => cur.push(ch),
                    }
                }
            }
        }
    }

    if quote.is_some() {
        return None;
    }
    if in_token {
        tokens.push(cur);
    }
    Some(tokens)
}

fn split_prompt_tokens(prompt: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    let bytes = prompt.as_bytes();

    while idx < bytes.len() {
        let Some(ch) = prompt[idx..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            idx += ch.len_utf8();
            continue;
        }
        if (ch == '"' || ch == '\'')
            && let Some((quoted, end_idx)) = read_closed_quote(prompt, idx, ch)
        {
            out.push((quoted, true));
            idx = end_idx;
            continue;
        }
        let mut token = String::new();
        while idx < bytes.len() {
            let c = prompt[idx..].chars().next().expect("valid char");
            if c.is_whitespace() {
                break;
            }
            idx += c.len_utf8();
            if c == '\\' && idx < bytes.len() {
                let next = prompt[idx..].chars().next().expect("valid escaped char");
                idx += next.len_utf8();
                token.push(next);
            } else {
                token.push(c);
            }
        }
        if !token.is_empty() {
            out.push((token, false));
        }
    }
    out
}

fn read_closed_quote(prompt: &str, start_idx: usize, quote: char) -> Option<(String, usize)> {
    let mut idx = start_idx + quote.len_utf8();
    let mut content = String::new();
    while idx < prompt.len() {
        let c = prompt[idx..].chars().next()?;
        idx += c.len_utf8();
        if c == quote {
            return Some((content, idx));
        }
        if c == '\n' {
            return None;
        }
        if quote == '"' && c == '\\' && idx < prompt.len() {
            let next = prompt[idx..].chars().next()?;
            if next == '"' || next == '\\' {
                idx += next.len_utf8();
                content.push(next);
                continue;
            }
        }
        content.push(c);
    }
    None
}

fn resolve_explicit_prompt_image_path(
    token: &str,
    was_quoted: bool,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(path) = parse_explicit_local_path(token, home)
        && is_existing_regular_image_file(&path)
    {
        return Some(path);
    }
    if was_quoted {
        return None;
    }
    let trimmed_end = token.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
    let trimmed = trimmed_end
        .strip_prefix('(')
        .or_else(|| trimmed_end.strip_prefix('['))
        .unwrap_or(trimmed_end);
    if trimmed == token {
        return None;
    }
    let path = parse_explicit_local_path(trimmed, home)?;
    is_existing_regular_image_file(&path).then_some(path)
}

fn parse_explicit_local_path(token: &str, home: Option<&OsStr>) -> Option<PathBuf> {
    if token.starts_with("file://") {
        return parse_file_uri(token);
    }
    if let Some(rel) = token.strip_prefix("~/") {
        if rel.is_empty() {
            return None;
        }
        let home = home.filter(|value| !value.is_empty())?;
        return Some(PathBuf::from(home).join(rel));
    }
    if token.starts_with('/') {
        return Some(PathBuf::from(token));
    }
    None
}

fn parse_file_uri(token: &str) -> Option<PathBuf> {
    let rest = token.strip_prefix("file://")?;
    let raw_path = if rest.starts_with('/') {
        rest
    } else if rest.starts_with("localhost/") {
        &rest["localhost".len()..]
    } else {
        return None;
    };
    let decoded = percent_decode_bytes(raw_path)?;
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(decoded))
    };
    #[cfg(not(unix))]
    let path = PathBuf::from(String::from_utf8(decoded).ok()?);
    path.is_absolute().then_some(path)
}

fn percent_decode_bytes(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = *bytes.get(i + 1)?;
            let lo = *bytes.get(i + 2)?;
            let val = (hex_nibble(hi)? << 4) | hex_nibble(lo)?;
            out.push(val);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_existing_regular_image_file(path: &Path) -> bool {
    mime_for_image_extension(path).is_some()
        && std::fs::metadata(path)
            .map(|meta| meta.is_file())
            .unwrap_or(false)
}

fn read_bounded_image_bytes(path: &Path, empty_msg: &str, label: &str) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|err| format!("cannot inspect {label}: {err}"))?;
    if !meta.is_file() {
        return Err(format!("{label} is not a regular file"));
    }
    if meta.len() == 0 {
        return Err(empty_msg.to_string());
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(format!("{label} exceeds 20 MiB limit"));
    }
    let mut file = File::open(path).map_err(|err| format!("cannot read {label}: {err}"))?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.by_ref()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("cannot read {label}: {err}"))?;
    if bytes.is_empty() {
        return Err(empty_msg.to_string());
    }
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(format!("{label} exceeds 20 MiB limit"));
    }
    Ok(bytes)
}

fn load_bounded_image_with_error(path: &Path) -> Result<ContentPart, String> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("image");
    if mime_for_image_extension(path).is_none() {
        return Err(format!("unsupported image extension for `{filename}`"));
    }
    let label = format!("image `{filename}`");
    let bytes = read_bounded_image_bytes(path, &format!("{label} is empty"), &label)?;
    let Some(mime_type) = mime_for_magic_bytes(&bytes) else {
        return Err(format!(
            "image `{filename}` is not a valid PNG, JPEG, GIF, or WEBP file"
        ));
    };
    Ok(build_attachment_part(filename, mime_type, &bytes))
}

fn build_attachment_part(filename: &str, mime_type: &str, bytes: &[u8]) -> ContentPart {
    let encoded = encode_base64(bytes);
    let hash = fnv1a_hex(bytes);
    ContentPart::Attachment {
        hash,
        mime_type: mime_type.to_string(),
        metadata: serde_json::json!({
            "filename": filename,
            "byte_len": bytes.len(),
            "data": encoded,
            "image_url": format!("data:{mime_type};base64,{encoded}"),
        }),
    }
}

fn mime_for_image_extension(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn mime_for_magic_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}-{}", bytes.len())
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(target_os = "macos")]
pub(in crate::file_backed) fn load_clipboard_image_attachment() -> Result<ContentPart, String> {
    use std::fs::OpenOptions;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};

    static CLIPBOARD_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempCleanup(Vec<PathBuf>);
    impl Drop for TempCleanup {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    let create_temp = |suffix: &str| -> Result<PathBuf, String> {
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..8 {
            let seq = CLIPBOARD_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!("alan-clipboard-{pid}-{seq}.{suffix}"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(path),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(format!("cannot create clipboard temp file: {err}")),
            }
        }
        Err("cannot allocate clipboard temp file".to_string())
    };

    let raw_path = create_temp("img")?;
    let tiff_path = create_temp("tiff")?;
    let _cleanup = TempCleanup(vec![raw_path.clone(), tiff_path.clone()]);

    const SCRIPT: &str = r#"
on writeFileData(rawData, outPath)
    set outFile to POSIX file outPath
    set fRef to open for access outFile with write permission
    try
        set eof fRef to 0
        write rawData to fRef
        close access fRef
        return true
    on error
        try
            close access fRef
        end try
        return false
    end try
end writeFileData

on run argv
    set rawPath to item 1 of argv
    set tiffPath to item 2 of argv
    try
        if my writeFileData(the clipboard as «class PNGf», rawPath) then return "raw"
    end try
    try
        if my writeFileData(the clipboard as «class JPEG», rawPath) then return "raw"
    end try
    try
        if my writeFileData(the clipboard as «class GIFf», rawPath) then return "raw"
    end try
    try
        if my writeFileData(the clipboard as «class TIFF», tiffPath) then return "tiff"
    end try
    return "none"
end run
"#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(SCRIPT)
        .arg(&raw_path)
        .arg(&tiff_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("clipboard read failed: {err}"))?;
    if !output.status.success() {
        return Err("clipboard has no readable image".to_string());
    }
    let kind = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if kind == "tiff" {
        let conv = Command::new("sips")
            .args(["-s", "format", "png"])
            .arg(&tiff_path)
            .arg("--out")
            .arg(&raw_path)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| format!("clipboard image conversion failed: {err}"))?;
        if !conv.status.success() {
            return Err("clipboard image conversion failed".to_string());
        }
    } else if kind != "raw" {
        return Err("no image in clipboard".to_string());
    }

    let bytes = read_bounded_image_bytes(&raw_path, "no image in clipboard", "clipboard image")?;
    let Some(mime_type) = mime_for_magic_bytes(&bytes) else {
        return Err("clipboard data is not a valid PNG, JPEG, GIF, or WEBP image".to_string());
    };
    let filename = match mime_type {
        "image/png" => "clipboard.png",
        "image/jpeg" => "clipboard.jpg",
        "image/gif" => "clipboard.gif",
        "image/webp" => "clipboard.webp",
        _ => "clipboard.png",
    };
    Ok(build_attachment_part(filename, mime_type, &bytes))
}

#[cfg(test)]
#[path = "attachment_io_tests.rs"]
mod tests;
