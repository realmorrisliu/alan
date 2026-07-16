use super::*;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tempfile::TempDir;
use tokio::io::AsyncWrite;
use tokio::time::{Duration, Instant, sleep};

#[derive(Default)]
struct FlushFailWriter {
    buffer: Vec<u8>,
}

impl AsyncWrite for FlushFailWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.buffer.extend_from_slice(buf);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("synthetic flush failure")))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[test]
fn test_rollout_recorder_creation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new().unwrap();
        let recorder =
            RolloutRecorder::new_in_dir("/proc/123", "gemini-2.0-flash", temp_dir.path()).await;
        assert!(recorder.is_ok());

        let recorder = recorder.unwrap();
        let path = recorder.path();
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("rollout-")
        );
        assert!(path.to_string_lossy().ends_with(".jsonl"));
        let items = RolloutRecorder::load_history(path).await.unwrap();
        assert!(matches!(
            items.first(),
            Some(RolloutItem::AgentMachineMeta(meta)) if meta.process_path == "/proc/123"
        ));

        // Clean up - remove the created file
        let _ = fs::remove_file(path).await;
    });
}

#[test]
fn test_rollout_recorder_creation_records_explicit_cwd() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new().unwrap();
        let explicit_cwd = Path::new("/mnt/source/src");

        let recorder = RolloutRecorder::new_in_dir_with_cwd(
            "test-machine-cwd",
            "gemini-2.0-flash",
            temp_dir.path(),
            Some(explicit_cwd),
        )
        .await
        .unwrap();

        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        match &items[0] {
            RolloutItem::AgentMachineMeta(meta) => {
                assert_eq!(meta.cwd, "/mnt/source/src");
            }
            _ => panic!("Expected AgentMachineMeta"),
        }

        let _ = fs::remove_file(recorder.path()).await;
    });
}

#[test]
fn test_record_message_flushes() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let temp_dir = TempDir::new().unwrap();
        let recorder =
            RolloutRecorder::new_in_dir("test-machine-flush", "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();
        recorder
            .record_message("user", Some("Hello"), None)
            .await
            .unwrap();

        let start = Instant::now();
        let mut found = false;
        while start.elapsed() < Duration::from_secs(1) {
            if let Ok(content) = fs::read_to_string(recorder.path()).await
                && content.contains("\"type\":\"message\"")
                && content.contains("Hello")
            {
                found = true;
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }

        assert!(found, "Expected message to be flushed to rollout file");
    });
}

#[tokio::test]
async fn test_record_tape_message_persists_rich_message() {
    let temp_dir = TempDir::new().unwrap();
    let recorder =
        RolloutRecorder::new_in_dir("test-rich-message", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();

    let message = crate::tape::Message::Assistant {
        parts: vec![
            crate::tape::ContentPart::thinking("internal reasoning"),
            crate::tape::ContentPart::text("final answer"),
        ],
        tool_requests: vec![],
    };
    recorder.record_tape_message(&message).await.unwrap();

    let items = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    let restored = items.into_iter().find_map(|item| match item {
        RolloutItem::Message(msg) => msg.message,
        _ => None,
    });

    let restored = restored.expect("expected rich message payload");
    assert_eq!(restored.non_thinking_text_content(), "final answer");
    assert_eq!(
        restored.thinking_content().as_deref(),
        Some("internal reasoning")
    );
}

#[tokio::test]
async fn test_record_checkpoint_persists_knowledge_root() {
    let temp_dir = TempDir::new().unwrap();
    let recorder =
        RolloutRecorder::new_in_dir("test-root-checkpoint", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();

    recorder
        .record_checkpoint_with_knowledge_root(
            "turn-1",
            "turn_completed",
            "turn completed",
            None,
            "sha256:abc123",
        )
        .await
        .unwrap();

    let items = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    let checkpoint = items.into_iter().find_map(|item| match item {
        RolloutItem::Checkpoint(checkpoint) => Some(checkpoint),
        _ => None,
    });

    let checkpoint = checkpoint.expect("checkpoint should be persisted");
    assert_eq!(checkpoint.checkpoint_id, "turn-1");
    assert_eq!(checkpoint.knowledge_root.as_deref(), Some("sha256:abc123"));
}

#[tokio::test]
async fn test_load_history() {
    let temp_dir = TempDir::new().unwrap();
    let recorder = RolloutRecorder::new_in_dir("/proc/7", "gemini-2.0-flash", temp_dir.path())
        .await
        .unwrap();
    recorder
        .record_tape_message(&crate::tape::Message::user("Hello"))
        .await
        .unwrap();
    recorder
        .record_tool_call(
            "test_tool",
            serde_json::json!({}),
            serde_json::json!({}),
            true,
        )
        .await
        .unwrap();

    let items = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    assert_eq!(items.len(), 3);

    // Verify first item is machine meta
    match &items[0] {
        RolloutItem::AgentMachineMeta(meta) => {
            assert_eq!(meta.process_path, "/proc/7");
            assert!(!meta.rollout_id.is_empty());
            assert_eq!(meta.model, "gemini-2.0-flash");
        }
        _ => panic!("Expected AgentMachineMeta"),
    }

    // Verify second item is message
    match &items[1] {
        RolloutItem::Message(msg) => {
            assert_eq!(msg.role, "user");
            assert_eq!(msg.content, Some("Hello".to_string()));
            assert!(msg.tool_name.is_none());
        }
        _ => panic!("Expected Message"),
    }

    // Verify third item is tool call
    match &items[2] {
        RolloutItem::ToolCall(tool) => {
            assert_eq!(tool.name, "test_tool");
            assert!(tool.success);
        }
        _ => panic!("Expected ToolCall"),
    }
}

#[tokio::test]
async fn test_persist_batch_writes_compaction_attempt_and_summary_together() {
    let temp_dir = TempDir::new().unwrap();
    let recorder =
        RolloutRecorder::new_in_dir("test-compaction-batch", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
    let attempt = CompactionAttemptSnapshot {
        attempt_id: "attempt-123".to_string(),
        submission_id: Some("sub-456".to_string()),
        request: alan_agent_protocol::CompactionRequestMetadata {
            mode: alan_agent_protocol::CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: Some("preserve todos".to_string()),
        },
        result: CompactionResult::Retry,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(12),
        output_messages: Some(4),
        input_prompt_tokens: Some(900),
        output_prompt_tokens: Some(300),
        retry_count: 1,
        tape_mutated: true,
        warning_message: None,
        error_message: None,
        failure_streak: None,
        reference_context_revision_before: Some(3),
        reference_context_revision_after: Some(3),
        timestamp: "2026-01-29T14:31:00Z".to_string(),
    };
    let compacted = CompactedItem {
        message: "Summary after retry".to_string(),
        attempt_id: Some(attempt.attempt_id.clone()),
        trigger: Some(CompactionTrigger::Manual),
        reason: Some(CompactionReason::ExplicitRequest),
        focus: Some("preserve todos".to_string()),
        input_messages: Some(12),
        output_messages: Some(4),
        input_tokens: Some(900),
        output_tokens: Some(300),
        duration_ms: Some(42),
        retry_count: Some(1),
        result: Some(CompactionResult::Retry),
        reference_context_revision: Some(3),
        timestamp: "2026-01-29T14:31:01Z".to_string(),
    };

    recorder
        .persist_batch(vec![
            RolloutItem::CompactionAttempt(attempt.clone()),
            RolloutItem::Compacted(compacted.clone()),
        ])
        .await
        .unwrap();

    let items = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    let persisted_attempt = items.iter().find_map(|item| match item {
        RolloutItem::CompactionAttempt(attempt) => Some(attempt),
        _ => None,
    });
    let persisted_compacted = items.iter().find_map(|item| match item {
        RolloutItem::Compacted(compacted) => Some(compacted),
        _ => None,
    });

    assert_eq!(persisted_attempt, Some(&attempt));
    assert_eq!(
        persisted_compacted.map(|item| item.attempt_id.as_deref()),
        Some(Some("attempt-123"))
    );
    assert_eq!(
        persisted_compacted.map(|item| item.message.as_str()),
        Some("Summary after retry")
    );
}

#[tokio::test]
async fn test_persist_items_and_flush_propagates_flush_error() {
    let attempt = CompactionAttemptSnapshot {
        attempt_id: "attempt-flush-failure".to_string(),
        submission_id: None,
        request: alan_agent_protocol::CompactionRequestMetadata {
            mode: alan_agent_protocol::CompactionMode::Manual,
            trigger: CompactionTrigger::Manual,
            reason: CompactionReason::ExplicitRequest,
            focus: None,
        },
        result: CompactionResult::Failure,
        pressure_level: None,
        memory_flush_attempt_id: None,
        input_messages: Some(4),
        output_messages: None,
        input_prompt_tokens: Some(256),
        output_prompt_tokens: None,
        retry_count: 0,
        tape_mutated: false,
        warning_message: None,
        error_message: Some("synthetic".to_string()),
        failure_streak: Some(1),
        reference_context_revision_before: Some(2),
        reference_context_revision_after: None,
        timestamp: "2026-03-18T00:00:00Z".to_string(),
    };
    let mut writer = FlushFailWriter::default();

    let err = RolloutRecorder::persist_items_and_flush(
        &mut writer,
        &[RolloutItem::CompactionAttempt(attempt)],
    )
    .await
    .expect_err("flush failure should be returned to the caller");

    assert!(err.to_string().contains("synthetic flush failure"));
    assert!(
        !writer.buffer.is_empty(),
        "writer should receive bytes before flush fails"
    );
}

#[tokio::test]
async fn test_flush_writer_propagates_flush_error() {
    let mut writer = FlushFailWriter::default();

    let err = RolloutRecorder::flush_writer(&mut writer)
        .await
        .expect_err("flush failure should be returned to the caller");

    assert!(err.to_string().contains("synthetic flush failure"));
}

#[tokio::test]
async fn test_load_history_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("empty.jsonl");

    fs::write(&file_path, "").await.unwrap();

    let items = RolloutRecorder::load_history(&file_path).await.unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn test_load_history_with_empty_lines() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("with_empty_lines.jsonl");

    let content = r#"
{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}

{"type":"message","role":"assistant","content":"Hi!","tool_name":null,"timestamp":"2026-01-29T14:30:56Z"}
"#;

    fs::write(&file_path, content).await.unwrap();

    let items = RolloutRecorder::load_history(&file_path).await.unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn test_load_history_with_invalid_lines() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("with_invalid.jsonl");

    let content = r#"{"type":"message","role":"user","content":"Valid","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
this is not valid json
{"type":"message","role":"assistant","content":"Also valid","tool_name":null,"timestamp":"2026-01-29T14:30:56Z"}
"#;

    fs::write(&file_path, content).await.unwrap();

    let error = RolloutRecorder::load_history(&file_path).await.unwrap_err();
    assert!(error.to_string().contains("invalid current rollout record"));
}

#[tokio::test]
async fn test_load_history_ignores_torn_trailing_json_record() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("with_torn_tail.jsonl");
    let content = concat!(
        "{\"type\":\"message\",\"role\":\"user\",\"content\":\"Valid\",",
        "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
        "{\"type\":\"message\",\"role\":\"assistant\",\"content\":"
    );

    fs::write(&file_path, content).await.unwrap();

    let items = RolloutRecorder::load_history(&file_path).await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn test_load_history_ignores_torn_trailing_utf8_record() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("with_torn_utf8_tail.jsonl");
    let mut content = concat!(
        "{\"type\":\"message\",\"role\":\"user\",\"content\":\"Valid\",",
        "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
        "{\"type\":\"message\",\"role\":\"assistant\",\"content\":\""
    )
    .as_bytes()
    .to_vec();
    content.extend_from_slice(&[0xe2, 0x82]);

    fs::write(&file_path, content).await.unwrap();

    let items = RolloutRecorder::load_history(&file_path).await.unwrap();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn test_load_history_rejects_non_torn_invalid_trailing_record() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("with_invalid_tail.jsonl");
    let content = concat!(
        "{\"type\":\"message\",\"role\":\"user\",\"content\":\"Valid\",",
        "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
        "not json"
    );

    fs::write(&file_path, content).await.unwrap();

    let error = RolloutRecorder::load_history(&file_path).await.unwrap_err();
    assert!(error.to_string().contains("invalid current rollout record"));
}

#[tokio::test]
async fn test_load_history_accepts_valid_final_record_without_newline() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("valid_without_newline.jsonl");
    let content = concat!(
        "{\"type\":\"message\",\"role\":\"user\",\"content\":\"First\",",
        "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
        "{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"Second\",",
        "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:56Z\"}"
    );

    fs::write(&file_path, content).await.unwrap();

    let items = RolloutRecorder::load_history(&file_path).await.unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn test_load_history_file_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("nonexistent.jsonl");

    let result = RolloutRecorder::load_history(&file_path).await;
    assert!(result.is_err());
}
