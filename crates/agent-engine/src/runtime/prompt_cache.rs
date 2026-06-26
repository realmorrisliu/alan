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
use std::time::{Instant, SystemTime};
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PromptAssemblyMetrics {
    pub builds: u64,
    pub hits: u64,
    pub misses: u64,
    pub skills_hits: u64,
    pub skills_misses: u64,
    pub persona_hits: u64,
    pub persona_misses: u64,
}

impl PromptAssemblyMetrics {
    fn record_build(&mut self, skills_hit: bool, persona_hit: bool) {
        self.builds += 1;
        if skills_hit && persona_hit {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        if skills_hit {
            self.skills_hits += 1;
        } else {
            self.skills_misses += 1;
        }
        if persona_hit {
            self.persona_hits += 1;
        } else {
            self.persona_misses += 1;
        }
    }

    fn hit_ratio(&self) -> f64 {
        if self.builds == 0 {
            0.0
        } else {
            self.hits as f64 / self.builds as f64
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PromptAssemblyResult {
    pub domain_prompt: String,
    pub system_prompt: String,
    pub active_skills: Vec<ActiveSkillEnvelope>,
    pub metrics: PromptAssemblyMetrics,
    pub elapsed_ms: u128,
    pub skills_cache_hit: bool,
    pub persona_cache_hit: bool,
}

#[derive(Debug, Clone)]
struct RenderedDomainPrompt {
    prompt: String,
    active_skills: Vec<ActiveSkillEnvelope>,
    cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathFingerprint {
    path: PathBuf,
    content_fingerprint_mode: ContentFingerprintMode,
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
enum ContentFingerprintMode {
    MetadataOnly,
    FullFile,
    PrefixBytes(u64),
}

// Prompt cache validation runs every turn, so prefix fingerprints avoid
// re-hashing large prompt inputs like append-only daily notes on every build.
const WORKSPACE_PERSONA_TRACKED_PREFIX_BYTES: u64 = 16 * 1024;
const WORKSPACE_MEMORY_TRACKED_PREFIX_BYTES: u64 = 16 * 1024;

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

fn hash_file_contents(path: &Path, max_bytes: Option<u64>) -> Option<[u8; 32]> {
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
struct CachedWorkspacePersona {
    tracked_paths: Vec<PathFingerprint>,
    rendered_section: String,
}

impl CachedWorkspacePersona {
    fn load(workspace_persona_dirs: &[PathBuf]) -> Self {
        let tracked_paths =
            prompts::workspace_persona_tracked_paths_from_dirs(workspace_persona_dirs)
                .into_iter()
                .map(|path| {
                    PathFingerprint::capture_with_mode(
                        path,
                        ContentFingerprintMode::PrefixBytes(WORKSPACE_PERSONA_TRACKED_PREFIX_BYTES),
                    )
                })
                .collect();
        let rendered_section =
            prompts::render_workspace_persona_context_from_dirs(workspace_persona_dirs);
        Self {
            tracked_paths,
            rendered_section,
        }
    }

    fn is_current(&self) -> bool {
        self.tracked_paths
            .iter()
            .all(PathFingerprint::matches_current)
    }
}

#[derive(Debug, Clone)]
struct CachedWorkspaceMemory {
    memory_dir: PathBuf,
    tracked_paths: Vec<PathFingerprint>,
    rendered_section: String,
}

impl CachedWorkspaceMemory {
    fn load(memory_dir: &Path) -> Self {
        let tracked_paths = prompts::workspace_memory_tracked_paths(memory_dir)
            .into_iter()
            .map(|path| {
                PathFingerprint::capture_with_mode(
                    path,
                    ContentFingerprintMode::PrefixBytes(WORKSPACE_MEMORY_TRACKED_PREFIX_BYTES),
                )
            })
            .collect();
        let rendered_section = prompts::render_workspace_memory_context(memory_dir);
        Self {
            memory_dir: memory_dir.to_path_buf(),
            tracked_paths,
            rendered_section,
        }
    }

    fn is_current_for(&self, memory_dir: &Path) -> bool {
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

    fn is_current(&self) -> bool {
        self.tracked_paths
            .iter()
            .all(PathFingerprint::matches_current)
    }
}

#[derive(Clone)]
struct CachedSkillsRegistry {
    registry: SkillsRegistry,
    tracked_paths: Vec<PathFingerprint>,
    listed_skills: Vec<SkillMetadata>,
    mentionable_skill_ids: BTreeSet<String>,
    unavailable_skill_messages: HashMap<String, String>,
    skills_list: Option<String>,
    active_skill_cache: HashMap<String, CachedSkillRender>,
    host_capabilities: SkillHostCapabilities,
    delegated_invocation_available: bool,
}

impl CachedSkillsRegistry {
    fn load_capability_view(
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

    fn is_current(&self) -> bool {
        self.tracked_paths
            .iter()
            .all(PathFingerprint::matches_current)
    }

    fn resolve_explicit_mention(&self, mention: &str) -> Option<String> {
        self.mentionable_skill_ids
            .contains(mention)
            .then(|| mention.to_string())
    }

    fn listed_skill_metadata(&self, skill_id: &str) -> Option<SkillMetadata> {
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

    fn render_domain_prompt(&mut self, user_input: Option<&[ContentPart]>) -> RenderedDomainPrompt {
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

    fn render_domain_prompt_for_active_skills(
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

pub(crate) struct PromptAssemblyCache {
    fixed_capability_view: Option<ResolvedCapabilityView>,
    skill_overrides: Vec<SkillOverride>,
    workspace_persona_dirs: Vec<PathBuf>,
    workspace_memory_dir: Option<PathBuf>,
    host_capabilities: SkillHostCapabilities,
    skills_snapshot: Option<CachedSkillsRegistry>,
    workspace_persona_snapshot: Option<CachedWorkspacePersona>,
    workspace_memory_snapshot: Option<CachedWorkspaceMemory>,
    metrics: PromptAssemblyMetrics,
}

impl PromptAssemblyCache {
    #[cfg(test)]
    pub(crate) fn new(workspace_persona_dirs: Vec<PathBuf>) -> Self {
        Self {
            fixed_capability_view: None,
            skill_overrides: Vec::new(),
            workspace_persona_dirs,
            workspace_memory_dir: None,
            host_capabilities: SkillHostCapabilities::default(),
            skills_snapshot: None,
            workspace_persona_snapshot: None,
            workspace_memory_snapshot: None,
            metrics: PromptAssemblyMetrics::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_fixed_capability_view(
        fixed_capability_view: ResolvedCapabilityView,
        workspace_persona_dirs: Vec<PathBuf>,
        host_capabilities: SkillHostCapabilities,
    ) -> Self {
        Self::with_fixed_capability_view_and_overrides(
            fixed_capability_view,
            Vec::new(),
            workspace_persona_dirs,
            host_capabilities,
        )
    }

    pub(crate) fn with_fixed_capability_view_and_overrides(
        fixed_capability_view: ResolvedCapabilityView,
        skill_overrides: Vec<SkillOverride>,
        workspace_persona_dirs: Vec<PathBuf>,
        host_capabilities: SkillHostCapabilities,
    ) -> Self {
        Self {
            fixed_capability_view: Some(fixed_capability_view),
            skill_overrides,
            workspace_persona_dirs,
            workspace_memory_dir: None,
            host_capabilities,
            skills_snapshot: None,
            workspace_persona_snapshot: None,
            workspace_memory_snapshot: None,
            metrics: PromptAssemblyMetrics::default(),
        }
    }

    pub(crate) fn rebind_paths(&mut self, workspace_persona_dirs: Vec<PathBuf>) {
        if self.workspace_persona_dirs != workspace_persona_dirs {
            self.workspace_persona_dirs = workspace_persona_dirs;
            self.workspace_persona_snapshot = None;
        }
    }

    pub(crate) fn set_workspace_memory_dir(&mut self, workspace_memory_dir: Option<PathBuf>) {
        if self.workspace_memory_dir != workspace_memory_dir {
            self.workspace_memory_dir = workspace_memory_dir;
            self.workspace_memory_snapshot = None;
        }
    }

    pub(crate) fn set_host_capabilities(&mut self, host_capabilities: SkillHostCapabilities) {
        if self.host_capabilities != host_capabilities {
            self.host_capabilities = host_capabilities;
            self.skills_snapshot = None;
        }
    }

    pub(crate) fn supports_delegated_skill_invocation(&self) -> bool {
        self.host_capabilities.supports_delegated_skill_invocation()
    }

    pub(crate) fn capability_view(&self) -> Option<&ResolvedCapabilityView> {
        self.fixed_capability_view.as_ref()
    }

    pub(crate) fn resolve_listed_skill_metadata(
        &mut self,
        skill_id: &str,
    ) -> Result<Option<SkillMetadata>, crate::skills::SkillsError> {
        if self.fixed_capability_view.is_none() {
            return Ok(None);
        }

        self.ensure_skills_snapshot()?;
        Ok(self
            .skills_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.listed_skill_metadata(skill_id)))
    }

    pub(crate) fn build(&mut self, user_input: Option<&[ContentPart]>) -> PromptAssemblyResult {
        let started_at = Instant::now();
        let (domain_prompt, active_skills, skills_cache_hit) =
            self.domain_prompt_with_cache(user_input);
        let (workspace_section, persona_cache_hit) = self.workspace_section_with_cache();
        let system_prompt = prompts::build_agent_system_prompt_with_workspace_sections(
            &domain_prompt,
            workspace_section.workspace_persona_section.as_deref(),
            workspace_section.workspace_memory_section.as_deref(),
        );

        self.metrics
            .record_build(skills_cache_hit, persona_cache_hit);
        let elapsed_ms = started_at.elapsed().as_millis();
        debug!(
            elapsed_ms,
            skills_cache_hit,
            persona_cache_hit,
            builds = self.metrics.builds,
            hit_ratio = self.metrics.hit_ratio(),
            "Prompt assembly completed"
        );

        PromptAssemblyResult {
            domain_prompt,
            system_prompt,
            active_skills,
            metrics: self.metrics,
            elapsed_ms,
            skills_cache_hit,
            persona_cache_hit,
        }
    }

    pub(crate) fn build_with_active_skills(
        &mut self,
        active_skills: &[ActiveSkillEnvelope],
        user_input: Option<&[ContentPart]>,
    ) -> PromptAssemblyResult {
        let started_at = Instant::now();
        let (domain_prompt, active_skills, skills_cache_hit) =
            self.domain_prompt_with_active_skills_cache(active_skills, user_input);
        let (workspace_section, persona_cache_hit) = self.workspace_section_with_cache();
        let system_prompt = prompts::build_agent_system_prompt_with_workspace_sections(
            &domain_prompt,
            workspace_section.workspace_persona_section.as_deref(),
            workspace_section.workspace_memory_section.as_deref(),
        );

        self.metrics
            .record_build(skills_cache_hit, persona_cache_hit);
        let elapsed_ms = started_at.elapsed().as_millis();
        debug!(
            elapsed_ms,
            skills_cache_hit,
            persona_cache_hit,
            builds = self.metrics.builds,
            hit_ratio = self.metrics.hit_ratio(),
            "Prompt assembly completed"
        );

        PromptAssemblyResult {
            domain_prompt,
            system_prompt,
            active_skills,
            metrics: self.metrics,
            elapsed_ms,
            skills_cache_hit,
            persona_cache_hit,
        }
    }

    fn domain_prompt_with_cache(
        &mut self,
        user_input: Option<&[ContentPart]>,
    ) -> (String, Vec<ActiveSkillEnvelope>, bool) {
        if self.fixed_capability_view.is_none() {
            return (String::new(), Vec::new(), true);
        }

        let cache_hit = match self.ensure_skills_snapshot() {
            Ok(cache_hit) => cache_hit,
            Err(err) => {
                let path = self
                    .fixed_capability_view
                    .as_ref()
                    .and_then(|capability_view| capability_view.package_dirs.first())
                    .map(|dir| dir.path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                warn!(path = %path, error = %err, "Failed to load skills registry; continuing without skill injection");
                self.skills_snapshot = None;
                return (String::new(), Vec::new(), false);
            }
        };

        let rendered = self
            .skills_snapshot
            .as_mut()
            .map(|snapshot| snapshot.render_domain_prompt(user_input))
            .unwrap_or_else(|| RenderedDomainPrompt {
                prompt: String::new(),
                active_skills: Vec::new(),
                cache_hit: true,
            });
        (
            rendered.prompt,
            rendered.active_skills,
            cache_hit && rendered.cache_hit,
        )
    }

    fn domain_prompt_with_active_skills_cache(
        &mut self,
        active_skills: &[ActiveSkillEnvelope],
        user_input: Option<&[ContentPart]>,
    ) -> (String, Vec<ActiveSkillEnvelope>, bool) {
        if self.fixed_capability_view.is_none() {
            return (String::new(), Vec::new(), true);
        }

        let cache_hit = match self.ensure_skills_snapshot() {
            Ok(cache_hit) => cache_hit,
            Err(err) => {
                let path = self
                    .fixed_capability_view
                    .as_ref()
                    .and_then(|capability_view| capability_view.package_dirs.first())
                    .map(|dir| dir.path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                warn!(path = %path, error = %err, "Failed to load skills registry; continuing without skill injection");
                self.skills_snapshot = None;
                return (String::new(), Vec::new(), false);
            }
        };

        let rendered = self
            .skills_snapshot
            .as_mut()
            .map(|snapshot| {
                snapshot.render_domain_prompt_for_active_skills(active_skills, user_input)
            })
            .unwrap_or_else(|| RenderedDomainPrompt {
                prompt: String::new(),
                active_skills: Vec::new(),
                cache_hit: true,
            });
        (
            rendered.prompt,
            rendered.active_skills,
            cache_hit && rendered.cache_hit,
        )
    }

    fn ensure_skills_snapshot(&mut self) -> Result<bool, crate::skills::SkillsError> {
        let Some(capability_view) = self.fixed_capability_view.as_ref() else {
            return Ok(true);
        };

        let cache_hit = self
            .skills_snapshot
            .as_ref()
            .is_some_and(CachedSkillsRegistry::is_current);
        if !cache_hit {
            self.skills_snapshot = Some(CachedSkillsRegistry::load_capability_view(
                capability_view,
                &self.skill_overrides,
                &self.host_capabilities,
            )?);
        }

        Ok(cache_hit)
    }

    fn workspace_section_with_cache(&mut self) -> (WorkspaceSectionResult, bool) {
        let (workspace_persona_section, persona_cache_hit) =
            self.workspace_persona_section_with_cache();
        let (workspace_memory_section, memory_cache_hit) =
            self.workspace_memory_section_with_cache();
        (
            WorkspaceSectionResult {
                workspace_persona_section,
                workspace_memory_section,
            },
            persona_cache_hit && memory_cache_hit,
        )
    }

    fn workspace_persona_section_with_cache(&mut self) -> (Option<String>, bool) {
        if self.workspace_persona_dirs.is_empty() {
            return (None, true);
        }
        if !self.workspace_persona_dirs.iter().any(|dir| dir.exists()) {
            self.workspace_persona_snapshot = None;
            return (None, true);
        }

        let cache_hit = self
            .workspace_persona_snapshot
            .as_ref()
            .is_some_and(CachedWorkspacePersona::is_current);
        if !cache_hit {
            self.workspace_persona_snapshot =
                Some(CachedWorkspacePersona::load(&self.workspace_persona_dirs));
        }

        let rendered = self
            .workspace_persona_snapshot
            .as_ref()
            .map(|snapshot| snapshot.rendered_section.clone())
            .filter(|section| !section.is_empty());
        (rendered, cache_hit)
    }

    fn workspace_memory_section_with_cache(&mut self) -> (Option<String>, bool) {
        let Some(memory_dir) = self.workspace_memory_dir.as_deref() else {
            self.workspace_memory_snapshot = None;
            return (None, true);
        };
        if !memory_dir.exists() {
            self.workspace_memory_snapshot = None;
            return (None, true);
        }

        let cache_hit = self
            .workspace_memory_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.is_current_for(memory_dir));
        if !cache_hit {
            self.workspace_memory_snapshot = Some(CachedWorkspaceMemory::load(memory_dir));
        }

        let rendered = self
            .workspace_memory_snapshot
            .as_ref()
            .map(|snapshot| snapshot.rendered_section.clone())
            .filter(|section| !section.is_empty());
        (rendered, cache_hit)
    }
}

#[derive(Debug, Clone, Default)]
struct WorkspaceSectionResult {
    workspace_persona_section: Option<String>,
    workspace_memory_section: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::{ensure_workspace_bootstrap_files_at, ensure_workspace_memory_layout_at};
    use crate::skills::{
        ResolvedSkillExecution, ScopedPackageDir, SkillExecutionResolutionSource,
        SkillHostCapabilities, SkillOverride, SkillScope,
    };
    use sha2::{Digest, Sha256};

    fn create_repo_skill(
        workspace_root: &std::path::Path,
        dir_name: &str,
        skill_name: &str,
        description: &str,
        body: &str,
    ) {
        let skill_dir = workspace_root
            .join(".alan/agents/default/skills")
            .join(dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {skill_name}
description: {description}
---

{body}
"#
            ),
        )
        .unwrap();
    }

    fn create_repo_skill_with_frontmatter(
        workspace_root: &std::path::Path,
        dir_name: &str,
        frontmatter: &str,
        body: &str,
    ) {
        let skill_dir = workspace_root
            .join(".alan/agents/default/skills")
            .join(dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\n{frontmatter}\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    fn create_repo_child_agent(
        workspace_root: &std::path::Path,
        package_dir: &str,
        agent_name: &str,
    ) {
        let agent_root = workspace_root
            .join(".alan/agents/default/skills")
            .join(package_dir)
            .join("agents")
            .join(agent_name);
        std::fs::create_dir_all(&agent_root).unwrap();
        std::fs::write(
            agent_root.join("agent.toml"),
            "llm_provider = \"openai_responses\"\n",
        )
        .unwrap();
    }

    fn capability_view_for_workspace_root(
        workspace_root: &std::path::Path,
    ) -> ResolvedCapabilityView {
        ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: workspace_root.join(".alan/agents/default/skills"),
            scope: SkillScope::Repo,
        }])
    }

    fn prompt_cache_for_workspace_root(
        workspace_root: &std::path::Path,
        workspace_persona_dirs: Vec<PathBuf>,
    ) -> PromptAssemblyCache {
        PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(workspace_root),
            workspace_persona_dirs,
            SkillHostCapabilities::default(),
        )
    }

    fn prompt_cache_for_workspace_root_with_overrides(
        workspace_root: &std::path::Path,
        skill_overrides: Vec<SkillOverride>,
        workspace_persona_dirs: Vec<PathBuf>,
    ) -> PromptAssemblyCache {
        PromptAssemblyCache::with_fixed_capability_view_and_overrides(
            capability_view_for_workspace_root(workspace_root),
            skill_overrides,
            workspace_persona_dirs,
            SkillHostCapabilities::default(),
        )
    }

    #[test]
    fn prompt_cache_hits_on_repeated_builds() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let persona_dir = workspace_root.join(".alan/agents/default/persona");
        std::fs::create_dir_all(&workspace_root).unwrap();
        ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, vec![persona_dir.clone()]);
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("Workspace Persona Context"));
        assert!(first.system_prompt.contains("## Skill: My Skill"));
        assert!(!first.skills_cache_hit);
        assert!(!first.persona_cache_hit);
        assert!(second.skills_cache_hit);
        assert!(second.persona_cache_hit);
        assert_eq!(second.metrics.builds, 2);
        assert_eq!(second.metrics.hits, 1);
    }

    #[test]
    fn prompt_cache_includes_workspace_memory_bootstrap_when_configured() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let persona_dir = workspace_root.join(".alan/agents/default/persona");
        let memory_dir = workspace_root.join(".alan/memory");
        std::fs::create_dir_all(&workspace_root).unwrap();
        ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("USER.md"), "# User Memory\n- Morris\n").unwrap();

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, vec![persona_dir]);
        cache.set_workspace_memory_dir(Some(memory_dir.clone()));

        let first = cache.build(None);
        let second = cache.build(None);

        assert!(first.system_prompt.contains("Workspace Memory Bootstrap"));
        assert!(
            first
                .system_prompt
                .contains(memory_dir.join("USER.md").to_string_lossy().as_ref())
        );
        assert!(first.system_prompt.contains("# User Memory"));
        assert!(!first.persona_cache_hit);
        assert!(second.persona_cache_hit);
    }

    #[test]
    fn workspace_persona_cache_uses_prefix_fingerprints_for_tracked_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let persona_dir = workspace_root.join(".alan/agents/default/persona");
        std::fs::create_dir_all(&workspace_root).unwrap();
        ensure_workspace_bootstrap_files_at(&persona_dir).unwrap();

        let snapshot = CachedWorkspacePersona::load(&[persona_dir]);

        assert!(!snapshot.tracked_paths.is_empty());
        assert!(snapshot.tracked_paths.iter().all(|fingerprint| {
            fingerprint.content_fingerprint_mode
                == ContentFingerprintMode::PrefixBytes(WORKSPACE_PERSONA_TRACKED_PREFIX_BYTES)
        }));
    }

    #[test]
    fn workspace_memory_cache_uses_prefix_fingerprints_for_tracked_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("daily/2026-04-17.md"),
            "# 2026-04-17\nappended daily note",
        )
        .unwrap();

        let snapshot = CachedWorkspaceMemory::load(&memory_dir);

        assert!(!snapshot.tracked_paths.is_empty());
        assert!(snapshot.tracked_paths.iter().all(|fingerprint| {
            fingerprint.content_fingerprint_mode
                == ContentFingerprintMode::PrefixBytes(WORKSPACE_MEMORY_TRACKED_PREFIX_BYTES)
        }));
    }

    #[test]
    fn hash_file_contents_matches_sha256_for_large_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("large.txt");
        let content = "0123456789abcdef".repeat(16 * 1024);
        std::fs::write(&path, &content).unwrap();

        let digest = hash_file_contents(&path, None).unwrap();
        let mut expected = Sha256::new();
        expected.update(content.as_bytes());

        assert_eq!(digest, <[u8; 32]>::from(expected.finalize()));
    }

    #[test]
    fn hash_file_contents_respects_prefix_limit() {
        let temp = tempfile::TempDir::new().unwrap();
        let first = temp.path().join("first.txt");
        let second = temp.path().join("second.txt");
        std::fs::write(&first, "prefix-one-suffix-a").unwrap();
        std::fs::write(&second, "prefix-one-suffix-b").unwrap();

        assert_eq!(
            hash_file_contents(&first, Some(10)),
            hash_file_contents(&second, Some(10))
        );
        assert_ne!(
            hash_file_contents(&first, None),
            hash_file_contents(&second, None)
        );
    }

    #[test]
    fn prompt_cache_exposes_active_skill_envelopes_with_canonical_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let prompt = cache.build(Some(&user_input));
        assert_eq!(prompt.active_skills.len(), 1);

        let active_skill = &prompt.active_skills[0];
        let expected_root =
            std::fs::canonicalize(workspace_root.join(".alan/agents/default/skills/my-skill"))
                .unwrap();
        assert_eq!(active_skill.metadata.id, "my-skill");
        assert_eq!(
            active_skill.metadata.package_id.as_deref(),
            Some("skill:my-skill")
        );
        assert_eq!(active_skill.metadata.path, expected_root.join("SKILL.md"));
        assert_eq!(
            active_skill.metadata.package_root.as_deref(),
            Some(expected_root.as_path())
        );
        assert_eq!(
            active_skill.metadata.resource_root.as_deref(),
            Some(expected_root.as_path())
        );
        assert!(matches!(
            active_skill.activation_reason,
            SkillActivationReason::ExplicitMention { .. }
        ));
        assert!(prompt.system_prompt.contains(&format!(
            "canonical_path: {}",
            expected_root.join("SKILL.md").display()
        )));
        assert!(
            prompt
                .system_prompt
                .contains(&format!("resource_root: {}", expected_root.display()))
        );
        assert_eq!(
            active_skill.metadata.execution,
            ResolvedSkillExecution::Inline {
                source: SkillExecutionResolutionSource::NoChildAgentExports,
            }
        );
    }

    #[test]
    fn prompt_cache_invalidates_when_child_agent_exports_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        assert_eq!(
            first.active_skills[0].metadata.execution,
            ResolvedSkillExecution::Inline {
                source: SkillExecutionResolutionSource::NoChildAgentExports,
            }
        );

        create_repo_child_agent(&workspace_root, "my-skill", "my-skill");

        let second = cache.build(Some(&user_input));
        assert!(!second.skills_cache_hit);
        assert_eq!(
            second.active_skills[0].metadata.execution,
            ResolvedSkillExecution::Delegate {
                target: "my-skill".to_string(),
                source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            }
        );
        assert!(second.system_prompt.contains(
            "execution: delegate(target=my-skill, source=same_name_skill_and_child_agent)"
        ));
    }

    #[test]
    fn prompt_cache_revalidates_carried_active_skills_when_they_become_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "release-check",
            "Release Check",
            "Review risky release actions",
            "# Instructions\nUse this skill when asked.",
        );

        let host_capabilities =
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults();
        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(&workspace_root),
            Vec::new(),
            host_capabilities,
        );
        let user_input = vec![ContentPart::text("please use $release-check for this task")];

        let first = cache.build(Some(&user_input));
        assert_eq!(first.active_skills.len(), 1);

        std::fs::write(
            workspace_root.join(".alan/agents/default/skills/release-check/SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
capabilities:
  required_tools: ["missing_tool"]
---

# Instructions
Do not use stale instructions.
"#,
        )
        .unwrap();

        let resumed = cache.build_with_active_skills(&first.active_skills, None);

        assert!(resumed.active_skills.is_empty());
        assert!(
            resumed
                .system_prompt
                .contains("Skill '$release-check' is unavailable")
        );
        assert!(
            resumed
                .system_prompt
                .contains("missing dependencies: tool:missing_tool")
        );
        assert!(!resumed.system_prompt.contains("## Skill: Release Check"));
        assert!(!resumed.system_prompt.contains("Use this skill when asked."));
    }

    #[test]
    fn prompt_cache_revalidates_carried_active_skills_when_they_disappear() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "release-check",
            "Release Check",
            "Review risky release actions",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $release-check for this task")];

        let first = cache.build(Some(&user_input));
        assert_eq!(first.active_skills.len(), 1);

        std::fs::remove_dir_all(workspace_root.join(".alan/agents/default/skills/release-check"))
            .unwrap();

        let resumed = cache.build_with_active_skills(&first.active_skills, None);

        assert!(resumed.active_skills.is_empty());
        assert!(
            resumed
                .system_prompt
                .contains("Skill '$release-check' not found")
        );
        assert!(!resumed.system_prompt.contains("## Skill: Release Check"));
        assert!(!resumed.system_prompt.contains("Use this skill when asked."));
    }

    #[test]
    fn prompt_cache_renders_delegated_skill_as_stub() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_repo_child_agent(&workspace_root, "repo-review", "repo-review");

        let host_capabilities = SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation();
        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(&workspace_root),
            Vec::new(),
            host_capabilities,
        );
        let user_input = vec![ContentPart::text("please use $repo-review for this task")];

        let prompt = cache.build(Some(&user_input));
        assert_eq!(prompt.active_skills.len(), 1);
        assert!(prompt.system_prompt.contains("execution: delegate("));
        assert!(prompt.system_prompt.contains("### Delegated Capability"));
        assert!(prompt.system_prompt.contains("invoke_delegated_skill"));
        assert!(!prompt.system_prompt.contains("SECRET INLINE REVIEW BODY"));
    }

    #[test]
    fn prompt_cache_falls_back_to_inline_when_delegated_invocation_is_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_repo_child_agent(&workspace_root, "repo-review", "repo-review");

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $repo-review for this task")];

        let prompt = cache.build(Some(&user_input));
        assert_eq!(prompt.active_skills.len(), 1);
        assert!(prompt.system_prompt.contains("execution: delegate("));
        assert!(prompt.system_prompt.contains("### Runtime Fallback"));
        assert!(prompt.system_prompt.contains("SECRET INLINE REVIEW BODY"));
        assert!(!prompt.system_prompt.contains("### Delegated Capability"));
    }

    #[test]
    fn prompt_cache_lists_delegated_skill_with_inline_guidance_when_runtime_lacks_delegated_support()
     {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_repo_child_agent(&workspace_root, "repo-review", "repo-review");

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let prompt = cache.build(Some(&[ContentPart::text(
            "please help with this workspace",
        )]));

        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(prompt.system_prompt.contains("skill_id: repo-review"));
        assert!(prompt.system_prompt.contains("skill_path: "));
        assert!(!prompt.system_prompt.contains("invoke_delegated_skill"));
    }

    #[test]
    fn prompt_cache_rebuilds_delegated_skill_when_invocation_support_changes() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_repo_child_agent(&workspace_root, "repo-review", "repo-review");

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $repo-review for this task")];

        let before = cache.build(Some(&user_input));
        assert!(before.system_prompt.contains("### Runtime Fallback"));
        assert!(before.system_prompt.contains("SECRET INLINE REVIEW BODY"));

        let delegated_runtime = SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation();
        cache.set_host_capabilities(delegated_runtime);

        let after = cache.build(Some(&user_input));
        assert!(!after.skills_cache_hit);
        assert!(after.system_prompt.contains("### Delegated Capability"));
        assert!(after.system_prompt.contains("invoke_delegated_skill"));
        assert!(!after.system_prompt.contains("SECRET INLINE REVIEW BODY"));
    }

    #[test]
    fn prompt_cache_keeps_inline_fallback_when_dynamic_tools_only_match_delegated_tool_name() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_repo_child_agent(&workspace_root, "repo-review", "repo-review");

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $repo-review for this task")];

        let mut dynamic_tool_runtime = SkillHostCapabilities::default().with_runtime_defaults();
        dynamic_tool_runtime.extend_tools(["invoke_delegated_skill"]);
        cache.set_host_capabilities(dynamic_tool_runtime);

        let prompt = cache.build(Some(&user_input));
        assert!(prompt.system_prompt.contains("### Runtime Fallback"));
        assert!(prompt.system_prompt.contains("SECRET INLINE REVIEW BODY"));
        assert!(!prompt.system_prompt.contains("### Delegated Capability"));
    }

    #[test]
    fn explicit_mention_activation_reason_is_canonical() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let mentioned = vec![ContentPart::text("please use $my-skill for this task")];
        let prompt = cache.build(Some(&mentioned));

        assert_eq!(prompt.active_skills.len(), 1);
        assert!(matches!(
            prompt.active_skills[0].activation_reason,
            SkillActivationReason::ExplicitMention { .. }
        ));
        assert!(
            prompt
                .system_prompt
                .contains("activation_reason: explicit_mention($my-skill)")
        );
    }

    #[test]
    fn obsolete_structured_trigger_aliases_do_not_activate_skill() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill_with_frontmatter(
            &workspace_root,
            "my-skill",
            r#"name: My Skill
description: Custom test skill
capabilities:
  triggers:
    explicit: ["$Ship_It"]"#,
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $ship-it for this task")];

        let prompt = cache.build(Some(&user_input));

        assert!(prompt.active_skills.is_empty());
        assert!(prompt.system_prompt.contains("Skill '$ship-it' not found"));
    }

    #[test]
    fn implicit_skill_is_listed_but_not_auto_activated() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please help with this workspace")];

        let prompt = cache.build(Some(&user_input));

        assert!(prompt.active_skills.is_empty());
        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(prompt.system_prompt.contains("skill_id: my-skill"));
        assert!(!prompt.system_prompt.contains("## Skill: My Skill"));
    }

    #[test]
    fn prompt_cache_builtin_skills_do_not_expose_materialized_temp_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();

        let host_capabilities =
            SkillHostCapabilities::with_tools(["read_file", "write_file", "edit_file", "bash"])
                .with_runtime_defaults();
        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(&workspace_root),
            Vec::new(),
            host_capabilities,
        );
        let prompt = cache.build(Some(&[ContentPart::text(
            "please use $memory for this task",
        )]));

        assert_eq!(prompt.active_skills.len(), 1);
        assert_eq!(prompt.active_skills[0].metadata.id, "memory");
        assert!(prompt.system_prompt.contains("## Skill: memory"));
        assert!(prompt.system_prompt.contains("skill_id: memory"));
        assert!(
            prompt
                .system_prompt
                .contains("canonical_path: builtin:memory")
        );
        assert!(
            prompt
                .system_prompt
                .contains("resource_root: <builtin capability package>")
        );
        assert!(!prompt.system_prompt.contains("builtin-skill-packages"));
        assert!(!prompt.system_prompt.contains("/private/tmp/alan"));
    }

    #[test]
    fn prompt_cache_lists_workspace_inspect_with_core_tools_when_delegation_is_supported() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();

        let host_capabilities =
            SkillHostCapabilities::with_tools(["read_file", "write_file", "edit_file", "bash"])
                .with_runtime_defaults()
                .with_delegated_skill_invocation();
        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(&workspace_root),
            Vec::new(),
            host_capabilities,
        );
        let prompt = cache.build(Some(&[ContentPart::text(
            "帮我看另一个本地 workspace 的文档",
        )]));

        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(prompt.system_prompt.contains("skill_id: workspace-inspect"));
        assert!(
            prompt
                .system_prompt
                .contains("execution: delegate(target=workspace-reader)")
        );
        assert!(prompt.system_prompt.contains("workspace_root"));
        assert!(prompt.system_prompt.contains("optional `cwd`"));
    }

    #[test]
    fn prompt_cache_invalidates_when_skill_changes() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            workspace_root.join(".alan/agents/default/skills/my-skill/SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Updated instructions.
"#,
        )
        .unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("Use this skill when asked."));
        assert!(second.system_prompt.contains("Updated instructions."));
        assert!(!second.skills_cache_hit);
        assert_eq!(second.metrics.skills_misses, 2);
    }

    #[test]
    fn prompt_cache_invalidates_when_skill_contents_change_with_same_length() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();

        let initial = r#"---
name: My Skill
description: Custom test skill
---

# Instructions
ABCD
"#;
        let updated = r#"---
name: My Skill
description: Custom test skill
---

# Instructions
WXYZ
"#;
        assert_eq!(initial.len(), updated.len());

        let skill_path = workspace_root.join(".alan/agents/default/skills/my-skill/SKILL.md");
        std::fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
        std::fs::write(&skill_path, initial).unwrap();

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        std::fs::write(&skill_path, updated).unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("# Instructions\nABCD"));
        assert!(second.system_prompt.contains("# Instructions\nWXYZ"));
        assert!(!second.skills_cache_hit);
    }

    #[test]
    fn prompt_cache_uses_disclosure_level2_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let skill_dir = workspace_root.join(".alan/agents/default/skills/my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
capabilities:
  disclosure:
    level2: details.md
---

# Instructions
Fallback instructions.
"#,
        )
        .unwrap();
        std::fs::write(skill_dir.join("details.md"), "Expanded instructions.").unwrap();

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let prompt = cache.build(Some(&user_input));

        assert!(prompt.system_prompt.contains("source: details.md"));
        assert!(prompt.system_prompt.contains("Expanded instructions."));
        assert!(!prompt.system_prompt.contains("Fallback instructions."));
    }

    #[test]
    fn prompt_cache_invalidates_when_disclosed_resource_changes() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let skill_dir = workspace_root.join(".alan/agents/default/skills/my-skill");
        let references_dir = skill_dir.join("references");
        std::fs::create_dir_all(&references_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Read `references/guide.md` before acting.
"#,
        )
        .unwrap();

        let initial = "ALPHA";
        let updated = "OMEGA";
        assert_eq!(initial.len(), updated.len());
        std::fs::write(references_dir.join("guide.md"), initial).unwrap();

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        std::fs::write(references_dir.join("guide.md"), updated).unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("ALPHA"));
        assert!(second.system_prompt.contains("OMEGA"));
        assert!(!second.skills_cache_hit);
    }

    #[cfg(unix)]
    #[test]
    fn prompt_cache_invalidates_when_skill_symlink_is_retargeted() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let skills_root = workspace_root.join(".alan/agents/default/skills");
        let pack_v1 = temp.path().join("pack-v1");
        let pack_v2 = temp.path().join("pack-v2");
        let linked_pack = skills_root.join("linked-pack");

        std::fs::create_dir_all(&skills_root).unwrap();
        std::fs::create_dir_all(pack_v1.join("my-skill")).unwrap();
        std::fs::create_dir_all(pack_v2.join("my-skill")).unwrap();
        std::fs::write(
            pack_v1.join("my-skill/SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Version one.
"#,
        )
        .unwrap();
        std::fs::write(
            pack_v2.join("my-skill/SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Version two.
"#,
        )
        .unwrap();
        symlink(&pack_v1, &linked_pack).unwrap();

        let mut cache = prompt_cache_for_workspace_root(&workspace_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        std::fs::remove_file(&linked_pack).unwrap();
        symlink(&pack_v2, &linked_pack).unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("Version one."));
        assert!(second.system_prompt.contains("Version two."));
        assert!(!second.skills_cache_hit);
    }

    #[test]
    fn implicit_false_skills_are_mentionable_but_not_implicitly_listed() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root_with_overrides(
            &workspace_root,
            vec![SkillOverride {
                skill_id: "my-skill".to_string(),
                enabled: Some(true),
                allow_implicit_invocation: Some(false),
            }],
            Vec::new(),
        );

        let unmentioned = vec![ContentPart::text("please help with this task")];
        let unmentioned_prompt = cache.build(Some(&unmentioned));
        assert!(unmentioned_prompt.active_skills.is_empty());
        assert!(
            !unmentioned_prompt
                .system_prompt
                .contains("- skill_id: my-skill")
        );

        let mentioned = vec![ContentPart::text("please use $my-skill for this task")];
        let mentioned_prompt = cache.build(Some(&mentioned));
        assert!(
            mentioned_prompt
                .system_prompt
                .contains("## Skill: My Skill")
        );
    }

    #[test]
    fn disabled_skills_are_hidden_from_catalog_and_activation() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_workspace_root_with_overrides(
            &workspace_root,
            vec![SkillOverride {
                skill_id: "my-skill".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            }],
            Vec::new(),
        );

        let mentioned = vec![ContentPart::text("please use $my-skill for this task")];
        let prompt = cache.build(Some(&mentioned));
        assert!(!prompt.system_prompt.contains("skill_id: my-skill"));
        assert!(!prompt.system_prompt.contains("## Skill: My Skill"));
        assert!(prompt.system_prompt.contains("Skill '$my-skill' not found"));
    }

    #[test]
    fn disabled_skills_with_missing_tools_still_render_as_not_found() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let skill_dir = workspace_root.join(".alan/agents/default/skills/hidden-helper");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Hidden Helper
description: Should stay hidden
capabilities:
  required_tools: ["missing_tool"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let mut cache = PromptAssemblyCache::with_fixed_capability_view_and_overrides(
            capability_view_for_workspace_root(&workspace_root),
            vec![SkillOverride {
                skill_id: "hidden-helper".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            }],
            Vec::new(),
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults(),
        );

        let mentioned = vec![ContentPart::text("please use $hidden-helper for this task")];
        let prompt = cache.build(Some(&mentioned));

        assert!(!prompt.system_prompt.contains("## Skill: Hidden Helper"));
        assert!(
            !prompt
                .system_prompt
                .contains("Skill '$hidden-helper' is unavailable")
        );
        assert!(
            prompt
                .system_prompt
                .contains("Skill '$hidden-helper' not found")
        );
    }

    #[test]
    fn skills_with_missing_required_tools_are_reported_as_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let skill_dir = workspace_root.join(".alan/agents/default/skills/tool-heavy");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Tool Heavy
description: Needs extra tools
capabilities:
  required_tools: ["missing_tool"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(&workspace_root),
            Vec::new(),
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults(),
        );
        let mentioned = vec![ContentPart::text("please use $tool-heavy for this task")];
        let prompt = cache.build(Some(&mentioned));

        assert!(!prompt.system_prompt.contains("## Skill: Tool Heavy"));
        assert!(
            prompt
                .system_prompt
                .contains("Skill '$tool-heavy' is unavailable")
        );
        assert!(
            prompt
                .system_prompt
                .contains("missing dependencies: tool:missing_tool")
        );
        assert!(prompt.system_prompt.contains("Suggested next steps:"));
        assert!(
            prompt
                .system_prompt
                .contains("Enable or register the required tool: missing_tool.")
        );
    }

    #[test]
    fn skills_with_unresolved_execution_are_reported_as_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        create_repo_skill(
            &workspace_root,
            "skill-creator",
            "Skill Creator",
            "Creates new skills",
            "# Instructions\nUse this skill when asked.",
        );
        create_repo_child_agent(&workspace_root, "skill-creator", "creator");
        create_repo_child_agent(&workspace_root, "skill-creator", "grader");

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(&workspace_root),
            Vec::new(),
            SkillHostCapabilities::with_tools(["bash"]).with_runtime_defaults(),
        );
        let mentioned = vec![ContentPart::text("please use $skill-creator for this task")];
        let prompt = cache.build(Some(&mentioned));

        assert!(!prompt.system_prompt.contains("## Skill: Skill Creator"));
        assert!(
            prompt
                .system_prompt
                .contains("Skill '$skill-creator' is unavailable")
        );
        assert!(
            prompt
                .system_prompt
                .contains("unresolved execution: unresolved(ambiguous_package_shape)")
        );
        assert!(prompt.system_prompt.contains("Suggested next steps:"));
        assert!(
            prompt
                .system_prompt
                .contains("Fix delegated execution metadata")
        );
    }

    #[test]
    fn builtin_skill_creator_uses_directory_backed_resource_root() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let capability_view = capability_view_for_workspace_root(&workspace_root);
        let host_capabilities = SkillHostCapabilities::with_tools(["bash"]).with_runtime_defaults();
        let snapshot =
            CachedSkillsRegistry::load_capability_view(&capability_view, &[], &host_capabilities)
                .unwrap();

        assert!(snapshot.mentionable_skill_ids.contains("skill-creator"));
        assert!(
            snapshot
                .listed_skills
                .iter()
                .any(|skill| skill.id == "skill-creator")
        );
        assert!(
            !snapshot
                .unavailable_skill_messages
                .contains_key("skill-creator")
        );

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view,
            Vec::new(),
            host_capabilities,
        );
        let user_input = vec![ContentPart::text("please use $skill-creator for this task")];
        let prompt = cache.build(Some(&user_input));

        let active_skill = prompt
            .active_skills
            .iter()
            .find(|skill| skill.metadata.id == "skill-creator")
            .unwrap();
        let resource_root = active_skill.metadata.resource_root.as_ref().unwrap();

        assert_eq!(active_skill.metadata.id, "skill-creator");
        assert_eq!(
            active_skill.metadata.package_id.as_deref(),
            Some("builtin:alan-skill-creator")
        );
        assert!(resource_root.join("references/authoring.md").is_file());
        assert!(resource_root.join("scripts/quick_validate.py").is_file());
        assert!(resource_root.join("agents/openai.yaml").is_file());
        assert_eq!(
            active_skill.metadata.execution,
            ResolvedSkillExecution::Delegate {
                target: "skill-creator".to_string(),
                source: crate::skills::SkillExecutionResolutionSource::ExplicitMetadata,
            }
        );
        assert!(
            prompt
                .system_prompt
                .contains("resource_root: <builtin capability package>")
        );
        assert!(
            !prompt
                .system_prompt
                .contains(&resource_root.display().to_string())
        );
    }

    #[test]
    fn prompt_cache_invalidates_when_host_capabilities_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();
        let skill_dir = workspace_root.join(".alan/agents/default/skills/dynamic-helper");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Dynamic Helper
description: Needs a dynamic tool
capabilities:
  required_tools: ["custom_tool"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_root(&workspace_root),
            Vec::new(),
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults(),
        );
        let mentioned = vec![ContentPart::text(
            "please use $dynamic-helper for this task",
        )];

        let before = cache.build(Some(&mentioned));
        assert!(
            before
                .system_prompt
                .contains("Skill '$dynamic-helper' is unavailable")
        );

        let mut refreshed =
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults();
        refreshed.extend_tools(["custom_tool"]);
        cache.set_host_capabilities(refreshed);

        let after = cache.build(Some(&mentioned));
        assert!(after.system_prompt.contains("## Skill: Dynamic Helper"));
        assert!(
            !after
                .system_prompt
                .contains("Skill '$dynamic-helper' is unavailable")
        );
    }
}
