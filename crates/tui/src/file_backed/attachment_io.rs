use std::fs::File;
use std::io::Read;
use std::path::Path;

use alan_agent_protocol::ContentPart;

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

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

    let mut seen = Vec::new();
    let mut parts = Vec::new();
    for raw_token in prompt.split_whitespace() {
        let Some(rel) = raw_token.strip_prefix('@') else {
            continue;
        };
        let rel = rel.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
        if rel.is_empty() || mime_for_image_extension(Path::new(rel)).is_none() {
            continue;
        }
        let candidate = canonical_root.join(rel);
        let Ok(canonical_file) = std::fs::canonicalize(&candidate) else {
            continue;
        };
        if !canonical_file.starts_with(&canonical_root)
            || !canonical_file.is_file()
            || seen.contains(&canonical_file)
        {
            continue;
        }
        if let Some(part) = load_bounded_image(&canonical_file) {
            seen.push(canonical_file);
            parts.push(part);
        }
    }
    parts
}

fn load_bounded_image(path: &Path) -> Option<ContentPart> {
    let mime_type = mime_for_image_extension(path)?;
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() == 0 || meta.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.by_ref()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return None;
    }
    let encoded = encode_base64(&bytes);
    let hash = fnv1a_hex(&bytes);
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("image");
    Some(ContentPart::Attachment {
        hash,
        mime_type: mime_type.to_string(),
        metadata: serde_json::json!({
            "filename": filename,
            "data": encoded,
            "image_url": format!("data:{mime_type};base64,{encoded}"),
        }),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64_and_resolves_project_scoped_at_image_only() {
        assert_eq!(encode_base64(b"hi!"), "aGkh");
        assert_eq!(encode_base64(b"a"), "YQ==");

        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shot.png"), b"\x89PNG\r\n\x1a\n1234").unwrap();
        std::fs::write(outside.path().join("secret.png"), b"\x89PNG\r\n\x1a\n5678").unwrap();

        let prompt = format!(
            "inspect @shot.png and ignore @../{}/secret.png",
            outside.path().file_name().unwrap().to_str().unwrap()
        );
        let parts = extract_at_image_attachments(&prompt, Some(dir.path()));
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            ContentPart::Attachment {
                mime_type,
                metadata,
                ..
            } => {
                assert_eq!(mime_type, "image/png");
                assert_eq!(metadata["filename"], "shot.png");
                assert!(
                    metadata["image_url"]
                        .as_str()
                        .unwrap()
                        .starts_with("data:image/png;base64,")
                );
            }
            other => panic!("unexpected part: {other:?}"),
        }
    }
}
