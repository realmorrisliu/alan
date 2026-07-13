use std::path::Path;

use alan_agent_protocol::{
    CompactionMode, CompactionPressureLevel, MemoryFlushAttemptSnapshot, MemoryFlushResult,
    MemoryFlushSkipReason,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::{
    llm::{Message, MessageRole, build_generation_request},
    prompts::{self, MEMORY_STORE_FILENAME},
};

use super::agent_loop::RuntimeLoopState;
use super::memory_promotion::{InboxEntryDraft, stage_inbox_entry};
use crate::prompts::MEMORY_DAILY_DIRNAME;

const MEMORY_FLUSH_MAX_SECTION_ITEMS: usize = 6;
const MEMORY_FLUSH_MAX_ITEM_CHARS: usize = 240;
const MEMORY_FLUSH_MAX_WHY_CHARS: usize = 320;
const MEMORY_FLUSH_MAX_TOKENS: i32 = 1024;

#[derive(Debug, Deserialize)]
struct MemoryFlushModelOutput {
    #[serde(default)]
    why: String,
    #[serde(default)]
    key_decisions: Vec<String>,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    next_steps: Vec<String>,
    #[serde(default)]
    important_refs: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct MemoryFlushContent {
    why: String,
    key_decisions: Vec<String>,
    constraints: Vec<String>,
    next_steps: Vec<String>,
    important_refs: Vec<String>,
}

pub(crate) async fn perform_memory_flush_attempt(
    state: &mut RuntimeLoopState,
    compaction_mode: CompactionMode,
    pressure_level: CompactionPressureLevel,
    sanitized_messages: &[crate::tape::Message],
    cancel: &CancellationToken,
) -> MemoryFlushAttemptSnapshot {
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let timestamp = now.to_rfc3339();
    let note_date = now.format("%F").to_string();
    let source_messages = Some(sanitized_messages.len());

    if !state.core_config.memory.enabled {
        return skipped_attempt(
            attempt_id,
            compaction_mode,
            pressure_level,
            MemoryFlushSkipReason::MemoryDisabled,
            source_messages,
            timestamp,
        );
    }

    let Some(memory_dir) = state.core_config.memory.store_dir.clone() else {
        return skipped_attempt(
            attempt_id,
            compaction_mode,
            pressure_level,
            MemoryFlushSkipReason::MissingMemoryDir,
            source_messages,
            timestamp,
        );
    };

    match tokio::fs::metadata(&memory_dir).await {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return skipped_attempt(
                attempt_id,
                compaction_mode,
                pressure_level,
                MemoryFlushSkipReason::MissingMemoryDir,
                source_messages,
                timestamp,
            );
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return skipped_attempt(
                attempt_id,
                compaction_mode,
                pressure_level,
                MemoryFlushSkipReason::MissingMemoryDir,
                source_messages,
                timestamp,
            );
        }
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            return skipped_attempt(
                attempt_id,
                compaction_mode,
                pressure_level,
                MemoryFlushSkipReason::ReadOnlyMemoryDir,
                source_messages,
                timestamp,
            );
        }
        Err(err) => {
            return failure_attempt(
                attempt_id,
                compaction_mode,
                pressure_level,
                source_messages,
                timestamp,
                format!("failed to inspect memory directory: {err}"),
            );
        }
    }

    let flush_content = match generate_flush_content(state, sanitized_messages, cancel).await {
        Ok(Some(content)) => content,
        Ok(None) => {
            return skipped_attempt(
                attempt_id,
                compaction_mode,
                pressure_level,
                MemoryFlushSkipReason::NoDurableContent,
                source_messages,
                timestamp,
            );
        }
        Err(_err) if cancel.is_cancelled() => {
            return skipped_attempt(
                attempt_id,
                compaction_mode,
                pressure_level,
                MemoryFlushSkipReason::Cancelled,
                source_messages,
                timestamp,
            );
        }
        Err(err) => {
            return failure_attempt(
                attempt_id,
                compaction_mode,
                pressure_level,
                source_messages,
                timestamp,
                err.to_string(),
            );
        }
    };

    if cancel.is_cancelled() {
        return skipped_attempt(
            attempt_id,
            compaction_mode,
            pressure_level,
            MemoryFlushSkipReason::Cancelled,
            source_messages,
            timestamp,
        );
    }

    let note_path = memory_dir
        .join(MEMORY_DAILY_DIRNAME)
        .join(format!("{note_date}.md"));
    let process_path = state.process_path().to_string();
    let entry = render_memory_flush_entry(
        &process_path,
        &attempt_id,
        compaction_mode,
        pressure_level,
        source_messages,
        &flush_content,
        &timestamp,
    );
    match append_memory_entry(&note_path, &entry).await {
        Ok(()) => {
            if let Some(inbox_draft) =
                build_memory_flush_inbox_draft(&process_path, &attempt_id, &flush_content)
                && let Err(err) = stage_inbox_entry(&memory_dir, inbox_draft, now).await
            {
                tracing::warn!(
                    error = %err,
                    memory_dir = %memory_dir.display(),
                    "failed to stage memory flush inbox entry"
                );
            }

            MemoryFlushAttemptSnapshot {
                attempt_id,
                compaction_mode,
                pressure_level,
                result: MemoryFlushResult::Success,
                skip_reason: None,
                source_messages,
                output_path: Some(snapshot_output_path(&memory_dir, &note_path)),
                warning_message: None,
                error_message: None,
                timestamp,
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => skipped_attempt(
            attempt_id,
            compaction_mode,
            pressure_level,
            MemoryFlushSkipReason::ReadOnlyMemoryDir,
            source_messages,
            timestamp,
        ),
        Err(err) => failure_attempt(
            attempt_id,
            compaction_mode,
            pressure_level,
            source_messages,
            timestamp,
            format!("failed to append memory flush note: {err}"),
        ),
    }
}

pub(crate) fn skipped_memory_flush_attempt(
    compaction_mode: CompactionMode,
    pressure_level: CompactionPressureLevel,
    reason: MemoryFlushSkipReason,
    source_messages: Option<usize>,
) -> MemoryFlushAttemptSnapshot {
    skipped_attempt(
        uuid::Uuid::new_v4().to_string(),
        compaction_mode,
        pressure_level,
        reason,
        source_messages,
        chrono::Utc::now().to_rfc3339(),
    )
}

fn skipped_attempt(
    attempt_id: String,
    compaction_mode: CompactionMode,
    pressure_level: CompactionPressureLevel,
    reason: MemoryFlushSkipReason,
    source_messages: Option<usize>,
    timestamp: String,
) -> MemoryFlushAttemptSnapshot {
    MemoryFlushAttemptSnapshot {
        attempt_id,
        compaction_mode,
        pressure_level,
        result: MemoryFlushResult::Skipped,
        skip_reason: Some(reason),
        source_messages,
        output_path: None,
        warning_message: None,
        error_message: None,
        timestamp,
    }
}

fn failure_attempt(
    attempt_id: String,
    compaction_mode: CompactionMode,
    pressure_level: CompactionPressureLevel,
    source_messages: Option<usize>,
    timestamp: String,
    error_message: String,
) -> MemoryFlushAttemptSnapshot {
    let warning_message = format!(
        "Silent memory flush failed before compaction: {error_message}. Continuing with compaction."
    );
    MemoryFlushAttemptSnapshot {
        attempt_id,
        compaction_mode,
        pressure_level,
        result: MemoryFlushResult::Failure,
        skip_reason: None,
        source_messages,
        output_path: None,
        warning_message: Some(warning_message),
        error_message: Some(error_message),
        timestamp,
    }
}

async fn generate_flush_content(
    state: &mut RuntimeLoopState,
    sanitized_messages: &[crate::tape::Message],
    cancel: &CancellationToken,
) -> Result<Option<MemoryFlushContent>> {
    let mut llm_messages = Vec::new();
    if let Some(existing_summary) = state.machine.tape.summary() {
        llm_messages.push(Message {
            role: MessageRole::Context,
            content: format!("[Current compaction summary]\n{existing_summary}"),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }
    llm_messages.extend(state.project_generation_messages(sanitized_messages));

    let request = build_generation_request(
        Some(prompts::MEMORY_FLUSH_PROMPT.to_string()),
        llm_messages,
        Vec::new(),
        Some(0.1),
        Some(MEMORY_FLUSH_MAX_TOKENS),
    );

    let response = state
        .generate_once_with_cancel(request, cancel, "memory flush cancelled")
        .await?;

    parse_memory_flush_content(&response.content)
}

fn parse_memory_flush_content(raw: &str) -> Result<Option<MemoryFlushContent>> {
    let json = extract_json_object(raw)
        .ok_or_else(|| anyhow::anyhow!("memory flush response did not contain a JSON object"))?;
    let parsed: MemoryFlushModelOutput =
        serde_json::from_str(json).context("failed to parse memory flush response as JSON")?;
    Ok(normalize_memory_flush_content(parsed))
}

fn normalize_memory_flush_content(raw: MemoryFlushModelOutput) -> Option<MemoryFlushContent> {
    let why = truncate_with_suffix(raw.why.trim(), MEMORY_FLUSH_MAX_WHY_CHARS, "...");
    let key_decisions = normalize_items(raw.key_decisions);
    let constraints = normalize_items(raw.constraints);
    let next_steps = normalize_items(raw.next_steps);
    let important_refs = normalize_items(raw.important_refs);

    if why.is_empty()
        && key_decisions.is_empty()
        && constraints.is_empty()
        && next_steps.is_empty()
        && important_refs.is_empty()
    {
        return None;
    }

    Some(MemoryFlushContent {
        why,
        key_decisions,
        constraints,
        next_steps,
        important_refs,
    })
}

fn normalize_items(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|item| {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(truncate_with_suffix(
                    trimmed,
                    MEMORY_FLUSH_MAX_ITEM_CHARS,
                    "...",
                ))
            }
        })
        .take(MEMORY_FLUSH_MAX_SECTION_ITEMS)
        .collect()
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (start <= end).then_some(&trimmed[start..=end])
}

fn render_memory_flush_entry(
    process_path: &str,
    attempt_id: &str,
    compaction_mode: CompactionMode,
    pressure_level: CompactionPressureLevel,
    source_messages: Option<usize>,
    content: &MemoryFlushContent,
    timestamp: &str,
) -> String {
    let mut lines = vec![
        format!("## {timestamp}"),
        String::new(),
        format!("- process_path: `{process_path}`"),
        format!("- attempt_id: `{attempt_id}`"),
        format!("- compaction_mode: `{}`", mode_label(compaction_mode)),
        format!("- pressure_level: `{}`", pressure_label(pressure_level)),
    ];

    if let Some(source_messages) = source_messages {
        lines.push(format!("- source_messages: {source_messages}"));
    }

    if !content.why.is_empty() {
        lines.push(String::new());
        lines.push("### Why".to_string());
        lines.push(content.why.clone());
    }

    push_section(&mut lines, "### Key Decisions", &content.key_decisions);
    push_section(&mut lines, "### Constraints", &content.constraints);
    push_section(&mut lines, "### Next Steps", &content.next_steps);
    push_section(&mut lines, "### Important Refs", &content.important_refs);

    lines.join("\n")
}

fn build_memory_flush_inbox_draft(
    process_path: &str,
    attempt_id: &str,
    content: &MemoryFlushContent,
) -> Option<InboxEntryDraft> {
    let observation = if !content.why.is_empty() {
        content.why.clone()
    } else {
        content
            .key_decisions
            .first()
            .or_else(|| content.constraints.first())
            .or_else(|| content.next_steps.first())
            .cloned()
            .unwrap_or_default()
    };
    if observation.trim().is_empty() {
        return None;
    }

    let mut evidence = Vec::new();
    evidence.extend(content.key_decisions.iter().cloned());
    evidence.extend(content.constraints.iter().cloned());
    evidence.extend(content.next_steps.iter().cloned());
    evidence.extend(content.important_refs.iter().cloned());

    Some(InboxEntryDraft {
        kind: "domain_fact",
        target: MEMORY_STORE_FILENAME.to_string(),
        confidence: if content.key_decisions.is_empty() && content.constraints.is_empty() {
            "low"
        } else {
            "medium"
        },
        observation,
        evidence,
        promotion_rationale: format!(
            "Captured from automatic memory flush attempt `{attempt_id}` in machine `{process_path}`. Review before promoting into stable memory."
        ),
        source_processes: vec![process_path.to_string()],
    })
}

fn push_section(lines: &mut Vec<String>, title: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    lines.push(String::new());
    lines.push(title.to_string());
    lines.extend(items.iter().map(|item| format!("- {item}")));
}

async fn append_memory_entry(note_path: &Path, entry: &str) -> std::io::Result<()> {
    if let Some(parent) = note_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let existing_len = match tokio::fs::metadata(note_path).await {
        Ok(metadata) => metadata.len(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => 0,
        Err(err) => return Err(err),
    };

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(note_path)
        .await?;

    if existing_len > 0 {
        file.write_all(b"\n\n").await?;
    }
    file.write_all(entry.as_bytes()).await?;
    file.write_all(b"\n").await
}

fn snapshot_output_path(memory_dir: &Path, note_path: &Path) -> String {
    note_path
        .strip_prefix(memory_dir)
        .map(|relative| format!("/memory/{}", relative.to_string_lossy().replace('\\', "/")))
        .unwrap_or_else(|_| "/memory".to_string())
}

fn mode_label(mode: CompactionMode) -> &'static str {
    match mode {
        CompactionMode::Manual => "manual",
        CompactionMode::AutoPreTurn => "auto_pre_turn",
        CompactionMode::AutoMidTurn => "auto_mid_turn",
    }
}

fn pressure_label(level: CompactionPressureLevel) -> &'static str {
    match level {
        CompactionPressureLevel::BelowSoft => "below_soft",
        CompactionPressureLevel::Soft => "soft",
        CompactionPressureLevel::Hard => "hard",
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
    use alan_llmfs::LlmFs;
    use tokio::sync::mpsc;

    use crate::{
        agent_machine::AgentMachine,
        config::Config,
        runtime::{
            NamespaceRuntimeEnvironment, RuntimeConfig, RuntimeLoopState, TurnState,
            prompt_cache::PromptAssemblyCache,
        },
    };
    use std::path::PathBuf;

    struct RecordingProvider {
        requests: Arc<Mutex<Vec<GenerationRequest>>>,
        response: String,
    }

    #[async_trait::async_trait]
    impl LlmProvider for RecordingProvider {
        async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
            unimplemented!()
        }

        async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
            unimplemented!()
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
            self.requests.lock().unwrap().push(request);
            let (tx, rx) = mpsc::channel(4);
            let response = self.response.clone();
            tokio::spawn(async move {
                let _ = tx
                    .send(StreamChunk {
                        text: Some(response),
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("stop".to_string()),
                    })
                    .await;
            });
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "recording"
        }
    }

    fn namespace_state_with_provider(provider: impl LlmProvider + 'static) -> RuntimeLoopState {
        let llmfs = Arc::new(LlmFs::new());
        llmfs.register_connection("default", Box::new(provider));

        let mut namespace = Namespace::new();
        namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));

        RuntimeLoopState {
            machine: AgentMachine::new(),
            current_submission_id: None,
            environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            core_config: Config::default(),
            runtime_config: RuntimeConfig::default(),
            definition_persona_dirs: Vec::new(),
            prompt_cache: PromptAssemblyCache::new(Vec::new()),
            turn_state: TurnState::default(),
        }
    }

    #[tokio::test]
    async fn memory_flush_generation_uses_namespace_llmfs() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut state = namespace_state_with_provider(RecordingProvider {
            requests: Arc::clone(&requests),
            response: r#"{"why":"namespace flush","key_decisions":["via llmfs"],"constraints":[],"next_steps":[],"important_refs":["/mnt/llm"]}"#
                .to_string(),
        });
        state
            .machine
            .add_user_message("remember this namespace fact");
        let messages = state.machine.tape.messages().to_vec();

        let content = generate_flush_content(&mut state, &messages, &CancellationToken::new())
            .await
            .unwrap()
            .expect("flush content");

        assert_eq!(content.why, "namespace flush");
        assert_eq!(content.key_decisions, vec!["via llmfs"]);
        let recorded = requests.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0].system_prompt.as_deref(),
            Some(prompts::MEMORY_FLUSH_PROMPT)
        );
        assert!(
            recorded[0]
                .messages
                .iter()
                .any(|message| message.content.contains("remember this namespace fact"))
        );
    }

    #[test]
    fn test_parse_memory_flush_content_accepts_json_fences() {
        let parsed = parse_memory_flush_content(
            "```json\n{\"why\":\"retain blockers\",\"key_decisions\":[\"Use cargo test\"],\"constraints\":[],\"next_steps\":[\"land PR\"],\"important_refs\":[\"crates/agent-engine/src/runtime/compaction.rs\"]}\n```",
        )
        .unwrap()
        .expect("expected durable flush content");

        assert_eq!(parsed.why, "retain blockers");
        assert_eq!(parsed.key_decisions, vec!["Use cargo test"]);
        assert_eq!(parsed.next_steps, vec!["land PR"]);
        assert_eq!(
            parsed.important_refs,
            vec!["crates/agent-engine/src/runtime/compaction.rs"]
        );
    }

    #[test]
    fn test_parse_memory_flush_content_treats_empty_payload_as_noop() {
        let parsed = parse_memory_flush_content(
            "{\"why\":\"\",\"key_decisions\":[],\"constraints\":[],\"next_steps\":[],\"important_refs\":[]}",
        )
        .unwrap();

        assert_eq!(parsed, None);
    }

    #[test]
    fn test_render_memory_flush_entry_includes_required_metadata() {
        let entry = render_memory_flush_entry(
            "sess-123",
            "flush-456",
            CompactionMode::AutoPreTurn,
            CompactionPressureLevel::Soft,
            Some(7),
            &MemoryFlushContent {
                why: "retain stable blockers".to_string(),
                key_decisions: vec!["Keep the degraded fallback".to_string()],
                constraints: vec!["Do not orphan tool results".to_string()],
                next_steps: vec!["Ship the follow-up PR".to_string()],
                important_refs: vec!["crates/agent-engine/src/tape.rs".to_string()],
            },
            "2026-03-18T08:00:00Z",
        );

        assert!(entry.contains("process_path: `sess-123`"));
        assert!(entry.contains("attempt_id: `flush-456`"));
        assert!(entry.contains("compaction_mode: `auto_pre_turn`"));
        assert!(entry.contains("pressure_level: `soft`"));
        assert!(entry.contains("source_messages: 7"));
        assert!(entry.contains("crates/agent-engine/src/tape.rs"));
    }

    #[test]
    fn test_snapshot_output_path_uses_memory_store_namespace_path() {
        let memory_dir = PathBuf::from("/host/system-store/memory/stores/default");
        let note_path = memory_dir.join("daily/2026-03-18.md");
        assert_eq!(
            snapshot_output_path(&memory_dir, &note_path),
            "/memory/daily/2026-03-18.md"
        );
    }

    #[test]
    fn test_snapshot_output_path_never_exposes_host_backing_path() {
        let memory_dir = PathBuf::from("/host/system-store/memory/stores/default");
        let outside = PathBuf::from("/host/private/note.md");
        assert_eq!(snapshot_output_path(&memory_dir, &outside), "/memory");
    }

    #[test]
    fn test_build_memory_flush_inbox_draft_targets_memory_md() {
        let draft = build_memory_flush_inbox_draft(
            "sess-123",
            "flush-456",
            &MemoryFlushContent {
                why: "Preserve the current rollout constraints.".to_string(),
                key_decisions: vec!["Keep the lexical recall path.".to_string()],
                constraints: vec!["Do not introduce vector search.".to_string()],
                next_steps: vec!["Land the next slice.".to_string()],
                important_refs: vec!["docs/spec/pure_text_memory_contract.md".to_string()],
            },
        )
        .expect("expected inbox draft");

        assert_eq!(draft.kind, "domain_fact");
        assert_eq!(draft.target, MEMORY_STORE_FILENAME);
        assert_eq!(draft.confidence, "medium");
        assert!(
            draft
                .observation
                .contains("Preserve the current rollout constraints.")
        );
        assert!(
            draft
                .promotion_rationale
                .contains("automatic memory flush attempt `flush-456`")
        );
    }
}
