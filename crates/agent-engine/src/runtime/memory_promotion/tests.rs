use super::*;
use std::sync::Arc;

use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_llm::{
    GenerationRequest, GenerationResponse, LlmProvider, MockLlmProvider, StreamChunk, TokenUsage,
};
use alan_llmfs::LlmFs;
use async_trait::async_trait;
use tempfile::TempDir;

use crate::{
    config::Config,
    runtime::{
        NamespaceRuntimeEnvironment, RuntimeConfig, prompt_cache::PromptAssemblyCache,
        transition::RuntimeLoopState,
    },
};

#[tokio::test]
async fn stage_inbox_entry_writes_expected_observed_entry() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let inbox_path = stage_inbox_entry(
        &memory_dir,
        InboxEntryDraft {
            kind: "domain_fact",
            target: MEMORY_STORE_FILENAME.to_string(),
            confidence: "medium",
            observation: "The recall router should stay lexical-only.".to_string(),
            evidence: vec!["Observed in machine summary.".to_string()],
            promotion_rationale: "Useful, but not yet confirmed as stable memory.".to_string(),
            source_processes: vec!["sess-123".to_string()],
        },
        now,
    )
    .await
    .unwrap();

    let stored = tokio::fs::read_to_string(&inbox_path).await.unwrap();
    let parsed = parse_inbox_entry(&stored).unwrap();
    assert_eq!(parsed.frontmatter.status, "observed");
    assert_eq!(parsed.frontmatter.target, MEMORY_STORE_FILENAME);
    assert!(stored.contains("## Observation"));
    assert!(stored.contains("lexical-only"));
}

#[tokio::test]
async fn promote_inbox_entry_updates_memory_file_and_marks_confirmed() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();
    let now = DateTime::parse_from_rfc3339("2026-04-15T10:30:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let inbox_path = stage_inbox_entry(
        &memory_dir,
        InboxEntryDraft {
            kind: "domain_fact",
            target: MEMORY_STORE_FILENAME.to_string(),
            confidence: "high",
            observation: "Keep memory recall lexical and file-backed.".to_string(),
            evidence: vec!["Repeated in design notes.".to_string()],
            promotion_rationale: "Confirmed by the user.".to_string(),
            source_processes: vec!["sess-456".to_string()],
        },
        now,
    )
    .await
    .unwrap();

    let outcome = promote_inbox_entry(&memory_dir, &inbox_path, now)
        .await
        .unwrap();

    assert_eq!(outcome.target_path, memory_dir.join(MEMORY_STORE_FILENAME));
    let memory_file = tokio::fs::read_to_string(memory_dir.join(MEMORY_STORE_FILENAME))
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
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();
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
            evidence: vec!["Repeated across multiple Agent Processes.".to_string()],
            promotion_rationale: "Recurring enough to deserve a topic page.".to_string(),
            source_processes: vec!["sess-789".to_string()],
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

    let memory_file = tokio::fs::read_to_string(memory_dir.join(MEMORY_STORE_FILENAME))
        .await
        .unwrap();
    assert!(memory_file.contains("## Topic Index"));
    assert!(memory_file.contains("memory-router -> topics/memory-router.md"));
}

#[tokio::test]
async fn promote_inbox_entry_rejects_topic_target_path_traversal() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();
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
            source_processes: vec!["sess-traversal".to_string()],
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
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();

    let mut machine = AgentMachine::new();
    machine.add_user_message("My name is Dr. Bob.");
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
        &machine,
        "/proc/test",
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
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();

    let mut machine = AgentMachine::new();
    machine.add_user_message("My name is Morris.");
    let provider = MockLlmProvider::new();
    let mut llm_client = LlmClient::new(provider.clone());

    capture_confirmed_turn_memory_for_test(
        false,
        Some(&memory_dir),
        &mut llm_client,
        30,
        &machine,
        "/proc/test",
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
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();
    let existing_memory = "# User Memory\n\n## Promoted Facts\n\n- [2026-04-14] Name: Bobby (promoted from /memory/inbox/2026/04/14/inbox-old.md)\n";
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
            source_processes: vec!["sess-bob".to_string()],
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
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();
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
source_processes:
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
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();

    let mut machine = AgentMachine::new();
    machine.add_user_message("The rule is use Python 3.12.");
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
        &machine,
        "/proc/test",
        Some(0),
    )
    .await
    .unwrap();

    let durable_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_STORE_FILENAME))
        .await
        .unwrap();
    assert_eq!(durable_memory, "# Memory\n");

    let inbox_entries = collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME));
    assert_eq!(inbox_entries.len(), 1);

    let stored = tokio::fs::read_to_string(&inbox_entries[0]).await.unwrap();
    assert!(stored.contains("status: observed"));
    assert!(stored.contains("Workflow rule: use Python 3.12"));
}

#[tokio::test]
async fn deferred_memory_promotion_uses_namespace_llmfs() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();

    let provider = MockLlmProvider::new().with_response(mock_generation_response(
        serde_json::json!({
            "writes": [
                {
                    "kind": "domain_fact",
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
    let state = RuntimeLoopState {
        machine: AgentMachine::new(),
        environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        core_config: Config::default(),
        runtime_config: RuntimeConfig::default(),
        definition_persona_dirs: Vec::new(),
        prompt_cache: PromptAssemblyCache::new(Vec::new()),
    };
    let job = TurnMemoryPromotionJob {
        memory_dir: memory_dir.clone(),
        process_path: "sess-namespace".to_string(),
        active_turn_user_messages: vec!["Remember this namespace fact.".to_string()],
        llm_request_timeout_secs: 30,
        warning_context: "test",
    };

    let generation = state.namespace_generation();
    run_turn_memory_promotion_job_for_runtime_with_cancel(
        &generation,
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

    let inbox_entries = collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME));
    assert_eq!(inbox_entries.len(), 1);
    let stored = tokio::fs::read_to_string(&inbox_entries[0]).await.unwrap();
    assert!(stored.contains("status: observed"));
    assert!(stored.contains("Namespace promotion uses llmfs."));
}

#[tokio::test]
async fn generate_memory_promotion_candidates_only_uses_active_turn_user_messages() {
    let mut machine = AgentMachine::new();
    machine.add_user_message("My name is Bob.");
    machine.add_assistant_message("Noted.", None);

    let active_turn_start = machine.messages().len();

    machine.add_user_message("Please continue with the previous task.");
    let provider = MockLlmProvider::new().with_response(mock_generation_response(
        serde_json::json!({ "writes": [] }).to_string(),
    ));
    let mut llm_client = LlmClient::new(provider.clone());

    let active_turn_user_messages =
        active_turn_user_messages(machine.messages(), Some(active_turn_start));
    let cancel = CancellationToken::new();
    let drafts = generate_memory_promotion_candidates(
        &mut llm_client,
        30,
        "/proc/test",
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
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();

    let mut llm_client = LlmClient::new(DelayedMemoryPromotionProvider {
        delay: Duration::from_secs(10),
    });
    let job = TurnMemoryPromotionJob {
        memory_dir: memory_dir.clone(),
        process_path: "sess-cancelled".to_string(),
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

    let result = run_turn_memory_promotion_job_with_cancel(&mut llm_client, &job, &cancel).await;
    let _ = task.await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert!(collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME)).is_empty());
}

#[tokio::test]
async fn capture_confirmed_turn_memory_stops_before_writes_when_cancelled_after_generation() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();

    let cancel = CancellationToken::new();
    let mut llm_client = LlmClient::new(CancelOnGenerateMemoryPromotionProvider {
        cancel: cancel.clone(),
    });
    let active_turn_user_messages = vec!["My name is Morris.".to_string()];

    let result = capture_confirmed_turn_memory_for_process(
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
    assert!(collect_markdown_files_recursively(&memory_dir.join(MEMORY_INBOX_DIRNAME)).is_empty());
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
    let line = "- [2026-04-15] Workflow rule: keep literal (promoted from docs) text intact (promoted from /memory/inbox/2026/04/15/inbox-rule.md)";
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
