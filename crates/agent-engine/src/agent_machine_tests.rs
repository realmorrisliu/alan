use super::*;
use crate::rollout::{
    AgentMachineMeta, CheckpointRecord, CompactedItem, EffectRecord, EffectStatus, EventRecord,
    MessageRecord, RolloutItem, RolloutRecorder,
};
use crate::tape::{ContentPart, ToolResponse};
use alan_agent_protocol::{
    CompactionAttemptSnapshot, CompactionMode, CompactionReason, CompactionRequestMetadata,
    CompactionResult, CompactionTrigger, MemoryFlushAttemptSnapshot, MemoryFlushResult,
    MemoryFlushSkipReason,
};
use tempfile::TempDir;

/// Truncate a JSON payload to prevent context overflow
/// Recursively truncates large string values while preserving structure
#[cfg(test)]
fn truncate_payload(payload: serde_json::Value, max_size: usize) -> serde_json::Value {
    let payload_str = payload.to_string();
    if payload_str.len() <= max_size {
        return payload;
    }

    match payload {
        serde_json::Value::Object(map) => {
            let mut truncated = serde_json::Map::new();
            let mut current_size = 0;

            for (key, value) in map {
                // Always include critical fields
                let is_critical = matches!(key.as_str(), "success" | "error" | "url" | "title");

                if is_critical {
                    truncated.insert(key, value);
                    continue;
                }

                // For content/aggregated_content fields, truncate aggressively
                let processed_value = if key == "content" || key == "aggregated_content" {
                    if let serde_json::Value::String(s) = &value {
                        let truncated_str = truncate_text(s, max_size / 4);
                        serde_json::Value::String(truncated_str)
                    } else {
                        value
                    }
                } else {
                    truncate_payload(value, max_size / 2)
                };

                let value_str = processed_value.to_string();
                if current_size + value_str.len() < max_size * 3 / 4 {
                    truncated.insert(key, processed_value);
                    current_size += value_str.len();
                } else {
                    truncated.insert(
                        "_truncated".to_string(),
                        serde_json::Value::String("Additional fields omitted".to_string()),
                    );
                    break;
                }
            }

            serde_json::Value::Object(truncated)
        }
        serde_json::Value::Array(arr) => {
            let arr_len = arr.len();
            let mut truncated = Vec::new();
            let mut current_size = 0;

            for item in arr {
                let processed = truncate_payload(item, max_size / arr_len.max(1));
                let item_str = processed.to_string();

                if current_size + item_str.len() < max_size * 3 / 4 {
                    truncated.push(processed);
                    current_size += item_str.len();
                } else {
                    truncated.push(serde_json::json!({
                        "_note": "Additional array items omitted"
                    }));
                    break;
                }
            }

            serde_json::Value::Array(truncated)
        }
        serde_json::Value::String(s) => {
            if s.len() > max_size / 10 {
                serde_json::Value::String(truncate_text(&s, max_size / 10))
            } else {
                serde_json::Value::String(s)
            }
        }
        other => other,
    }
}

/// Truncate text to a maximum length, adding ellipsis if truncated
#[cfg(test)]
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len).collect();
    format!("{}...[truncated]", truncated)
}

#[path = "agent_machine_compaction_recovery_tests.rs"]
mod compaction_recovery;
#[path = "agent_machine_persistence_tests.rs"]
mod persistence;
#[path = "agent_machine_recovery_tests.rs"]
mod recovery;
#[path = "agent_machine_tape_tests.rs"]
mod tape;
