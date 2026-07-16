use alan_agent_engine::tools::{Tool, ToolContext, ToolResult};
use anyhow::anyhow;
use serde_json::{Value, json};
use std::path::Path;

/// read_file - Read a file's contents
#[derive(Default)]
pub struct ReadFileTool;

impl ReadFileTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a file's contents. For images, returns metadata."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Alan OS path to the file, or a path relative to Process cwd"
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
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = match ctx.resolve_path(args["path"].as_str().unwrap_or("")) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();
        let offset = args["offset"].as_u64().unwrap_or(1) as usize;
        let limit = args["limit"].as_u64().unwrap_or(1000) as usize;

        Box::pin(async move {
            if is_image(&path) {
                let content = sandbox.read(&path).await?;
                return Ok(json!({
                    "type": "image",
                    "path": visible_path,
                    "size_bytes": content.len(),
                    "mime_type": detect_mime(&path)
                }));
            }

            let content = sandbox.read_string(&path).await?;
            let lines: Vec<&str> = content.lines().collect();

            let start = offset.saturating_sub(1);
            let end = (start + limit).min(lines.len());

            let selected: Vec<&str> = if start < lines.len() {
                lines[start..end].to_vec()
            } else {
                Vec::new()
            };

            Ok(json!({
                "type": "text",
                "path": visible_path,
                "content": selected.join("\n"),
                "total_lines": lines.len(),
                "start_line": start + 1,
                "end_line": end,
                "truncated": lines.len() > limit
            }))
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
        "Write content to a file. Creates parent directories if needed."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Alan OS path to the file, or a path relative to Process cwd"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = match ctx.resolve_path(args["path"].as_str().unwrap_or("")) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();
        let content = args["content"].as_str().unwrap_or("").to_string();

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
        "Edit a file by replacing search text with replacement text."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["path", "old_string", "new_string"],
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "old_string": {
                    "type": "string",
                    "description": "Text to search for (exact match)"
                },
                "new_string": {
                    "type": "string",
                    "description": "Text to replace with"
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = match ctx.resolve_path(args["path"].as_str().unwrap_or("")) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();
        let old_string = args["old_string"].as_str().unwrap_or("").to_string();
        let new_string = args["new_string"].as_str().unwrap_or("").to_string();

        Box::pin(async move {
            let content = sandbox.read_string(&path).await?;

            if !content.contains(&old_string) {
                return Err(anyhow!(
                    "Search text not found in file: '{}...'",
                    &old_string[..old_string.len().min(50)]
                ));
            }

            let new_content = content.replacen(&old_string, &new_string, 1);
            sandbox.write(&path, new_content.as_bytes()).await?;

            Ok(json!({
                "success": true,
                "path": visible_path,
                "replacements": 1
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Write
    }
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

pub(super) fn detect_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
#[path = "file_tools_tests.rs"]
mod tests;
