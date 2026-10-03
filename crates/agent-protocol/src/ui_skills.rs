//! Safe Process-local explicit Skill mentionability; never execution authority.
use crate::UI_SURFACE_VERSION;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiSkillSnapshot {
    pub version: u16,
    /// Monotonic within the actual Process, not a Package catalog revision.
    pub publication_version: u64,
    pub process_path: String,
    pub known: bool,
    pub mentionable_skill_ids: Vec<String>,
}
impl Default for UiSkillSnapshot {
    fn default() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            publication_version: 0,
            process_path: String::new(),
            known: false,
            mentionable_skill_ids: Vec::new(),
        }
    }
}
impl UiSkillSnapshot {
    /// Structural validation only. Canonical Skill validation belongs to the producer.
    pub fn is_valid(&self) -> bool {
        let valid_process = self.process_path.strip_prefix("/proc/").is_some_and(|pid| {
            !pid.is_empty()
                && pid.bytes().all(|byte| byte.is_ascii_digit())
                && !pid.starts_with('0')
                && pid.parse::<u64>().is_ok()
        });
        self.version == UI_SURFACE_VERSION
            && (valid_process || (!self.known && self.process_path.is_empty()))
            && (!self.known || self.publication_version > 0)
            && (self.known || self.mentionable_skill_ids.is_empty())
            && self.mentionable_skill_ids.iter().all(|id| {
                !id.is_empty()
                    && id.split('-').all(|part| {
                        !part.is_empty()
                            && part
                                .bytes()
                                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                    })
            })
    }
    pub fn unknown(mut self) -> Self {
        self.known = false;
        self.mentionable_skill_ids.clear();
        self
    }
}
