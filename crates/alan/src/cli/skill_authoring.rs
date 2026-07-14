use crate::cli::skill_eval::{
    SkillEvalCaseRunSummary as StructuredSkillEvalCaseRunSummary,
    SkillEvalRunOptions as StructuredSkillEvalRunOptions,
    SkillEvalRunSummary as StructuredSkillEvalRunSummary, default_eval_manifest_path,
    generate_review_bundle, load_eval_manifest, regenerate_benchmark, run_eval_manifest,
};
use alan_agent_engine::skills::{
    COMPATIBILITY_METADATA_DIR, COMPATIBILITY_METADATA_FILE, PACKAGE_SIDECAR_FILE,
    SKILL_SIDECAR_FILE, SkillScope, SkillsError, compatibility_metadata_path,
    load_compatibility_metadata, load_package_sidecar, load_skill, load_skill_sidecar, name_to_id,
    resolve_skill_execution,
};
use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const INLINE_SKILL_TEMPLATE: &str = include_str!("../../skill_templates/basic-inline/SKILL.md");
const INLINE_OPENAI_TEMPLATE: &str =
    include_str!("../../skill_templates/basic-inline/agents/openai.yaml");
const DELEGATE_SKILL_TEMPLATE: &str = include_str!("../../skill_templates/basic-delegate/SKILL.md");
const DELEGATE_OPENAI_TEMPLATE: &str =
    include_str!("../../skill_templates/basic-delegate/agents/openai.yaml");
const DELEGATE_AGENT_CONFIG_TEMPLATE: &str =
    include_str!("../../skill_templates/basic-delegate/agents/__SKILL_ID__/agent.toml");
const DELEGATE_ROLE_TEMPLATE: &str =
    include_str!("../../skill_templates/basic-delegate/agents/__SKILL_ID__/persona/ROLE.md");

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillTemplateKind {
    Inline,
    Delegate,
}

impl std::fmt::Display for SkillTemplateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Inline => write!(f, "inline"),
            Self::Delegate => write!(f, "delegate"),
        }
    }
}

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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillValidationStatus {
    Valid,
    ValidWithWarnings,
    Invalid,
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

pub fn init_skill_package(
    package_root: &Path,
    template: SkillTemplateKind,
    name: Option<&str>,
    description: Option<&str>,
    short_description: Option<&str>,
    force: bool,
) -> Result<SkillInitResult> {
    let package_root = package_root.to_path_buf();
    let skill_id = skill_id_from_package_root(&package_root)?;
    ensure_package_root_ready(&package_root, force)?;
    fs::create_dir_all(&package_root)?;

    let display_name = name
        .map(str::to_owned)
        .unwrap_or_else(|| title_case_skill_id(&skill_id));
    let description = description
        .map(str::to_owned)
        .unwrap_or_else(|| "TODO: describe what this skill does and when to use it.".to_string());
    let short_description = short_description
        .map(str::to_owned)
        .unwrap_or_else(|| display_name.clone());
    let when_to_use =
        "TODO: replace this with the concrete user intent or trigger condition.".to_string();

    let context = TemplateContext {
        skill_id: &skill_id,
        skill_name: &display_name,
        skill_description: &description,
        short_description: &short_description,
        when_to_use: &when_to_use,
    };

    let rendered_files = render_template_files(template, &context);
    let mut created_paths = Vec::new();
    for (relative_path, contents) in rendered_files {
        let full_path = package_root.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, contents)?;
        created_paths.push(full_path);
    }
    created_paths.sort();

    let validation = validate_skill_package(&package_root);

    Ok(SkillInitResult {
        package_root,
        skill_id,
        template,
        created_paths,
        validation,
    })
}

pub fn validate_skill_package(package_root: &Path) -> SkillPackageValidationReport {
    let package_root = package_root.to_path_buf();
    let mut diagnostics = Vec::new();
    let mut package_id = None;
    let mut skill_id = None;
    let mut execution = None;
    let mut child_agent_exports = Vec::new();
    let mut resource_dirs = Vec::new();

    if !package_root.exists() {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Error,
            "missing_package_root",
            "Skill package directory does not exist".to_string(),
            Some(package_root.clone()),
        );
        return SkillPackageValidationReport {
            package_root,
            package_id,
            skill_id,
            execution,
            child_agent_exports,
            resource_dirs,
            diagnostics,
        };
    }

    if !package_root.is_dir() {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Error,
            "invalid_package_root",
            "Skill package path is not a directory".to_string(),
            Some(package_root.clone()),
        );
        return SkillPackageValidationReport {
            package_root,
            package_id,
            skill_id,
            execution,
            child_agent_exports,
            resource_dirs,
            diagnostics,
        };
    }

    for dir_name in [
        "bin",
        "scripts",
        "references",
        "assets",
        "evals",
        "eval-viewer",
        COMPATIBILITY_METADATA_DIR,
    ] {
        let path = package_root.join(dir_name);
        if path.exists() && !path.is_dir() {
            push_diagnostic(
                &mut diagnostics,
                SkillDiagnosticSeverity::Error,
                "resource_path_not_directory",
                format!("Expected `{dir_name}` to be a directory"),
                Some(path),
            );
        }
    }

    let skill_path = package_root.join("SKILL.md");
    if !skill_path.is_file() {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Error,
            "missing_skill_md",
            "Skill package must contain a root SKILL.md".to_string(),
            Some(skill_path),
        );
        return SkillPackageValidationReport {
            package_root,
            package_id,
            skill_id,
            execution,
            child_agent_exports,
            resource_dirs,
            diagnostics,
        };
    }

    let mut skill = match load_skill(&skill_path, SkillScope::Descriptor) {
        Ok(skill) => skill,
        Err(err) => {
            push_skill_error(
                &mut diagnostics,
                "invalid_skill_md",
                "Failed to load SKILL.md",
                &skill_path,
                err,
            );
            return SkillPackageValidationReport {
                package_root,
                package_id,
                skill_id,
                execution,
                child_agent_exports,
                resource_dirs,
                diagnostics,
            };
        }
    };

    let resolved_package_root =
        fs::canonicalize(&package_root).unwrap_or_else(|_| package_root.clone());
    let resolved_package_id = format!("skill:{}", skill.metadata.id);
    package_id = Some(resolved_package_id.clone());
    skill_id = Some(skill.metadata.id.clone());
    skill.metadata.package_id = Some(resolved_package_id.clone());
    skill.metadata.package_root = Some(resolved_package_root.clone());
    skill.metadata.resource_root = Some(resolved_package_root.clone());

    let package_sidecar = match load_package_sidecar(&resolved_package_root) {
        Ok(sidecar) => sidecar,
        Err(err) => {
            push_skill_error(
                &mut diagnostics,
                "invalid_package_sidecar",
                "Failed to parse package.yaml",
                &resolved_package_root.join(PACKAGE_SIDECAR_FILE),
                err,
            );
            None
        }
    };
    let skill_sidecar = match load_skill_sidecar(&skill_path) {
        Ok(sidecar) => sidecar,
        Err(err) => {
            push_skill_error(
                &mut diagnostics,
                "invalid_skill_sidecar",
                "Failed to parse skill.yaml",
                &resolved_package_root.join(SKILL_SIDECAR_FILE),
                err,
            );
            None
        }
    };
    let compatibility_metadata = match load_compatibility_metadata(&resolved_package_root) {
        Ok(metadata) => metadata,
        Err(err) => {
            push_skill_error(
                &mut diagnostics,
                "invalid_compatibility_metadata",
                "Failed to parse compatibility metadata",
                &compatibility_metadata_path(&resolved_package_root),
                err,
            );
            None
        }
    };

    if let Some(compatibility_metadata) = compatibility_metadata {
        skill.metadata.compatible_metadata = compatibility_metadata;
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Info,
            "compatible_metadata_loaded",
            format!(
                "Loaded compatibility metadata from {COMPATIBILITY_METADATA_DIR}/{COMPATIBILITY_METADATA_FILE}"
            ),
            Some(compatibility_metadata_path(&resolved_package_root)),
        );
    }

    if let Some(sidecar) = package_sidecar.as_ref()
        && !sidecar.skill_defaults.runtime.is_empty()
    {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Info,
            "package_defaults_loaded",
            "Loaded package-level runtime defaults from package.yaml".to_string(),
            Some(resolved_package_root.join(PACKAGE_SIDECAR_FILE)),
        );
    }
    if let Some(sidecar) = skill_sidecar.as_ref()
        && !sidecar.runtime.is_empty()
    {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Info,
            "skill_sidecar_loaded",
            "Loaded skill-level runtime metadata from skill.yaml".to_string(),
            Some(resolved_package_root.join(SKILL_SIDECAR_FILE)),
        );
    }

    if let Err(err) = skill.metadata.apply_sidecar_metadata(
        package_sidecar
            .as_ref()
            .map(|sidecar| &sidecar.skill_defaults),
        skill_sidecar.as_ref(),
    ) {
        push_skill_error(
            &mut diagnostics,
            "invalid_sidecar_merge",
            "Failed to apply alan sidecar metadata",
            &skill_path,
            err,
        );
    }

    if skill.metadata.effective_short_description().is_none() {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Warning,
            "missing_short_description",
            "Skill package does not define a short description in SKILL.md metadata or compatibility metadata".to_string(),
            Some(skill_path.clone()),
        );
    }

    let exports = inspect_package_exports(&resolved_package_id, &resolved_package_root);
    child_agent_exports = exports
        .child_agents
        .iter()
        .map(|export| export.name.clone())
        .collect();
    resource_dirs = collect_resource_dirs(&exports);

    if !child_agent_exports.is_empty() {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Info,
            "child_agents_loaded",
            format!(
                "Discovered child-agent exports: {}",
                child_agent_exports.join(", ")
            ),
            Some(resolved_package_root.join(COMPATIBILITY_METADATA_DIR)),
        );
    }

    if !resource_dirs.is_empty() {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Info,
            "resource_dirs_loaded",
            format!(
                "Discovered resource directories: {}",
                resource_dirs.join(", ")
            ),
            Some(resolved_package_root.clone()),
        );
    }

    skill.metadata.execution = resolve_skill_execution(&skill.metadata, &child_agent_exports);
    execution = Some(skill.metadata.execution.render_label());

    if let alan_agent_engine::skills::ResolvedSkillExecution::Unresolved { reason } =
        &skill.metadata.execution
    {
        push_diagnostic(
            &mut diagnostics,
            SkillDiagnosticSeverity::Error,
            "unresolved_execution",
            format!("Resolved execution is {}", reason.render_label()),
            Some(skill_path),
        );
    }

    SkillPackageValidationReport {
        package_root: resolved_package_root,
        package_id,
        skill_id,
        execution,
        child_agent_exports,
        resource_dirs,
        diagnostics,
    }
}

pub fn eval_skill_package(
    package_root: &Path,
    manifest_path: Option<&Path>,
    output_dir: Option<&Path>,
    require_hook: bool,
) -> Result<SkillEvalResult> {
    let validation = validate_skill_package(package_root);
    let package_root = validation.package_root.clone();

    if !validation.is_valid() {
        return Ok(SkillEvalResult {
            package_root,
            status: SkillEvalStatus::ValidationFailed,
            validation,
            manifest_path: None,
            output_dir: None,
            benchmark_path: None,
            review_bundle_path: None,
            manifest_run: None,
            hook_path: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    let resolved_manifest = load_eval_manifest(&package_root, manifest_path)?;
    if let Some((manifest_path, _)) = resolved_manifest {
        let run = run_eval_manifest(&StructuredSkillEvalRunOptions {
            package_root: package_root.clone(),
            manifest_path: manifest_path.clone(),
            output_dir: output_dir.map(Path::to_path_buf),
        })?;
        let benchmark_path = run.output_dir.join("benchmark.json");
        return Ok(SkillEvalResult {
            package_root,
            status: if run.passed() {
                SkillEvalStatus::Passed
            } else {
                SkillEvalStatus::Failed
            },
            validation,
            manifest_path: Some(manifest_path),
            output_dir: Some(run.output_dir.clone()),
            benchmark_path: Some(benchmark_path),
            review_bundle_path: run.review_bundle_path.clone(),
            manifest_run: Some(run),
            hook_path: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    if let Some(manifest_path) = manifest_path {
        bail!(
            "Structured eval manifest not found: {}",
            resolve_eval_manifest_path(&package_root, manifest_path).display()
        );
    }

    let hook_path = find_eval_hook(&package_root);
    let Some(hook_path) = hook_path else {
        let default_manifest = default_eval_manifest_path(&package_root);
        return Ok(SkillEvalResult {
            package_root,
            status: SkillEvalStatus::NoHook,
            validation,
            manifest_path: None,
            output_dir: None,
            benchmark_path: None,
            review_bundle_path: None,
            manifest_run: None,
            hook_path: None,
            stdout: String::new(),
            stderr: if require_hook {
                format!(
                    "No eval workflow found. Add {} or scripts/eval.sh or scripts/eval.py.",
                    default_manifest.display()
                )
            } else {
                "No eval workflow found. Validation passed, but no package-local eval manifest or hook was run."
                    .to_string()
            },
        });
    };

    let mut command = eval_command_for_hook(&hook_path)?;
    if let Some(skill_id) = validation.skill_id.as_deref() {
        command.env("ALAN_SKILL_ID", skill_id);
    }
    if let Some(package_id) = validation.package_id.as_deref() {
        command.env("ALAN_SKILL_PACKAGE_ID", package_id);
    }
    command.env("ALAN_SKILL_PACKAGE_ROOT", &package_root);
    command.current_dir(&package_root);

    let output = command.output().with_context(|| {
        format!(
            "Failed to execute eval hook for package {}",
            package_root.display()
        )
    })?;

    Ok(SkillEvalResult {
        package_root,
        status: if output.status.success() {
            SkillEvalStatus::Passed
        } else {
            SkillEvalStatus::Failed
        },
        validation,
        manifest_path: None,
        output_dir: None,
        benchmark_path: None,
        review_bundle_path: None,
        manifest_run: None,
        hook_path: Some(hook_path),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn regenerate_skill_eval_benchmark(run_dir: &Path) -> Result<PathBuf> {
    regenerate_benchmark(run_dir)
}

pub fn regenerate_skill_eval_review_bundle(run_dir: &Path) -> Result<PathBuf> {
    generate_review_bundle(run_dir)
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

fn resolve_eval_manifest_path(package_root: &Path, manifest_path: &Path) -> PathBuf {
    if manifest_path.is_absolute() {
        manifest_path.to_path_buf()
    } else {
        package_root.join(manifest_path)
    }
}

struct TemplateContext<'a> {
    skill_id: &'a str,
    skill_name: &'a str,
    skill_description: &'a str,
    short_description: &'a str,
    when_to_use: &'a str,
}

fn render_template_files(
    template: SkillTemplateKind,
    context: &TemplateContext<'_>,
) -> Vec<(PathBuf, String)> {
    match template {
        SkillTemplateKind::Inline => vec![
            (
                PathBuf::from("SKILL.md"),
                render_template(INLINE_SKILL_TEMPLATE, context),
            ),
            (
                PathBuf::from(format!(
                    "{COMPATIBILITY_METADATA_DIR}/{COMPATIBILITY_METADATA_FILE}"
                )),
                render_template(INLINE_OPENAI_TEMPLATE, context),
            ),
        ],
        SkillTemplateKind::Delegate => vec![
            (
                PathBuf::from("SKILL.md"),
                render_template(DELEGATE_SKILL_TEMPLATE, context),
            ),
            (
                PathBuf::from(format!(
                    "{COMPATIBILITY_METADATA_DIR}/{COMPATIBILITY_METADATA_FILE}"
                )),
                render_template(DELEGATE_OPENAI_TEMPLATE, context),
            ),
            (
                PathBuf::from(format!(
                    "{COMPATIBILITY_METADATA_DIR}/{}/persona/ROLE.md",
                    context.skill_id
                )),
                render_template(DELEGATE_ROLE_TEMPLATE, context),
            ),
            (
                PathBuf::from(format!(
                    "{COMPATIBILITY_METADATA_DIR}/{}/agent.toml",
                    context.skill_id
                )),
                render_template(DELEGATE_AGENT_CONFIG_TEMPLATE, context),
            ),
        ],
    }
}

fn render_template(template: &str, context: &TemplateContext<'_>) -> String {
    template
        .replace("__SKILL_ID__", context.skill_id)
        .replace("__SKILL_NAME__", context.skill_name)
        .replace("__SKILL_DESCRIPTION__", context.skill_description)
        .replace("__SHORT_DESCRIPTION__", context.short_description)
        .replace("__WHEN_TO_USE__", context.when_to_use)
}

fn ensure_package_root_ready(package_root: &Path, force: bool) -> Result<()> {
    if package_root.exists() {
        if !package_root.is_dir() {
            bail!("Skill package path exists but is not a directory");
        }

        let mut entries = fs::read_dir(package_root)?;
        if entries.next().transpose()?.is_some() && !force {
            bail!(
                "Skill package directory already exists and is not empty; use --force to overwrite"
            );
        }
    }
    Ok(())
}

fn skill_id_from_package_root(package_root: &Path) -> Result<String> {
    let Some(file_name) = package_root.file_name().and_then(|name| name.to_str()) else {
        bail!(
            "Cannot derive a skill id from package path {}",
            package_root.display()
        );
    };
    let skill_id = name_to_id(file_name);
    if skill_id.is_empty() {
        bail!("Skill package path must end in a usable directory name");
    }
    Ok(skill_id)
}

fn title_case_skill_id(skill_id: &str) -> String {
    skill_id
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = first.to_uppercase().collect::<String>();
                    word.push_str(chars.as_str());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn push_skill_error(
    diagnostics: &mut Vec<SkillPackageDiagnostic>,
    code: &str,
    prefix: &str,
    path: &Path,
    err: SkillsError,
) {
    push_diagnostic(
        diagnostics,
        SkillDiagnosticSeverity::Error,
        code,
        format!("{prefix}: {err}"),
        Some(path.to_path_buf()),
    );
}

fn push_diagnostic(
    diagnostics: &mut Vec<SkillPackageDiagnostic>,
    severity: SkillDiagnosticSeverity,
    code: &str,
    message: String,
    path: Option<PathBuf>,
) {
    diagnostics.push(SkillPackageDiagnostic {
        severity,
        code: code.to_string(),
        message,
        path,
    });
}

fn inspect_package_exports(
    package_id: &str,
    package_root: &Path,
) -> alan_agent_engine::skills::CapabilityPackageExports {
    alan_agent_engine::skills::CapabilityPackageExports {
        child_agents: discover_child_agent_exports(package_id, package_root),
        resources: alan_agent_engine::skills::CapabilityPackageResources {
            bin_dir: existing_dir(package_root.join("bin")),
            scripts_dir: existing_dir(package_root.join("scripts")),
            references_dir: existing_dir(package_root.join("references")),
            assets_dir: existing_dir(package_root.join("assets")),
        },
    }
}

fn discover_child_agent_exports(
    package_id: &str,
    package_root: &Path,
) -> Vec<alan_agent_engine::skills::CapabilityChildAgentExport> {
    let agents_dir = package_root.join(COMPATIBILITY_METADATA_DIR);
    let canonical_package_root =
        fs::canonicalize(package_root).unwrap_or_else(|_| package_root.to_path_buf());
    let Ok(canonical_agents_dir) = fs::canonicalize(&agents_dir) else {
        return Vec::new();
    };
    if !canonical_agents_dir.starts_with(&canonical_package_root) {
        return Vec::new();
    }

    let Ok(entries) = fs::read_dir(&agents_dir) else {
        return Vec::new();
    };

    let mut exports: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let canonical_root = fs::canonicalize(&path).ok()?;
            if !canonical_root.starts_with(&canonical_agents_dir)
                || !canonical_root.is_dir()
                || !looks_like_child_agent_root(&canonical_root)
            {
                return None;
            }

            let name = path.file_name()?.to_str()?.to_string();
            Some(alan_agent_engine::skills::CapabilityChildAgentExport {
                handle: alan_agent_engine::skills::CapabilityChildAgentExport::package_handle(
                    package_id, &name,
                ),
                name,
                root_dir: canonical_root,
                file_tree: None,
            })
        })
        .collect();
    exports.sort_by(|left, right| left.name.cmp(&right.name));
    exports
}

fn looks_like_child_agent_root(root_dir: &Path) -> bool {
    root_dir.join("agent.toml").is_file()
        || root_dir.join("persona").is_dir()
        || root_dir.join("skills").is_dir()
        || root_dir.join("policy.yaml").is_file()
}

fn existing_dir(path: PathBuf) -> Option<PathBuf> {
    path.is_dir().then_some(path)
}

fn collect_resource_dirs(
    exports: &alan_agent_engine::skills::CapabilityPackageExports,
) -> Vec<String> {
    let mut resource_dirs = Vec::new();
    if exports.resources.bin_dir.is_some() {
        resource_dirs.push("bin".to_string());
    }
    if exports.resources.scripts_dir.is_some() {
        resource_dirs.push("scripts".to_string());
    }
    if exports.resources.references_dir.is_some() {
        resource_dirs.push("references".to_string());
    }
    if exports.resources.assets_dir.is_some() {
        resource_dirs.push("assets".to_string());
    }
    resource_dirs
}

fn find_eval_hook(package_root: &Path) -> Option<PathBuf> {
    ["scripts/eval.sh", "scripts/eval.py"]
        .into_iter()
        .map(|candidate| package_root.join(candidate))
        .find(|path| path.is_file())
}

fn eval_command_for_hook(path: &Path) -> Result<Command> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("sh") => {
            let mut command = Command::new("sh");
            command.arg(path);
            Ok(command)
        }
        Some("py") => {
            let mut command = Command::new("python3");
            command.arg(path);
            Ok(command)
        }
        _ => Err(anyhow!(
            "Unsupported eval hook {}; expected scripts/eval.sh or scripts/eval.py",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_skill(root: &Path, frontmatter: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("SKILL.md"), frontmatter).unwrap();
    }

    #[test]
    fn validate_package_reports_ambiguous_delegate_shape() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("skill-creator");
        write_skill(
            &package_root,
            r#"---
name: Skill Creator
description: Create skills
---

Body
"#,
        );
        fs::create_dir_all(package_root.join("agents/creator/persona")).unwrap();
        fs::create_dir_all(package_root.join("agents/grader/persona")).unwrap();

        let report = validate_skill_package(&package_root);

        assert_eq!(report.status(), SkillValidationStatus::Invalid);
        assert_eq!(
            report.execution.as_deref(),
            Some("unresolved(ambiguous_package_shape)")
        );
    }

    #[test]
    fn init_delegate_template_validates_cleanly() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("repo-review");
        let result = init_skill_package(
            &package_root,
            SkillTemplateKind::Delegate,
            Some("Repo Review"),
            Some("Review repositories when asked."),
            Some("Review repositories"),
            false,
        )
        .unwrap();

        assert_eq!(result.validation.status(), SkillValidationStatus::Valid);
        assert_eq!(
            result.validation.execution.as_deref(),
            Some("delegate(target=repo-review, source=same_name_skill_and_child_agent)")
        );
    }

    #[test]
    fn eval_without_hook_returns_no_hook_status() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("lint");
        write_skill(
            &package_root,
            r#"---
name: Lint
description: Run lint tasks
---

Body
"#,
        );

        let result = eval_skill_package(&package_root, None, None, false).unwrap();

        assert_eq!(result.status, SkillEvalStatus::NoHook);
        assert!(result.validation.is_valid());
    }
}
