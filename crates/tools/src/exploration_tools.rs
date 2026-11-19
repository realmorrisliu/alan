use super::exploration_gitignore::{
    GitignoreMatcher, compile_glob_regex, file_type_matches, normalize_slashes,
};
use alan_agent_engine::tools::{Sandbox, Tool, ToolContext, ToolExecutionAdapter, ToolResult};
use anyhow::{Result, anyhow};
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use std::fs::FileType;
use std::time::SystemTime;
use std::{path::Path, sync::Arc};

const DEFAULT_GREP_LIMIT: usize = 200;
const MAX_GREP_LIMIT: usize = 2000;
const DEFAULT_GLOB_LIMIT: usize = 500;
const MAX_GLOB_LIMIT: usize = 5000;

struct GrepOptions {
    case_sensitive: bool,
    glob_filter: Option<(Regex, bool)>,
    file_type: Option<String>,
    before_context: usize,
    after_context: usize,
    files_only: bool,
    limit: usize,
    respect_gitignore: bool,
}

/// grep - Search file contents
#[derive(Default)]
pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for GrepTool {
    fn presentation_is_read_only(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for patterns in files using regex, respecting .gitignore by default."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in (default: Process cwd)",
                    "default": "."
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Case sensitive search",
                    "default": false
                },
                "glob": {
                    "type": "string",
                    "description": "Filter files by glob pattern (e.g. '*.rs', 'src/**/*.ts')"
                },
                "file_type": {
                    "type": "string",
                    "description": "Filter files by language/type (e.g. 'rust', 'ts', 'py', 'json')"
                },
                "before_context": {
                    "type": "integer",
                    "description": "Number of context lines before each match (-B)",
                    "minimum": 0,
                    "default": 0
                },
                "after_context": {
                    "type": "integer",
                    "description": "Number of context lines after each match (-A)",
                    "minimum": 0,
                    "default": 0
                },
                "context": {
                    "type": "integer",
                    "description": "Number of context lines before and after each match (-C)",
                    "minimum": 0
                },
                "files_only": {
                    "type": "boolean",
                    "description": "Return matching file paths only instead of line matches",
                    "default": false
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of matches or files to return",
                    "minimum": 1,
                    "maximum": MAX_GREP_LIMIT,
                    "default": DEFAULT_GREP_LIMIT
                },
                "respect_gitignore": {
                    "type": "boolean",
                    "description": "Skip .gitignore, .git/, target/, and node_modules/ paths",
                    "default": true
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let path_arg = args["path"].as_str().unwrap_or(".");
        let path = match ctx.resolve_path(path_arg) {
            Ok(path) => path,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let adapter = match ctx.execution_adapter() {
            Ok(adapter) => adapter,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let pattern = args["pattern"].as_str().unwrap_or("").to_string();
        let glob_raw = args["glob"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let file_type = args
            .get("file_type")
            .or_else(|| args.get("type"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let shared_ctx = args
            .get("context")
            .or_else(|| args.get("C"))
            .or_else(|| args.get("-C"))
            .and_then(Value::as_u64);
        let before_context = shared_ctx
            .or_else(|| {
                args.get("before_context")
                    .or_else(|| args.get("B"))
                    .or_else(|| args.get("-B"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0) as usize;
        let after_context = shared_ctx
            .or_else(|| {
                args.get("after_context")
                    .or_else(|| args.get("A"))
                    .or_else(|| args.get("-A"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0) as usize;
        let limit = (args["limit"].as_u64().unwrap_or(DEFAULT_GREP_LIMIT as u64) as usize)
            .clamp(1, MAX_GREP_LIMIT);
        let case_sensitive = args["case_sensitive"].as_bool().unwrap_or(false);
        let files_only = args["files_only"].as_bool().unwrap_or(false);
        let respect_gitignore = args["respect_gitignore"].as_bool().unwrap_or(true);

        Box::pin(async move {
            let regex = RegexBuilder::new(&pattern)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| anyhow!("Invalid regex pattern: {}", e))?;
            let glob_filter = match glob_raw {
                Some(raw) => {
                    let match_full_rel = raw.contains('/');
                    Some((compile_glob_regex(&raw)?, match_full_rel))
                }
                None => None,
            };
            let opts = GrepOptions {
                case_sensitive,
                glob_filter,
                file_type,
                before_context,
                after_context,
                files_only,
                limit,
                respect_gitignore,
            };
            let _ = opts.case_sensitive;

            let mut matches = Vec::new();
            let mut truncated = false;

            if path.is_file() {
                if grep_filters_match(&path, &path, &opts) {
                    let content = sandbox.read_string(&path).await?;
                    let visible = adapter.visible_path(&path);
                    search_file_content(
                        &content,
                        &visible.to_string_lossy(),
                        &regex,
                        &opts,
                        &mut matches,
                        &mut truncated,
                    );
                }
            } else if path.is_dir() {
                let ignore = GitignoreMatcher::new(&path, &sandbox, opts.respect_gitignore);
                search_directory(
                    &sandbox,
                    &path,
                    &path,
                    &regex,
                    &adapter,
                    &opts,
                    &ignore,
                    &mut matches,
                    &mut truncated,
                )
                .await?;
            }

            Ok(json!({
                "matches": matches,
                "total": matches.len(),
                "truncated": truncated
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

fn grep_filters_match(root: &Path, path: &Path, opts: &GrepOptions) -> bool {
    if let Some(ref ft) = opts.file_type
        && !file_type_matches(ft, path)
    {
        return false;
    }
    if let Some((ref glob_re, match_full_rel)) = opts.glob_filter {
        let candidate = if match_full_rel {
            let rel = path.strip_prefix(root).unwrap_or(path);
            normalize_slashes(rel)
        } else {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        };
        if !glob_re.is_match(&candidate) {
            return false;
        }
    }
    true
}

fn search_file_content(
    content: &str,
    visible_path: &str,
    regex: &Regex,
    opts: &GrepOptions,
    matches: &mut Vec<Value>,
    truncated: &mut bool,
) {
    let lines: Vec<&str> = content.lines().collect();
    for (line_num, line) in lines.iter().enumerate() {
        if !regex.is_match(line) {
            continue;
        }
        if matches.len() >= opts.limit {
            *truncated = true;
            return;
        }
        if opts.files_only {
            matches.push(Value::String(visible_path.to_string()));
            return;
        }
        let mut entry = json!({
            "path": visible_path,
            "line": line_num + 1,
            "content": line
        });
        if opts.before_context > 0 {
            let start = line_num.saturating_sub(opts.before_context);
            entry["before"] = json!(lines[start..line_num]);
        }
        if opts.after_context > 0 {
            let end = (line_num + 1 + opts.after_context).min(lines.len());
            entry["after"] = json!(lines[line_num + 1..end]);
        }
        matches.push(entry);
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Directory search helper passes search state directly without allocating an intermediate context struct"
)]
async fn search_directory(
    sandbox: &Sandbox,
    root: &Path,
    dir: &Path,
    regex: &Regex,
    adapter: &Arc<dyn ToolExecutionAdapter>,
    opts: &GrepOptions,
    parent_ignore: &GitignoreMatcher,
    matches: &mut Vec<Value>,
    truncated: &mut bool,
) -> Result<()> {
    if matches.len() >= opts.limit {
        *truncated = true;
        return Ok(());
    }
    let ignore = parent_ignore.child_for_dir(dir, root, sandbox);
    let mut entries = sandbox.list_dir(dir).await?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if matches.len() >= opts.limit {
            *truncated = true;
            return Ok(());
        }
        let path = entry.path();
        let file_type: FileType = entry.file_type().await?;
        let is_dir = file_type.is_dir();

        if is_dir
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if ignore.is_ignored(&path, is_dir) {
            continue;
        }

        if is_dir {
            Box::pin(search_directory(
                sandbox, root, &path, regex, adapter, opts, &ignore, matches, truncated,
            ))
            .await?;
        } else if file_type.is_file() {
            if is_binary_file(&path) || !grep_filters_match(root, &path, opts) {
                continue;
            }
            if let Ok(content) = sandbox.read_string(&path).await {
                let visible = adapter.visible_path(&path);
                search_file_content(
                    &content,
                    &visible.to_string_lossy(),
                    regex,
                    opts,
                    matches,
                    truncated,
                );
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
    fn presentation_is_read_only(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern, respecting .gitignore and sorted by mtime descending."
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
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of matched files to return",
                    "minimum": 1,
                    "maximum": MAX_GLOB_LIMIT,
                    "default": DEFAULT_GLOB_LIMIT
                },
                "respect_gitignore": {
                    "type": "boolean",
                    "description": "Skip .gitignore, .git/, target/, and node_modules/ paths",
                    "default": true
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
            match ctx.cwd() {
                Ok(cwd) => cwd,
                Err(err) => return Box::pin(async move { Err(err) }),
            }
        };
        let pattern = args["pattern"].as_str().unwrap_or("").to_string();
        let limit = (args["limit"].as_u64().unwrap_or(DEFAULT_GLOB_LIMIT as u64) as usize)
            .clamp(1, MAX_GLOB_LIMIT);
        let respect_gitignore = args["respect_gitignore"].as_bool().unwrap_or(true);
        let adapter = match ctx.execution_adapter() {
            Ok(adapter) => adapter,
            Err(err) => return Box::pin(async move { Err(err) }),
        };

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

            let glob_re = compile_glob_regex(&pattern)?;
            let include_hidden = pattern.starts_with('.') || pattern.contains("/.");
            let ignore = GitignoreMatcher::new(&base_path, &sandbox, respect_gitignore);
            let mut found = Vec::new();

            if base_path.is_dir() {
                collect_glob_matches(
                    &sandbox,
                    &base_path,
                    &base_path,
                    &glob_re,
                    include_hidden,
                    &ignore,
                    &adapter,
                    &mut found,
                )
                .await?;
            }

            // Sort newest modification time first, breaking ties deterministically.
            found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            let truncated = found.len() > limit;
            found.truncate(limit);
            let matches: Vec<String> = found.into_iter().map(|(_, path)| path).collect();

            Ok(json!({
                "matches": matches,
                "total": matches.len(),
                "truncated": truncated
            }))
        })
    }

    fn capability(&self, _args: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Recursive glob collector avoids extra state wrapper allocation"
)]
async fn collect_glob_matches(
    sandbox: &Sandbox,
    base_path: &Path,
    dir: &Path,
    glob_re: &Regex,
    include_hidden: bool,
    parent_ignore: &GitignoreMatcher,
    adapter: &Arc<dyn ToolExecutionAdapter>,
    found: &mut Vec<(SystemTime, String)>,
) -> Result<()> {
    let ignore = parent_ignore.child_for_dir(dir, base_path, sandbox);
    let mut entries = sandbox.list_dir(dir).await?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let file_type: FileType = entry.file_type().await?;
        let is_dir = file_type.is_dir();

        if is_dir
            && !include_hidden
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if ignore.is_ignored(&path, is_dir) {
            continue;
        }

        if is_dir {
            Box::pin(collect_glob_matches(
                sandbox,
                base_path,
                &path,
                glob_re,
                include_hidden,
                &ignore,
                adapter,
                found,
            ))
            .await?;
        } else if file_type.is_file() && sandbox.is_readable(&path) {
            let rel = path.strip_prefix(base_path).unwrap_or(&path);
            let rel_slash = normalize_slashes(rel);
            if glob_re.is_match(&rel_slash) {
                let mtime = entry
                    .metadata()
                    .await
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                found.push((
                    mtime,
                    adapter.visible_path(&path).to_string_lossy().into_owned(),
                ));
            }
        }
    }

    Ok(())
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
    fn presentation_is_read_only(&self) -> bool {
        true
    }

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
            match ctx.cwd() {
                Ok(cwd) => cwd,
                Err(err) => return Box::pin(async move { Err(err) }),
            }
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
