//! Skills injector for adding skill content to prompts.

use crate::skills::types::*;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

const MAX_DISCLOSED_RESOURCE_COUNT: usize = 8;
const MAX_DISCLOSED_RESOURCE_CHARS: usize = 4000;
const MAX_DISCLOSED_RESOURCE_BYTES: u64 = 16 * 1024;
const MAX_DISCLOSED_LEVEL2_BYTES: u64 = 64 * 1024;
const DELEGATED_INLINE_FALLBACK_NOTE: &str = "Delegated runtime execution is not available in this runtime yet, so alan is falling back to inline skill instructions for this turn.";

#[derive(Debug, Clone)]
pub struct RenderedActiveSkillPrompt {
    pub rendered: String,
    pub tracked_paths: Vec<PromptTrackedPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PromptTrackedPath {
    pub path: PathBuf,
    pub fingerprint: PromptTrackedPathFingerprint,
}

impl PromptTrackedPath {
    fn prefix_bytes(path: PathBuf, max_bytes: u64) -> Self {
        Self {
            path,
            fingerprint: PromptTrackedPathFingerprint::PrefixBytes(max_bytes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PromptTrackedPathFingerprint {
    PrefixBytes(u64),
}

impl PromptTrackedPathFingerprint {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::PrefixBytes(lhs), Self::PrefixBytes(rhs)) => Self::PrefixBytes(lhs.max(rhs)),
        }
    }
}

#[derive(Debug, Clone)]
struct DisclosedSkillPrompt {
    level2: DisclosedLevel2Content,
    resources: Vec<DisclosedSkillResource>,
}

#[derive(Debug, Clone)]
struct DisclosedLevel2Content {
    source_display: String,
    body: String,
    tracked_paths: Vec<PromptTrackedPath>,
}

#[derive(Debug, Clone)]
struct DisclosedSkillResource {
    kind: SkillResourceKind,
    display_path: String,
    tracked_path: PromptTrackedPath,
    content: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingDisclosedSkillResource {
    kind: SkillResourceKind,
    display_path: String,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SkillResourceKind {
    Reference,
    Script,
    Asset,
}

impl SkillResourceKind {
    fn label(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Script => "script",
            Self::Asset => "asset",
        }
    }

    fn default_dir(self) -> &'static str {
        match self {
            Self::Reference => "references",
            Self::Script => "scripts",
            Self::Asset => "assets",
        }
    }
}

/// Extract canonical `$skill-id` mentions from user input.
pub fn extract_mentions(input: &str) -> Vec<SkillId> {
    let mut mentions = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            i += 1;
            continue;
        }

        let mut j = i + 1;
        while j < chars.len() {
            let c = chars[j];
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                j += 1;
            } else {
                break;
            }
        }

        if j > i + 1 {
            let raw: String = chars[i + 1..j].iter().collect();
            let trimmed = raw.trim_end_matches('.');
            if is_canonical_skill_id(trimmed) && seen.insert(trimmed.to_string()) {
                mentions.push(trimmed.to_string());
            }
        }

        i = j;
    }

    mentions
}

/// Inject skill content into a prompt.
pub fn inject_skills(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut sections = Vec::new();

    for skill in skills {
        let envelope = ActiveSkillEnvelope::available(
            skill.metadata.clone(),
            SkillActivationReason::ExplicitMention {
                mention: skill.metadata.id.clone(),
            },
        );
        sections.push(inject_active_skill(skill, &envelope));
    }

    sections.join("\n\n")
}

/// Inject one active skill using the structured runtime envelope.
pub fn inject_active_skill(skill: &Skill, envelope: &ActiveSkillEnvelope) -> String {
    render_active_skill_prompt(skill, envelope).rendered
}

/// Render one active skill prompt together with the exact files it depends on.
pub fn render_active_skill_prompt(
    skill: &Skill,
    envelope: &ActiveSkillEnvelope,
) -> RenderedActiveSkillPrompt {
    // Without explicit runtime capability context, conservatively avoid assuming
    // delegated runtime execution is available.
    render_active_skill_prompt_for_runtime(skill, envelope, false)
}

pub(crate) fn render_active_skill_prompt_for_runtime(
    skill: &Skill,
    envelope: &ActiveSkillEnvelope,
    delegated_invocation_available: bool,
) -> RenderedActiveSkillPrompt {
    if let Some(target) = envelope.metadata.execution.delegate_target() {
        if !delegated_invocation_available {
            return render_inline_active_skill_prompt(
                skill,
                envelope,
                Some(DELEGATED_INLINE_FALLBACK_NOTE),
            );
        }
        return render_delegated_skill_prompt(skill, envelope, target);
    }
    if !envelope.metadata.execution.renders_inline_body() {
        return render_unresolved_skill_prompt(skill, envelope);
    }

    render_inline_active_skill_prompt(skill, envelope, None)
}

fn render_inline_active_skill_prompt(
    skill: &Skill,
    envelope: &ActiveSkillEnvelope,
    runtime_note: Option<&str>,
) -> RenderedActiveSkillPrompt {
    let runtime_context = format_active_skill_context(envelope);
    let disclosed = disclose_skill_prompt(skill, envelope);
    let resources = format_disclosed_resources(&disclosed.resources);
    let runtime_note = runtime_note
        .map(|note| format!("### Runtime Fallback\n{note}\n\n"))
        .unwrap_or_default();
    let rendered = format!(
        r#"## Skill: {}

{runtime_context}

{runtime_note}### Active Instructions
source: {}

{}

{resources}

---"#,
        skill.metadata.name,
        disclosed.level2.source_display,
        disclosed.level2.body,
        runtime_context = runtime_context,
        runtime_note = runtime_note,
        resources = resources
    );

    let mut tracked_paths = disclosed.level2.tracked_paths.clone();
    tracked_paths.extend(
        disclosed
            .resources
            .iter()
            .map(|resource| resource.tracked_path.clone()),
    );
    dedupe_tracked_paths(&mut tracked_paths);

    RenderedActiveSkillPrompt {
        rendered,
        tracked_paths,
    }
}

fn render_delegated_skill_prompt(
    skill: &Skill,
    envelope: &ActiveSkillEnvelope,
    target: &str,
) -> RenderedActiveSkillPrompt {
    let runtime_context = format_active_skill_context(envelope);
    let summary = skill
        .metadata
        .short_description
        .as_deref()
        .unwrap_or(&skill.metadata.description);
    let rendered = format!(
        r#"## Skill: {}

{runtime_context}

### Delegated Capability
summary: {summary}
delegated_target: {target}

This skill executes through alan's delegated runtime path.
Do not inline or restate the full `SKILL.md` body in this session.
When you need this capability, call `invoke_delegated_skill` with a concise bounded task for the delegated runtime.
If the delegated task targets a different local workspace than the current runtime, include an explicit `workspace_root` and, when helpful, a narrower nested `cwd`.
The tool returns a bounded result object with `status`, `summary`, optional `child_run`, optional inline `output_text`, optional `output_ref`, optional `structured_output`, and explicit `truncation` metadata.
If `output_ref` or truncation metadata is present, treat the inline text as a preview and inspect the referenced child rollout/session only when the full delegated output is needed.
Use `child_run` metadata to inspect or terminate a still-active child run through the available child-run controls.

```json
{{
  "skill_id": "{}",
  "target": "{target}",
  "task": "Describe the delegated task for the delegated runtime."
}}
```

After the tool completes, continue using only the returned tool result.

---"#,
        skill.metadata.name,
        skill.metadata.id,
        runtime_context = runtime_context,
    );

    RenderedActiveSkillPrompt {
        rendered,
        tracked_paths: Vec::new(),
    }
}

fn render_unresolved_skill_prompt(
    skill: &Skill,
    envelope: &ActiveSkillEnvelope,
) -> RenderedActiveSkillPrompt {
    let runtime_context = format_active_skill_context(envelope);
    let rendered = format!(
        r#"## Skill: {}

{runtime_context}

### Skill Execution Status
summary: {}
This skill did not resolve to an executable parent-runtime capability.
Do not inline the `SKILL.md` body. Treat this skill as unavailable until its package metadata is fixed.
{}

---"#,
        skill.metadata.name,
        skill
            .metadata
            .short_description
            .as_deref()
            .unwrap_or(&skill.metadata.description),
        format_unresolved_execution_details(&envelope.metadata.execution),
        runtime_context = runtime_context,
    );

    RenderedActiveSkillPrompt {
        rendered,
        tracked_paths: Vec::new(),
    }
}

fn format_active_skill_context(envelope: &ActiveSkillEnvelope) -> String {
    let builtin_package = envelope.metadata.is_builtin_package();
    let mut lines = vec![
        "### alan Runtime Context".to_string(),
        format!("skill_id: {}", envelope.metadata.id),
        format!(
            "package_id: {}",
            envelope.metadata.package_id.as_deref().unwrap_or("<none>")
        ),
        format!("enabled: {}", envelope.metadata.enabled),
        format!(
            "allow_implicit_invocation: {}",
            envelope.metadata.allow_implicit_invocation
        ),
        format!(
            "canonical_path: {}",
            render_prompt_visible_skill_path(&envelope.metadata)
        ),
        format!(
            "package_root: {}",
            render_prompt_visible_package_root(&envelope.metadata)
        ),
        format!(
            "resource_root: {}",
            render_prompt_visible_resource_root(&envelope.metadata)
        ),
        format!("availability: {}", envelope.availability.render_label()),
        format!(
            "activation_reason: {}",
            envelope.activation_reason.render_label()
        ),
        format!("execution: {}", envelope.metadata.execution.render_label()),
    ];

    if builtin_package {
        lines.push(
            "Builtin capability packages are already disclosed through this prompt context. Do not use tools to open builtin package files by path."
                .to_string(),
        );
    } else if envelope.metadata.resource_root().is_some() {
        lines.push(
            "Resolve relative skill resource references against `resource_root`.".to_string(),
        );
    }

    lines.join("\n")
}

fn disclose_skill_prompt(skill: &Skill, envelope: &ActiveSkillEnvelope) -> DisclosedSkillPrompt {
    let disclosure = skill_disclosure_config(skill);
    let base_dir = disclosure_base_dir(skill, envelope);
    let level2 = load_level2_content(skill, &disclosure, base_dir.as_deref());
    let resources = collect_disclosed_resources(&level2.body, &disclosure, base_dir.as_deref());

    DisclosedSkillPrompt { level2, resources }
}

fn skill_disclosure_config(skill: &Skill) -> DisclosureConfig {
    skill
        .metadata
        .capabilities
        .as_ref()
        .map(|capabilities| capabilities.disclosure.clone())
        .unwrap_or_else(|| skill.frontmatter.capabilities.disclosure.clone())
}

fn disclosure_base_dir(skill: &Skill, envelope: &ActiveSkillEnvelope) -> Option<PathBuf> {
    envelope
        .metadata
        .resource_root()
        .or_else(|| skill.metadata.path.parent())
        .map(Path::to_path_buf)
}

fn load_level2_content(
    skill: &Skill,
    disclosure: &DisclosureConfig,
    base_dir: Option<&Path>,
) -> DisclosedLevel2Content {
    let mut tracked_paths = Vec::new();

    let requested = disclosure.level2.trim();
    if requested.is_empty() || requested == "SKILL.md" {
        return fallback_level2_content(skill, tracked_paths);
    }

    let Some(base_dir) = base_dir else {
        return fallback_level2_content(skill, tracked_paths);
    };
    let Some((display_path, path)) = resolve_relative_path(base_dir, requested) else {
        return fallback_level2_content(skill, tracked_paths);
    };

    if path == skill.metadata.path {
        return DisclosedLevel2Content {
            source_display: display_path,
            body: skill.content.clone(),
            tracked_paths,
        };
    }

    tracked_paths.push(PromptTrackedPath::prefix_bytes(
        path.clone(),
        MAX_DISCLOSED_LEVEL2_BYTES,
    ));

    let Some(content) = load_disclosed_text_content(&path, MAX_DISCLOSED_LEVEL2_BYTES, None) else {
        return fallback_level2_content(skill, tracked_paths);
    };

    DisclosedLevel2Content {
        source_display: display_path,
        body: strip_frontmatter_if_present(content),
        tracked_paths,
    }
}

fn fallback_level2_content(
    skill: &Skill,
    tracked_paths: Vec<PromptTrackedPath>,
) -> DisclosedLevel2Content {
    DisclosedLevel2Content {
        source_display: "SKILL.md".to_string(),
        body: skill.content.clone(),
        tracked_paths,
    }
}

fn collect_disclosed_resources(
    level2_body: &str,
    disclosure: &DisclosureConfig,
    base_dir: Option<&Path>,
) -> Vec<DisclosedSkillResource> {
    let Some(base_dir) = base_dir else {
        return Vec::new();
    };

    let mut resources = BTreeMap::new();

    for entry in &disclosure.level3.references {
        add_declared_resource_if_referenced(
            &mut resources,
            level2_body,
            base_dir,
            SkillResourceKind::Reference,
            entry,
        );
    }
    for entry in &disclosure.level3.scripts {
        add_declared_resource_if_referenced(
            &mut resources,
            level2_body,
            base_dir,
            SkillResourceKind::Script,
            entry,
        );
    }
    for entry in &disclosure.level3.assets {
        add_declared_resource_if_referenced(
            &mut resources,
            level2_body,
            base_dir,
            SkillResourceKind::Asset,
            entry,
        );
    }

    for reference in extract_resource_references(level2_body) {
        add_prefixed_resource(&mut resources, base_dir, &reference);
    }

    materialize_disclosed_resources(resources.into_values(), load_resource_content)
}

fn add_declared_resource_if_referenced(
    resources: &mut BTreeMap<String, PendingDisclosedSkillResource>,
    level2_body: &str,
    base_dir: &Path,
    kind: SkillResourceKind,
    entry: &str,
) {
    let Some((display_path, path)) = resolve_resource_entry(base_dir, kind, entry) else {
        return;
    };
    if !declared_resource_is_referenced(level2_body, kind, entry, &display_path) {
        return;
    }
    resources
        .entry(display_path.clone())
        .or_insert_with(|| PendingDisclosedSkillResource {
            kind,
            display_path,
            path,
        });
}

fn add_prefixed_resource(
    resources: &mut BTreeMap<String, PendingDisclosedSkillResource>,
    base_dir: &Path,
    entry: &str,
) {
    let Some((kind, display_path, path)) = resolve_prefixed_resource_entry(base_dir, entry) else {
        return;
    };
    resources
        .entry(display_path.clone())
        .or_insert_with(|| PendingDisclosedSkillResource {
            kind,
            display_path,
            path,
        });
}

fn materialize_disclosed_resources<I, F>(
    resources: I,
    mut load_content: F,
) -> Vec<DisclosedSkillResource>
where
    I: IntoIterator<Item = PendingDisclosedSkillResource>,
    F: FnMut(&Path) -> Option<String>,
{
    resources
        .into_iter()
        .take(MAX_DISCLOSED_RESOURCE_COUNT)
        .map(|resource| {
            let content = load_content(&resource.path);
            DisclosedSkillResource {
                kind: resource.kind,
                display_path: resource.display_path,
                tracked_path: PromptTrackedPath::prefix_bytes(
                    resource.path,
                    MAX_DISCLOSED_RESOURCE_BYTES,
                ),
                content,
            }
        })
        .collect()
}

fn resolve_resource_entry(
    base_dir: &Path,
    kind: SkillResourceKind,
    entry: &str,
) -> Option<(String, PathBuf)> {
    let relative = sanitize_relative_path(entry)?;
    let relative = if relative.starts_with(kind.default_dir()) {
        relative
    } else {
        PathBuf::from(kind.default_dir()).join(relative)
    };
    let display_path = relative_display_path(&relative);
    let path = resolve_relative_under_root(base_dir, &relative)?;
    Some((display_path, path))
}

fn resolve_prefixed_resource_entry(
    base_dir: &Path,
    entry: &str,
) -> Option<(SkillResourceKind, String, PathBuf)> {
    let relative = sanitize_relative_path(entry)?;
    let first = relative.components().next()?.as_os_str().to_str()?;
    let kind = match first {
        "references" => SkillResourceKind::Reference,
        "scripts" => SkillResourceKind::Script,
        "assets" => SkillResourceKind::Asset,
        _ => return None,
    };
    let display_path = relative_display_path(&relative);
    let path = resolve_relative_under_root(base_dir, &relative)?;
    Some((kind, display_path, path))
}

fn resolve_relative_path(base_dir: &Path, entry: &str) -> Option<(String, PathBuf)> {
    let relative = sanitize_relative_path(entry)?;
    let display_path = relative_display_path(&relative);
    let path = resolve_relative_under_root(base_dir, &relative)?;
    Some((display_path, path))
}

fn resolve_relative_under_root(root: &Path, relative: &Path) -> Option<PathBuf> {
    let candidate = root.join(relative);
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

    match std::fs::canonicalize(&candidate) {
        Ok(path) if path.starts_with(&canonical_root) => Some(path),
        Ok(_) => None,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(candidate),
        Err(_) => None,
    }
}

fn sanitize_relative_path(entry: &str) -> Option<PathBuf> {
    let trimmed = entry.trim().trim_matches(|c| matches!(c, '`' | '"' | '\''));
    let trimmed = trimmed.split(['#', '?']).next()?.trim();
    if trimmed.is_empty() || trimmed.contains("://") {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn extract_resource_references(content: &str) -> Vec<String> {
    static RESOURCE_REF_RE: OnceLock<Regex> = OnceLock::new();
    let regex = RESOURCE_REF_RE.get_or_init(|| {
        Regex::new(r"(references|scripts|assets)/[A-Za-z0-9](?:[A-Za-z0-9._/\-]*[A-Za-z0-9_-])?")
            .unwrap()
    });

    let mut references = BTreeSet::new();
    for capture in regex.find_iter(content) {
        if has_valid_resource_reference_prefix(content, capture.start()) {
            references.insert(capture.as_str().to_string());
        }
    }
    references.into_iter().collect()
}

fn has_valid_resource_reference_prefix(content: &str, start: usize) -> bool {
    if start == 0 {
        return true;
    }

    let prefix = &content[..start];
    if prefix.ends_with("./") {
        return true;
    }

    matches!(
        prefix.chars().next_back(),
        Some(ch)
            if ch.is_whitespace()
                || matches!(ch, '`' | '"' | '\'' | '(' | '[' | '{' | '<' | '*')
    )
}

fn has_valid_resource_reference_suffix(content: &str, end: usize) -> bool {
    if end >= content.len() {
        return true;
    }

    matches!(
        content[end..].chars().next(),
        Some(ch)
            if ch.is_whitespace()
                || matches!(ch, '`' | '"' | '\'' | ')' | ']' | '}' | '>' | '*' | ',' | '.' | ':' | ';' | '!' | '?' | '#')
                || matches!(ch, '`' | '"' | '\'' | ')' | ']' | '}' | '>' | '*' | ',' | '.' | ':' | ';' | '!' | '?' | '#')
    )
}

fn declared_resource_is_referenced(
    level2_body: &str,
    kind: SkillResourceKind,
    entry: &str,
    display_path: &str,
) -> bool {
    declared_resource_reference_candidates(kind, entry, display_path)
        .into_iter()
        .any(|candidate| content_contains_resource_reference(level2_body, &candidate))
}

fn declared_resource_reference_candidates(
    kind: SkillResourceKind,
    entry: &str,
    display_path: &str,
) -> Vec<String> {
    let Some(relative) = sanitize_relative_path(entry) else {
        return vec![display_path.to_string()];
    };

    let mut candidates = BTreeSet::from([display_path.to_string()]);
    candidates.insert(relative_display_path(&relative));
    if let Ok(unprefixed) = relative.strip_prefix(kind.default_dir())
        && !unprefixed.as_os_str().is_empty()
    {
        candidates.insert(relative_display_path(unprefixed));
    }
    if !relative.starts_with(kind.default_dir()) {
        candidates.insert(relative_display_path(
            &PathBuf::from(kind.default_dir()).join(&relative),
        ));
    }

    candidates.into_iter().collect()
}

fn content_contains_resource_reference(content: &str, reference: &str) -> bool {
    if reference.is_empty() {
        return false;
    }

    content.match_indices(reference).any(|(start, _)| {
        let end = start + reference.len();
        has_valid_resource_reference_prefix(content, start)
            && has_valid_resource_reference_suffix(content, end)
    })
}

fn relative_display_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
fn load_resource_content(path: &Path) -> Option<String> {
    load_disclosed_text_content(
        path,
        MAX_DISCLOSED_RESOURCE_BYTES,
        Some(MAX_DISCLOSED_RESOURCE_CHARS),
    )
}

fn load_disclosed_text_content(
    path: &Path,
    max_bytes: u64,
    max_chars: Option<usize>,
) -> Option<String> {
    let mut reader = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    let bytes_read = reader
        .by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    let truncated_by_bytes = bytes_read as u64 > max_bytes;
    if truncated_by_bytes {
        bytes.truncate(max_bytes as usize);
    }

    let valid_len = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(err) if truncated_by_bytes && err.error_len().is_none() && err.valid_up_to() > 0 => {
            err.valid_up_to()
        }
        Err(_) => return None,
    };
    bytes.truncate(valid_len);

    let content = String::from_utf8(bytes).ok()?;
    Some(truncate_disclosed_text_content(
        content,
        truncated_by_bytes,
        max_bytes,
        max_chars,
    ))
}

fn truncate_disclosed_text_content(
    content: String,
    truncated_by_bytes: bool,
    max_bytes: u64,
    max_chars: Option<usize>,
) -> String {
    let total_chars = content.chars().count();
    let truncated_by_chars = max_chars.is_some_and(|limit| total_chars > limit);
    if !truncated_by_bytes && !truncated_by_chars {
        return content;
    }

    let truncated: String = match max_chars {
        Some(limit) => content.chars().take(limit).collect(),
        None => content,
    };
    let visible_chars = truncated.chars().count();
    let notice = if truncated_by_bytes {
        format!("truncated after {visible_chars} chars from a file that exceeded {max_bytes} bytes")
    } else {
        format!("truncated after {visible_chars} chars from {total_chars}")
    };
    format!("{truncated}\n...[{notice}]")
}

fn strip_frontmatter_if_present(content: String) -> String {
    extract_frontmatter(&content)
        .map(|(_, body)| body)
        .unwrap_or(content)
}

fn format_disclosed_resources(resources: &[DisclosedSkillResource]) -> String {
    if resources.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "### Disclosed Resources".to_string(),
        "Only resources referenced by the active instructions or declared disclosure metadata are expanded here.".to_string(),
    ];

    for resource in resources {
        lines.push(format!(
            "#### {}: {}",
            resource.kind.label(),
            resource.display_path
        ));
        if let Some(content) = resource.content.as_ref() {
            lines.push(render_fenced_resource_content(
                &resource.display_path,
                content,
            ));
        } else {
            lines.push(
                "Binary or unreadable resource; resolve it from `resource_root` if deeper inspection is needed."
                    .to_string(),
            );
        }
    }

    lines.join("\n")
}

fn guess_code_fence(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("md") => "md",
        Some("rs") => "rust",
        Some("sh") => "bash",
        Some("py") => "python",
        Some("json") => "json",
        Some("yaml" | "yml") => "yaml",
        Some("toml") => "toml",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("html") => "html",
        Some("css") => "css",
        _ => "text",
    }
}

fn render_fenced_resource_content(path: &str, content: &str) -> String {
    let language = guess_code_fence(path);
    let fence = "`".repeat(fence_length(content));
    format!("{fence}{language}\n{content}\n{fence}")
}

fn fence_length(content: &str) -> usize {
    longest_backtick_run(content).max(3) + 1
}

fn longest_backtick_run(content: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;

    for ch in content.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }

    longest
}

fn dedupe_tracked_paths(paths: &mut Vec<PromptTrackedPath>) {
    let mut seen: BTreeMap<PathBuf, PromptTrackedPathFingerprint> = BTreeMap::new();
    for tracked_path in paths.drain(..) {
        seen.entry(tracked_path.path)
            .and_modify(|fingerprint| {
                *fingerprint = fingerprint.merge(tracked_path.fingerprint);
            })
            .or_insert(tracked_path.fingerprint);
    }
    *paths = seen
        .into_iter()
        .map(|(path, fingerprint)| PromptTrackedPath { path, fingerprint })
        .collect();
}

fn render_optional_path(path: Option<&Path>) -> String {
    path.map(|value| value.display().to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

fn render_prompt_visible_skill_path(skill: &SkillMetadata) -> String {
    if skill.is_builtin_package() {
        format!("builtin:{}", skill.id)
    } else {
        skill.path.display().to_string()
    }
}

fn render_prompt_visible_package_root(skill: &SkillMetadata) -> String {
    if skill.is_builtin_package() {
        "<builtin capability package>".to_string()
    } else {
        render_optional_path(skill.package_root())
    }
}

fn render_prompt_visible_resource_root(skill: &SkillMetadata) -> String {
    if skill.is_builtin_package() {
        "<builtin capability package>".to_string()
    } else {
        render_optional_path(skill.resource_root())
    }
}

fn format_unresolved_execution_details(execution: &ResolvedSkillExecution) -> String {
    let ResolvedSkillExecution::Unresolved { reason } = execution else {
        return String::new();
    };

    match reason {
        SkillExecutionUnresolvedReason::NotResolved => String::new(),
        SkillExecutionUnresolvedReason::MissingChildAgentExports => {
            "reason: missing_child_agent_exports".to_string()
        }
        SkillExecutionUnresolvedReason::DelegateTargetNotFound {
            target,
            available_targets,
        } => format!(
            "reason: delegate_target_not_found({target})\navailable_targets: {}",
            render_csv_or_none(available_targets)
        ),
        SkillExecutionUnresolvedReason::AmbiguousPackageShape {
            skill_id,
            child_agent_exports,
        } => format!(
            "reason: ambiguous_package_shape\nskill_id: {skill_id}\nchild_agent_exports: {}",
            render_csv_or_none(child_agent_exports)
        ),
    }
}

fn render_csv_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".to_string()
    } else {
        values.join(", ")
    }
}

/// Build a prompt with injected skills.
pub fn build_prompt_with_skills(user_input: &str, skills: &[Skill]) -> String {
    if skills.is_empty() {
        return user_input.to_string();
    }

    let skill_context = inject_skills(skills);

    format!(
        r#"{skill_context}

## User Request

{user_input}"#
    )
}

/// Render a list of implicitly available skills for the system prompt.
pub fn render_skills_list(
    skills: &[SkillMetadata],
    delegated_invocation_available: bool,
) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let mut lines = vec![
        "## Available Skills".to_string(),
        "The following skills are enabled for implicit use in this runtime.".to_string(),
        "Use them when the task clearly matches. Read `SKILL.md` only when needed, then load referenced resources progressively.".to_string(),
        String::new(),
    ];

    for skill in skills {
        let builtin_package = skill.is_builtin_package();
        lines.push(format!("- skill_id: {}", skill.id));
        lines.push(format!("  name: {}", skill.name));
        lines.push(format!("  description: {}", skill.description));
        if builtin_package {
            lines.push("  skill_source: builtin_capability_package".to_string());
        }
        match &skill.execution {
            ResolvedSkillExecution::Delegate { .. } if !delegated_invocation_available => {
                if !builtin_package {
                    lines.push(format!("  skill_path: {}", skill.path.display()));
                    lines.push(
                        "  use: open `SKILL.md` only when needed, then follow its instructions"
                            .to_string(),
                    );
                } else {
                    lines.push(
                        "  use: activate when needed; this runtime cannot delegate the builtin capability directly, so rely on the runtime-disclosed instructions instead of opening builtin package files via tools"
                            .to_string(),
                    );
                }
            }
            ResolvedSkillExecution::Delegate { target, .. } => {
                lines.push(format!("  execution: delegate(target={target})"));
                if builtin_package {
                    lines.push("  use: call `invoke_delegated_skill` directly with this `skill_id`, the delegated `target`, and a concise bounded task; do not open builtin package files via tools".to_string());
                } else {
                    lines.push("  use: call `invoke_delegated_skill` directly with this `skill_id`, the delegated `target`, and a concise bounded task".to_string());
                }
                lines.push("  note: when the delegated task targets a different local workspace, include `workspace_root` and optional `cwd` so the child runtime binds to the correct scope".to_string());
            }
            _ => {
                if builtin_package {
                    lines.push(
                        "  use: activate when needed; rely on the runtime-disclosed instructions instead of opening builtin package files via tools"
                            .to_string(),
                    );
                } else {
                    lines.push(format!("  skill_path: {}", skill.path.display()));
                    lines.push(
                        "  use: open `SKILL.md` only when needed, then follow its instructions"
                            .to_string(),
                    );
                }
            }
        }
        lines.push(String::new());
    }

    lines.push(
        "Explicit `$skill` mentions from the user still take priority over your own implicit selection."
            .to_string(),
    );

    Some(lines.join("\n"))
}

/// Render a skill not found message.
pub fn render_skill_not_found(mention: &str, available: &[SkillMetadata]) -> String {
    let mut msg = format!("Skill '${}' not found. ", mention);

    // Suggest similar skills
    let similar: Vec<_> = available
        .iter()
        .filter(|s| s.id.contains(mention) || mention.contains(&s.id))
        .take(3)
        .collect();

    if !similar.is_empty() {
        msg.push_str("Did you mean: ");
        let names: Vec<_> = similar.iter().map(|s| format!("${}", s.id)).collect();
        msg.push_str(&names.join(", "));
        msg.push('?');
    } else {
        msg.push_str("Use `/skills` to see available skills.");
    }

    msg
}

/// Render a skill unavailable message with concrete host/runtime requirements.
pub fn render_skill_unavailable(mention: &str, reasons: &str) -> String {
    format!("Skill '${mention}' is unavailable in this runtime: {reasons}.")
}

/// Render a skill unavailable message with structured remediation guidance.
pub fn render_skill_unavailable_with_remediation(
    mention: &str,
    remediation: &SkillRemediation,
) -> String {
    let mut lines = vec![format!(
        "Skill '${mention}' is unavailable in this runtime: {}.",
        remediation.reasons.join("; ")
    )];

    if !remediation.next_steps.is_empty() {
        lines.push("Suggested next steps:".to_string());
        lines.extend(
            remediation
                .next_steps
                .iter()
                .map(|step| format!("- {step}")),
        );
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_mentions() {
        assert_eq!(
            extract_mentions("Use $test-skill for testing"),
            vec!["test-skill"]
        );

        assert!(extract_mentions("Run $my_skill please").is_empty());

        // Multiple mentions
        let mentions = extract_mentions("$skill-a and $skill-b");
        assert_eq!(mentions, vec!["skill-a", "skill-b"]);

        // With punctuation at end
        assert_eq!(extract_mentions("Use $skill-name."), vec!["skill-name"]);

        // No mentions
        assert!(extract_mentions("Plain text without mentions").is_empty());
    }

    #[test]
    fn test_inject_skills() {
        let skill = Skill {
            metadata: SkillMetadata {
                id: "test".to_string(),
                package_id: None,
                name: "Test Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/tmp/test/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("/tmp/test/SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "# Instructions\n\nDo this and that.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("## Skill: Test Skill"));
        assert!(injected.contains("# Instructions"));
    }

    #[test]
    fn test_inject_skills_conservatively_falls_back_inline_without_runtime_support() {
        let skill = Skill {
            metadata: SkillMetadata {
                id: "repo-review".to_string(),
                package_id: Some("pkg:repo-review".to_string()),
                name: "Repo Review".to_string(),
                description: "Review repository changes".to_string(),
                short_description: Some("Delegated review capability".to_string()),
                path: std::path::PathBuf::from("/tmp/repo-review/SKILL.md"),
                package_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
                resource_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from(
                    "/tmp/repo-review/SKILL.md",
                )),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: ResolvedSkillExecution::Delegate {
                    target: "reviewer".to_string(),
                    source: SkillExecutionResolutionSource::ExplicitMetadata,
                },
            },
            content: "SECRET INLINE BODY".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Repo Review".to_string(),
                description: "Review repository changes".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("### Runtime Fallback"));
        assert!(injected.contains("SECRET INLINE BODY"));
        assert!(!injected.contains("### Delegated Capability"));
    }

    #[test]
    fn test_render_active_skill_prompt_can_render_delegated_stub_with_runtime_support() {
        let skill = Skill {
            metadata: SkillMetadata {
                id: "repo-review".to_string(),
                package_id: Some("pkg:repo-review".to_string()),
                name: "Repo Review".to_string(),
                description: "Review repository changes".to_string(),
                short_description: Some("Delegated review capability".to_string()),
                path: std::path::PathBuf::from("/tmp/repo-review/SKILL.md"),
                package_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
                resource_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from(
                    "/tmp/repo-review/SKILL.md",
                )),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: ResolvedSkillExecution::Delegate {
                    target: "reviewer".to_string(),
                    source: SkillExecutionResolutionSource::ExplicitMetadata,
                },
            },
            content: "SECRET INLINE BODY".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Repo Review".to_string(),
                description: "Review repository changes".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let envelope = ActiveSkillEnvelope::available(
            skill.metadata.clone(),
            SkillActivationReason::ExplicitMention {
                mention: "repo-review".to_string(),
            },
        );
        let rendered = render_active_skill_prompt_for_runtime(&skill, &envelope, true).rendered;

        assert!(rendered.contains("### Delegated Capability"));
        assert!(rendered.contains("invoke_delegated_skill"));
        assert!(rendered.contains("output_ref"));
        assert!(rendered.contains("child_run"));
        assert!(rendered.contains("\"target\": \"reviewer\""));
        assert!(!rendered.contains("SECRET INLINE BODY"));
    }

    #[test]
    fn test_inject_skills_renders_unresolved_stub_without_full_body() {
        let skill = Skill {
            metadata: SkillMetadata {
                id: "skill-creator".to_string(),
                package_id: Some("pkg:skill-creator".to_string()),
                name: "Skill Creator".to_string(),
                description: "Create and grade skills".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/tmp/skill-creator/SKILL.md"),
                package_root: Some(std::path::PathBuf::from("/tmp/skill-creator")),
                resource_root: Some(std::path::PathBuf::from("/tmp/skill-creator")),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from(
                    "/tmp/skill-creator/SKILL.md",
                )),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: ResolvedSkillExecution::Unresolved {
                    reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                        skill_id: "skill-creator".to_string(),
                        child_agent_exports: vec![
                            "creator".to_string(),
                            "grader".to_string(),
                            "analyzer".to_string(),
                        ],
                    },
                },
            },
            content: "INLINE BODY SHOULD NOT APPEAR".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Skill Creator".to_string(),
                description: "Create and grade skills".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("### Skill Execution Status"));
        assert!(injected.contains("reason: ambiguous_package_shape"));
        assert!(injected.contains("child_agent_exports: creator, grader, analyzer"));
        assert!(!injected.contains("INLINE BODY SHOULD NOT APPEAR"));
    }

    #[test]
    fn test_build_prompt_with_skills() {
        let skill = Skill {
            metadata: SkillMetadata {
                id: "eval".to_string(),
                package_id: None,
                name: "Evaluation".to_string(),
                description: "Eval".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/tmp/eval/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("/tmp/eval/SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Follow these steps.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Evaluation".to_string(),
                description: "Eval".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let prompt = build_prompt_with_skills("Evaluate this", &[skill]);
        assert!(prompt.contains("Follow these steps"));
        assert!(prompt.contains("Evaluate this"));
    }

    #[test]
    fn test_render_skills_list() {
        let skills = vec![
            SkillMetadata {
                id: "skill-a".to_string(),
                package_id: None,
                name: "Skill A".to_string(),
                description: "Does A".to_string(),
                short_description: Some("Short A".to_string()),
                path: std::path::PathBuf::from("/a/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("/a/SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: CompatibleSkillMetadata {
                    interface: CompatibleSkillInterface {
                        display_name: Some("UI Skill A".to_string()),
                        short_description: Some("UI Short A".to_string()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                execution: Default::default(),
            },
            SkillMetadata {
                id: "skill-b".to_string(),
                package_id: None,
                name: "Skill B".to_string(),
                description: "Does B".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/b/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: SkillScope::Repo,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("/b/SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            SkillMetadata {
                id: "skill-c".to_string(),
                package_id: None,
                name: "Skill C".to_string(),
                description: "Does C".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/c/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: SkillScope::Repo,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("/c/SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: ResolvedSkillExecution::Delegate {
                    target: "reviewer".to_string(),
                    source: SkillExecutionResolutionSource::ExplicitMetadata,
                },
            },
        ];

        let list = render_skills_list(&skills, true).unwrap();
        assert!(list.contains("## Available Skills"));
        assert!(list.contains("- skill_id: skill-a"));
        assert!(list.contains("  name: Skill A"));
        assert!(list.contains("  description: Does A"));
        assert!(!list.contains("UI Skill A"));
        assert!(!list.contains("UI Short A"));
        assert!(list.contains("  skill_path: /a/SKILL.md"));
        assert!(list.contains("- skill_id: skill-b"));
        assert!(list.contains("  description: Does B"));
        assert!(list.contains("- skill_id: skill-c"));
        assert!(list.contains("  execution: delegate(target=reviewer)"));
        assert!(list.contains("  use: call `invoke_delegated_skill` directly"));
        assert!(list.contains("Available Skills"));
    }

    #[test]
    fn test_render_skills_list_falls_back_to_inline_guidance_without_delegated_support() {
        let skills = vec![SkillMetadata {
            id: "skill-c".to_string(),
            package_id: None,
            name: "Skill C".to_string(),
            description: "Does C".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/c/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/c/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Delegate {
                target: "reviewer".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            },
        }];

        let list = render_skills_list(&skills, false).unwrap();
        assert!(list.contains("  skill_path: /c/SKILL.md"));
        assert!(!list.contains("invoke_delegated_skill"));
        assert!(!list.contains("execution: delegate("));
    }

    #[test]
    fn test_render_skills_list_hides_builtin_package_paths() {
        let skills = vec![SkillMetadata {
            id: "memory".to_string(),
            package_id: Some("builtin:alan-memory".to_string()),
            name: "Memory".to_string(),
            description: "Persistent memory across sessions".to_string(),
            short_description: None,
            path: std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
            ),
            package_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
            )),
            resource_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
            )),
            scope: SkillScope::Builtin,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
            )),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        }];

        let list = render_skills_list(&skills, false).unwrap();
        assert!(list.contains("  skill_source: builtin_capability_package"));
        assert!(list.contains("rely on the runtime-disclosed instructions"));
        assert!(!list.contains("skill_path:"));
        assert!(!list.contains("builtin-skill-packages"));
    }

    #[test]
    fn test_render_skills_list_keeps_builtin_delegated_target_guidance() {
        let skills = vec![SkillMetadata {
            id: "skill-creator".to_string(),
            package_id: Some("builtin:alan-skill-creator".to_string()),
            name: "Skill Creator".to_string(),
            description: "Create or update alan skill packages".to_string(),
            short_description: None,
            path: std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
            ),
            package_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
            )),
            resource_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
            )),
            scope: SkillScope::Builtin,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
            )),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Delegate {
                target: "skill-creator".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            },
        }];

        let list = render_skills_list(&skills, true).unwrap();
        assert!(list.contains("  skill_source: builtin_capability_package"));
        assert!(list.contains("  execution: delegate(target=skill-creator)"));
        assert!(list.contains("invoke_delegated_skill"));
        assert!(!list.contains("skill_path:"));
        assert!(!list.contains("builtin-skill-packages"));
    }

    #[test]
    fn test_render_skills_list_builtin_delegated_skill_degrades_without_path_leak() {
        let skills = vec![SkillMetadata {
            id: "skill-creator".to_string(),
            package_id: Some("builtin:alan-skill-creator".to_string()),
            name: "Skill Creator".to_string(),
            description: "Create or update alan skill packages".to_string(),
            short_description: None,
            path: std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
            ),
            package_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
            )),
            resource_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
            )),
            scope: SkillScope::Builtin,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
            )),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Delegate {
                target: "skill-creator".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            },
        }];

        let list = render_skills_list(&skills, false).unwrap();
        assert!(list.contains("  skill_source: builtin_capability_package"));
        assert!(list.contains("this runtime cannot delegate the builtin capability directly"));
        assert!(!list.contains("invoke_delegated_skill"));
        assert!(!list.contains("skill_path:"));
        assert!(!list.contains("builtin-skill-packages"));
    }

    #[test]
    fn test_active_skill_context_hides_builtin_package_paths() {
        let skill = Skill {
            metadata: SkillMetadata {
                id: "memory".to_string(),
                package_id: Some("builtin:alan-memory".to_string()),
                name: "Memory".to_string(),
                description: "Persistent memory across sessions".to_string(),
                short_description: None,
                path: std::path::PathBuf::from(
                    "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
                ),
                package_root: Some(std::path::PathBuf::from(
                    "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
                )),
                resource_root: Some(std::path::PathBuf::from(
                    "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
                )),
                scope: SkillScope::Builtin,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from(
                    "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
                )),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "# Instructions\nPersist durable context.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Memory".to_string(),
                description: "Persistent memory across sessions".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };
        let envelope = ActiveSkillEnvelope::available(
            skill.metadata.clone(),
            SkillActivationReason::ExplicitMention {
                mention: "memory".to_string(),
            },
        );

        let rendered = render_active_skill_prompt_for_runtime(&skill, &envelope, false).rendered;
        assert!(rendered.contains("canonical_path: builtin:memory"));
        assert!(rendered.contains("package_root: <builtin capability package>"));
        assert!(rendered.contains("resource_root: <builtin capability package>"));
        assert!(rendered.contains("Do not use tools to open builtin package files by path."));
        assert!(!rendered.contains("builtin-skill-packages"));
    }

    #[test]
    fn test_extract_mentions_edge_cases() {
        // Empty input
        assert!(extract_mentions("").is_empty());

        // Only $ sign
        assert!(extract_mentions("$").is_empty());

        // $ at end
        assert!(extract_mentions("text $").is_empty());

        // Duplicate mentions (should dedupe)
        assert_eq!(extract_mentions("$skill-a and $skill-a"), vec!["skill-a"]);

        // Legacy underscore separator is rejected
        assert!(extract_mentions("$skill_name").is_empty());

        // Legacy dot separator is rejected
        assert!(extract_mentions("$repo.review").is_empty());
    }

    #[test]
    fn test_extract_mentions_multiple_same_and_different() {
        // Multiple skills with duplicates in various positions
        let mentions = extract_mentions("$skill-a $skill-b $skill-a $skill-c $skill-b");
        assert_eq!(mentions, vec!["skill-a", "skill-b", "skill-c"]);
    }

    #[test]
    fn test_extract_mentions_with_numbers() {
        assert_eq!(extract_mentions("Use $skill-123"), vec!["skill-123"]);
        assert!(extract_mentions("$test-v2.0").is_empty());
        assert_eq!(extract_mentions("Use $skill-name."), vec!["skill-name"]);
    }

    #[test]
    fn test_inject_skills_empty() {
        let result = inject_skills(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_inject_skills_with_resources() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("test-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::create_dir(skill_dir.join("scripts")).unwrap();
        std::fs::create_dir(skill_dir.join("references")).unwrap();
        std::fs::create_dir(skill_dir.join("assets")).unwrap();

        // Create resource files
        std::fs::write(skill_dir.join("scripts/test.sh"), "#!/bin/bash").unwrap();
        std::fs::write(skill_dir.join("references/ref.md"), "# Reference").unwrap();
        std::fs::write(skill_dir.join("assets/logo.png"), [0_u8, 159, 146, 150]).unwrap();

        let skill = Skill {
            metadata: SkillMetadata {
                id: "test-res".to_string(),
                package_id: None,
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: skill_dir.join("SKILL.md"),
                package_root: Some(skill_dir.clone()),
                resource_root: Some(skill_dir.clone()),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: Some(SkillCapabilities {
                    disclosure: DisclosureConfig {
                        level3: Level3Resources {
                            assets: vec!["logo.png".to_string()],
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                compatibility: Default::default(),
                source: SkillContentSource::File(skill_dir.join("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Read `references/ref.md` before running `scripts/test.sh`.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("## Skill: Test Resource Skill"));
        assert!(injected.contains("### alan Runtime Context"));
        assert!(injected.contains("### Disclosed Resources"));
        assert!(injected.contains("#### script: scripts/test.sh"));
        assert!(injected.contains("#!/bin/bash"));
        assert!(injected.contains("#### reference: references/ref.md"));
        assert!(injected.contains("# Reference"));
        assert!(!injected.contains("#### asset: assets/logo.png"));
    }

    #[test]
    fn test_inject_skills_only_expands_declared_resources_when_level2_references_them() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("test-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::create_dir(skill_dir.join("references")).unwrap();

        std::fs::write(skill_dir.join("references/quickstart.md"), "# Quickstart").unwrap();

        let skill = Skill {
            metadata: SkillMetadata {
                id: "test-res".to_string(),
                package_id: None,
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: skill_dir.join("SKILL.md"),
                package_root: Some(skill_dir.clone()),
                resource_root: Some(skill_dir.clone()),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: Some(SkillCapabilities {
                    disclosure: DisclosureConfig {
                        level3: Level3Resources {
                            references: vec!["quickstart.md".to_string()],
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                compatibility: Default::default(),
                source: SkillContentSource::File(skill_dir.join("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Read `quickstart.md` before using this skill.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("#### reference: references/quickstart.md"));
        assert!(injected.contains("# Quickstart"));
    }

    #[test]
    fn test_inject_skills_matches_declared_resources_with_fragment_or_query_suffixes() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("test-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::create_dir(skill_dir.join("references")).unwrap();

        std::fs::write(skill_dir.join("references/quickstart.md"), "# Quickstart").unwrap();

        let build_skill = |content: &str| Skill {
            metadata: SkillMetadata {
                id: "test-res".to_string(),
                package_id: None,
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: skill_dir.join("SKILL.md"),
                package_root: Some(skill_dir.clone()),
                resource_root: Some(skill_dir.clone()),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: Some(SkillCapabilities {
                    disclosure: DisclosureConfig {
                        level3: Level3Resources {
                            references: vec!["quickstart.md".to_string()],
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                compatibility: Default::default(),
                source: SkillContentSource::File(skill_dir.join("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: content.to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let fragment_injected = inject_skills(&[build_skill(
            "Read `quickstart.md#setup` before using this skill.",
        )]);
        assert!(fragment_injected.contains("#### reference: references/quickstart.md"));

        let query_injected = inject_skills(&[build_skill(
            "Read `quickstart.md?view=plain` before using this skill.",
        )]);
        assert!(query_injected.contains("#### reference: references/quickstart.md"));
    }

    #[test]
    fn test_inject_skills_matches_prefixed_declared_resources_from_bare_references() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("test-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::create_dir(skill_dir.join("references")).unwrap();

        std::fs::write(skill_dir.join("references/quickstart.md"), "# Quickstart").unwrap();

        let skill = Skill {
            metadata: SkillMetadata {
                id: "test-res".to_string(),
                package_id: None,
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: skill_dir.join("SKILL.md"),
                package_root: Some(skill_dir.clone()),
                resource_root: Some(skill_dir.clone()),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: Some(SkillCapabilities {
                    disclosure: DisclosureConfig {
                        level3: Level3Resources {
                            references: vec!["references/quickstart.md".to_string()],
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                compatibility: Default::default(),
                source: SkillContentSource::File(skill_dir.join("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Read `quickstart.md` before using this skill.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("#### reference: references/quickstart.md"));
        assert!(injected.contains("# Quickstart"));
    }

    #[test]
    fn test_declared_resource_reference_candidates_normalize_windows_separators() {
        let candidates = declared_resource_reference_candidates(
            SkillResourceKind::Reference,
            "guides/setup.md",
            r"references\guides\setup.md",
        );

        assert!(candidates.contains(&"references/guides/setup.md".to_string()));
        assert!(content_contains_resource_reference(
            "Read `references/guides/setup.md` before running the skill.",
            "references/guides/setup.md",
        ));
    }

    #[test]
    fn test_inject_skills_uses_custom_level2_file() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("test-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("details.md"), "Expanded instructions.").unwrap();

        let skill = Skill {
            metadata: SkillMetadata {
                id: "test-res".to_string(),
                package_id: None,
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: skill_dir.join("SKILL.md"),
                package_root: Some(skill_dir.clone()),
                resource_root: Some(skill_dir.clone()),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: Some(SkillCapabilities {
                    disclosure: DisclosureConfig {
                        level2: "details.md".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                compatibility: Default::default(),
                source: SkillContentSource::File(skill_dir.join("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Fallback instructions.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("source: details.md"));
        assert!(injected.contains("Expanded instructions."));
        assert!(!injected.contains("Fallback instructions."));
    }

    #[test]
    fn test_load_resource_content_caps_large_files_by_byte_budget() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("large.txt");
        std::fs::write(
            &path,
            "a".repeat(MAX_DISCLOSED_RESOURCE_BYTES as usize + 1024),
        )
        .unwrap();

        let content = load_resource_content(&path).unwrap();

        assert!(content.starts_with('a'));
        assert!(content.contains(&format!("exceeded {MAX_DISCLOSED_RESOURCE_BYTES} bytes")));
    }

    #[test]
    fn test_extract_resource_references_ignores_urls_and_trims_punctuation() {
        let references = extract_resource_references(
            "Use https://example.com/scripts/setup.sh, then read references/guide.md.",
        );

        assert_eq!(references, vec!["references/guide.md"]);
    }

    #[test]
    fn test_inject_skills_extracts_sentence_refs_and_dot_slash_paths() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("test-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::create_dir(skill_dir.join("scripts")).unwrap();
        std::fs::create_dir(skill_dir.join("references")).unwrap();
        std::fs::write(
            skill_dir.join("scripts/setup.sh"),
            "#!/bin/bash\necho setup",
        )
        .unwrap();
        std::fs::write(skill_dir.join("references/guide.md"), "# Guide").unwrap();

        let skill = Skill {
            metadata: SkillMetadata {
                id: "test-res".to_string(),
                package_id: None,
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: skill_dir.join("SKILL.md"),
                package_root: Some(skill_dir.clone()),
                resource_root: Some(skill_dir.clone()),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(skill_dir.join("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Use https://example.com/scripts/setup.sh, then read references/guide.md. After that run ./scripts/setup.sh."
                .to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("#### reference: references/guide.md"));
        assert!(injected.contains("#### script: scripts/setup.sh"));
        assert_eq!(injected.matches("#### script: scripts/setup.sh").count(), 1);
    }

    #[test]
    fn test_materialize_disclosed_resources_loads_only_capped_selection() {
        let load_count = std::cell::Cell::new(0);
        let resources = (0..12).map(|index| PendingDisclosedSkillResource {
            kind: SkillResourceKind::Reference,
            display_path: format!("references/ref-{index:02}.md"),
            path: PathBuf::from(format!("/tmp/ref-{index:02}.md")),
        });

        let loaded = materialize_disclosed_resources(resources, |_| {
            load_count.set(load_count.get() + 1);
            Some("content".to_string())
        });

        assert_eq!(loaded.len(), MAX_DISCLOSED_RESOURCE_COUNT);
        assert_eq!(load_count.get(), MAX_DISCLOSED_RESOURCE_COUNT);
        assert_eq!(loaded[0].display_path, "references/ref-00.md");
        assert_eq!(loaded[7].display_path, "references/ref-07.md");
    }

    #[test]
    fn test_inject_skills_caps_large_level2_file_by_byte_budget() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("test-skill");
        std::fs::create_dir(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("details.md"),
            "b".repeat(MAX_DISCLOSED_LEVEL2_BYTES as usize + 1024),
        )
        .unwrap();

        let skill = Skill {
            metadata: SkillMetadata {
                id: "test-res".to_string(),
                package_id: None,
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: skill_dir.join("SKILL.md"),
                package_root: Some(skill_dir.clone()),
                resource_root: Some(skill_dir.clone()),
                scope: SkillScope::User,
                tags: vec![],
                capabilities: Some(SkillCapabilities {
                    disclosure: DisclosureConfig {
                        level2: "details.md".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                compatibility: Default::default(),
                source: SkillContentSource::File(skill_dir.join("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Fallback instructions.".to_string(),
            frontmatter: SkillFrontmatter {
                name: "Test Resource Skill".to_string(),
                description: "A test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("source: details.md"));
        assert!(injected.contains(&format!("exceeded {MAX_DISCLOSED_LEVEL2_BYTES} bytes")));
        assert!(!injected.contains("Fallback instructions."));
    }

    #[test]
    fn test_format_disclosed_resources_uses_safe_fence_for_embedded_backticks() {
        let rendered = format_disclosed_resources(&[DisclosedSkillResource {
            kind: SkillResourceKind::Reference,
            display_path: "references/ref.md".to_string(),
            tracked_path: PromptTrackedPath::prefix_bytes(PathBuf::from("/tmp/ref.md"), 16),
            content: Some("before\n```md\ninside\n```\nafter".to_string()),
        }]);

        assert!(rendered.contains("````md"));
        assert!(rendered.contains("\n````"));
    }

    #[test]
    fn test_inject_skills_no_parent_path() {
        // Test the edge case where skill path has no parent
        let skill = Skill {
            metadata: SkillMetadata {
                id: "no-parent".to_string(),
                package_id: None,
                name: "No Parent".to_string(),
                description: "Test".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("SKILL.md"), // No parent
                package_root: None,
                resource_root: None,
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            content: "Content".to_string(),
            frontmatter: SkillFrontmatter {
                name: "No Parent".to_string(),
                description: "Test".to_string(),
                metadata: Default::default(),
                capabilities: Default::default(),
                compatibility: Default::default(),
            },
        };

        let injected = inject_skills(&[skill]);
        assert!(injected.contains("## Skill: No Parent"));
        // Should not panic and should not have Resources section
        assert!(!injected.contains("### Disclosed Resources"));
    }

    #[test]
    fn test_build_prompt_with_skills_empty() {
        let prompt = build_prompt_with_skills("Just user input", &[]);
        assert_eq!(prompt, "Just user input");
    }

    #[test]
    fn test_render_skills_list_empty() {
        assert!(render_skills_list(&[], true).is_none());
    }

    #[test]
    fn test_render_skill_not_found_with_similar() {
        let available = vec![
            SkillMetadata {
                id: "test-skill".to_string(),
                package_id: None,
                name: "Test Skill".to_string(),
                description: "A test".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/test/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("/test/SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            SkillMetadata {
                id: "testing".to_string(),
                package_id: None,
                name: "Testing".to_string(),
                description: "Testing skill".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/testing/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: SkillScope::User,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: SkillContentSource::File(std::path::PathBuf::from("/testing/SKILL.md")),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
        ];

        let msg = render_skill_not_found("test", &available);
        assert!(msg.contains("Skill '$test' not found"));
        assert!(msg.contains("Did you mean:"));
        assert!(msg.contains("$test-skill"));
    }

    #[test]
    fn test_render_skill_not_found_no_similar() {
        let available = vec![SkillMetadata {
            id: "other".to_string(),
            package_id: None,
            name: "Other".to_string(),
            description: "Other skill".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/other/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::User,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/other/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        }];

        let msg = render_skill_not_found("xyz", &available);
        assert!(msg.contains("Skill '$xyz' not found"));
        assert!(msg.contains("Use `/skills` to see available skills"));
        assert!(!msg.contains("Did you mean:"));
    }

    #[test]
    fn test_render_skill_not_found_partial_match() {
        // Test when the mention contains the skill id
        let available = vec![SkillMetadata {
            id: "rust".to_string(),
            package_id: None,
            name: "Rust".to_string(),
            description: "Rust skill".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/rust/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::User,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/rust/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        }];

        // "rustacean" contains "rust"
        let msg = render_skill_not_found("rustacean", &available);
        assert!(msg.contains("Did you mean:"));
        assert!(msg.contains("$rust"));
    }
}
