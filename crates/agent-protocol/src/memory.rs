use serde::{Deserialize, Serialize};

use crate::{CompactionMode, CompactionPressureLevel};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFlushResult {
    Success,
    Skipped,
    Failure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFlushSkipReason {
    AlreadyFlushedThisCycle,
    MemoryDisabled,
    MissingMemoryDir,
    ReadOnlyMemoryDir,
    NoDurableContent,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryFlushAttemptSnapshot {
    pub attempt_id: String,
    pub compaction_mode: CompactionMode,
    pub pressure_level: CompactionPressureLevel,
    pub result: MemoryFlushResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_reason: Option<MemoryFlushSkipReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_messages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub timestamp: String,
}
