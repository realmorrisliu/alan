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
Do not inline or restate the full `SKILL.md` body in this machine.
When you need this capability, call `invoke_delegated_skill` with a concise bounded task for the delegated runtime.
The delegated runtime receives only descriptors and inherited namespace mounts. Use an Alan OS `cwd` already present in that namespace; request a Host Mount before delegation when required files are absent.
The tool returns a bounded result object with `status`, `summary`, optional `child_run`, optional inline `output_text`, optional namespace-path `output_ref`, optional `structured_output`, and explicit `truncation` metadata.
If `output_ref` or truncation metadata is present, treat the inline text as a preview. When the full delegated output is needed, open or read the namespace file at `output_ref.path`; raw rollout/machine paths are debug metadata, not evidence access paths.
Use `child_run` metadata only for delegation-scoped launch and handoff context. Inspect live child state through `/agent/<pid>/children` and `/proc`. Parent Agent Processes terminate children through governed `terminate_child_run` handling; external operators may stop a child through `/proc/<pid>/ctl` with `cancel` or `interrupt`. Inspect and control execution only through the owning file surfaces.

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
    let descriptor = match &skill.metadata.source {
        SkillContentSource::Descriptor { file_tree, .. } => Some(file_tree),
        SkillContentSource::File(_) | SkillContentSource::Embedded(_) => None,
    };
    let level2 = load_level2_content(skill, &disclosure, base_dir.as_deref(), descriptor);
    let resources =
        collect_disclosed_resources(&level2.body, &disclosure, base_dir.as_deref(), descriptor);

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
    match &skill.metadata.source {
        SkillContentSource::File(path) => path.parent(),
        SkillContentSource::Embedded(_) | SkillContentSource::Descriptor { .. } => None,
    }
    .or_else(|| envelope.metadata.resource_root())
    .or_else(|| skill.metadata.path.parent())
    .map(Path::to_path_buf)
}

fn load_level2_content(
    skill: &Skill,
    disclosure: &DisclosureConfig,
    base_dir: Option<&Path>,
    descriptor: Option<&crate::ProcessFileTree>,
) -> DisclosedLevel2Content {
    let mut tracked_paths = Vec::new();

    let requested = disclosure.level2.trim();
    if requested.is_empty() || requested == "SKILL.md" {
        return fallback_level2_content(skill, tracked_paths);
    }

    let Some(base_dir) = base_dir else {
        return fallback_level2_content(skill, tracked_paths);
    };
    let Some((display_path, path)) =
        resolve_relative_path(base_dir, requested, descriptor.is_some())
    else {
        return fallback_level2_content(skill, tracked_paths);
    };

    let source_path = match &skill.metadata.source {
        SkillContentSource::File(path) => path.as_path(),
        SkillContentSource::Embedded(_) | SkillContentSource::Descriptor { .. } => {
            skill.metadata.path.as_path()
        }
    };
    if path == source_path {
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

    let content = load_disclosure_content(
        descriptor,
        base_dir,
        &path,
        MAX_DISCLOSED_LEVEL2_BYTES,
        None,
    );
    let Some(content) = content else {
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
    descriptor: Option<&crate::ProcessFileTree>,
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
            descriptor.is_some(),
            SkillResourceKind::Reference,
            entry,
        );
    }
    for entry in &disclosure.level3.scripts {
        add_declared_resource_if_referenced(
            &mut resources,
            level2_body,
            base_dir,
            descriptor.is_some(),
            SkillResourceKind::Script,
            entry,
        );
    }
    for entry in &disclosure.level3.assets {
        add_declared_resource_if_referenced(
            &mut resources,
            level2_body,
            base_dir,
            descriptor.is_some(),
            SkillResourceKind::Asset,
            entry,
        );
    }

    for reference in extract_resource_references(level2_body) {
        add_prefixed_resource(&mut resources, base_dir, descriptor.is_some(), &reference);
    }

    materialize_disclosed_resources(resources.into_values(), |path| {
        load_disclosure_content(
            descriptor,
            base_dir,
            path,
            MAX_DISCLOSED_RESOURCE_BYTES,
            Some(MAX_DISCLOSED_RESOURCE_CHARS),
        )
    })
}

fn add_declared_resource_if_referenced(
    resources: &mut BTreeMap<String, PendingDisclosedSkillResource>,
    level2_body: &str,
    base_dir: &Path,
    descriptor_path: bool,
    kind: SkillResourceKind,
    entry: &str,
) {
    let Some((display_path, path)) = resolve_resource_entry(base_dir, descriptor_path, kind, entry)
    else {
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
    descriptor_path: bool,
    entry: &str,
) {
    let Some((kind, display_path, path)) =
        resolve_prefixed_resource_entry(base_dir, descriptor_path, entry)
    else {
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
    descriptor_path: bool,
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
    let path = resolve_relative_under_root(base_dir, &relative, descriptor_path)?;
    Some((display_path, path))
}

fn resolve_prefixed_resource_entry(
    base_dir: &Path,
    descriptor_path: bool,
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
    let path = resolve_relative_under_root(base_dir, &relative, descriptor_path)?;
    Some((kind, display_path, path))
}

fn resolve_relative_path(
    base_dir: &Path,
    entry: &str,
    descriptor_path: bool,
) -> Option<(String, PathBuf)> {
    let relative = sanitize_relative_path(entry)?;
    let display_path = relative_display_path(&relative);
    let path = resolve_relative_under_root(base_dir, &relative, descriptor_path)?;
    Some((display_path, path))
}

fn resolve_relative_under_root(
    root: &Path,
    relative: &Path,
    descriptor_path: bool,
) -> Option<PathBuf> {
    let candidate = root.join(relative);
    if descriptor_path {
        return Some(candidate);
    }
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
fn load_disclosure_content(
    descriptor: Option<&crate::ProcessFileTree>,
    base_dir: &Path,
    path: &Path,
    max_bytes: u64,
    max_chars: Option<usize>,
) -> Option<String> {
    match descriptor {
        Some(descriptor) => {
            load_descriptor_content(descriptor, base_dir, path, max_bytes, max_chars)
        }
        None => load_disclosed_text_content(path, max_bytes, max_chars),
    }
}

fn load_descriptor_content(
    descriptor: &crate::ProcessFileTree,
    base_dir: &Path,
    path: &Path,
    max_bytes: u64,
    max_chars: Option<usize>,
) -> Option<String> {
    let relative = path.strip_prefix(base_dir).ok()?;
    let relative = relative
        .components()
        .map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?
        .join("/");
    let source = descriptor.bytes(&relative)?;
    let truncated_by_bytes = source.len() as u64 > max_bytes;
    let mut bytes = source[..source.len().min(max_bytes as usize)].to_vec();
    let valid_len = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(error)
            if truncated_by_bytes && error.error_len().is_none() && error.valid_up_to() > 0 =>
        {
            error.valid_up_to()
        }
        Err(_) => return None,
    };
    bytes.truncate(valid_len);
    Some(truncate_disclosed_text_content(
        String::from_utf8(bytes).ok()?,
        truncated_by_bytes,
        max_bytes,
        max_chars,
    ))
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
                lines.push("  note: delegated children receive descriptors and inherited mounts; pass only an Alan OS `cwd` already present in the child namespace".to_string());
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
#[path = "injector_prompt_tests.rs"]
mod prompt_tests;

#[cfg(test)]
#[path = "injector_resource_tests.rs"]
mod resource_tests;

#[cfg(test)]
#[path = "injector_message_tests.rs"]
mod message_tests;
