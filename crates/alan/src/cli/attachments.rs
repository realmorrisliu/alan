use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use alan_agent_protocol::ContentPart;
use anyhow::{Context, Result, bail, ensure};
use base64::Engine as _;
use sha2::{Digest, Sha256};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Load, validate, hash, and base64-encode a local image file as a protocol `ContentPart::Attachment`.
pub fn load_image_attachment(path: &Path) -> Result<ContentPart> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("inspect `--image` path `{}`", path.display()))?;
    ensure!(
        meta.is_file(),
        "`--image` path is not a regular file: `{}`",
        path.display()
    );
    ensure!(
        meta.len() > 0 && meta.len() <= MAX_IMAGE_BYTES,
        "`--image` file `{}` must be between 1 byte and 20 MiB",
        path.display()
    );

    let mut file =
        File::open(path).with_context(|| format!("open `--image` file `{}`", path.display()))?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.by_ref()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read `--image` file `{}`", path.display()))?;
    ensure!(
        bytes.len() as u64 <= MAX_IMAGE_BYTES,
        "`--image` file `{}` exceeds 20 MiB limit",
        path.display()
    );

    let mime_type = detect_image_mime(path, &bytes).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported image format for `{}` (expected png, jpg/jpeg, gif, or webp)",
            path.display()
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let hash = format!("sha256:{}", hex_lower(&digest));
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("image");

    Ok(ContentPart::Attachment {
        hash,
        mime_type: mime_type.to_string(),
        metadata: serde_json::json!({
            "filename": filename,
            "data": encoded,
            "image_url": format!("data:{mime_type};base64,{encoded}"),
        }),
    })
}

/// Collect CLI image attachments from explicit `--image` flags and inline `@image` tokens.
pub fn collect_cli_image_attachments(
    prompt: Option<&str>,
    base_dir: Option<&Path>,
    explicit_images: &[PathBuf],
) -> Result<Vec<ContentPart>> {
    let mut parts = Vec::new();
    let mut seen = BTreeSet::new();

    for path in explicit_images {
        let canonical = std::fs::canonicalize(path)
            .with_context(|| format!("resolve `--image` path `{}`", path.display()))?;
        if seen.insert(canonical.clone()) {
            parts.push(load_image_attachment(&canonical)?);
        }
    }

    if let Some(prompt) = prompt {
        for raw_token in prompt.split_whitespace() {
            let Some(raw_ref) = raw_token.strip_prefix('@') else {
                continue;
            };
            let trimmed = raw_ref.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
            if trimmed.is_empty() || mime_for_extension(Path::new(trimmed)).is_none() {
                continue;
            }
            let ref_path = Path::new(trimmed);
            let candidate = if ref_path.is_absolute() {
                ref_path.to_path_buf()
            } else if let Some(dir) = base_dir {
                dir.join(ref_path)
            } else {
                continue;
            };
            let Ok(canonical) = std::fs::canonicalize(&candidate) else {
                continue;
            };
            if canonical.is_file() && seen.insert(canonical.clone()) {
                parts.push(load_image_attachment(&canonical)?);
            }
        }
    }

    Ok(parts)
}

fn detect_image_mime(path: &Path, bytes: &[u8]) -> Option<&'static str> {
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
    mime_for_extension(path)
}

fn mime_for_extension(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Validate `-p`/`--print`, `--json`, and `--image` flag combinations before runtime boot.
pub fn validate_cli_scripting_flags(
    print: Option<&str>,
    json: bool,
    images: &[PathBuf],
    has_subcommand: bool,
) -> Result<()> {
    if let Some(prompt) = print {
        ensure!(!has_subcommand, "`--print` only applies to bare `alan`");
        ensure!(
            !prompt.trim().is_empty() || !images.is_empty(),
            "`--print` prompt cannot be empty unless `--image` is provided"
        );
    }
    if json {
        ensure!(
            !has_subcommand,
            "`--json` applies to bare one-shot/`-p` runs (or `host`/`skills` subcommands)"
        );
    }
    if !images.is_empty() {
        ensure!(!has_subcommand, "`--image` only applies to bare `alan`");
        for path in images {
            if !path.is_file() {
                bail!(
                    "`--image` path does not exist or is not a file: `{}`",
                    path.display()
                );
            }
        }
    }
    Ok(())
}
