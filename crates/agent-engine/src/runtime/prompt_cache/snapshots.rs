use crate::prompts;
use crate::skills::{
    ActiveSkillEnvelope, PromptTrackedPath, PromptTrackedPathFingerprint, ResolvedCapabilityView,
    Skill, SkillActivationReason, SkillHostCapabilities, SkillMetadata, SkillOverride,
    SkillsRegistry, extract_mentions, format_skill_availability_issues,
    render_active_skill_prompt_for_runtime, render_skill_not_found, render_skill_unavailable,
    render_skill_unavailable_with_remediation, render_skills_list, skill_availability_issues,
    skill_remediation_from_issues,
};
use crate::tape::{ContentPart, parts_to_text};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::Metadata;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::warn;

#[derive(Debug, Clone)]
pub(super) struct RenderedDomainPrompt {
    pub(super) prompt: String,
    pub(super) active_skills: Vec<ActiveSkillEnvelope>,
    pub(super) cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PathFingerprint {
    path: PathBuf,
    pub(super) content_fingerprint_mode: ContentFingerprintMode,
    state: PathState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathState {
    Missing,
    File(MetadataFingerprint),
    Directory(MetadataFingerprint),
    Other(MetadataFingerprint),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MetadataFingerprint {
    modified: Option<SystemTime>,
    len: u64,
    content_digest: Option<[u8; 32]>,
    platform: PlatformFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlatformFingerprint {
    #[cfg(unix)]
    device_id: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    change_secs: i64,
    #[cfg(unix)]
    change_nanos: i64,
    #[cfg(not(unix))]
    readonly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContentFingerprintMode {
    MetadataOnly,
    FullFile,
    PrefixBytes(u64),
}

// Prompt cache validation runs every turn, so prefix fingerprints avoid
// re-hashing large prompt inputs like append-only daily notes on every build.
pub(super) const WORKSPACE_PERSONA_TRACKED_PREFIX_BYTES: u64 = 16 * 1024;
pub(super) const WORKSPACE_MEMORY_TRACKED_PREFIX_BYTES: u64 = 16 * 1024;

impl PlatformFingerprint {
    fn capture(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            Self {
                device_id: metadata.dev(),
                inode: metadata.ino(),
                change_secs: metadata.ctime(),
                change_nanos: metadata.ctime_nsec(),
            }
        }

        #[cfg(not(unix))]
        {
            Self {
                readonly: metadata.permissions().readonly(),
            }
        }
    }
}

impl MetadataFingerprint {
    fn capture(path: &Path, metadata: &Metadata, mode: ContentFingerprintMode) -> Self {
        Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
            content_digest: match mode {
                ContentFingerprintMode::MetadataOnly => None,
                ContentFingerprintMode::FullFile => hash_file_contents(path, None),
                ContentFingerprintMode::PrefixBytes(max_bytes) => {
                    hash_file_contents(path, Some(max_bytes))
                }
            },
            platform: PlatformFingerprint::capture(metadata),
        }
    }
}

impl PathFingerprint {
    fn capture(path: impl Into<PathBuf>) -> Self {
        Self::capture_with_mode(path, ContentFingerprintMode::FullFile)
    }

    fn capture_prompt_path(tracked_path: PromptTrackedPath) -> Self {
        Self::capture_with_mode(tracked_path.path, tracked_path.fingerprint.into())
    }

    fn capture_with_mode(
        path: impl Into<PathBuf>,
        content_fingerprint_mode: ContentFingerprintMode,
    ) -> Self {
        let path = path.into();
        let state =
            match std::fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => PathState::File(
                    MetadataFingerprint::capture(&path, &metadata, content_fingerprint_mode),
                ),
                Ok(metadata) if metadata.is_dir() => {
                    PathState::Directory(MetadataFingerprint::capture(
                        &path,
                        &metadata,
                        ContentFingerprintMode::MetadataOnly,
                    ))
                }
                Ok(metadata) => PathState::Other(MetadataFingerprint::capture(
                    &path,
                    &metadata,
                    ContentFingerprintMode::MetadataOnly,
                )),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => PathState::Missing,
                Err(_) => PathState::Missing,
            };
        Self {
            path,
            content_fingerprint_mode,
            state,
        }
    }

    fn matches_current(&self) -> bool {
        Self::capture_with_mode(self.path.clone(), self.content_fingerprint_mode) == *self
    }
}

impl From<PromptTrackedPathFingerprint> for ContentFingerprintMode {
    fn from(value: PromptTrackedPathFingerprint) -> Self {
        match value {
            PromptTrackedPathFingerprint::PrefixBytes(max_bytes) => Self::PrefixBytes(max_bytes),
        }
    }
}

pub(super) fn hash_file_contents(path: &Path, max_bytes: Option<u64>) -> Option<[u8; 32]> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8 * 1024];
    let mut remaining = max_bytes;
    loop {
        let slice_len = remaining
            .map(|bytes| bytes.min(buffer.len() as u64) as usize)
            .unwrap_or(buffer.len());
        if slice_len == 0 {
            break;
        }
        let bytes_read = file.read(&mut buffer[..slice_len]).ok()?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        if let Some(bytes_left) = remaining.as_mut() {
            *bytes_left = bytes_left.saturating_sub(bytes_read as u64);
        }
    }
    Some(hasher.finalize().into())
}

#[derive(Debug, Clone)]
pub(super) struct CachedDefinitionPersona {
    pub(super) tracked_paths: Vec<PathFingerprint>,
    pub(super) rendered_section: String,
}

impl CachedDefinitionPersona {
    pub(super) fn load(definition_persona_dirs: &[PathBuf]) -> Self {
        let tracked_paths =
            prompts::definition_persona_tracked_paths_from_dirs(definition_persona_dirs)
                .into_iter()
                .map(|path| {
                    PathFingerprint::capture_with_mode(
                        path,
                        ContentFingerprintMode::PrefixBytes(WORKSPACE_PERSONA_TRACKED_PREFIX_BYTES),
                    )
                })
                .collect();
        let rendered_section =
            prompts::render_definition_persona_context_from_dirs(definition_persona_dirs);
        Self {
            tracked_paths,
            rendered_section,
        }
    }

    pub(super) fn is_current(&self) -> bool {
        self.tracked_paths
            .iter()
            .all(PathFingerprint::matches_current)
    }
}

#[derive(Debug, Clone)]
pub(super) struct CachedMemoryStore {
    pub(super) memory_dir: PathBuf,
    pub(super) tracked_paths: Vec<PathFingerprint>,
    pub(super) rendered_section: String,
}

impl CachedMemoryStore {
    pub(super) fn load(memory_dir: &Path) -> Self {
        let tracked_paths = prompts::memory_store_tracked_paths(memory_dir)
            .into_iter()
            .map(|path| {
                PathFingerprint::capture_with_mode(
                    path,
                    ContentFingerprintMode::PrefixBytes(WORKSPACE_MEMORY_TRACKED_PREFIX_BYTES),
                )
            })
            .collect();
        let rendered_section = prompts::render_memory_store_context(memory_dir);
        Self {
            memory_dir: memory_dir.to_path_buf(),
            tracked_paths,
            rendered_section,
        }
    }

    pub(super) fn is_current_for(&self, memory_dir: &Path) -> bool {
        self.memory_dir == memory_dir
            && self
                .tracked_paths
                .iter()
                .all(PathFingerprint::matches_current)
    }
}

#[derive(Debug, Clone)]
struct CachedSkillRender {
    tracked_paths: Vec<PathFingerprint>,
    rendered: String,
}

impl CachedSkillRender {
    fn load(
        skill: &Skill,
        envelope: &ActiveSkillEnvelope,
        delegated_invocation_available: bool,
    ) -> Self {
        let rendered =
            render_active_skill_prompt_for_runtime(skill, envelope, delegated_invocation_available);
        let tracked_paths = rendered
            .tracked_paths
            .into_iter()
            .map(PathFingerprint::capture_prompt_path)
            .collect();
        Self {
            tracked_paths,
            rendered: rendered.rendered,
        }
    }

    pub(super) fn is_current(&self) -> bool {
        self.tracked_paths
            .iter()
            .all(PathFingerprint::matches_current)
    }
}

#[derive(Clone)]
pub(super) struct CachedSkillsRegistry {
    registry: SkillsRegistry,
    tracked_paths: Vec<PathFingerprint>,
    pub(super) listed_skills: Vec<SkillMetadata>,
    pub(super) mentionable_skill_ids: BTreeSet<String>,
    pub(super) unavailable_skill_messages: HashMap<String, String>,
    skills_list: Option<String>,
    active_skill_cache: HashMap<String, CachedSkillRender>,
    host_capabilities: SkillHostCapabilities,
    delegated_invocation_available: bool,
}

impl CachedSkillsRegistry {
    pub(super) fn load_capability_view(
        capability_view: &ResolvedCapabilityView,
        skill_overrides: &[SkillOverride],
        host_capabilities: &SkillHostCapabilities,
    ) -> Result<Self, crate::skills::SkillsError> {
        let registry = SkillsRegistry::load_capability_view(capability_view, skill_overrides)?;
        Self::from_registry(registry, host_capabilities)
    }

    fn from_registry(
        registry: SkillsRegistry,
        host_capabilities: &SkillHostCapabilities,
    ) -> Result<Self, crate::skills::SkillsError> {
        let mut listed_skills = Vec::new();
        let mut mentionable_skill_ids = BTreeSet::new();
        let mut unavailable_skill_messages = HashMap::new();

        for skill in registry.list_sorted().into_iter().cloned() {
            if !skill.enabled {
                continue;
            }
            let availability_issues = skill_availability_issues(&skill, host_capabilities);
            if !availability_issues.is_empty() {
                let message = skill_remediation_from_issues(&skill, &availability_issues)
                    .map(|remediation| {
                        render_skill_unavailable_with_remediation(&skill.id, &remediation)
                    })
                    .unwrap_or_else(|| {
                        render_skill_unavailable(
                            &skill.id,
                            &format_skill_availability_issues(&availability_issues),
                        )
                    });
                unavailable_skill_messages.insert(skill.id.clone(), message.clone());
                continue;
            }
            if skill.allow_implicit_invocation {
                listed_skills.push(skill.clone());
            }
            mentionable_skill_ids.insert(skill.id.clone());
        }

        let delegated_invocation_available =
            host_capabilities.supports_delegated_skill_invocation();
        let skills_list = render_skills_list(&listed_skills, delegated_invocation_available);
        let tracked_paths = registry
            .tracked_paths()
            .iter()
            .cloned()
            .map(PathFingerprint::capture)
            .collect();
        Ok(Self {
            registry,
            tracked_paths,
            listed_skills,
            mentionable_skill_ids,
            unavailable_skill_messages,
            skills_list,
            active_skill_cache: HashMap::new(),
            host_capabilities: host_capabilities.clone(),
            delegated_invocation_available,
        })
    }

    pub(super) fn is_current(&self) -> bool {
        self.tracked_paths
            .iter()
            .all(PathFingerprint::matches_current)
    }

    fn resolve_explicit_mention(&self, mention: &str) -> Option<String> {
        self.mentionable_skill_ids
            .contains(mention)
            .then(|| mention.to_string())
    }

    pub(super) fn listed_skill_metadata(&self, skill_id: &str) -> Option<SkillMetadata> {
        self.listed_skills
            .iter()
            .find(|skill| skill.id == skill_id)
            .cloned()
    }

    fn select_active_skills_from_input(
        &self,
        user_input: Option<&[ContentPart]>,
    ) -> (BTreeMap<String, ActiveSkillEnvelope>, Vec<String>) {
        let mention_text = user_input.map(parts_to_text).unwrap_or_default();
        let mentioned_ids = extract_mentions(&mention_text);

        let mut active_reasons = BTreeMap::new();
        for mention in &mentioned_ids {
            if let Some(skill_id) = self.resolve_explicit_mention(mention) {
                active_reasons.insert(
                    skill_id,
                    SkillActivationReason::ExplicitMention {
                        mention: mention.clone(),
                    },
                );
            }
        }

        let mut selected_skills = BTreeMap::new();
        for (skill_id, activation_reason) in active_reasons {
            let Some(metadata) = self.registry.get(&skill_id).cloned() else {
                continue;
            };
            selected_skills.insert(
                skill_id,
                ActiveSkillEnvelope::available(metadata, activation_reason),
            );
        }

        let mut unresolved_mentions = Vec::new();
        for mention in mentioned_ids {
            if self.resolve_explicit_mention(&mention).is_none() {
                if let Some(message) = self.unavailable_skill_messages.get(&mention) {
                    unresolved_mentions.push(message.clone());
                } else {
                    unresolved_mentions.push(render_skill_not_found(&mention, &self.listed_skills));
                }
            }
        }

        (selected_skills, unresolved_mentions)
    }

    pub(super) fn render_domain_prompt(
        &mut self,
        user_input: Option<&[ContentPart]>,
    ) -> RenderedDomainPrompt {
        if !self.registry.errors().is_empty() {
            warn!(
                errors = self.registry.errors().len(),
                "Loaded skills with non-fatal parse/scan errors"
            );
        }

        let mut sections = Vec::new();
        let mut active_skills = Vec::new();
        if let Some(skills_list) = &self.skills_list {
            sections.push(skills_list.clone());
        }

        let (selected_skills, unresolved_mentions) =
            self.select_active_skills_from_input(user_input);
        let mut active_skill_cache_hit = true;

        let mut active_sections = Vec::new();
        for envelope in selected_skills.into_values() {
            match self.render_active_skill(&envelope) {
                Ok((rendered, cache_hit)) => {
                    active_skills.push(envelope);
                    active_sections.push(rendered);
                    active_skill_cache_hit &= cache_hit;
                }
                Err(err) => {
                    warn!(
                        skill_id = %envelope.metadata.id,
                        error = %err,
                        "Failed to load active skill"
                    );
                    active_skill_cache_hit = false;
                }
            }
        }

        if !active_sections.is_empty() {
            sections.push(
                "## Active Skill Instructions\nFollow these active skill instructions when relevant."
                    .to_string(),
            );
            sections.push(active_sections.join("\n\n"));
        }

        for unresolved in unresolved_mentions {
            sections.push(unresolved);
        }

        RenderedDomainPrompt {
            prompt: sections.join("\n\n"),
            active_skills,
            cache_hit: active_skill_cache_hit,
        }
    }

    pub(super) fn render_domain_prompt_for_active_skills(
        &mut self,
        active_skills: &[ActiveSkillEnvelope],
        user_input: Option<&[ContentPart]>,
    ) -> RenderedDomainPrompt {
        if !self.registry.errors().is_empty() {
            warn!(
                errors = self.registry.errors().len(),
                "Loaded skills with non-fatal parse/scan errors"
            );
        }

        let mut sections = Vec::new();
        if let Some(skills_list) = &self.skills_list {
            sections.push(skills_list.clone());
        }

        let (selected_skills, unresolved_mentions) =
            self.select_active_skills_from_input(user_input);
        let mut merged_active_skills = BTreeMap::new();
        let mut revalidation_messages = Vec::new();
        for envelope in active_skills {
            match self.refresh_active_skill_envelope(envelope) {
                RefreshedActiveSkill::Active(refreshed) => {
                    merged_active_skills.insert(refreshed.metadata.id.clone(), *refreshed);
                }
                RefreshedActiveSkill::Message(message) => {
                    push_unique_message(&mut revalidation_messages, message);
                }
            }
        }
        for (skill_id, envelope) in selected_skills {
            merged_active_skills.entry(skill_id).or_insert(envelope);
        }

        let mut resolved_active_skills = Vec::new();
        let mut active_sections = Vec::new();
        let mut active_skill_cache_hit = true;
        for envelope in merged_active_skills.into_values() {
            match self.render_active_skill(&envelope) {
                Ok((rendered, cache_hit)) => {
                    resolved_active_skills.push(envelope.clone());
                    active_sections.push(rendered);
                    active_skill_cache_hit &= cache_hit;
                }
                Err(err) => {
                    warn!(
                        skill_id = %envelope.metadata.id,
                        error = %err,
                        "Failed to load resumed active skill"
                    );
                    active_skill_cache_hit = false;
                }
            }
        }

        if !active_sections.is_empty() {
            sections.push(
                "## Active Skill Instructions\nFollow these active skill instructions when relevant."
                    .to_string(),
            );
            sections.push(active_sections.join("\n\n"));
        }

        for message in revalidation_messages {
            sections.push(message);
        }
        for unresolved in unresolved_mentions {
            sections.push(unresolved);
        }

        RenderedDomainPrompt {
            prompt: sections.join("\n\n"),
            active_skills: resolved_active_skills,
            cache_hit: active_skill_cache_hit,
        }
    }

    fn render_active_skill(
        &mut self,
        envelope: &ActiveSkillEnvelope,
    ) -> Result<(String, bool), crate::skills::SkillsError> {
        let cache_key = envelope.cache_key();
        if let Some(cached) = self.active_skill_cache.get(&cache_key)
            && cached.is_current()
        {
            return Ok((cached.rendered.clone(), true));
        }

        let skill = self.registry.load_skill(&envelope.metadata.id)?;
        let cached = CachedSkillRender::load(&skill, envelope, self.delegated_invocation_available);
        let rendered = cached.rendered.clone();
        self.active_skill_cache.insert(cache_key, cached);
        Ok((rendered, false))
    }

    fn refresh_active_skill_envelope(
        &self,
        envelope: &ActiveSkillEnvelope,
    ) -> RefreshedActiveSkill {
        let skill_id = &envelope.metadata.id;
        let Some(metadata) = self.registry.get(skill_id).cloned() else {
            return RefreshedActiveSkill::Message(render_skill_not_found(
                skill_id,
                &self.listed_skills,
            ));
        };
        if !metadata.enabled {
            return RefreshedActiveSkill::Message(render_skill_not_found(
                skill_id,
                &self.listed_skills,
            ));
        }

        let availability_issues = skill_availability_issues(&metadata, &self.host_capabilities);
        if !availability_issues.is_empty() {
            let message = skill_remediation_from_issues(&metadata, &availability_issues)
                .map(|remediation| {
                    render_skill_unavailable_with_remediation(skill_id, &remediation)
                })
                .unwrap_or_else(|| {
                    render_skill_unavailable(
                        skill_id,
                        &format_skill_availability_issues(&availability_issues),
                    )
                });
            return RefreshedActiveSkill::Message(message);
        }

        RefreshedActiveSkill::Active(Box::new(ActiveSkillEnvelope::available(
            metadata,
            envelope.activation_reason.clone(),
        )))
    }
}

enum RefreshedActiveSkill {
    Active(Box<ActiveSkillEnvelope>),
    Message(String),
}

fn push_unique_message(messages: &mut Vec<String>, message: String) {
    if !messages.iter().any(|existing| existing == &message) {
        messages.push(message);
    }
}
