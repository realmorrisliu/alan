#[cfg(test)]
use std::time::Duration;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
use crate::agent_machine::AgentMachine;
#[cfg(test)]
use crate::llm::LlmClient;
use crate::llm::{GenerationRequest, Message as LlmMessage, build_generation_request};
use crate::prompts::{
    MEMORY_INBOX_DIRNAME, MEMORY_PROMOTION_PROMPT, MEMORY_STORE_FILENAME, MEMORY_TOPICS_DIRNAME,
    MEMORY_USER_FILENAME, ensure_memory_store_layout_at,
};
use crate::tape::Message;

use super::agent_loop::RuntimeLoopState;

const DEFAULT_PROMOTED_FACTS_HEADER: &str = "## Promoted Facts";
const DEFAULT_TOPIC_SUMMARY: &str = "Promoted from inbox entries.";
const DEFAULT_EVIDENCE_ITEM: &str = "No evidence recorded.";
const MEMORY_PROMOTION_MAX_TOKENS: i32 = 768;
const MEMORY_PROMOTION_MAX_WRITES: usize = 6;
const MEMORY_PROMOTION_MAX_OBSERVATION_CHARS: usize = 240;
const MEMORY_PROMOTION_MAX_RATIONALE_CHARS: usize = 320;
const MEMORY_PROMOTION_MAX_EVIDENCE_ITEMS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromotionDisposition {
    PromoteNow,
    StageInbox,
}

#[derive(Debug, Clone)]
pub(crate) struct InboxEntryDraft {
    pub kind: &'static str,
    pub target: String,
    pub confidence: &'static str,
    pub observation: String,
    pub evidence: Vec<String>,
    pub promotion_rationale: String,
    pub source_processes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromotionOutcome {
    pub inbox_path: PathBuf,
    pub target_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct InboxEntryFrontmatter {
    id: String,
    kind: String,
    status: String,
    target: String,
    confidence: String,
    created_at: String,
    updated_at: String,
    source_processes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InboxEntryDocument {
    frontmatter: InboxEntryFrontmatter,
    observation: String,
    evidence: Vec<String>,
    promotion_rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TopicPageFrontmatter {
    title: String,
    aliases: Vec<String>,
    tags: Vec<String>,
    entities: Vec<String>,
    updated_at: String,
    source_processes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryPromotionModelOutput {
    #[serde(default)]
    writes: Vec<MemoryPromotionModelWrite>,
}

#[derive(Debug, Deserialize)]
struct MemoryPromotionModelWrite {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    confidence: String,
    #[serde(default)]
    disposition: String,
    #[serde(default)]
    observation: String,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    promotion_rationale: String,
}

#[derive(Debug, Clone)]
struct MemoryPromotionCandidate {
    disposition: PromotionDisposition,
    draft: InboxEntryDraft,
}

pub(crate) async fn stage_inbox_entry(
    memory_dir: &Path,
    draft: InboxEntryDraft,
    now: DateTime<Utc>,
) -> Result<PathBuf> {
    ensure_memory_store_layout_at(memory_dir).with_context(|| {
        format!(
            "failed to ensure Memory Store layout before staging inbox entry at {}",
            memory_dir.display()
        )
    })?;

    let id = format!("inbox-{}", uuid::Uuid::new_v4().simple());
    let path = inbox_entry_path(memory_dir, now, &id);
    let document = InboxEntryDocument {
        frontmatter: InboxEntryFrontmatter {
            id,
            kind: draft.kind.to_string(),
            status: "observed".to_string(),
            target: draft.target,
            confidence: draft.confidence.to_string(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
            source_processes: dedup_strings(draft.source_processes),
        },
        observation: normalize_inline_text(&draft.observation),
        evidence: normalize_items(draft.evidence),
        promotion_rationale: draft.promotion_rationale.trim().to_string(),
    };

    write_text_file(&path, &render_inbox_entry(&document)).await?;
    Ok(path)
}

pub(crate) async fn promote_inbox_entry(
    memory_dir: &Path,
    inbox_path: &Path,
    now: DateTime<Utc>,
) -> Result<PromotionOutcome> {
    ensure_memory_store_layout_at(memory_dir).with_context(|| {
        format!(
            "failed to ensure Memory Store layout before promoting inbox entry at {}",
            memory_dir.display()
        )
    })?;

    let raw = tokio::fs::read_to_string(inbox_path)
        .await
        .with_context(|| format!("read inbox entry {}", inbox_path.display()))?;
    let mut document = parse_inbox_entry(&raw)
        .with_context(|| format!("parse inbox entry {}", inbox_path.display()))?;
    let target_path = resolve_target_path(memory_dir, &document.frontmatter.target)?;
    let promoted_from = format_relative_memory_path(memory_dir, inbox_path);
    let promoted_stamp = now.format("%F").to_string();
    let promoted_observation = normalize_inline_text(&document.observation);
    if promoted_observation.is_empty() {
        bail!(
            "inbox entry observation was empty after normalization: {}",
            inbox_path.display()
        );
    }
    document.observation = promoted_observation.clone();
    let promoted_line = format!(
        "- [{}] {} (promoted from {})",
        promoted_stamp, promoted_observation, promoted_from
    );

    match document.frontmatter.target.as_str() {
        MEMORY_USER_FILENAME | MEMORY_STORE_FILENAME => {
            let existing = read_text_file_or_default(&target_path).await?;
            if !contains_promoted_observation(&existing, &document.observation) {
                let updated = append_markdown_section_item(
                    &existing,
                    DEFAULT_PROMOTED_FACTS_HEADER,
                    &promoted_line,
                );
                write_text_file(&target_path, &updated).await?;
            }
        }
        target if is_topic_target(target) => {
            let existing = read_text_file_or_default(&target_path).await?;
            let title = slug_to_title(topic_slug_from_target(target)?);
            let mut topic = ensure_topic_page_frontmatter(
                &existing,
                &title,
                now,
                &document.frontmatter.source_processes,
            )?;
            if !contains_promoted_observation(&topic, &document.observation) {
                topic = append_markdown_section_item(&topic, "## Stable Facts", &promoted_line);
                for evidence in document
                    .evidence
                    .iter()
                    .filter(|value| !value.trim().is_empty())
                {
                    topic = append_markdown_section_item(
                        &topic,
                        "## References",
                        &format!("- {evidence}"),
                    );
                }
                topic = append_markdown_section_item(
                    &topic,
                    "## References",
                    &format!("- Source inbox entry: {promoted_from}"),
                );
            }
            write_text_file(&target_path, &topic).await?;

            let memory_path = memory_dir.join(MEMORY_STORE_FILENAME);
            let memory_content = read_text_file_or_default(&memory_path).await?;
            let topic_index_line = format!(
                "- {} -> topics/{}.md",
                topic_slug_from_target(target)?,
                topic_slug_from_target(target)?
            );
            let updated_memory =
                append_markdown_section_item(&memory_content, "## Topic Index", &topic_index_line);
            write_text_file(&memory_path, &updated_memory).await?;
        }
        other => bail!("unsupported inbox promotion target: {other}"),
    }

    document.frontmatter.status = "confirmed".to_string();
    document.frontmatter.updated_at = now.to_rfc3339();
    write_text_file(inbox_path, &render_inbox_entry(&document)).await?;

    Ok(PromotionOutcome {
        inbox_path: inbox_path.to_path_buf(),
        target_path,
    })
}

#[derive(Debug, Clone)]
pub(crate) struct TurnMemoryPromotionJob {
    memory_dir: PathBuf,
    process_path: String,
    active_turn_user_messages: Vec<String>,
    llm_request_timeout_secs: u64,
    pub(crate) warning_context: &'static str,
}

pub(crate) fn build_turn_memory_promotion_job(
    state: &RuntimeLoopState,
    warning_context: &'static str,
) -> Option<TurnMemoryPromotionJob> {
    if !state.core_config.memory.enabled {
        return None;
    }

    let memory_dir = state.core_config.memory.store_dir.clone()?;
    let active_turn_user_messages = active_turn_user_messages(
        state.machine.messages(),
        state.machine.active_turn_message_start(),
    );
    if active_turn_user_messages.is_empty() {
        return None;
    }

    Some(TurnMemoryPromotionJob {
        memory_dir,
        process_path: state.process_path(),
        active_turn_user_messages,
        llm_request_timeout_secs: state.runtime_config.llm_request_timeout_secs,
        warning_context,
    })
}

#[cfg(test)]
pub(crate) async fn run_turn_memory_promotion_job_with_cancel(
    llm_client: &mut LlmClient,
    job: &TurnMemoryPromotionJob,
    cancel: &CancellationToken,
) -> Result<()> {
    capture_confirmed_turn_memory_for_process(
        llm_client,
        job.llm_request_timeout_secs,
        &job.memory_dir,
        &job.process_path,
        &job.active_turn_user_messages,
        cancel,
    )
    .await
}

pub(crate) async fn run_turn_memory_promotion_job_for_runtime_with_cancel(
    state: &mut RuntimeLoopState,
    job: &TurnMemoryPromotionJob,
    cancel: &CancellationToken,
) -> Result<()> {
    let request = build_memory_promotion_request(job.active_turn_user_messages.clone());
    let response = state
        .generate_response_with_retry(request, job.llm_request_timeout_secs, cancel)
        .await
        .context("generate turn-end memory promotion plan")?;
    let candidates = parse_memory_promotion_candidates(&response.content, &job.process_path)?;
    apply_memory_promotion_candidates(&job.memory_dir, candidates, cancel).await
}

#[cfg(test)]
async fn capture_confirmed_turn_memory_for_process(
    llm_client: &mut LlmClient,
    llm_request_timeout_secs: u64,
    memory_dir: &Path,
    process_path: &str,
    active_turn_user_messages: &[String],
    cancel: &CancellationToken,
) -> Result<()> {
    let candidates = generate_memory_promotion_candidates(
        llm_client,
        llm_request_timeout_secs,
        process_path,
        active_turn_user_messages,
        cancel,
    )
    .await?;
    if candidates.is_empty() {
        return Ok(());
    }

    apply_memory_promotion_candidates(memory_dir, candidates, cancel).await
}

async fn apply_memory_promotion_candidates(
    memory_dir: &Path,
    candidates: Vec<MemoryPromotionCandidate>,
    cancel: &CancellationToken,
) -> Result<()> {
    if candidates.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    ensure_memory_promotion_not_cancelled(cancel)?;
    for candidate in candidates {
        ensure_memory_promotion_not_cancelled(cancel)?;
        let inbox_path = stage_inbox_entry(memory_dir, candidate.draft, now).await?;
        if candidate.disposition == PromotionDisposition::PromoteNow {
            ensure_memory_promotion_not_cancelled(cancel)?;
            promote_inbox_entry(memory_dir, &inbox_path, now).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
async fn capture_confirmed_turn_memory_for_test(
    memory_enabled: bool,
    memory_dir: Option<&Path>,
    llm_client: &mut LlmClient,
    llm_request_timeout_secs: u64,
    machine: &AgentMachine,
    process_path: &str,
    active_turn_start: Option<usize>,
) -> Result<()> {
    if !memory_enabled {
        return Ok(());
    }

    let Some(memory_dir) = memory_dir else {
        return Ok(());
    };

    let active_turn_user_messages =
        active_turn_user_messages(machine.messages(), active_turn_start);
    if active_turn_user_messages.is_empty() {
        return Ok(());
    }

    let cancel = CancellationToken::new();
    let candidates = generate_memory_promotion_candidates(
        llm_client,
        llm_request_timeout_secs,
        process_path,
        &active_turn_user_messages,
        &cancel,
    )
    .await?;
    if candidates.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    for candidate in candidates {
        let inbox_path = stage_inbox_entry(memory_dir, candidate.draft, now).await?;
        if candidate.disposition == PromotionDisposition::PromoteNow {
            promote_inbox_entry(memory_dir, &inbox_path, now).await?;
        }
    }

    Ok(())
}

fn parse_inbox_entry(content: &str) -> Result<InboxEntryDocument> {
    let (frontmatter, body) = split_frontmatter(content)?;
    let frontmatter: InboxEntryFrontmatter =
        serde_yaml::from_str(frontmatter).context("parse inbox frontmatter")?;

    let observation = extract_markdown_section(body, "## Observation")
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let evidence = extract_markdown_section(body, "## Evidence")
        .map(parse_markdown_list)
        .unwrap_or_default();
    let promotion_rationale = extract_markdown_section(body, "## Promotion Rationale")
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    Ok(InboxEntryDocument {
        frontmatter,
        observation,
        evidence,
        promotion_rationale,
    })
}

fn render_inbox_entry(document: &InboxEntryDocument) -> String {
    let frontmatter = render_yaml_without_leading_delimiter(&document.frontmatter)
        .expect("serialize inbox frontmatter");
    let evidence = if document.evidence.is_empty() {
        format!("- {DEFAULT_EVIDENCE_ITEM}")
    } else {
        document
            .evidence
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "---\n{frontmatter}---\n\n## Observation\n{}\n\n## Evidence\n{}\n\n## Promotion Rationale\n{}\n",
        document.observation.trim(),
        evidence,
        document.promotion_rationale.trim()
    )
}

fn resolve_target_path(memory_dir: &Path, target: &str) -> Result<PathBuf> {
    match target {
        MEMORY_USER_FILENAME => Ok(memory_dir.join(MEMORY_USER_FILENAME)),
        MEMORY_STORE_FILENAME => Ok(memory_dir.join(MEMORY_STORE_FILENAME)),
        _ if is_topic_target(target) => Ok(memory_dir
            .join(MEMORY_TOPICS_DIRNAME)
            .join(format!("{}.md", topic_slug_from_target(target)?))),
        _ => bail!("unsupported inbox target path: {target}"),
    }
}

fn ensure_topic_page_frontmatter(
    content: &str,
    title: &str,
    now: DateTime<Utc>,
    source_processes: &[String],
) -> Result<String> {
    let existing_body = if content.trim().is_empty() {
        default_topic_body(title)
    } else if let Ok((_, body)) = split_frontmatter(content) {
        body.trim().to_string()
    } else {
        content.trim().to_string()
    };

    let frontmatter = if let Ok((yaml, _)) = split_frontmatter(content) {
        let mut parsed: TopicPageFrontmatter =
            serde_yaml::from_str(yaml).context("parse topic page frontmatter")?;
        parsed.updated_at = now.to_rfc3339();
        parsed.source_processes = merge_source_processes(parsed.source_processes, source_processes);
        parsed
    } else {
        TopicPageFrontmatter {
            title: title.to_string(),
            aliases: Vec::new(),
            tags: Vec::new(),
            entities: Vec::new(),
            updated_at: now.to_rfc3339(),
            source_processes: dedup_strings(source_processes.to_vec()),
        }
    };

    let frontmatter = render_yaml_without_leading_delimiter(&frontmatter)
        .context("serialize topic page frontmatter")?;
    Ok(format!(
        "---\n{frontmatter}---\n\n{}\n",
        existing_body.trim()
    ))
}

fn default_topic_body(title: &str) -> String {
    format!(
        "# {title}\n\n## Summary\n{DEFAULT_TOPIC_SUMMARY}\n\n## Stable Facts\n\n## Key Decisions\n\n## Open Questions\n\n## References\n"
    )
}

fn append_markdown_section_item(content: &str, heading: &str, item: &str) -> String {
    if item.trim().is_empty() || content.contains(item) {
        return ensure_trailing_newline(content);
    }

    let normalized = ensure_trailing_newline(content);
    if let Some(start) = normalized.find(heading) {
        let search_start = start + heading.len();
        let section_tail = &normalized[search_start..];
        let next_section_offset = section_tail
            .find("\n## ")
            .map(|offset| search_start + offset);
        let insertion_at = next_section_offset.unwrap_or(normalized.len());
        let mut updated = String::with_capacity(normalized.len() + item.len() + 4);
        updated.push_str(&normalized[..insertion_at]);
        if !updated.ends_with("\n\n") {
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push('\n');
        }
        updated.push_str(item.trim());
        updated.push('\n');
        if insertion_at < normalized.len() && !normalized[insertion_at..].starts_with('\n') {
            updated.push('\n');
        }
        updated.push_str(&normalized[insertion_at..]);
        return updated;
    }

    let mut updated = normalized.trim_end().to_string();
    if !updated.is_empty() {
        updated.push_str("\n\n");
    }
    updated.push_str(heading);
    updated.push_str("\n\n");
    updated.push_str(item.trim());
    updated.push('\n');
    updated
}

fn extract_markdown_section<'a>(content: &'a str, heading: &str) -> Option<&'a str> {
    let start = content.find(heading)?;
    let body_start = start + heading.len();
    let section_tail = &content[body_start..];
    let next_section_offset = section_tail.find("\n## ").unwrap_or(section_tail.len());
    Some(section_tail[..next_section_offset].trim())
}

fn split_frontmatter(content: &str) -> Result<(&str, &str)> {
    let trimmed = content.trim_start();
    let remainder = trimmed
        .strip_prefix("---\n")
        .ok_or_else(|| anyhow!("missing frontmatter delimiter"))?;
    let (frontmatter, body) = remainder
        .split_once("\n---\n")
        .ok_or_else(|| anyhow!("missing closing frontmatter delimiter"))?;
    Ok((frontmatter, body))
}

fn render_yaml_without_leading_delimiter<T: Serialize>(value: &T) -> Result<String> {
    let rendered = serde_yaml::to_string(value).context("render yaml")?;
    Ok(rendered
        .strip_prefix("---\n")
        .unwrap_or(rendered.as_str())
        .to_string())
}

fn parse_markdown_list(section: &str) -> Vec<String> {
    section
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- "))
        .map(|line| line.trim_start_matches("- ").trim().to_string())
        .filter(|line| !line.is_empty() && line != DEFAULT_EVIDENCE_ITEM)
        .collect()
}

fn normalize_items(items: Vec<String>) -> Vec<String> {
    dedup_strings(
        items
            .into_iter()
            .map(|item| normalize_inline_text(&item))
            .filter(|item| !item.is_empty())
            .collect(),
    )
}

fn normalize_inline_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn merge_source_processes(mut existing: Vec<String>, additional: &[String]) -> Vec<String> {
    existing.extend(additional.iter().cloned());
    dedup_strings(existing)
}

fn dedup_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

#[cfg(test)]
async fn generate_memory_promotion_candidates(
    llm_client: &mut LlmClient,
    llm_request_timeout_secs: u64,
    process_path: &str,
    active_turn_user_messages: &[String],
    cancel: &CancellationToken,
) -> Result<Vec<MemoryPromotionCandidate>> {
    if active_turn_user_messages.is_empty() {
        return Ok(Vec::new());
    }

    let response = generate_memory_promotion_response(
        llm_client,
        llm_request_timeout_secs,
        build_memory_promotion_request(active_turn_user_messages.to_vec()),
        cancel,
    )
    .await?;

    parse_memory_promotion_candidates(&response.content, process_path)
}

fn ensure_memory_promotion_not_cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        bail!("LLM request cancelled");
    }

    Ok(())
}

fn active_turn_messages(messages: &[Message], active_turn_start: Option<usize>) -> &[Message] {
    let turn_start = active_turn_start.unwrap_or(0).min(messages.len());
    &messages[turn_start..]
}

fn active_turn_user_messages(
    messages: &[Message],
    active_turn_start: Option<usize>,
) -> Vec<String> {
    active_turn_messages(messages, active_turn_start)
        .iter()
        .filter(|message| message.is_user())
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect()
}

fn build_memory_promotion_request(active_turn_user_messages: Vec<String>) -> GenerationRequest {
    let messages = active_turn_user_messages
        .into_iter()
        .map(LlmMessage::user)
        .collect();

    build_generation_request(
        Some(MEMORY_PROMOTION_PROMPT.to_string()),
        messages,
        Vec::new(),
        Some(0.1),
        Some(MEMORY_PROMOTION_MAX_TOKENS),
    )
}

#[cfg(test)]
async fn generate_memory_promotion_response(
    llm_client: &mut LlmClient,
    llm_request_timeout_secs: u64,
    request: GenerationRequest,
    cancel: &CancellationToken,
) -> Result<crate::llm::GenerationResponse> {
    if llm_request_timeout_secs == 0 {
        return tokio::select! {
            _ = cancel.cancelled() => Err(anyhow!("LLM request cancelled")),
            result = llm_client.generate(request) => result.context("generate turn-end memory promotion plan"),
        };
    }

    tokio::select! {
        _ = cancel.cancelled() => Err(anyhow!("LLM request cancelled")),
        result = tokio::time::timeout(
            Duration::from_secs(llm_request_timeout_secs),
            llm_client.generate(request),
        ) => result
            .context("turn-end memory promotion plan timed out")?
            .context("generate turn-end memory promotion plan"),
    }
}

fn parse_memory_promotion_candidates(
    raw: &str,
    process_path: &str,
) -> Result<Vec<MemoryPromotionCandidate>> {
    let json = extract_json_object(raw).ok_or_else(|| {
        anyhow!("turn-end memory promotion response did not contain a JSON object")
    })?;
    let parsed: MemoryPromotionModelOutput = serde_json::from_str(json)
        .context("failed to parse turn-end memory promotion response as JSON")?;

    Ok(normalize_memory_promotion_candidates(parsed, process_path))
}

fn normalize_memory_promotion_candidates(
    raw: MemoryPromotionModelOutput,
    process_path: &str,
) -> Vec<MemoryPromotionCandidate> {
    let mut seen_observations = HashSet::new();

    raw.writes
        .into_iter()
        .filter_map(|write| normalize_memory_promotion_candidate(write, process_path))
        .filter(|candidate| seen_observations.insert(candidate.draft.observation.clone()))
        .take(MEMORY_PROMOTION_MAX_WRITES)
        .collect()
}

fn normalize_memory_promotion_candidate(
    raw: MemoryPromotionModelWrite,
    process_path: &str,
) -> Option<MemoryPromotionCandidate> {
    let kind = normalize_memory_kind(&raw.kind)?;
    let target = canonical_target_for_kind(kind);
    let confidence = normalize_memory_confidence(&raw.confidence)?;
    let observation = normalize_inline_text(&raw.observation);
    let observation =
        truncate_with_suffix(&observation, MEMORY_PROMOTION_MAX_OBSERVATION_CHARS, "...");
    if observation.is_empty() {
        return None;
    }

    let evidence = normalize_items(raw.evidence)
        .into_iter()
        .take(MEMORY_PROMOTION_MAX_EVIDENCE_ITEMS)
        .collect::<Vec<_>>();
    if evidence.is_empty() {
        return None;
    }

    let promotion_rationale = truncate_with_suffix(
        raw.promotion_rationale.trim(),
        MEMORY_PROMOTION_MAX_RATIONALE_CHARS,
        "...",
    );
    if promotion_rationale.is_empty() {
        return None;
    }

    let disposition = normalize_promotion_disposition(&raw.disposition, confidence);
    let target_matches_kind = raw.target.trim().eq_ignore_ascii_case(target);
    if !raw.target.trim().is_empty() && !target_matches_kind {
        return None;
    }

    Some(MemoryPromotionCandidate {
        disposition,
        draft: InboxEntryDraft {
            kind,
            target: target.to_string(),
            confidence,
            observation,
            evidence,
            promotion_rationale,
            source_processes: vec![process_path.to_string()],
        },
    })
}

fn normalize_memory_kind(kind: &str) -> Option<&'static str> {
    match kind.trim() {
        "user_identity" => Some("user_identity"),
        "user_preference" => Some("user_preference"),
        "domain_fact" => Some("domain_fact"),
        "workflow_rule" => Some("workflow_rule"),
        _ => None,
    }
}

fn canonical_target_for_kind(kind: &str) -> &'static str {
    match kind {
        "user_identity" | "user_preference" => MEMORY_USER_FILENAME,
        "domain_fact" | "workflow_rule" => MEMORY_STORE_FILENAME,
        _ => MEMORY_STORE_FILENAME,
    }
}

fn normalize_memory_confidence(confidence: &str) -> Option<&'static str> {
    match confidence.trim() {
        "high" => Some("high"),
        "medium" => Some("medium"),
        "low" => Some("low"),
        _ => None,
    }
}

fn normalize_promotion_disposition(
    disposition: &str,
    confidence: &'static str,
) -> PromotionDisposition {
    match disposition.trim() {
        "promote_now" if confidence == "high" => PromotionDisposition::PromoteNow,
        "promote_now" | "stage_inbox" => PromotionDisposition::StageInbox,
        _ => PromotionDisposition::StageInbox,
    }
}

fn contains_promoted_observation(content: &str, observation: &str) -> bool {
    let observation = observation.trim();
    if observation.is_empty() {
        return false;
    }

    content
        .lines()
        .filter_map(promoted_observation_from_line)
        .any(|existing| existing == observation)
}

fn promoted_observation_from_line(line: &str) -> Option<&str> {
    let line = line.trim();
    let (_, remainder) = line.strip_prefix("- [")?.split_once("] ")?;
    let (observation, _) = remainder.rsplit_once(" (promoted from ")?;
    Some(observation.trim())
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (start <= end).then_some(&trimmed[start..=end])
}

fn is_topic_target(target: &str) -> bool {
    topic_slug_from_target(target).is_ok()
}

fn topic_slug_from_target(target: &str) -> Result<&str> {
    let mut components = Path::new(target).components();
    let Some(Component::Normal(dirname)) = components.next() else {
        bail!("invalid topic target: {target}");
    };
    if dirname != std::ffi::OsStr::new(MEMORY_TOPICS_DIRNAME) {
        bail!("invalid topic target: {target}");
    }

    let Some(Component::Normal(filename)) = components.next() else {
        bail!("invalid topic target: {target}");
    };
    if components.next().is_some() {
        bail!("invalid topic target: {target}");
    }

    filename
        .to_str()
        .and_then(|value| value.strip_suffix(".md"))
        .filter(|value| !value.trim().is_empty() && *value != "." && *value != "..")
        .ok_or_else(|| anyhow!("invalid topic target: {target}"))
}

fn slug_to_title(slug: &str) -> String {
    slug.split('-')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn inbox_entry_path(memory_dir: &Path, now: DateTime<Utc>, id: &str) -> PathBuf {
    memory_dir.join(MEMORY_INBOX_DIRNAME).join(format!(
        "{:04}/{:02}/{:02}/{}.md",
        now.year(),
        now.month(),
        now.day(),
        id
    ))
}

fn format_relative_memory_path(memory_dir: &Path, path: &Path) -> String {
    path.strip_prefix(memory_dir)
        .map(|relative| {
            let relative = relative.to_string_lossy().replace('\\', "/");
            format!("/memory/{relative}")
        })
        .unwrap_or_else(|_| "/memory".to_string())
}

fn truncate_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }

    let suffix_chars = suffix.chars().count();
    if suffix_chars >= max_chars {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_chars))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
}

fn ensure_trailing_newline(content: &str) -> String {
    let mut normalized = content.trim_end().to_string();
    if !normalized.is_empty() {
        normalized.push('\n');
    }
    normalized
}

async fn read_text_file_or_default(path: &Path) -> Result<String> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err).with_context(|| format!("read {}", path.display())),
    }
}

async fn write_text_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    tokio::fs::write(path, content)
        .await
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests;
