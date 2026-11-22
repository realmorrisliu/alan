//! Lightweight `.gitignore`, file-glob, and file-type matching helpers for
//! `grep` and `glob` without external ignore-walker dependencies.

use alan_agent_engine::tools::Sandbox;
use anyhow::{Result, anyhow};
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct GitignoreRule {
    negated: bool,
    dir_only: bool,
    anchored: bool,
    regex: Regex,
}

#[derive(Debug, Clone)]
struct GitignoreScope {
    base: PathBuf,
    rules: Vec<GitignoreRule>,
}

/// Scoped stack of `.gitignore` rules accumulated from repo ancestors down to
/// the directory currently being traversed.
#[derive(Debug, Clone, Default)]
pub(crate) struct GitignoreMatcher {
    scopes: Vec<GitignoreScope>,
    respect_gitignore: bool,
}

impl GitignoreMatcher {
    pub(crate) fn empty(respect_gitignore: bool) -> Self {
        Self {
            scopes: Vec::new(),
            respect_gitignore,
        }
    }

    pub(crate) fn push_rules(&mut self, base: impl Into<PathBuf>, content: &str) {
        if !self.respect_gitignore {
            return;
        }
        let rules = parse_gitignore_rules(content);
        if !rules.is_empty() {
            self.scopes.push(GitignoreScope {
                base: base.into(),
                rules,
            });
        }
    }

    pub(crate) fn with_rules_for_dir(&self, dir: &Path, content: &str) -> Self {
        if !self.respect_gitignore {
            return self.clone();
        }
        let rules = parse_gitignore_rules(content);
        if rules.is_empty() {
            return self.clone();
        }
        let mut next = self.clone();
        next.scopes.push(GitignoreScope {
            base: dir.to_path_buf(),
            rules,
        });
        next
    }

    pub(super) fn new(root: &Path, sandbox: &Sandbox, respect_gitignore: bool) -> Self {
        if !respect_gitignore {
            return Self::empty(false);
        }

        let mut ancestors = Vec::new();
        let mut cursor = Some(if root.is_dir() {
            root
        } else {
            root.parent().unwrap_or(root)
        });
        while let Some(dir) = cursor {
            if !sandbox.is_readable(dir) && dir != root {
                break;
            }
            ancestors.push(dir.to_path_buf());
            if dir.join(".git").exists() {
                break;
            }
            cursor = dir.parent();
        }
        ancestors.reverse();

        let mut matcher = Self::empty(true);
        for dir in ancestors {
            if dir.join(".git").is_dir() {
                let exclude_path = dir.join(".git/info/exclude");
                if sandbox.is_readable(&exclude_path)
                    && let Ok(content) = std::fs::read_to_string(&exclude_path)
                {
                    matcher.push_rules(dir.clone(), &content);
                }
            }
            let gitignore_path = dir.join(".gitignore");
            if gitignore_path.is_file()
                && sandbox.is_readable(&gitignore_path)
                && let Ok(content) = std::fs::read_to_string(&gitignore_path)
            {
                matcher.push_rules(dir, &content);
            }
        }

        matcher
    }

    pub(super) fn child_for_dir(&self, dir: &Path, root: &Path, sandbox: &Sandbox) -> Self {
        if !self.respect_gitignore || dir == root {
            return self.clone();
        }
        let gitignore_path = dir.join(".gitignore");
        if gitignore_path.is_file()
            && sandbox.is_readable(&gitignore_path)
            && let Ok(content) = std::fs::read_to_string(&gitignore_path)
        {
            return self.with_rules_for_dir(dir, &content);
        }
        self.clone()
    }

    pub(crate) fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        if !self.respect_gitignore {
            return false;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if is_dir && name == ".git" {
            return true;
        }

        let mut decision: Option<bool> = None;
        for scope in &self.scopes {
            let Ok(rel) = path.strip_prefix(&scope.base) else {
                continue;
            };
            let rel_slash = normalize_slashes(rel);
            for rule in &scope.rules {
                if rule.dir_only && !is_dir {
                    continue;
                }
                let target = if rule.anchored {
                    rel_slash.as_str()
                } else {
                    name
                };
                if rule.regex.is_match(target) {
                    decision = Some(!rule.negated);
                }
            }
        }

        if let Some(ignored) = decision {
            return ignored;
        }
        is_dir && matches!(name, "target" | "node_modules")
    }
}

pub(super) fn normalize_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn parse_gitignore_rules(content: &str) -> Vec<GitignoreRule> {
    content
        .lines()
        .filter_map(parse_single_gitignore_rule)
        .collect()
}

fn parse_single_gitignore_rule(raw_line: &str) -> Option<GitignoreRule> {
    let line = trim_unescaped_trailing_spaces(raw_line);
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let (negated, rest) = if let Some(stripped) = line.strip_prefix('!') {
        (true, stripped)
    } else if let Some(escaped) = line.strip_prefix("\\!") {
        (false, escaped)
    } else if let Some(escaped) = line.strip_prefix("\\#") {
        (false, escaped)
    } else {
        (false, line)
    };

    if rest.is_empty() {
        return None;
    }

    let (dir_only, rest) = if let Some(stripped) = rest.strip_suffix('/') {
        (true, stripped)
    } else {
        (false, rest)
    };
    if rest.is_empty() {
        return None;
    }

    let anchored = rest.starts_with('/') || rest.contains('/');
    let pattern = rest.strip_prefix('/').unwrap_or(rest);
    let regex = compile_glob_regex(pattern).ok()?;
    Some(GitignoreRule {
        negated,
        dir_only,
        anchored,
        regex,
    })
}

fn trim_unescaped_trailing_spaces(mut s: &str) -> &str {
    s = s.trim_end_matches('\r');
    while s.ends_with(' ') && !s.ends_with("\\ ") {
        s = &s[..s.len() - 1];
    }
    s
}

/// Compile a glob pattern (supporting `*`, `**`, `?`, `[abc]`, `{a,b}`) into a
/// full-string anchored `Regex`.
pub(super) fn compile_glob_regex(pattern: &str) -> Result<Regex> {
    let variants = expand_braces(pattern);
    let mut parts = Vec::with_capacity(variants.len());
    for variant in variants {
        parts.push(glob_body_to_regex(&variant)?);
    }
    let combined = if parts.len() == 1 {
        format!("^(?:{})$", parts[0])
    } else {
        format!("^(?:{})$", parts.join("|"))
    };
    Regex::new(&combined).map_err(|err| anyhow!("Invalid glob pattern '{pattern}': {err}"))
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let mut open = None;
    let mut depth = 0usize;
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b'\\' {
            continue;
        }
        if b == b'{' {
            if depth == 0 {
                open = Some(idx);
            }
            depth += 1;
        } else if b == b'}' && depth > 0 {
            depth -= 1;
            if depth == 0
                && let Some(start) = open
            {
                let prefix = &pattern[..start];
                let inner = &pattern[start + 1..idx];
                let suffix = &pattern[idx + 1..];
                let alts = split_top_level_commas(inner);
                if alts.len() > 1 {
                    let mut out = Vec::new();
                    for alt in alts {
                        for expanded in expand_braces(&format!("{prefix}{alt}{suffix}")) {
                            out.push(expanded);
                        }
                    }
                    return out;
                }
            }
        }
    }
    vec![pattern.to_string()]
}

fn split_top_level_commas(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&inner[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    parts
}

fn glob_body_to_regex(pattern: &str) -> Result<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() => {
                out.push_str(&regex::escape(&chars[i + 1].to_string()));
                i += 2;
            }
            '*' => {
                if chars.get(i + 1).copied() == Some('*') {
                    if chars.get(i + 2).copied() == Some('/') {
                        out.push_str("(?:.+/)?");
                        i += 3;
                    } else {
                        out.push_str(".*");
                        i += 2;
                    }
                } else {
                    out.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                out.push_str("[^/]");
                i += 1;
            }
            '[' => {
                if let Some(close) = chars[i + 1..].iter().position(|&c| c == ']') {
                    let inner: String = chars[i + 1..i + 1 + close].iter().collect();
                    let body = inner
                        .strip_prefix('!')
                        .map(|rest| format!("^{rest}"))
                        .unwrap_or(inner);
                    out.push('[');
                    out.push_str(&body);
                    out.push(']');
                    i += close + 2;
                } else {
                    out.push_str("\\[");
                    i += 1;
                }
            }
            ch => {
                out.push_str(&regex::escape(&ch.to_string()));
                i += 1;
            }
        }
    }
    Ok(out)
}

pub(super) fn file_type_matches(file_type: &str, path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let ext_lower = ext.to_ascii_lowercase();
    let needle = file_type
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    match needle.as_str() {
        "rust" | "rs" => ext_lower == "rs",
        "ts" | "typescript" => matches!(ext_lower.as_str(), "ts" | "tsx" | "mts" | "cts"),
        "js" | "javascript" => matches!(ext_lower.as_str(), "js" | "jsx" | "mjs" | "cjs"),
        "py" | "python" => matches!(ext_lower.as_str(), "py" | "pyi"),
        "go" => ext_lower == "go",
        "json" => matches!(ext_lower.as_str(), "json" | "jsonc"),
        "toml" => ext_lower == "toml",
        "yaml" | "yml" => matches!(ext_lower.as_str(), "yaml" | "yml"),
        "md" | "markdown" => matches!(ext_lower.as_str(), "md" | "mdx"),
        "sh" | "shell" | "bash" => matches!(ext_lower.as_str(), "sh" | "bash" | "zsh"),
        "html" => matches!(ext_lower.as_str(), "html" | "htm"),
        "css" => matches!(ext_lower.as_str(), "css" | "scss" | "sass"),
        "c" => matches!(ext_lower.as_str(), "c" | "h"),
        "cpp" | "c++" => matches!(
            ext_lower.as_str(),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx"
        ),
        _ => ext_lower == needle,
    }
}
