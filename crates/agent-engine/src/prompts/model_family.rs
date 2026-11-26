//! Model-family-specific static system prompt framing.
//!
//! Formats the static system prompt once per `(provider, model)` family so the
//! prompt prefix stays byte-identical across turns and preserves KV/prefix caches.

/// Canonical model families with distinct instruction-following characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelFamily {
    /// OpenAI GPT-5 and Codex reasoning models (`openai_responses`, `chatgpt`, `openai/*`).
    OpenAiGpt5,
    /// Google Gemini 2.5/3 models (`google_gemini_generate_content`, `google/*`).
    GoogleGemini,
    /// Anthropic Claude 4.x models (`anthropic_messages`, `anthropic/*`).
    AnthropicClaude,
    /// Magpie and open-weight / compatible models (DeepSeek, Qwen, Kimi, default).
    OpenWeightDefault,
}

impl ModelFamily {
    /// Detect the target model family from effective `(provider, model)` metadata.
    pub(crate) fn detect(provider: &str, model: &str) -> Self {
        let provider_lower = provider.trim().to_ascii_lowercase();
        let model_lower = model.trim().to_ascii_lowercase();

        if model_lower.starts_with("anthropic/") || model_lower.starts_with("claude-") {
            return Self::AnthropicClaude;
        }
        if model_lower.starts_with("google/") || model_lower.starts_with("gemini-") {
            return Self::GoogleGemini;
        }
        if model_lower.starts_with("openai/")
            || model_lower.starts_with("gpt-5")
            || model_lower == "o1"
            || model_lower.starts_with("o1-")
            || model_lower == "o3"
            || model_lower.starts_with("o3-")
            || model_lower == "o4"
            || model_lower.starts_with("o4-")
        {
            return Self::OpenAiGpt5;
        }

        match provider_lower.as_str() {
            "openai_responses" | "chatgpt" => Self::OpenAiGpt5,
            "google_gemini_generate_content" | "vertex" | "google_gemini" => Self::GoogleGemini,
            "anthropic_messages" => Self::AnthropicClaude,
            _ => Self::OpenWeightDefault,
        }
    }
}

/// Apply static, cache-safe model-family framing to `base_system_prompt`.
pub(crate) fn format_system_prompt_for_model(
    base_system_prompt: &str,
    provider: &str,
    model: &str,
) -> String {
    let family = ModelFamily::detect(provider, model);
    format_system_prompt_with_family(base_system_prompt, family)
}

fn format_system_prompt_with_family(base_system_prompt: &str, family: ModelFamily) -> String {
    let trimmed = base_system_prompt.trim();
    match family {
        ModelFamily::OpenAiGpt5 => format!(
            "{trimmed}\n\n\
             ## Execution Profile (GPT-5 / Codex)\n\
             - Goal: Deliver exact, verified outcomes cleanly with minimal prose.\n\
             - Constraints: Keep reasoning concise and internal; batch independent read-only tool calls; keep edits tightly scoped; adjust autonomy to the task without verbose step-by-step narration.\n\
             - Done When: Target files are read directly, changes are minimal, and relevant verification checks pass."
        ),
        ModelFamily::GoogleGemini => format_gemini_inverted_prompt(trimmed),
        ModelFamily::AnthropicClaude => format!(
            "<instructions>\n{trimmed}\n</instructions>\n\n\
             <tools>\n\
             Use dedicated file and search tools (`read_file`, `edit_file`, `write_file`, `grep`, `glob`, `list_dir`) for inspection and edits because they preserve exact line structure without shell escaping issues; use `bash` for builds, tests, git, and commands lacking a dedicated tool. Batch independent read-only tool calls in parallel to minimize round-trips.\n\
             </tools>\n\n\
             <constraints>\n\
             Read files before editing so patches match exact source lines, make minimal localized changes, and run real verification checks after your final edit because unverified assumptions hide failures. State only observed paths, symbols, and outputs, and ask or request grants explicitly when scope is missing.\n\
             </constraints>"
        ),
        ModelFamily::OpenWeightDefault => format!(
            "{trimmed}\n\n\
             ## Verification Checkpoints\n\
             1. Read target code via `read_file`/`grep` before editing (batch independent reads in parallel).\n\
             2. Use `edit_file` with exact source text (never copy `N| ` line prefixes) for minimal diffs.\n\
             3. Run focused checks via `bash` after final edits and report only directly observed outputs."
        ),
    }
}

fn format_gemini_inverted_prompt(trimmed: &str) -> String {
    const FINAL_ANCHOR: &str = "## Final Operational Instructions\n\
        - Be direct, logical, and concise.\n\
        - Grounding rule: explicitly flag missing files, unknown symbols, absent APIs, or unverified behavior instead of guessing or fabricating details.\n\
        - Use dedicated tools in parallel for reads, make minimal edits, and verify with real checks before claiming completion.";

    if let Some((prefix, knowledge, suffix)) = extract_project_knowledge_block(trimmed) {
        let mut sections = Vec::new();
        for part in [prefix.trim(), suffix.trim(), knowledge.trim()] {
            if !part.is_empty() {
                sections.push(part);
            }
        }
        sections.push(FINAL_ANCHOR);
        sections.join("\n\n")
    } else if trimmed.is_empty() {
        FINAL_ANCHOR.to_string()
    } else {
        format!("{trimmed}\n\n{FINAL_ANCHOR}")
    }
}

fn extract_project_knowledge_block(prompt: &str) -> Option<(&str, &str, &str)> {
    let start = prompt.find("<project_knowledge")?;
    let end_rel = prompt[start..].find("</project_knowledge>")?;
    let end = start + end_rel + "</project_knowledge>".len();
    Some((&prompt[..start], &prompt[start..end], &prompt[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_model_families_across_first_party_and_openrouter() {
        assert_eq!(
            ModelFamily::detect("openai_responses", "gpt-5.4"),
            ModelFamily::OpenAiGpt5
        );
        assert_eq!(
            ModelFamily::detect("chatgpt", "gpt-5.4"),
            ModelFamily::OpenAiGpt5
        );
        assert_eq!(
            ModelFamily::detect("openrouter", "openai/gpt-5.4"),
            ModelFamily::OpenAiGpt5
        );
        assert_eq!(
            ModelFamily::detect("google_gemini_generate_content", "gemini-2.5-pro"),
            ModelFamily::GoogleGemini
        );
        assert_eq!(
            ModelFamily::detect("openrouter", "google/gemini-3-flash-preview"),
            ModelFamily::GoogleGemini
        );
        assert_eq!(
            ModelFamily::detect("anthropic_messages", "claude-sonnet-4-5"),
            ModelFamily::AnthropicClaude
        );
        assert_eq!(
            ModelFamily::detect("openrouter", "anthropic/claude-opus-4-6"),
            ModelFamily::AnthropicClaude
        );
        assert_eq!(
            ModelFamily::detect("magpie", "deepseek-chat"),
            ModelFamily::OpenWeightDefault
        );
        assert_eq!(
            ModelFamily::detect("openrouter", "qwen/qwen3-coder"),
            ModelFamily::OpenWeightDefault
        );
    }

    #[test]
    fn gemini_places_project_knowledge_before_final_operational_anchor() {
        let base = "Core rules\n\n<project_knowledge authoritative=\"false\">\nRepo hint\n</project_knowledge>\n\nExtra context";
        let formatted = format_system_prompt_for_model(
            base,
            "google_gemini_generate_content",
            "gemini-2.5-pro",
        );
        let knowledge_idx = formatted.find("<project_knowledge").unwrap();
        let anchor_idx = formatted.find("## Final Operational Instructions").unwrap();
        assert!(knowledge_idx < anchor_idx);
        assert!(formatted.contains("Grounding rule:"));
    }

    #[test]
    fn claude_wraps_xml_sections_and_openai_uses_goal_constraints_done_when() {
        let claude = format_system_prompt_for_model(
            "Base prompt",
            "openrouter",
            "anthropic/claude-sonnet-4-5",
        );
        assert!(claude.contains("<instructions>\nBase prompt\n</instructions>"));
        assert!(claude.contains("<tools>"));
        assert!(claude.contains("<constraints>"));

        let gpt5 = format_system_prompt_for_model("Base prompt", "openai_responses", "gpt-5.4");
        assert!(gpt5.contains("Goal:"));
        assert!(gpt5.contains("Constraints:"));
        assert!(gpt5.contains("Done When:"));
    }

    #[test]
    fn open_weight_uses_compact_verification_checkpoints_and_is_deterministic() {
        let a = format_system_prompt_for_model("Base prompt", "magpie", "kimi-k2");
        let b = format_system_prompt_for_model("Base prompt", "magpie", "kimi-k2");
        assert_eq!(a, b);
        assert!(a.contains("## Verification Checkpoints"));
    }
}
