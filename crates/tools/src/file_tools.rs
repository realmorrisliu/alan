use crate::namespace_fs::{MAX_NAMESPACE_READ_BYTES, NamespaceFs, resolve_namespace_path};
use alan_agent_engine::tools::alan_ap::FileKind;
use alan_agent_engine::tools::{Tool, ToolContext, ToolResult};
use alan_agent_protocol::{ContentPart, DiffHunk, DiffLine, ToolResultPresentation};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::path::Path;

const MAX_INLINE_IMAGE_BYTES: usize = 512 * 1024;
const MAX_TEXT_READ_BYTES: usize = 20_000;
const EDIT_DIFF_CONTEXT_LINES: usize = 2;
const EDIT_DIFF_MAX_LINES: usize = 400;

/// read_file - Read a file's contents
#[derive(Default)]
pub struct ReadFileTool;

impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for ReadFileTool {
    fn presentation_is_read_only(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a file's contents. Text files return `1| `-prefixed line-numbered lines (default up to 1000 lines; use `offset` and `limit` for large files). Supported images (<= 512 KiB) are returned as multimodal image content plus metadata. Retained action outputs at `/agent/<pid>/actions/<id>/output` can be paged with `byte_offset` and `byte_limit` (at most 4096 bytes per read) without rerunning the producer."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "alan9 path to the file, or a path relative to Process cwd"
                },
                "offset": {
                    "type": "integer",
                    "description": "Start reading from this line (1-indexed)",
                    "minimum": 1
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read",
                    "minimum": 1,
                    "maximum": 1000
                },
                "byte_offset": {
                    "type": "integer",
                    "description": "Zero-indexed original byte offset for your own retained Action output only; use next_byte_offset to continue. Do not combine with line offset/limit.",
                    "minimum": 0
                },
                "byte_limit": {
                    "type": "integer",
                    "description": "Maximum original bytes for retained Action output only (default 4096). Do not combine with line offset/limit.",
                    "minimum": 1,
                    "maximum": ToolContext::MAX_EVIDENCE_READ_BYTES
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let requested = args["path"].as_str().unwrap_or("");
        let has_non_null = |key: &str| args.get(key).is_some_and(|v| !v.is_null());
        let byte_offset = args["byte_offset"].as_u64().unwrap_or(0);
        let byte_limit = args["byte_limit"]
            .as_u64()
            .unwrap_or(ToolContext::MAX_EVIDENCE_READ_BYTES);
        if let Some(read) = ctx.read_namespace_evidence(requested, byte_offset, byte_limit) {
            if has_non_null("offset")
                || has_non_null("limit")
                || args
                    .get("byte_offset")
                    .is_some_and(|v| !v.is_null() && !v.is_u64())
                || args
                    .get("byte_limit")
                    .is_some_and(|v| !v.is_null() && !v.is_u64())
            {
                return Box::pin(async {
                    Err(anyhow!(
                        "Retained evidence requires valid byte ranges (`byte_offset` >= 0, `byte_limit` in 1..={}), not line `offset`/`limit`. Omit `offset`/`limit` and start with `byte_offset: 0` (then pass `next_byte_offset` to continue); for regular files under `/mnt/...`, use `offset`/`limit` on the original file path.",
                        ToolContext::MAX_EVIDENCE_READ_BYTES
                    ))
                });
            }
            return read;
        }
        if has_non_null("byte_offset") || has_non_null("byte_limit") {
            return Box::pin(async {
                Err(anyhow!(
                    "Byte ranges are supported only for retained Action output"
                ))
            });
        }
        let offset = args["offset"].as_u64().unwrap_or(1) as usize;
        let limit = args["limit"].as_u64().unwrap_or(1000) as usize;

        if let Some(transport) = ctx.namespace().cloned() {
            let path = match resolve_namespace_path(&ctx.namespace_cwd, Some(requested)) {
                Ok(path) => path,
                Err(err) => return Box::pin(async move { Err(err) }),
            };
            if path == "/agent" || path.starts_with("/agent/") {
                return Box::pin(async {
                    Err(anyhow!(
                        "Only the receiving Agent's concrete Action output path can be read"
                    ))
                });
            }
            let ns = NamespaceFs::new(transport);
            return Box::pin(async move {
                let ns_path = Path::new(&path);
                let bytes = ns.read_file_bytes(&path, MAX_NAMESPACE_READ_BYTES).await?;
                if is_image(ns_path) {
                    return format_image_read(ns_path, &bytes, &path);
                }
                let content = String::from_utf8(bytes)
                    .map_err(|_| anyhow!("File '{path}' is not valid UTF-8"))?;
                Ok(format_text_read(&content, &path, offset, limit))
            });
        }

        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = match ctx.resolve_path(requested) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();

        Box::pin(async move {
            if is_image(&path) {
                let content = sandbox.read(&path).await?;
                return format_image_read(&path, &content, &visible_path);
            }

            let content = sandbox.read_string(&path).await?;
            Ok(format_text_read(&content, &visible_path, offset, limit))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

/// write_file - Write content to a file
#[derive(Default)]
pub struct WriteFileTool;

impl WriteFileTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Create a new file or overwrite an existing file with the given content, creating parent directories if needed. Prefer `edit_file` for targeted modifications to existing files."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "alan9 path to the file, or a path relative to Process cwd"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let requested = args["path"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("").to_string();

        if let Some(transport) = ctx.namespace().cloned() {
            let path = match resolve_namespace_path(&ctx.namespace_cwd, Some(requested)) {
                Ok(path) => path,
                Err(err) => return Box::pin(async move { Err(err) }),
            };
            let ns = NamespaceFs::new(transport);
            return Box::pin(async move {
                ns.write_file(&path, content.as_bytes()).await?;
                Ok(json!({
                    "success": true,
                    "path": path,
                    "bytes_written": content.len()
                }))
            });
        }

        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = match ctx.resolve_path(requested) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();

        Box::pin(async move {
            sandbox.write(&path, content.as_bytes()).await?;
            Ok(json!({
                "success": true,
                "path": visible_path,
                "bytes_written": content.len()
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Write
    }
}

/// edit_file - Edit a file using search/replace
#[derive(Default)]
pub struct EditFileTool;

impl EditFileTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Make targeted search-and-replace edits in an existing file. Read the file with `read_file` first. Pass exact text in `old_string` (omitting `read_file`'s `N| ` line-number prefixes); `old_string` must match uniquely unless `replace_all: true`."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "old_string", "new_string"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "alan9 path to the file, or a path relative to Process cwd"
                },
                "old_string": {
                    "type": "string",
                    "description": "Text to search for (must be unique unless replace_all is true)"
                },
                "new_string": {
                    "type": "string",
                    "description": "Text to replace with"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences of old_string instead of requiring a single unique match",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let requested = args["path"].as_str().unwrap_or("");
        let old_string = args["old_string"].as_str().unwrap_or("").to_string();
        let new_string = args["new_string"].as_str().unwrap_or("").to_string();
        let replace_all = args["replace_all"].as_bool().unwrap_or(false);

        if let Some(transport) = ctx.namespace().cloned() {
            let path = match resolve_namespace_path(&ctx.namespace_cwd, Some(requested)) {
                Ok(path) => path,
                Err(err) => return Box::pin(async move { Err(err) }),
            };
            let ns = NamespaceFs::new(transport);
            return Box::pin(async move {
                if old_string.is_empty() {
                    return Err(anyhow!("old_string must not be empty"));
                }
                let stat = ns.stat(&path).await?;
                if stat.qid.kind != FileKind::File {
                    return Err(anyhow!("Path '{path}' is not a regular file"));
                }
                if !stat.writable {
                    return Err(anyhow!("Failed to edit '{path}': no access"));
                }
                let bytes = ns.read_file_bytes(&path, MAX_NAMESPACE_READ_BYTES).await?;
                let content = String::from_utf8(bytes)
                    .map_err(|_| anyhow!("File '{path}' is not valid UTF-8"))?;
                let (new_content, match_count, presentation) =
                    apply_edit(&content, &path, &old_string, &new_string, replace_all)?;
                ns.write_file(&path, new_content.as_bytes()).await?;
                Ok(json!({
                    "success": true,
                    "path": path,
                    "replacements": match_count,
                    "presentation": serde_json::to_value(&presentation)?
                }))
            });
        }

        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = match ctx.resolve_path(requested) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();

        Box::pin(async move {
            if old_string.is_empty() {
                return Err(anyhow!("old_string must not be empty"));
            }
            let content = sandbox.read_string(&path).await?;
            let (new_content, match_count, presentation) = apply_edit(
                &content,
                &visible_path,
                &old_string,
                &new_string,
                replace_all,
            )?;
            sandbox.write(&path, new_content.as_bytes()).await?;

            Ok(json!({
                "success": true,
                "path": visible_path,
                "replacements": match_count,
                "presentation": serde_json::to_value(&presentation)?
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Write
    }
}

fn format_image_read(path: &Path, content: &[u8], visible_path: &str) -> Result<Value> {
    let mime_type = detect_mime(path);
    let size_bytes = content.len();
    let mut result = json!({
        "type": "image",
        "path": visible_path,
        "size_bytes": size_bytes,
        "mime_type": mime_type
    });
    if is_multimodal_image(path) {
        if size_bytes <= MAX_INLINE_IMAGE_BYTES {
            let hash = content_hash_hex(content);
            let encoded = encode_base64(content);
            let attachment = ContentPart::Attachment {
                hash,
                mime_type: mime_type.to_string(),
                metadata: json!({
                    "path": visible_path,
                    "size_bytes": size_bytes,
                    "data": encoded
                }),
            };
            result["content_parts"] = serde_json::to_value(vec![attachment])?;
        } else {
            result["attachment_omitted_reason"] = json!(format!(
                "image exceeds {} byte inline attachment limit",
                MAX_INLINE_IMAGE_BYTES
            ));
        }
    }
    Ok(result)
}

fn format_text_read(content: &str, visible_path: &str, offset: usize, limit: usize) -> Value {
    let lines: Vec<&str> = content.lines().collect();
    let start = offset.saturating_sub(1);
    let max_end = (start + limit).min(lines.len());

    let mut formatted = Vec::new();
    let mut accumulated_bytes = 0usize;
    if start < lines.len() {
        for (idx, line) in lines[start..max_end].iter().enumerate() {
            let entry = format!("{}| {}", start + idx + 1, line);
            let next_bytes = accumulated_bytes + entry.len() + usize::from(!formatted.is_empty());
            if !formatted.is_empty() && next_bytes > MAX_TEXT_READ_BYTES {
                break;
            }
            accumulated_bytes = next_bytes;
            formatted.push(entry);
        }
    }
    let end = start + formatted.len();

    json!({
        "type": "text",
        "path": visible_path,
        "content": formatted.join("\n"),
        "total_lines": lines.len(),
        "start_line": start + 1,
        "end_line": end,
        "truncated": end < lines.len()
    })
}

fn apply_edit(
    content: &str,
    visible_path: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Result<(String, usize, ToolResultPresentation)> {
    let match_count = content.matches(old_string).count();

    if match_count == 0 {
        return Err(anyhow!(
            "Search text not found in file: '{}...'",
            old_string.chars().take(50).collect::<String>()
        ));
    }
    if match_count > 1 && !replace_all {
        return Err(anyhow!(
            "Search text matches {match_count} occurrences in '{visible_path}'; provide more surrounding context so old_string is unique, or pass `replace_all: true` to replace all occurrences"
        ));
    }

    let hunks = build_edit_diff_hunks(content, old_string, new_string, replace_all);
    let presentation = ToolResultPresentation::Diff {
        path: visible_path.to_string(),
        hunks,
    };
    let new_content = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };
    Ok((new_content, match_count, presentation))
}

pub(super) fn is_image(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp"
        )
    } else {
        false
    }
}

fn is_multimodal_image(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
        )
    } else {
        false
    }
}

pub(super) fn detect_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

fn content_hash_hex(bytes: &[u8]) -> String {
    let mut hash: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for &byte in bytes {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    format!("{hash:032x}")
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
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

fn build_edit_diff_hunks(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Vec<DiffHunk> {
    let old_lines: Vec<&str> = content.split('\n').collect();
    let old_span_lines: Vec<&str> = old_string.split('\n').collect();
    let new_span_lines: Vec<&str> = new_string.split('\n').collect();

    let mut match_starts = Vec::new();
    for (byte_idx, _) in content.match_indices(old_string) {
        match_starts.push(byte_idx);
        if !replace_all {
            break;
        }
    }

    let mut hunks = Vec::new();
    let mut total_emitted = 0usize;
    let mut line_delta: isize = 0;

    for byte_start in match_starts {
        if total_emitted >= EDIT_DIFF_MAX_LINES {
            break;
        }
        let byte_end = byte_start + old_string.len();
        let start_line_idx = content[..byte_start]
            .bytes()
            .filter(|&b| b == b'\n')
            .count();
        let end_line_idx = content[..byte_end].bytes().filter(|&b| b == b'\n').count();
        let affected_old_end = end_line_idx + 1;
        let ctx_before_start = start_line_idx.saturating_sub(EDIT_DIFF_CONTEXT_LINES);
        let ctx_after_end = (affected_old_end + EDIT_DIFF_CONTEXT_LINES).min(old_lines.len());

        let old_count = ctx_after_end - ctx_before_start;
        let new_span_count = new_span_lines.len();
        let old_span_count = affected_old_end - start_line_idx;
        let new_count = old_count + new_span_count - old_span_count;
        let new_start_line = (ctx_before_start as isize + line_delta + 1).max(1) as usize;
        line_delta += new_span_count as isize - old_span_count as isize;

        let mut hunk_lines = Vec::new();
        for &line in &old_lines[ctx_before_start..start_line_idx] {
            hunk_lines.push(DiffLine::Context {
                text: line.to_string(),
            });
        }
        // Build exact pre/post text of the spanned lines so partial-line edits
        // render cleanly as full-line removals/additions.
        let prefix = line_prefix_before_offset(content, byte_start);
        let suffix = line_suffix_after_offset(content, byte_end);
        for (idx, line) in old_span_lines.iter().enumerate() {
            let mut full = String::new();
            if idx == 0 {
                full.push_str(prefix);
            }
            full.push_str(line);
            if idx + 1 == old_span_lines.len() {
                full.push_str(suffix);
            }
            hunk_lines.push(DiffLine::Removed { text: full });
        }
        for (idx, line) in new_span_lines.iter().enumerate() {
            let mut full = String::new();
            if idx == 0 {
                full.push_str(prefix);
            }
            full.push_str(line);
            if idx + 1 == new_span_lines.len() {
                full.push_str(suffix);
            }
            hunk_lines.push(DiffLine::Added { text: full });
        }
        for &line in &old_lines[affected_old_end..ctx_after_end] {
            hunk_lines.push(DiffLine::Context {
                text: line.to_string(),
            });
        }
        let remaining = EDIT_DIFF_MAX_LINES.saturating_sub(total_emitted);
        if hunk_lines.len() > remaining {
            hunk_lines.truncate(remaining);
        }
        total_emitted += hunk_lines.len();
        hunks.push(DiffHunk {
            header: Some(format!(
                "@@ -{},{} +{},{} @@",
                ctx_before_start + 1,
                old_count,
                new_start_line,
                new_count
            )),
            lines: hunk_lines,
        });
    }
    hunks
}

fn line_prefix_before_offset(content: &str, offset: usize) -> &str {
    let start = content[..offset].rfind('\n').map_or(0, |idx| idx + 1);
    &content[start..offset]
}

fn line_suffix_after_offset(content: &str, offset: usize) -> &str {
    let end = content[offset..]
        .find('\n')
        .map_or(content.len(), |rel| offset + rel);
    &content[offset..end]
}

#[cfg(test)]
#[path = "file_tools_tests.rs"]
mod tests;
