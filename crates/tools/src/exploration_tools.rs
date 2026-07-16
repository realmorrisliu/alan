use alan_agent_engine::tools::{Sandbox, Tool, ToolContext, ToolResult};
use anyhow::{Result, anyhow};
use regex::RegexBuilder;
use serde_json::{Value, json};
use std::fs::FileType;
use std::path::Path;

/// grep - Search file contents
#[derive(Default)]
pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for patterns in files using regex."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern", "path"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in"
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Case sensitive search",
                    "default": false
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
        let host_mounts = ctx.host_mounts.clone();
        let pattern = args["pattern"].as_str().unwrap_or("").to_string();
        let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);

        Box::pin(async move {
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| anyhow!("Invalid regex pattern: {}", e))?;

            let mut matches = Vec::new();

            if path.is_file() {
                let content = sandbox.read_string(&path).await?;
                for (line_num, line) in content.lines().enumerate() {
                    if regex.is_match(line) {
                        matches.push(json!({
                            "path": visible_host_path(&path, &host_mounts),
                            "line": line_num + 1,
                            "content": line
                        }));
                    }
                }
            } else if path.is_dir() {
                search_directory(&sandbox, &path, &regex, &host_mounts, &mut matches).await?;
            }

            Ok(json!({
                "matches": matches,
                "total": matches.len()
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

async fn search_directory(
    sandbox: &Sandbox,
    dir: &Path,
    regex: &regex::Regex,
    host_mounts: &[alan_agent_engine::HostMountGrant],
    matches: &mut Vec<Value>,
) -> Result<()> {
    let entries = sandbox.list_dir(dir).await?;

    for entry in entries {
        let path = entry.path();
        let file_type: FileType = entry.file_type().await?;

        if file_type.is_dir() {
            if let Some(name) = path.file_name()
                && name.to_string_lossy().starts_with('.')
            {
                continue;
            }
            Box::pin(search_directory(
                sandbox,
                &path,
                regex,
                host_mounts,
                matches,
            ))
            .await?;
        } else if file_type.is_file() {
            if is_binary_file(&path) {
                continue;
            }

            if let Ok(content) = sandbox.read_string(&path).await {
                for (line_num, line) in content.lines().enumerate() {
                    if regex.is_match(line) {
                        matches.push(json!({
                            "path": visible_host_path(&path, host_mounts),
                            "line": line_num + 1,
                            "content": line
                        }));
                    }
                }
            }
        }
    }

    Ok(())
}

/// glob - Find files matching patterns
#[derive(Default)]
pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., '**/*.rs', 'src/*.txt')"
                },
                "path": {
                    "type": "string",
                    "description": "Base directory (default: Process cwd)",
                    "default": "."
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let base_path = if let Some(path) = args["path"].as_str() {
            match ctx.resolve_path(path) {
                Ok(path) => path,
                Err(err) => return Box::pin(async move { Err(err) }),
            }
        } else {
            ctx.cwd.clone()
        };
        let pattern = args["pattern"].as_str().unwrap_or("").to_string();
        let host_mounts = ctx.host_mounts.clone();

        Box::pin(async move {
            if !sandbox.is_readable(&base_path) {
                return Err(anyhow!(
                    "Path outside the Process file view: {}",
                    base_path.to_string_lossy()
                ));
            }

            if Path::new(&pattern).is_absolute() {
                return Err(anyhow!("Glob pattern must be relative to base path"));
            }

            let pattern_str = base_path.join(&pattern);
            let pattern_str = pattern_str.to_string_lossy();

            let mut matches = Vec::new();

            for path in glob::glob(&pattern_str)?.flatten() {
                if path.is_file() && sandbox.is_readable(&path) {
                    matches.push(visible_host_path(&path, &host_mounts));
                }
            }

            Ok(json!({
                "matches": matches,
                "total": matches.len()
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

/// list_dir - List directory contents
#[derive(Default)]
pub struct ListDirTool;

impl ListDirTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List contents of a directory."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path (default: current directory)",
                    "default": "."
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path = if let Some(p) = args["path"].as_str() {
            match ctx.resolve_path(p) {
                Ok(path) => path,
                Err(err) => return Box::pin(async move { Err(err) }),
            }
        } else {
            ctx.cwd.clone()
        };
        let visible_path = ctx.visible_path(&path).to_string_lossy().to_string();

        Box::pin(async move {
            let entries = sandbox.list_dir(&path).await?;
            let mut items = Vec::new();

            for entry in entries {
                let file_type = entry.file_type().await?;
                let metadata = entry.metadata().await?;
                let name = entry.file_name().to_string_lossy().to_string();

                items.push(json!({
                    "name": name,
                    "type": if file_type.is_dir() { "directory" } else { "file" },
                    "size": metadata.len()
                }));
            }

            items.sort_by(|a, b| {
                let a_is_dir = a["type"] == "directory";
                let b_is_dir = b["type"] == "directory";
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a["name"].as_str().cmp(&b["name"].as_str()),
                }
            });

            Ok(json!({
                "path": visible_path,
                "entries": items,
                "total": items.len()
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

fn visible_host_path(path: &Path, mounts: &[alan_agent_engine::HostMountGrant]) -> String {
    if mounts.is_empty() {
        return path.to_string_lossy().to_string();
    }
    mounts
        .iter()
        .filter_map(|grant| {
            let requested =
                dunce::canonicalize(path).unwrap_or_else(|_| dunce::simplified(path).to_path_buf());
            let root = dunce::canonicalize(&grant.host_path)
                .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf());
            let suffix = requested.strip_prefix(&root).ok()?;
            Some((
                root.components().count(),
                Path::new(&grant.namespace_path).join(suffix),
            ))
        })
        .max_by_key(|(prefix_len, _)| *prefix_len)
        .map(|(_, path)| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "<unmapped-host-path>".to_string())
}

pub(super) fn is_binary_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(
            ext.as_str(),
            "exe"
                | "dll"
                | "so"
                | "dylib"
                | "bin"
                | "o"
                | "a"
                | "zip"
                | "tar"
                | "gz"
                | "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "mp3"
                | "mp4"
                | "pdf"
        )
    } else {
        false
    }
}

#[cfg(test)]
#[path = "exploration_tools_tests.rs"]
mod tests;
