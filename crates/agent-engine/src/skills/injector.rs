//! Skills injector for adding skill content to prompts.

use crate::skills::types::*;
use std::path::Path;

mod disclosure;

pub use disclosure::{PromptTrackedPath, PromptTrackedPathFingerprint};
use disclosure::{dedupe_tracked_paths, disclose_skill_prompt, format_disclosed_resources};

const DELEGATED_INLINE_FALLBACK_NOTE: &str = "Delegated runtime execution is not available in this runtime yet, so alan is falling back to inline skill instructions for this turn.";

#[derive(Debug, Clone)]
pub struct RenderedActiveSkillPrompt {
    pub rendered: String,
    pub tracked_paths: Vec<PromptTrackedPath>,
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
