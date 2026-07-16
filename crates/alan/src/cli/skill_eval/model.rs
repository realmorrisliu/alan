//! Skill-eval manifest, run, and benchmark data contracts.

use std::{collections::BTreeMap, path::PathBuf};

use alan_agent_engine::skills::SkillActivationReason;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalManifest {
    #[serde(default = "default_manifest_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(default)]
    pub review: SkillEvalReviewManifest,
    #[serde(default)]
    pub cases: Vec<SkillEvalCaseManifest>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillEvalReviewManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillEvalCaseManifest {
    Trigger {
        id: String,
        input: String,
        expected: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    },
    Command {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        command: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comparison: Option<SkillEvalComparisonManifest>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grading: Option<SkillEvalStageManifest>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        analyzer: Option<SkillEvalStageManifest>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comparator: Option<SkillEvalStageManifest>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalComparisonManifest {
    pub mode: SkillEvalComparisonMode,
    pub baseline_command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillEvalComparisonMode {
    WithWithoutSkill,
    NewOldSkill,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalStageManifest {
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SkillEvalRunOptions {
    pub package_root: PathBuf,
    pub manifest_path: PathBuf,
    pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalRunSummary {
    pub package_root: PathBuf,
    pub manifest_path: PathBuf,
    pub output_dir: PathBuf,
    pub suite: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_source: Option<PathBuf>,
    pub cases: Vec<SkillEvalCaseRunSummary>,
    pub benchmark: SkillEvalBenchmarkSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_bundle_path: Option<PathBuf>,
}

impl SkillEvalRunSummary {
    pub fn passed(&self) -> bool {
        self.benchmark.failed_cases == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillEvalCaseRunSummary {
    Trigger {
        id: String,
        input: String,
        expected: bool,
        actual: bool,
        passed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_reason: Option<SkillActivationReason>,
        artifact_path: PathBuf,
    },
    Command {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        passed: bool,
        case_dir: PathBuf,
        candidate_label: String,
        candidate: SkillEvalCommandRunSummary,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comparison_mode: Option<SkillEvalComparisonMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        baseline_label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        baseline: Box<Option<SkillEvalCommandRunSummary>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grading: Box<Option<SkillEvalCommandRunSummary>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        analyzer: Box<Option<SkillEvalCommandRunSummary>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comparator: Box<Option<SkillEvalCommandRunSummary>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalCommandRunSummary {
    pub label: String,
    pub command: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_output: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_file: Option<PathBuf>,
    pub artifact_path: PathBuf,
}

impl SkillEvalCommandRunSummary {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    pub fn indicates_pass(&self) -> bool {
        if let Some(Value::Bool(passed)) = self
            .json_output
            .as_ref()
            .and_then(|json| json.get("passed"))
        {
            return *passed;
        }
        self.success()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvalBenchmarkSummary {
    pub total_cases: usize,
    pub passed_cases: usize,
    pub failed_cases: usize,
    pub trigger_cases: usize,
    pub command_cases: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_success_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_success_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_delta: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_candidate_duration_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_baseline_duration_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub comparison_mode_counts: BTreeMap<String, usize>,
}

pub(super) fn default_manifest_version() -> u32 {
    1
}
