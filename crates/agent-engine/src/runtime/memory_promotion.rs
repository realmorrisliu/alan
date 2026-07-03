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
use crate::llm::LlmClient;
use crate::llm::{GenerationRequest, Message as LlmMessage, MessageRole, build_generation_request};
use crate::prompts::{
    MEMORY_INBOX_DIRNAME, MEMORY_PROMOTION_PROMPT, MEMORY_TOPICS_DIRNAME, MEMORY_USER_FILENAME,
    WORKSPACE_MEMORY_FILENAME, ensure_workspace_memory_layout_at,
};
#[cfg(test)]
use crate::session::Session;
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
    pub source_sessions: Vec<String>,
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
    source_sessions: Vec<String>,
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
    source_sessions: Vec<String>,
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
    ensure_workspace_memory_layout_at(memory_dir).with_context(|| {
        format!(
            "failed to ensure workspace memory layout before staging inbox entry at {}",
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
            source_sessions: dedup_strings(draft.source_sessions),
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
    ensure_workspace_memory_layout_at(memory_dir).with_context(|| {
        format!(
            "failed to ensure workspace memory layout before promoting inbox entry at {}",
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
        MEMORY_USER_FILENAME | WORKSPACE_MEMORY_FILENAME => {
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
                &document.frontmatter.source_sessions,
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

            let memory_path = memory_dir.join(WORKSPACE_MEMORY_FILENAME);
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
    session_id: String,
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

    let memory_dir = state.core_config.memory.workspace_dir.clone()?;
    let active_turn_user_messages = active_turn_user_messages(
        state.session.tape.messages(),
        state.turn_state.active_turn_message_start(),
    );
    if active_turn_user_messages.is_empty() {
        return None;
    }

    Some(TurnMemoryPromotionJob {
        memory_dir,
        session_id: state.session.id.clone(),
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
    capture_confirmed_turn_memory_for_session(
        llm_client,
        job.llm_request_timeout_secs,
        &job.memory_dir,
        &job.session_id,
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
    let candidates = parse_memory_promotion_candidates(&response.content, &job.session_id)?;
    apply_memory_promotion_candidates(&job.memory_dir, candidates, cancel).await
}

#[cfg(test)]
async fn capture_confirmed_turn_memory_for_session(
    llm_client: &mut LlmClient,
    llm_request_timeout_secs: u64,
    memory_dir: &Path,
    session_id: &str,
    active_turn_user_messages: &[String],
    cancel: &CancellationToken,
) -> Result<()> {
    let candidates = generate_memory_promotion_candidates(
        llm_client,
        llm_request_timeout_secs,
        session_id,
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
    session: &Session,
    active_turn_start: Option<usize>,
) -> Result<()> {
    if !memory_enabled {
        return Ok(());
    }

    let Some(memory_dir) = memory_dir else {
        return Ok(());
    };

    let active_turn_user_messages =
        active_turn_user_messages(session.tape.messages(), active_turn_start);
    if active_turn_user_messages.is_empty() {
        return Ok(());
    }

    let cancel = CancellationToken::new();
    let candidates = generate_memory_promotion_candidates(
        llm_client,
        llm_request_timeout_secs,
        &session.id,
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
        WORKSPACE_MEMORY_FILENAME => Ok(memory_dir.join(WORKSPACE_MEMORY_FILENAME)),
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
    source_sessions: &[String],
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
        parsed.source_sessions = merge_source_sessions(parsed.source_sessions, source_sessions);
        parsed
    } else {
        TopicPageFrontmatter {
            title: title.to_string(),
            aliases: Vec::new(),
            tags: Vec::new(),
            entities: Vec::new(),
            updated_at: now.to_rfc3339(),
            source_sessions: dedup_strings(source_sessions.to_vec()),
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

fn merge_source_sessions(mut existing: Vec<String>, additional: &[String]) -> Vec<String> {
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
    session_id: &str,
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

    parse_memory_promotion_candidates(&response.content, session_id)
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
        .map(|content| LlmMessage {
            role: MessageRole::User,
            content,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        })
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
    session_id: &str,
) -> Result<Vec<MemoryPromotionCandidate>> {
    let json = extract_json_object(raw).ok_or_else(|| {
        anyhow!("turn-end memory promotion response did not contain a JSON object")
    })?;
    let parsed: MemoryPromotionModelOutput = serde_json::from_str(json)
        .context("failed to parse turn-end memory promotion response as JSON")?;

    Ok(normalize_memory_promotion_candidates(parsed, session_id))
}

fn normalize_memory_promotion_candidates(
    raw: MemoryPromotionModelOutput,
    session_id: &str,
) -> Vec<MemoryPromotionCandidate> {
    let mut seen_observations = HashSet::new();

    raw.writes
        .into_iter()
        .filter_map(|write| normalize_memory_promotion_candidate(write, session_id))
        .filter(|candidate| seen_observations.insert(candidate.draft.observation.clone()))
        .take(MEMORY_PROMOTION_MAX_WRITES)
        .collect()
}

fn normalize_memory_promotion_candidate(
    raw: MemoryPromotionModelWrite,
    session_id: &str,
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
            source_sessions: vec![session_id.to_string()],
        },
    })
}

fn normalize_memory_kind(kind: &str) -> Option<&'static str> {
    match kind.trim() {
        "user_identity" => Some("user_identity"),
        "user_preference" => Some("user_preference"),
        "workspace_fact" => Some("workspace_fact"),
        "workflow_rule" => Some("workflow_rule"),
        _ => None,
    }
}

fn canonical_target_for_kind(kind: &str) -> &'static str {
    match kind {
        "user_identity" | "user_preference" => MEMORY_USER_FILENAME,
        "workspace_fact" | "workflow_rule" => WORKSPACE_MEMORY_FILENAME,
        _ => WORKSPACE_MEMORY_FILENAME,
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
            if let Some(channel_dir) = memory_dir.parent()
                && channel_dir
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == "runtime")
                && let Some(channel_id) = channel_dir.file_name().and_then(|name| name.to_str())
            {
                return format!(".alan/runtime/{channel_id}/memory/{relative}");
            }
            format!(".alan/memory/{relative}")
        })
        .unwrap_or_else(|_| path.display().to_string())
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
mod tests {
    use super::*;
    use std::sync::Arc;

    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use alan_llm::{
        GenerationRequest, GenerationResponse, LlmProvider, MockLlmProvider, StreamChunk,
        TokenUsage,
    };
    use alan_llmfs::LlmFs;
    use async_trait::async_trait;
    use tempfile::TempDir;

    use crate::{
        config::Config,
        runtime::{
            NamespaceRuntimeEnvironment, RuntimeConfig, RuntimeEnvironment, RuntimeLoopState,
            TurnState, prompt_cache::PromptAssemblyCache,
        },
    };

    #[tokio::test]
    async fn stage_inbox_entry_writes_expected_observed_entry() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let inbox_path = stage_inbox_entry(
            &memory_dir,
            InboxEntryDraft {
                kind: "workspace_fact",
                target: WORKSPACE_MEMORY_FILENAME.to_string(),
                confidence: "medium",
                observation: "The recall router should stay lexical-only.".to_string(),
                evidence: vec!["Observed in session summary.".to_string()],
                promotion_rationale: "Useful, but not yet confirmed as stable memory.".to_string(),
                source_sessions: vec!["sess-123".to_string()],
            },
            now,
        )
        .await
        .unwrap();

        let stored = tokio::fs::read_to_string(&inbox_path).await.unwrap();
        let parsed = parse_inbox_entry(&stored).unwrap();
        assert_eq!(parsed.frontmatter.status, "observed");
        assert_eq!(parsed.frontmatter.target, WORKSPACE_MEMORY_FILENAME);
        assert!(stored.contains("## Observation"));
        assert!(stored.contains("lexical-only"));
    }

    #[tokio::test]
    async fn promote_inbox_entry_updates_memory_file_and_marks_confirmed() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let inbox_path = stage_inbox_entry(
            &memory_dir,
            InboxEntryDraft {
                kind: "workspace_fact",
                target: WORKSPACE_MEMORY_FILENAME.to_string(),
                confidence: "high",
                observation: "Keep memory recall lexical and file-backed.".to_string(),
                evidence: vec!["Repeated in design notes.".to_string()],
                promotion_rationale: "Confirmed by the user.".to_string(),
                source_sessions: vec!["sess-456".to_string()],
            },
            now,
        )
        .await
        .unwrap();

        let outcome = promote_inbox_entry(&memory_dir, &inbox_path, now)
            .await
            .unwrap();

        assert_eq!(
            outcome.target_path,
            memory_dir.join(WORKSPACE_MEMORY_FILENAME)
        );
        let memory_file = tokio::fs::read_to_string(memory_dir.join(WORKSPACE_MEMORY_FILENAME))
            .await
            .unwrap();
        assert!(memory_file.contains("## Promoted Facts"));
        assert!(memory_file.contains("lexical and file-backed"));

        let updated_inbox = tokio::fs::read_to_string(inbox_path).await.unwrap();
        let parsed = parse_inbox_entry(&updated_inbox).unwrap();
        assert_eq!(parsed.frontmatter.status, "confirmed");
    }

    #[tokio::test]
    async fn promote_topic_entry_creates_topic_page_and_memory_index() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let inbox_path = stage_inbox_entry(
            &memory_dir,
            InboxEntryDraft {
                kind: "topic_fact",
                target: "topics/memory-router.md".to_string(),
                confidence: "medium",
                observation: "Topic pages are the overflow surface for recurring memory facts."
                    .to_string(),
                evidence: vec!["Repeated across multiple sessions.".to_string()],
                promotion_rationale: "Recurring enough to deserve a topic page.".to_string(),
                source_sessions: vec!["sess-789".to_string()],
            },
            now,
        )
        .await
        .unwrap();

        let outcome = promote_inbox_entry(&memory_dir, &inbox_path, now)
            .await
            .unwrap();

        let topic_path = memory_dir.join("topics/memory-router.md");
        assert_eq!(outcome.target_path, topic_path);

        let topic_page = tokio::fs::read_to_string(memory_dir.join("topics/memory-router.md"))
            .await
            .unwrap();
        assert!(topic_page.contains("title: Memory Router"));
        assert!(topic_page.contains("## Stable Facts"));
        assert!(topic_page.contains("overflow surface"));

        let memory_file = tokio::fs::read_to_string(memory_dir.join(WORKSPACE_MEMORY_FILENAME))
            .await
            .unwrap();
        assert!(memory_file.contains("## Topic Index"));
        assert!(memory_file.contains("memory-router -> topics/memory-router.md"));
    }

    #[tokio::test]
    async fn promote_inbox_entry_rejects_topic_target_path_traversal() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let inbox_path = stage_inbox_entry(
            &memory_dir,
            InboxEntryDraft {
                kind: "topic_fact",
                target: "topics/../../outside.md".to_string(),
                confidence: "medium",
                observation: "Traversal should never be accepted.".to_string(),
                evidence: vec!["Imported inbox entry.".to_string()],
                promotion_rationale: "Regression coverage for target validation.".to_string(),
                source_sessions: vec!["sess-traversal".to_string()],
            },
            now,
        )
        .await
        .unwrap();

        let err = promote_inbox_entry(&memory_dir, &inbox_path, now)
            .await
            .expect_err("traversal target should be rejected");
        assert!(err.to_string().contains("unsupported inbox target path"));
        assert!(!temp.path().join("outside.md").exists());
    }

    #[tokio::test]
    async fn capture_confirmed_turn_memory_promotes_model_selected_user_fact() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let mut session = Session::new();
        session.id = "sess-confirm".to_string();
        session.add_user_message("My name is Dr. Bob.");
        let provider = MockLlmProvider::new().with_response(mock_generation_response(
            serde_json::json!({
                "writes": [
                    {
                        "kind": "user_identity",
                        "target": "USER.md",
                        "confidence": "high",
                        "disposition": "promote_now",
                        "observation": "Name: Dr. Bob",
                        "evidence": ["My name is Dr. Bob."],
                        "promotion_rationale": "Direct user-stated stable identity detail."
                    }
                ]
            })
            .to_string(),
        ));
        let mut llm_client = LlmClient::new(provider);

        capture_confirmed_turn_memory_for_test(
            true,
            Some(&memory_dir),
            &mut llm_client,
            30,
            &session,
            Some(0),
        )
        .await
        .unwrap();

        let user_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_USER_FILENAME))
            .await
            .unwrap();
        assert!(user_memory.contains("Name: Dr. Bob"));

        let inbox_root = memory_dir.join(MEMORY_INBOX_DIRNAME);
        let inbox_entries = collect_markdown_files_recursively(&inbox_root);
        assert!(!inbox_entries.is_empty());
    }

    #[tokio::test]
    async fn capture_confirmed_turn_memory_is_noop_when_memory_disabled() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let mut session = Session::new();
        session.id = "sess-disabled".to_string();
        session.add_user_message("My name is Morris.");
        let provider = MockLlmProvider::new();
        let mut llm_client = LlmClient::new(provider.clone());

        capture_confirmed_turn_memory_for_test(
            false,
            Some(&memory_dir),
            &mut llm_client,
            30,
            &session,
            Some(0),
        )
        .await
        .unwrap();

        let user_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_USER_FILENAME))
            .await
            .unwrap();
        assert_eq!(user_memory, "# User Memory\n");

        let inbox_root = memory_dir.join(MEMORY_INBOX_DIRNAME);
        let inbox_entries = collect_markdown_files_recursively(&inbox_root);
        assert!(inbox_entries.is_empty());
        assert!(provider.recorded_requests().is_empty());
    }

    #[tokio::test]
    async fn promote_inbox_entry_treats_similar_facts_as_distinct_observations() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        let existing_memory = "# User Memory\n\n## Promoted Facts\n\n- [2026-04-14] Name: Bobby (promoted from .alan/memory/inbox/2026/04/14/inbox-old.md)\n";
        tokio::fs::write(memory_dir.join(MEMORY_USER_FILENAME), existing_memory)
            .await
            .unwrap();
        let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let inbox_path = stage_inbox_entry(
            &memory_dir,
            InboxEntryDraft {
                kind: "user_identity",
                target: MEMORY_USER_FILENAME.to_string(),
                confidence: "high",
                observation: "Name: Bob".to_string(),
                evidence: vec!["My name is Bob.".to_string()],
                promotion_rationale: "Direct user-stated stable identity detail.".to_string(),
                source_sessions: vec!["sess-bob".to_string()],
            },
            now,
        )
        .await
        .unwrap();

        promote_inbox_entry(&memory_dir, &inbox_path, now)
            .await
            .unwrap();

        let user_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_USER_FILENAME))
            .await
            .unwrap();
        let promoted_observations = user_memory
            .lines()
            .filter_map(promoted_observation_from_line)
            .collect::<Vec<_>>();
        assert_eq!(promoted_observations, vec!["Name: Bobby", "Name: Bob"]);
    }

    #[tokio::test]
    async fn promote_inbox_entry_sanitizes_multiline_observation_before_writing() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let inbox_path = memory_dir.join("inbox/2026/04/15/inbox-multiline.md");
        tokio::fs::create_dir_all(inbox_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &inbox_path,
            r#"---
id: inbox-multiline
kind: user_identity
status: observed
target: USER.md
confidence: high
created_at: 2026-04-15T10:30:00Z
updated_at: 2026-04-15T10:30:00Z
source_sessions:
  - sess-multiline
---

## Observation
Name: Bob
Preferred editor: Vim

## Evidence
- My name is Bob.

## Promotion Rationale
Direct user-stated stable identity detail.
"#,
        )
        .await
        .unwrap();

        promote_inbox_entry(&memory_dir, &inbox_path, now)
            .await
            .unwrap();

        let user_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_USER_FILENAME))
            .await
            .unwrap();
        assert!(user_memory.contains("Name: Bob Preferred editor: Vim"));

        let confirmed_inbox = tokio::fs::read_to_string(&inbox_path).await.unwrap();
        assert!(confirmed_inbox.contains("## Observation\nName: Bob Preferred editor: Vim\n"));
    }

    #[tokio::test]
    async fn capture_confirmed_turn_memory_stages_medium_confidence_rule_without_promotion() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let mut session = Session::new();
        session.id = "sess-rule".to_string();
        session.add_user_message("The rule is use Python 3.12.");
        let provider = MockLlmProvider::new().with_response(mock_generation_response(
            serde_json::json!({
                "writes": [
                    {
                        "kind": "workflow_rule",
                        "target": "MEMORY.md",
                        "confidence": "medium",
                        "disposition": "stage_inbox",
                        "observation": "Workflow rule: use Python 3.12",
                        "evidence": ["The rule is use Python 3.12."],
                        "promotion_rationale": "Potentially durable workflow rule, but wait for confirmation."
                    }
                ]
            })
            .to_string(),
        ));
        let mut llm_client = LlmClient::new(provider);

        capture_confirmed_turn_memory_for_test(
            true,
            Some(&memory_dir),
            &mut llm_client,
            30,
            &session,
            Some(0),
        )
        .await
        .unwrap();

        let workspace_memory =
            tokio::fs::read_to_string(memory_dir.join(WORKSPACE_MEMORY_FILENAME))
                .await
                .unwrap();
        assert_eq!(workspace_memory, "# Memory\n");

        let inbox_entries =
            collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME));
        assert_eq!(inbox_entries.len(), 1);

        let stored = tokio::fs::read_to_string(&inbox_entries[0]).await.unwrap();
        assert!(stored.contains("status: observed"));
        assert!(stored.contains("Workflow rule: use Python 3.12"));
    }

    #[tokio::test]
    async fn deferred_memory_promotion_uses_namespace_llmfs() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let provider = MockLlmProvider::new().with_response(mock_generation_response(
            serde_json::json!({
                "writes": [
                    {
                        "kind": "workspace_fact",
                        "target": "MEMORY.md",
                        "confidence": "medium",
                        "disposition": "stage_inbox",
                        "observation": "Namespace promotion uses llmfs.",
                        "evidence": ["Remember this namespace fact."],
                        "promotion_rationale": "Captured from a confirmed turn."
                    }
                ]
            })
            .to_string(),
        ));
        let recorded_provider = provider.clone();
        let llmfs = Arc::new(LlmFs::new());
        llmfs.register_connection("default", Box::new(provider));

        let mut namespace = Namespace::new();
        namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let mut state = RuntimeLoopState {
            workspace_id: "test-workspace".to_string(),
            workspace_root_dir: None,
            session: Session::new(),
            current_submission_id: None,
            environment: RuntimeEnvironment::namespace(NamespaceRuntimeEnvironment::new(
                root, "/agent/1", "default",
            )),
            tool_catalog: crate::tools::ToolRegistry::new(),
            core_config: Config::default(),
            runtime_config: RuntimeConfig::default(),
            workspace_persona_dirs: Vec::new(),
            prompt_cache: PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        };
        let job = TurnMemoryPromotionJob {
            memory_dir: memory_dir.clone(),
            session_id: "sess-namespace".to_string(),
            active_turn_user_messages: vec!["Remember this namespace fact.".to_string()],
            llm_request_timeout_secs: 30,
            warning_context: "test",
        };

        run_turn_memory_promotion_job_for_runtime_with_cancel(
            &mut state,
            &job,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        let requests = recorded_provider.recorded_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].system_prompt.as_deref(),
            Some(MEMORY_PROMOTION_PROMPT)
        );
        assert!(
            requests[0]
                .messages
                .iter()
                .any(|message| message.content.contains("Remember this namespace fact."))
        );

        let inbox_entries =
            collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME));
        assert_eq!(inbox_entries.len(), 1);
        let stored = tokio::fs::read_to_string(&inbox_entries[0]).await.unwrap();
        assert!(stored.contains("status: observed"));
        assert!(stored.contains("Namespace promotion uses llmfs."));
    }

    #[tokio::test]
    async fn generate_memory_promotion_candidates_only_uses_active_turn_user_messages() {
        let mut session = Session::new();
        session.id = "sess-active-turn".to_string();
        session.add_user_message("My name is Bob.");
        session.add_assistant_message("Noted.", None);

        let active_turn_start = session.tape.messages().len();

        session.add_user_message("Please continue with the previous task.");
        let provider = MockLlmProvider::new().with_response(mock_generation_response(
            serde_json::json!({ "writes": [] }).to_string(),
        ));
        let mut llm_client = LlmClient::new(provider.clone());

        let active_turn_user_messages =
            active_turn_user_messages(session.tape.messages(), Some(active_turn_start));
        let cancel = CancellationToken::new();
        let drafts = generate_memory_promotion_candidates(
            &mut llm_client,
            30,
            &session.id,
            &active_turn_user_messages,
            &cancel,
        )
        .await
        .unwrap();

        assert!(drafts.is_empty());

        let requests = provider.recorded_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].messages.len(), 1);
        assert_eq!(
            requests[0].messages[0].content,
            "Please continue with the previous task."
        );
    }

    struct DelayedMemoryPromotionProvider {
        delay: Duration,
    }

    struct CancelOnGenerateMemoryPromotionProvider {
        cancel: CancellationToken,
    }

    #[async_trait]
    impl LlmProvider for DelayedMemoryPromotionProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            tokio::time::sleep(self.delay).await;
            Ok(mock_generation_response(
                serde_json::json!({ "writes": [] }).to_string(),
            ))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Err(anyhow!(
                "DelayedMemoryPromotionProvider does not implement chat"
            ))
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            Err(anyhow!(
                "DelayedMemoryPromotionProvider does not implement generate_stream"
            ))
        }

        fn provider_name(&self) -> &'static str {
            "delayed_memory_promotion"
        }
    }

    #[async_trait]
    impl LlmProvider for CancelOnGenerateMemoryPromotionProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.cancel.cancel();
            Ok(mock_generation_response(
                serde_json::json!({
                    "writes": [
                        {
                            "kind": "user_identity",
                            "target": "USER.md",
                            "confidence": "high",
                            "disposition": "promote_now",
                            "observation": "Name: Morris",
                            "evidence": ["My name is Morris."],
                            "promotion_rationale": "Direct user-stated stable identity detail."
                        }
                    ]
                })
                .to_string(),
            ))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Err(anyhow!(
                "CancelOnGenerateMemoryPromotionProvider does not implement chat"
            ))
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            Err(anyhow!(
                "CancelOnGenerateMemoryPromotionProvider does not implement generate_stream"
            ))
        }

        fn provider_name(&self) -> &'static str {
            "cancel_on_generate_memory_promotion"
        }
    }

    #[tokio::test]
    async fn run_turn_memory_promotion_job_timeout_zero_can_be_cancelled() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let mut llm_client = LlmClient::new(DelayedMemoryPromotionProvider {
            delay: Duration::from_secs(10),
        });
        let job = TurnMemoryPromotionJob {
            memory_dir: memory_dir.clone(),
            session_id: "sess-cancelled".to_string(),
            active_turn_user_messages: vec!["My name is Morris.".to_string()],
            llm_request_timeout_secs: 0,
            warning_context: "test cancellation",
        };
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel_for_task.cancel();
        });

        let result =
            run_turn_memory_promotion_job_with_cancel(&mut llm_client, &job, &cancel).await;
        let _ = task.await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(
            collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME)).is_empty()
        );
    }

    #[tokio::test]
    async fn capture_confirmed_turn_memory_stops_before_writes_when_cancelled_after_generation() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let cancel = CancellationToken::new();
        let mut llm_client = LlmClient::new(CancelOnGenerateMemoryPromotionProvider {
            cancel: cancel.clone(),
        });
        let active_turn_user_messages = vec!["My name is Morris.".to_string()];

        let result = capture_confirmed_turn_memory_for_session(
            &mut llm_client,
            30,
            &memory_dir,
            "sess-cancel-after-generation",
            &active_turn_user_messages,
            &cancel,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cancelled"));

        let user_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_USER_FILENAME))
            .await
            .unwrap();
        assert_eq!(user_memory, "# User Memory\n");
        assert!(
            collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME)).is_empty()
        );
    }

    #[test]
    fn parse_memory_promotion_candidates_downgrades_non_high_promote_now_to_stage_inbox() {
        let candidates = parse_memory_promotion_candidates(
            &serde_json::json!({
                "writes": [
                    {
                        "kind": "workflow_rule",
                        "target": "MEMORY.md",
                        "confidence": "medium",
                        "disposition": "promote_now",
                        "observation": "Workflow rule: use Python 3.12",
                        "evidence": ["The rule is use Python 3.12."],
                        "promotion_rationale": "Potentially durable rule."
                    }
                ]
            })
            .to_string(),
            "sess-parse",
        )
        .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].disposition, PromotionDisposition::StageInbox);
        assert_eq!(
            candidates[0].draft.observation,
            "Workflow rule: use Python 3.12"
        );
    }

    #[test]
    fn parse_memory_promotion_candidates_normalizes_multiline_inline_fields() {
        let candidates = parse_memory_promotion_candidates(
            &serde_json::json!({
                "writes": [
                    {
                        "kind": "user_identity",
                        "target": "USER.md",
                        "confidence": "high",
                        "disposition": "promote_now",
                        "observation": "Name: Bob\nPreferred editor: Vim",
                        "evidence": ["My name is Bob.\nI prefer Vim."],
                        "promotion_rationale": "Direct user-stated stable identity detail."
                    }
                ]
            })
            .to_string(),
            "sess-inline-normalize",
        )
        .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].draft.observation,
            "Name: Bob Preferred editor: Vim"
        );
        assert_eq!(
            candidates[0].draft.evidence,
            vec!["My name is Bob. I prefer Vim."]
        );
    }

    #[test]
    fn parse_memory_promotion_candidates_rejects_kind_target_mismatch() {
        let candidates = parse_memory_promotion_candidates(
            &serde_json::json!({
                "writes": [
                    {
                        "kind": "user_identity",
                        "target": "MEMORY.md",
                        "confidence": "high",
                        "disposition": "promote_now",
                        "observation": "Name: Dr. Bob",
                        "evidence": ["My name is Dr. Bob."],
                        "promotion_rationale": "Direct user-stated stable identity detail."
                    }
                ]
            })
            .to_string(),
            "sess-mismatch",
        )
        .unwrap();

        assert!(candidates.is_empty());
    }

    #[test]
    fn promoted_observation_from_line_uses_last_promoted_from_suffix() {
        let line = "- [2026-04-15] Workflow rule: keep literal (promoted from docs) text intact (promoted from .alan/memory/inbox/2026/04/15/inbox-rule.md)";
        assert_eq!(
            promoted_observation_from_line(line),
            Some("Workflow rule: keep literal (promoted from docs) text intact")
        );
    }

    fn mock_generation_response(content: String) -> GenerationResponse {
        GenerationResponse {
            content,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 10,
                cached_prompt_tokens: None,
                completion_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        }
    }

    fn collect_markdown_files_recursively(dir: &Path) -> Vec<PathBuf> {
        let mut collected = Vec::new();
        collect_markdown_files_recursively_inner(dir, &mut collected);
        collected
    }

    fn collect_markdown_files_recursively_inner(dir: &Path, collected: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                collect_markdown_files_recursively_inner(&path, collected);
            } else if path
                .extension()
                .is_some_and(|extension| extension == std::ffi::OsStr::new("md"))
            {
                collected.push(path);
            }
        }
    }
}
