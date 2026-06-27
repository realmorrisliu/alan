use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionMode {
    Manual,
    AutoPreTurn,
    AutoMidTurn,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionTrigger {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionReason {
    ExplicitRequest,
    WindowPressure,
    ContinuationPressure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionResult {
    Success,
    Retry,
    Degraded,
    Failure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionSkipReason {
    UnderThreshold,
    EmptySummarizeRegion,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionPressureLevel {
    BelowSoft,
    Soft,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionRequestMetadata {
    pub mode: CompactionMode,
    pub trigger: CompactionTrigger,
    pub reason: CompactionReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedCompactionOutcome {
    pub request: CompactionRequestMetadata,
    pub input_prompt_tokens: usize,
    pub output_prompt_tokens: usize,
    pub retry_count: u32,
    pub result: CompactionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailedCompactionOutcome {
    pub request: CompactionRequestMetadata,
    pub input_prompt_tokens: usize,
    pub retry_count: u32,
    pub result: CompactionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedCompactionOutcome {
    pub request: CompactionRequestMetadata,
    pub input_prompt_tokens: usize,
    pub reason: CompactionSkipReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CompactionOutcome {
    Applied(AppliedCompactionOutcome),
    Failed(FailedCompactionOutcome),
    Skipped(SkippedCompactionOutcome),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactionAttemptSnapshot {
    pub attempt_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_id: Option<String>,
    pub request: CompactionRequestMetadata,
    pub result: CompactionResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure_level: Option<CompactionPressureLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_flush_attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_messages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_messages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_prompt_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_prompt_tokens: Option<usize>,
    pub retry_count: u32,
    pub tape_mutated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_streak: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_context_revision_before: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_context_revision_after: Option<u64>,
    pub timestamp: String,
}
