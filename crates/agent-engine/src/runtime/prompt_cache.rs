mod snapshots;

use self::snapshots::{
    CachedDefinitionPersona, CachedMemoryStore, CachedSkillsRegistry, RenderedDomainPrompt,
};
#[cfg(test)]
use self::snapshots::{
    ContentFingerprintMode, WORKSPACE_MEMORY_TRACKED_PREFIX_BYTES,
    WORKSPACE_PERSONA_TRACKED_PREFIX_BYTES, hash_file_contents,
};
use crate::prompts;
use crate::skills::{
    ActiveSkillEnvelope, ResolvedCapabilityView, SkillHostCapabilities, SkillMetadata,
    SkillOverride,
};
use crate::tape::ContentPart;
use std::path::PathBuf;
use std::time::Instant;
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

pub(crate) struct PromptAssemblyCache {
    fixed_capability_view: Option<ResolvedCapabilityView>,
    skill_overrides: Vec<SkillOverride>,
    definition_persona_dirs: Vec<PathBuf>,
    fixed_definition_persona_section: Option<String>,
    memory_store_dir: Option<PathBuf>,
    host_capabilities: SkillHostCapabilities,
    skills_snapshot: Option<CachedSkillsRegistry>,
    definition_persona_snapshot: Option<CachedDefinitionPersona>,
    memory_store_snapshot: Option<CachedMemoryStore>,
    metrics: PromptAssemblyMetrics,
}

impl PromptAssemblyCache {
    #[cfg(test)]
    pub(crate) fn new(definition_persona_dirs: Vec<PathBuf>) -> Self {
        Self {
            fixed_capability_view: None,
            skill_overrides: Vec::new(),
            definition_persona_dirs,
            fixed_definition_persona_section: None,
            memory_store_dir: None,
            host_capabilities: SkillHostCapabilities::default(),
            skills_snapshot: None,
            definition_persona_snapshot: None,
            memory_store_snapshot: None,
            metrics: PromptAssemblyMetrics::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_fixed_capability_view(
        fixed_capability_view: ResolvedCapabilityView,
        definition_persona_dirs: Vec<PathBuf>,
        host_capabilities: SkillHostCapabilities,
    ) -> Self {
        Self::with_fixed_capability_view_and_overrides(
            fixed_capability_view,
            Vec::new(),
            definition_persona_dirs,
            host_capabilities,
        )
    }

    pub(crate) fn with_fixed_capability_view_and_overrides(
        fixed_capability_view: ResolvedCapabilityView,
        skill_overrides: Vec<SkillOverride>,
        definition_persona_dirs: Vec<PathBuf>,
        host_capabilities: SkillHostCapabilities,
    ) -> Self {
        Self {
            fixed_capability_view: Some(fixed_capability_view),
            skill_overrides,
            definition_persona_dirs,
            fixed_definition_persona_section: None,
            memory_store_dir: None,
            host_capabilities,
            skills_snapshot: None,
            definition_persona_snapshot: None,
            memory_store_snapshot: None,
            metrics: PromptAssemblyMetrics::default(),
        }
    }

    pub(crate) fn rebind_paths(&mut self, definition_persona_dirs: Vec<PathBuf>) {
        if self.definition_persona_dirs != definition_persona_dirs {
            self.definition_persona_dirs = definition_persona_dirs;
            self.definition_persona_snapshot = None;
        }
    }

    pub(crate) fn set_fixed_definition_persona_section(&mut self, section: Option<String>) {
        self.fixed_definition_persona_section = section;
    }

    pub(crate) fn set_memory_store_dir(&mut self, memory_store_dir: Option<PathBuf>) {
        if self.memory_store_dir != memory_store_dir {
            self.memory_store_dir = memory_store_dir;
            self.memory_store_snapshot = None;
        }
    }

    #[cfg(test)]
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
        let (context_sections, persona_cache_hit) = self.process_context_sections_with_cache();
        let system_prompt = prompts::build_agent_system_prompt_with_sections(
            &domain_prompt,
            context_sections.definition_persona_section.as_deref(),
            context_sections.memory_store_section.as_deref(),
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
        let (context_sections, persona_cache_hit) = self.process_context_sections_with_cache();
        let system_prompt = prompts::build_agent_system_prompt_with_sections(
            &domain_prompt,
            context_sections.definition_persona_section.as_deref(),
            context_sections.memory_store_section.as_deref(),
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

    fn process_context_sections_with_cache(&mut self) -> (ProcessContextSections, bool) {
        let (definition_persona_section, persona_cache_hit) =
            self.definition_persona_section_with_cache();
        let (memory_store_section, memory_cache_hit) = self.memory_store_section_with_cache();
        (
            ProcessContextSections {
                definition_persona_section,
                memory_store_section,
            },
            persona_cache_hit && memory_cache_hit,
        )
    }

    fn definition_persona_section_with_cache(&mut self) -> (Option<String>, bool) {
        if let Some(section) = self.fixed_definition_persona_section.as_ref() {
            return (Some(section.clone()), true);
        }
        if self.definition_persona_dirs.is_empty() {
            return (None, true);
        }
        if !self.definition_persona_dirs.iter().any(|dir| dir.exists()) {
            self.definition_persona_snapshot = None;
            return (None, true);
        }

        let cache_hit = self
            .definition_persona_snapshot
            .as_ref()
            .is_some_and(CachedDefinitionPersona::is_current);
        if !cache_hit {
            self.definition_persona_snapshot =
                Some(CachedDefinitionPersona::load(&self.definition_persona_dirs));
        }

        let rendered = self
            .definition_persona_snapshot
            .as_ref()
            .map(|snapshot| snapshot.rendered_section.clone())
            .filter(|section| !section.is_empty());
        (rendered, cache_hit)
    }

    fn memory_store_section_with_cache(&mut self) -> (Option<String>, bool) {
        let Some(memory_dir) = self.memory_store_dir.as_deref() else {
            self.memory_store_snapshot = None;
            return (None, true);
        };
        if !memory_dir.exists() {
            self.memory_store_snapshot = None;
            return (None, true);
        }

        let cache_hit = self
            .memory_store_snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.is_current_for(memory_dir));
        if !cache_hit {
            self.memory_store_snapshot = Some(CachedMemoryStore::load(memory_dir));
        }

        let rendered = self
            .memory_store_snapshot
            .as_ref()
            .map(|snapshot| snapshot.rendered_section.clone())
            .filter(|section| !section.is_empty());
        (rendered, cache_hit)
    }
}

#[derive(Debug, Clone, Default)]
struct ProcessContextSections {
    definition_persona_section: Option<String>,
    memory_store_section: Option<String>,
}

#[cfg(test)]
mod tests;
