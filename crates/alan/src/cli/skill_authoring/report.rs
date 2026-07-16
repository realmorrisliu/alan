//! Serializable skill-authoring command results and their text presentation.

use std::path::PathBuf;

use serde::Serialize;

use super::{SkillTemplateKind, SkillValidationStatus};
use crate::cli::skill_eval::{
    SkillEvalCaseRunSummary as StructuredSkillEvalCaseRunSummary,
    SkillEvalRunSummary as StructuredSkillEvalRunSummary,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillPackageDiagnostic {
    pub severity: SkillDiagnosticSeverity,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillPackageValidationReport {
    pub package_root: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_agent_exports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_dirs: Vec<String>,
    pub diagnostics: Vec<SkillPackageDiagnostic>,
}

impl SkillPackageValidationReport {
    pub fn status(&self) -> SkillValidationStatus {
        if self
            .diagnostics
            .iter()
            .any(|diag| diag.severity == SkillDiagnosticSeverity::Error)
        {
            SkillValidationStatus::Invalid
        } else if self
            .diagnostics
            .iter()
            .any(|diag| diag.severity == SkillDiagnosticSeverity::Warning)
        {
            SkillValidationStatus::ValidWithWarnings
        } else {
            SkillValidationStatus::Valid
        }
    }

    pub fn is_valid(&self) -> bool {
        self.status() != SkillValidationStatus::Invalid
    }

    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diag| diag.severity == SkillDiagnosticSeverity::Warning)
    }

    pub fn passes(&self, strict: bool) -> bool {
        self.is_valid() && (!strict || !self.has_warnings())
    }

    pub fn render_text(&self) -> String {
        let mut lines = vec![
            "Skill Package Validation".to_string(),
            "========================".to_string(),
            format!("package: {}", self.package_root.display()),
        ];

        if let Some(package_id) = self.package_id.as_deref() {
            lines.push(format!("package_id: {package_id}"));
        }
        if let Some(skill_id) = self.skill_id.as_deref() {
            lines.push(format!("skill: {skill_id}"));
        }
        if let Some(execution) = self.execution.as_deref() {
            lines.push(format!("execution: {execution}"));
        }
        lines.push(format!("status: {}", self.status().render_label()));

        if !self.child_agent_exports.is_empty() {
            lines.push(format!(
                "child_agents: {}",
                self.child_agent_exports.join(", ")
            ));
        }
        if !self.resource_dirs.is_empty() {
            lines.push(format!("resources: {}", self.resource_dirs.join(", ")));
        }

        if !self.diagnostics.is_empty() {
            lines.push(String::new());
            for diagnostic in &self.diagnostics {
                let prefix = diagnostic.severity.render_label();
                let location = diagnostic
                    .path
                    .as_deref()
                    .map(|path| format!(" ({})", path.display()))
                    .unwrap_or_default();
                lines.push(format!(
                    "{prefix}: {} [{}]{}",
                    diagnostic.message, diagnostic.code, location
                ));
            }
        }

        lines.push(String::new());
        lines.join("\n")
    }
}

impl SkillValidationStatus {
    pub fn render_label(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::ValidWithWarnings => "valid_with_warnings",
            Self::Invalid => "invalid",
        }
    }
}

impl SkillDiagnosticSeverity {
    fn render_label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillInitResult {
    pub package_root: PathBuf,
    pub skill_id: String,
    pub template: SkillTemplateKind,
    pub created_paths: Vec<PathBuf>,
    pub validation: SkillPackageValidationReport,
}

impl SkillInitResult {
    pub fn render_text(&self) -> String {
        let mut lines = vec![
            "Initialized Skill Package".to_string(),
            "=========================".to_string(),
            format!("package: {}", self.package_root.display()),
            format!("skill: {}", self.skill_id),
            format!("template: {}", self.template),
            format!("validation: {}", self.validation.status().render_label()),
            String::new(),
            "created:".to_string(),
        ];

        for path in &self.created_paths {
            lines.push(format!("- {}", path.display()));
        }

        lines.push(String::new());
        lines.push(self.validation.render_text());
        lines.join("\n")
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillEvalStatus {
    Passed,
    Failed,
    ValidationFailed,
    NoHook,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillEvalResult {
    pub package_root: PathBuf,
    pub status: SkillEvalStatus,
    pub validation: SkillPackageValidationReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_bundle_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_run: Option<StructuredSkillEvalRunSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stdout: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stderr: String,
}

impl SkillEvalResult {
    pub fn passed(&self, require_hook: bool) -> bool {
        match self.status {
            SkillEvalStatus::Passed => true,
            SkillEvalStatus::NoHook => !require_hook && self.validation.is_valid(),
            SkillEvalStatus::Failed | SkillEvalStatus::ValidationFailed => false,
        }
    }

    pub fn render_text(&self) -> String {
        let mut lines = vec![
            "Skill Package Eval".to_string(),
            "==================".to_string(),
            format!("package: {}", self.package_root.display()),
            format!("status: {}", self.status.render_label()),
            format!("validation: {}", self.validation.status().render_label()),
        ];

        if let Some(manifest_path) = self.manifest_path.as_deref() {
            lines.push(format!("manifest: {}", manifest_path.display()));
        }
        if let Some(output_dir) = self.output_dir.as_deref() {
            lines.push(format!("output_dir: {}", output_dir.display()));
        }
        if let Some(benchmark_path) = self.benchmark_path.as_deref() {
            lines.push(format!("benchmark: {}", benchmark_path.display()));
        }
        if let Some(review_bundle_path) = self.review_bundle_path.as_deref() {
            lines.push(format!("review_bundle: {}", review_bundle_path.display()));
        }
        if let Some(hook_path) = self.hook_path.as_deref() {
            lines.push(format!("hook: {}", hook_path.display()));
        }
        if let Some(manifest_run) = self.manifest_run.as_ref() {
            lines.push(format!(
                "cases: {} total, {} passed, {} failed",
                manifest_run.benchmark.total_cases,
                manifest_run.benchmark.passed_cases,
                manifest_run.benchmark.failed_cases
            ));
            if let Some(candidate_success_rate) = manifest_run.benchmark.candidate_success_rate {
                lines.push(format!(
                    "candidate_success_rate: {:.2}",
                    candidate_success_rate
                ));
            }
            if let Some(baseline_success_rate) = manifest_run.benchmark.baseline_success_rate {
                lines.push(format!(
                    "baseline_success_rate: {:.2}",
                    baseline_success_rate
                ));
            }
            if let Some(success_delta) = manifest_run.benchmark.success_delta {
                lines.push(format!("success_delta: {success_delta:+.2}"));
            }
            lines.push(String::new());
            lines.push("case_results:".to_string());
            for case in &manifest_run.cases {
                lines.push(format!("- {}", render_structured_eval_case(case)));
            }
        }
        if !self.stdout.is_empty() {
            lines.push(String::new());
            lines.push("stdout:".to_string());
            lines.push(self.stdout.trim_end().to_string());
        }
        if !self.stderr.is_empty() {
            lines.push(String::new());
            lines.push("stderr:".to_string());
            lines.push(self.stderr.trim_end().to_string());
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

impl SkillEvalStatus {
    fn render_label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::ValidationFailed => "validation_failed",
            Self::NoHook => "no_hook",
        }
    }
}

fn render_structured_eval_case(case: &StructuredSkillEvalCaseRunSummary) -> String {
    match case {
        StructuredSkillEvalCaseRunSummary::Trigger {
            id,
            passed,
            actual,
            expected,
            ..
        } => {
            format!(
                "{id} [trigger] [{}] expected={expected} actual={actual}",
                if *passed { "passed" } else { "failed" }
            )
        }
        StructuredSkillEvalCaseRunSummary::Command {
            id,
            passed,
            comparison_mode,
            candidate,
            baseline,
            ..
        } => {
            let mut details = vec![format!(
                "{id} [command] [{}] candidate_exit={}",
                if *passed { "passed" } else { "failed" },
                candidate.exit_code
            )];
            if let Some(mode) = comparison_mode {
                details.push(format!("comparison={mode:?}"));
            }
            if let Some(baseline) = baseline.as_ref() {
                details.push(format!("baseline_exit={}", baseline.exit_code));
            }
            details.join(" ")
        }
    }
}
