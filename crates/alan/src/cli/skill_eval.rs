use alan_agent_engine::skills::{SkillActivationReason, SkillScope, extract_mentions, load_skill};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const RUN_SUMMARY_FILE: &str = "run.json";
const BENCHMARK_FILE: &str = "benchmark.json";

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

pub fn default_eval_manifest_path(package_root: &Path) -> PathBuf {
    package_root.join("evals").join("evals.json")
}

pub fn load_eval_manifest(
    package_root: &Path,
    manifest_path: Option<&Path>,
) -> Result<Option<(PathBuf, SkillEvalManifest)>> {
    let manifest_path = manifest_path
        .map(|path| resolve_path(package_root, path))
        .unwrap_or_else(|| default_eval_manifest_path(package_root));
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let manifest_path = fs::canonicalize(&manifest_path).unwrap_or(manifest_path);
    let manifest: SkillEvalManifest =
        serde_json::from_str(&fs::read_to_string(&manifest_path).with_context(|| {
            format!("Failed to read eval manifest {}", manifest_path.display())
        })?)
        .with_context(|| format!("Failed to parse eval manifest {}", manifest_path.display()))?;
    validate_manifest(&manifest)?;
    Ok(Some((manifest_path, manifest)))
}

pub fn run_eval_manifest(options: &SkillEvalRunOptions) -> Result<SkillEvalRunSummary> {
    let package_root =
        fs::canonicalize(&options.package_root).unwrap_or_else(|_| options.package_root.clone());
    let manifest_path =
        fs::canonicalize(&options.manifest_path).unwrap_or_else(|_| options.manifest_path.clone());
    let (_, manifest) = load_eval_manifest(&package_root, Some(&manifest_path))?
        .ok_or_else(|| anyhow!("No eval manifest found at {}", manifest_path.display()))?;
    let skill = load_skill(&package_root.join("SKILL.md"), SkillScope::Descriptor)
        .with_context(|| format!("Failed to load skill package {}", package_root.display()))?;
    let suite = manifest
        .suite
        .clone()
        .unwrap_or_else(|| skill.metadata.id.clone());
    let output_dir = options
        .output_dir
        .clone()
        .unwrap_or_else(|| default_output_dir(&package_root, &suite));
    fs::create_dir_all(output_dir.join("cases"))?;

    let started_at = Utc::now();
    let mut cases = Vec::new();
    for case in &manifest.cases {
        let case_id = manifest_case_id(case);
        let case_dir = output_dir
            .join("cases")
            .join(sanitize_path_component(&case_id));
        fs::create_dir_all(&case_dir)?;
        let case_summary = match case {
            SkillEvalCaseManifest::Trigger {
                id,
                input,
                expected,
                ..
            } => run_trigger_case(id, input, *expected, &skill.metadata, &case_dir)?,
            SkillEvalCaseManifest::Command {
                id,
                prompt,
                command,
                comparison,
                grading,
                analyzer,
                comparator,
            } => run_command_case(
                id,
                prompt.as_deref(),
                command,
                comparison.as_ref(),
                grading.as_ref(),
                analyzer.as_ref(),
                comparator.as_ref(),
                &package_root,
                &manifest_path,
                &output_dir,
                &case_dir,
            )?,
        };
        cases.push(case_summary);
    }

    let viewer_source = resolve_viewer_source(&package_root, &manifest.review);
    let mut summary = SkillEvalRunSummary {
        package_root,
        manifest_path,
        output_dir: output_dir.clone(),
        suite,
        started_at,
        completed_at: Utc::now(),
        viewer_source,
        benchmark: SkillEvalBenchmarkSummary {
            total_cases: 0,
            passed_cases: 0,
            failed_cases: 0,
            trigger_cases: 0,
            command_cases: 0,
            candidate_success_rate: None,
            baseline_success_rate: None,
            success_delta: None,
            mean_candidate_duration_ms: None,
            mean_baseline_duration_ms: None,
            comparison_mode_counts: BTreeMap::new(),
        },
        cases,
        review_bundle_path: None,
    };
    summary.benchmark = summarize_benchmark(&summary.cases);
    write_json(output_dir.join(BENCHMARK_FILE), &summary.benchmark)?;
    let review_bundle_path = generate_review_bundle_from_summary(&summary)?;
    summary.review_bundle_path = Some(review_bundle_path);
    summary.completed_at = Utc::now();
    write_json(output_dir.join(RUN_SUMMARY_FILE), &summary)?;
    Ok(summary)
}

pub fn regenerate_benchmark(run_dir: &Path) -> Result<PathBuf> {
    let run_dir = fs::canonicalize(run_dir).unwrap_or_else(|_| run_dir.to_path_buf());
    let mut summary = load_run_summary(&run_dir)?;
    summary.benchmark = summarize_benchmark(&summary.cases);
    let benchmark_path = run_dir.join(BENCHMARK_FILE);
    write_json(&benchmark_path, &summary.benchmark)?;
    write_json(run_dir.join(RUN_SUMMARY_FILE), &summary)?;
    Ok(benchmark_path)
}

pub fn generate_review_bundle(run_dir: &Path) -> Result<PathBuf> {
    let run_dir = fs::canonicalize(run_dir).unwrap_or_else(|_| run_dir.to_path_buf());
    let summary = load_run_summary(&run_dir)?;
    generate_review_bundle_from_summary(&summary)
}

fn load_run_summary(run_dir: &Path) -> Result<SkillEvalRunSummary> {
    let run_path = run_dir.join(RUN_SUMMARY_FILE);
    serde_json::from_str(
        &fs::read_to_string(&run_path)
            .with_context(|| format!("Failed to read {}", run_path.display()))?,
    )
    .with_context(|| format!("Failed to parse {}", run_path.display()))
}

fn run_trigger_case(
    id: &str,
    input: &str,
    expected: bool,
    metadata: &alan_agent_engine::skills::SkillMetadata,
    case_dir: &Path,
) -> Result<SkillEvalCaseRunSummary> {
    let activation_reason = explicit_activation_reason(metadata, input);
    let actual = activation_reason.is_some();
    let passed = actual == expected;
    let artifact_path = case_dir.join("case.json");
    let summary = SkillEvalCaseRunSummary::Trigger {
        id: id.to_string(),
        input: input.to_string(),
        expected,
        actual,
        passed,
        activation_reason,
        artifact_path: artifact_path.clone(),
    };
    write_json(&artifact_path, &summary)?;
    Ok(summary)
}

fn explicit_activation_reason(
    metadata: &alan_agent_engine::skills::SkillMetadata,
    input: &str,
) -> Option<SkillActivationReason> {
    let mentions = extract_mentions(input);
    mentions
        .iter()
        .any(|mention| mention == &metadata.id)
        .then(|| SkillActivationReason::ExplicitMention {
            mention: metadata.id.clone(),
        })
}

#[allow(clippy::too_many_arguments)]
fn run_command_case(
    id: &str,
    prompt: Option<&str>,
    command: &[String],
    comparison: Option<&SkillEvalComparisonManifest>,
    grading: Option<&SkillEvalStageManifest>,
    analyzer: Option<&SkillEvalStageManifest>,
    comparator: Option<&SkillEvalStageManifest>,
    package_root: &Path,
    manifest_path: &Path,
    output_dir: &Path,
    case_dir: &Path,
) -> Result<SkillEvalCaseRunSummary> {
    let (candidate_label, baseline_label) = comparison
        .map(comparison_labels)
        .unwrap_or_else(|| ("candidate".to_string(), None));
    let candidate = run_stage_command(
        &candidate_label,
        command,
        None,
        package_root,
        manifest_path,
        output_dir,
        case_dir,
        id,
        prompt,
        None,
        None,
    )?;
    let baseline = if let Some(comparison) = comparison {
        Some(run_stage_command(
            baseline_label.as_deref().unwrap_or("baseline"),
            &comparison.baseline_command,
            None,
            package_root,
            manifest_path,
            output_dir,
            case_dir,
            id,
            prompt,
            Some(&candidate.artifact_path),
            Some(comparison.mode),
        )?)
    } else {
        None
    };
    let grading_run = grading
        .map(|stage| {
            run_stage_command(
                "grading",
                &stage.command,
                stage
                    .prompt_file
                    .as_deref()
                    .map(|path| resolve_path(package_root, Path::new(path))),
                package_root,
                manifest_path,
                output_dir,
                case_dir,
                id,
                prompt,
                Some(&candidate.artifact_path),
                comparison.map(|comparison| comparison.mode),
            )
        })
        .transpose()?;
    let analyzer_run = analyzer
        .map(|stage| {
            run_stage_command(
                "analyzer",
                &stage.command,
                stage
                    .prompt_file
                    .as_deref()
                    .map(|path| resolve_path(package_root, Path::new(path))),
                package_root,
                manifest_path,
                output_dir,
                case_dir,
                id,
                prompt,
                Some(&candidate.artifact_path),
                comparison.map(|comparison| comparison.mode),
            )
        })
        .transpose()?;
    let comparator_run = comparator
        .map(|stage| {
            run_stage_command(
                "comparator",
                &stage.command,
                stage
                    .prompt_file
                    .as_deref()
                    .map(|path| resolve_path(package_root, Path::new(path))),
                package_root,
                manifest_path,
                output_dir,
                case_dir,
                id,
                prompt,
                Some(&candidate.artifact_path),
                comparison.map(|comparison| comparison.mode),
            )
        })
        .transpose()?;
    let passed = candidate.success()
        && grading_run
            .as_ref()
            .is_none_or(SkillEvalCommandRunSummary::indicates_pass);
    let artifact_path = case_dir.join("case.json");
    let summary = SkillEvalCaseRunSummary::Command {
        id: id.to_string(),
        prompt: prompt.map(str::to_string),
        passed,
        case_dir: case_dir.to_path_buf(),
        candidate_label,
        candidate,
        comparison_mode: comparison.map(|comparison| comparison.mode),
        baseline_label,
        baseline: Box::new(baseline),
        grading: Box::new(grading_run),
        analyzer: Box::new(analyzer_run),
        comparator: Box::new(comparator_run),
    };
    write_json(artifact_path, &summary)?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn run_stage_command(
    label: &str,
    command: &[String],
    prompt_file: Option<PathBuf>,
    package_root: &Path,
    manifest_path: &Path,
    output_dir: &Path,
    case_dir: &Path,
    case_id: &str,
    prompt: Option<&str>,
    candidate_artifact: Option<&Path>,
    comparison_mode: Option<SkillEvalComparisonMode>,
) -> Result<SkillEvalCommandRunSummary> {
    if command.is_empty() {
        bail!("Eval stage `{label}` command must not be empty");
    }
    let artifact_path = case_dir.join(format!("{label}.json"));
    let started = Instant::now();
    let mut child = Command::new(&command[0]);
    child.args(&command[1..]);
    child.current_dir(package_root);
    child.env("ALAN_SKILL_EVAL_PACKAGE_ROOT", package_root);
    child.env("ALAN_SKILL_EVAL_MANIFEST", manifest_path);
    child.env("ALAN_SKILL_EVAL_OUTPUT_DIR", output_dir);
    child.env("ALAN_SKILL_EVAL_CASE_DIR", case_dir);
    child.env("ALAN_SKILL_EVAL_CASE_ID", case_id);
    child.env("ALAN_SKILL_EVAL_STAGE_LABEL", label);
    child.env("ALAN_SKILL_EVAL_PROMPT", prompt.unwrap_or(""));
    child.env(
        "ALAN_SKILL_EVAL_CANDIDATE_ARTIFACT",
        candidate_artifact
            .map(|path| path.as_os_str())
            .unwrap_or_default(),
    );
    child.env(
        "ALAN_SKILL_EVAL_BASELINE_ARTIFACT",
        case_dir.join("baseline.json").as_os_str(),
    );
    if let Some(prompt_file) = prompt_file.as_ref() {
        child.env("ALAN_SKILL_EVAL_STAGE_PROMPT_FILE", prompt_file);
    }
    if let Some(mode) = comparison_mode {
        child.env(
            "ALAN_SKILL_EVAL_COMPARISON_MODE",
            comparison_mode_label(mode),
        );
    }
    let output = child
        .output()
        .with_context(|| format!("Failed to execute eval stage `{label}`"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let json_output = serde_json::from_str::<Value>(&stdout).ok();
    let summary = SkillEvalCommandRunSummary {
        label: label.to_string(),
        command: command.to_vec(),
        exit_code: output.status.code().unwrap_or(-1),
        duration_ms: started.elapsed().as_millis() as u64,
        stdout,
        stderr,
        json_output,
        prompt_file,
        artifact_path: artifact_path.clone(),
    };
    write_json(&artifact_path, &summary)?;
    Ok(summary)
}

fn default_output_dir(package_root: &Path, suite: &str) -> PathBuf {
    package_root.join(".alan").join("eval-runs").join(format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        sanitize_path_component(suite)
    ))
}

fn resolve_viewer_source(package_root: &Path, review: &SkillEvalReviewManifest) -> Option<PathBuf> {
    review
        .viewer
        .as_deref()
        .map(|viewer| resolve_path(package_root, Path::new(viewer)))
        .filter(|path| path.exists())
        .or_else(|| {
            let default = package_root.join("eval-viewer");
            default.exists().then_some(default)
        })
}

fn generate_review_bundle_from_summary(summary: &SkillEvalRunSummary) -> Result<PathBuf> {
    let review_dir = summary.output_dir.join("review");
    fs::create_dir_all(&review_dir)?;
    if let Some(viewer_source) = summary.viewer_source.as_ref() {
        let viewer_target = review_dir.join("viewer");
        copy_path(viewer_source, &viewer_target)?;
    }
    let html = build_review_html(summary);
    let review_path = review_dir.join("index.html");
    fs::write(&review_path, html)?;
    Ok(review_path)
}

fn build_review_html(summary: &SkillEvalRunSummary) -> String {
    let review_dir = summary.output_dir.join("review");
    let mut rows = String::new();
    for case in &summary.cases {
        match case {
            SkillEvalCaseRunSummary::Trigger {
                id,
                passed,
                input,
                actual,
                expected,
                artifact_path,
                ..
            } => {
                rows.push_str(&format!(
                    "<tr><td>{}</td><td>trigger</td><td>{}</td><td>expected={} actual={}</td><td>{}</td></tr>",
                    html_escape(id),
                    status_badge(*passed),
                    expected,
                    actual,
                    link_to_review_path(&review_dir, &summary.output_dir, artifact_path, "artifact")
                ));
                rows.push_str(&format!(
                    "<tr><td colspan=\"5\"><pre>{}</pre></td></tr>",
                    html_escape(input)
                ));
            }
            SkillEvalCaseRunSummary::Command {
                id,
                passed,
                case_dir,
                candidate,
                baseline,
                grading,
                analyzer,
                comparator,
                ..
            } => {
                let mut links = vec![
                    link_to_review_path(&review_dir, &summary.output_dir, case_dir, "case_dir"),
                    link_to_review_path(
                        &review_dir,
                        &summary.output_dir,
                        &candidate.artifact_path,
                        "candidate",
                    ),
                ];
                if let Some(stage) = baseline.as_ref() {
                    links.push(link_to_review_path(
                        &review_dir,
                        &summary.output_dir,
                        &stage.artifact_path,
                        "baseline",
                    ));
                }
                if let Some(stage) = grading.as_ref() {
                    links.push(link_to_review_path(
                        &review_dir,
                        &summary.output_dir,
                        &stage.artifact_path,
                        "grading",
                    ));
                }
                if let Some(stage) = analyzer.as_ref() {
                    links.push(link_to_review_path(
                        &review_dir,
                        &summary.output_dir,
                        &stage.artifact_path,
                        "analyzer",
                    ));
                }
                if let Some(stage) = comparator.as_ref() {
                    links.push(link_to_review_path(
                        &review_dir,
                        &summary.output_dir,
                        &stage.artifact_path,
                        "comparator",
                    ));
                }
                rows.push_str(&format!(
                    "<tr><td>{}</td><td>command</td><td>{}</td><td>candidate_exit={}</td><td>{}</td></tr>",
                    html_escape(id),
                    status_badge(*passed),
                    candidate.exit_code,
                    links.join(" ")
                ));
            }
        }
    }

    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\" /><title>{}</title><style>body{{font-family:Menlo,monospace;padding:24px;}}table{{border-collapse:collapse;width:100%;}}td,th{{border:1px solid #ccc;padding:8px;vertical-align:top;}}.ok{{color:#116611;}}.bad{{color:#991111;}}</style></head><body><h1>{}</h1><p>Package: {}</p><p>Manifest: {}</p><p>Output: {}</p><p>Run: {}</p><p>Benchmark: {}</p><table><thead><tr><th>Case</th><th>Type</th><th>Status</th><th>Summary</th><th>Artifacts</th></tr></thead><tbody>{}</tbody></table></body></html>",
        html_escape(&summary.suite),
        html_escape(&summary.suite),
        html_escape(&summary.package_root.display().to_string()),
        html_escape(&summary.manifest_path.display().to_string()),
        html_escape(&summary.output_dir.display().to_string()),
        link_to_review_path(
            &review_dir,
            &summary.output_dir,
            &summary.output_dir.join(RUN_SUMMARY_FILE),
            "run",
        ),
        link_to_review_path(
            &review_dir,
            &summary.output_dir,
            &summary.output_dir.join(BENCHMARK_FILE),
            "benchmark",
        ),
        rows
    )
}

fn summarize_benchmark(cases: &[SkillEvalCaseRunSummary]) -> SkillEvalBenchmarkSummary {
    let mut passed_cases = 0;
    let mut trigger_cases = 0;
    let mut command_cases = 0;
    let mut candidate_successes = 0_usize;
    let mut baseline_successes = 0_usize;
    let mut baseline_total = 0_usize;
    let mut candidate_durations = Vec::new();
    let mut baseline_durations = Vec::new();
    let mut comparison_mode_counts = BTreeMap::new();

    for case in cases {
        match case {
            SkillEvalCaseRunSummary::Trigger { passed, .. } => {
                trigger_cases += 1;
                if *passed {
                    passed_cases += 1;
                }
            }
            SkillEvalCaseRunSummary::Command {
                passed,
                candidate,
                comparison_mode,
                baseline,
                ..
            } => {
                command_cases += 1;
                if *passed {
                    passed_cases += 1;
                }
                if candidate.success() {
                    candidate_successes += 1;
                }
                candidate_durations.push(candidate.duration_ms as f64);
                if let Some(mode) = comparison_mode {
                    *comparison_mode_counts
                        .entry(comparison_mode_label(*mode).to_string())
                        .or_insert(0) += 1;
                }
                if let Some(baseline) = baseline.as_ref() {
                    baseline_total += 1;
                    if baseline.success() {
                        baseline_successes += 1;
                    }
                    baseline_durations.push(baseline.duration_ms as f64);
                }
            }
        }
    }

    let total_cases = cases.len();
    let failed_cases = total_cases.saturating_sub(passed_cases);
    let candidate_success_rate =
        (command_cases > 0).then_some(candidate_successes as f64 / command_cases as f64);
    let baseline_success_rate =
        (baseline_total > 0).then_some(baseline_successes as f64 / baseline_total as f64);

    SkillEvalBenchmarkSummary {
        total_cases,
        passed_cases,
        failed_cases,
        trigger_cases,
        command_cases,
        candidate_success_rate,
        baseline_success_rate,
        success_delta: candidate_success_rate
            .zip(baseline_success_rate)
            .map(|(candidate, baseline)| candidate - baseline),
        mean_candidate_duration_ms: mean(&candidate_durations),
        mean_baseline_duration_ms: mean(&baseline_durations),
        comparison_mode_counts,
    }
}

fn validate_manifest(manifest: &SkillEvalManifest) -> Result<()> {
    if manifest.version != default_manifest_version() {
        bail!(
            "Unsupported eval manifest version {}; expected {}",
            manifest.version,
            default_manifest_version()
        );
    }
    if manifest.cases.is_empty() {
        bail!("Eval manifest must contain at least one case");
    }

    let mut ids = BTreeSet::new();
    for case in &manifest.cases {
        let id = manifest_case_id(case);
        if !ids.insert(id.clone()) {
            bail!("Eval manifest contains duplicate case id `{id}`");
        }
        match case {
            SkillEvalCaseManifest::Trigger { input, .. } if input.trim().is_empty() => {
                bail!("Trigger eval case `{id}` must provide non-empty input");
            }
            SkillEvalCaseManifest::Command {
                command,
                comparison,
                grading,
                analyzer,
                comparator,
                ..
            } => {
                if command.is_empty() {
                    bail!("Command eval case `{id}` must provide a candidate command");
                }
                if let Some(comparison) = comparison
                    && comparison.baseline_command.is_empty()
                {
                    bail!("Command eval case `{id}` comparison must provide a baseline command");
                }
                for (stage_label, stage) in [
                    ("grading", grading.as_ref()),
                    ("analyzer", analyzer.as_ref()),
                    ("comparator", comparator.as_ref()),
                ] {
                    if let Some(stage) = stage
                        && stage.command.is_empty()
                    {
                        bail!(
                            "Command eval case `{id}` {stage_label} stage command must not be empty"
                        );
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn manifest_case_id(case: &SkillEvalCaseManifest) -> String {
    match case {
        SkillEvalCaseManifest::Trigger { id, .. } | SkillEvalCaseManifest::Command { id, .. } => {
            id.clone()
        }
    }
}

fn comparison_labels(comparison: &SkillEvalComparisonManifest) -> (String, Option<String>) {
    match comparison.mode {
        SkillEvalComparisonMode::WithWithoutSkill => (
            comparison
                .candidate_label
                .clone()
                .unwrap_or_else(|| "with_skill".to_string()),
            Some(
                comparison
                    .baseline_label
                    .clone()
                    .unwrap_or_else(|| "without_skill".to_string()),
            ),
        ),
        SkillEvalComparisonMode::NewOldSkill => (
            comparison
                .candidate_label
                .clone()
                .unwrap_or_else(|| "new_skill".to_string()),
            Some(
                comparison
                    .baseline_label
                    .clone()
                    .unwrap_or_else(|| "old_skill".to_string()),
            ),
        ),
        SkillEvalComparisonMode::Custom => (
            comparison
                .candidate_label
                .clone()
                .unwrap_or_else(|| "candidate".to_string()),
            Some(
                comparison
                    .baseline_label
                    .clone()
                    .unwrap_or_else(|| "baseline".to_string()),
            ),
        ),
    }
}

fn comparison_mode_label(mode: SkillEvalComparisonMode) -> &'static str {
    match mode {
        SkillEvalComparisonMode::WithWithoutSkill => "with_without_skill",
        SkillEvalComparisonMode::NewOldSkill => "new_old_skill",
        SkillEvalComparisonMode::Custom => "custom",
    }
}

fn resolve_path(package_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        package_root.join(path)
    }
}

fn sanitize_path_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    sanitized.trim_matches('-').to_string()
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then_some(values.iter().sum::<f64>() / values.len() as f64)
}

fn status_badge(passed: bool) -> &'static str {
    if passed {
        "<span class=\"ok\">passed</span>"
    } else {
        "<span class=\"bad\">failed</span>"
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn link_to_review_path(review_dir: &Path, output_dir: &Path, path: &Path, label: &str) -> String {
    let href_path = if let Ok(relative) = path.strip_prefix(review_dir) {
        relative.to_path_buf()
    } else if let Ok(relative) = path.strip_prefix(output_dir) {
        PathBuf::from("..").join(relative)
    } else {
        path.to_path_buf()
    };
    format!(
        "<a href=\"{}\">{}</a>",
        html_escape(&href_path.to_string_lossy().replace('\\', "/")),
        html_escape(label)
    )
}

fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn copy_path(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let child_source = entry.path();
            let child_destination = destination.join(entry.file_name());
            copy_path(&child_source, &child_destination)?;
        }
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "Failed to copy {} -> {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn default_manifest_version() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_skill_package(root: &Path) {
        fs::create_dir_all(root).unwrap();
        fs::write(
            root.join("SKILL.md"),
            r#"---
name: skill-creator
description: Create and validate skills
---

Body
"#,
        )
        .unwrap();
    }

    #[test]
    fn load_eval_manifest_rejects_duplicate_case_ids() {
        let temp = TempDir::new().unwrap();
        create_skill_package(temp.path());
        fs::create_dir_all(temp.path().join("evals")).unwrap();
        fs::write(
            temp.path().join("evals/evals.json"),
            r#"{
  "version": 1,
  "cases": [
    {"id": "dup", "type": "trigger", "input": "create skill", "expected": true},
    {"id": "dup", "type": "trigger", "input": "validate skill", "expected": true}
  ]
}"#,
        )
        .unwrap();

        let error = load_eval_manifest(temp.path(), None).unwrap_err();
        assert!(error.to_string().contains("duplicate case id"));
    }

    #[test]
    fn run_eval_manifest_executes_trigger_and_command_cases() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("skill-creator");
        create_skill_package(&package_root);
        fs::create_dir_all(package_root.join("evals")).unwrap();
        fs::write(
            package_root.join("evals/evals.json"),
            r#"{
  "version": 1,
  "suite": "skill-creator",
  "cases": [
    {"id": "trigger", "type": "trigger", "input": "please use $skill-creator", "expected": true},
    {
      "id": "compare",
      "type": "command",
      "prompt": "Compare candidate and baseline outputs",
      "command": ["sh", "-c", "printf '{\"passed\":true,\"variant\":\"candidate\"}'"],
      "comparison": {
        "mode": "with_without_skill",
        "baseline_command": ["sh", "-c", "printf '{\"passed\":false,\"variant\":\"baseline\"}'"]
      },
      "grading": {
        "command": ["sh", "-c", "printf '{\"passed\":true,\"score\":1}'"]
      }
    }
  ]
}"#,
        )
        .unwrap();

        let summary = run_eval_manifest(&SkillEvalRunOptions {
            package_root: package_root.clone(),
            manifest_path: package_root.join("evals/evals.json"),
            output_dir: Some(package_root.join("run-output")),
        })
        .unwrap();

        assert!(summary.passed());
        assert_eq!(summary.cases.len(), 2);
        assert_eq!(summary.benchmark.total_cases, 2);
        assert_eq!(summary.benchmark.passed_cases, 2);
        assert_eq!(summary.benchmark.command_cases, 1);
        assert!(summary.output_dir.join(BENCHMARK_FILE).is_file());
        assert!(summary.output_dir.join(RUN_SUMMARY_FILE).is_file());
        assert!(summary.review_bundle_path.unwrap().is_file());
    }

    #[test]
    fn run_eval_manifest_supports_new_old_comparison_mode_labels() {
        let temp = TempDir::new().unwrap();
        create_skill_package(temp.path());
        fs::create_dir_all(temp.path().join("evals")).unwrap();
        fs::write(
            temp.path().join("evals/evals.json"),
            r#"{
  "version": 1,
  "cases": [
    {
      "id": "compare-version",
      "type": "command",
      "command": ["sh", "-c", "printf '{\"passed\":true,\"variant\":\"new\"}'"],
      "comparison": {
        "mode": "new_old_skill",
        "baseline_command": ["sh", "-c", "printf '{\"passed\":true,\"variant\":\"old\"}'"]
      }
    }
  ]
}"#,
        )
        .unwrap();

        let summary = run_eval_manifest(&SkillEvalRunOptions {
            package_root: temp.path().to_path_buf(),
            manifest_path: temp.path().join("evals/evals.json"),
            output_dir: Some(temp.path().join("run-output")),
        })
        .unwrap();

        let SkillEvalCaseRunSummary::Command {
            candidate_label,
            baseline_label,
            ..
        } = &summary.cases[0]
        else {
            panic!("expected command case");
        };

        assert_eq!(candidate_label, "new_skill");
        assert_eq!(baseline_label.as_deref(), Some("old_skill"));
        assert!(
            summary
                .output_dir
                .join("cases/compare-version/new_skill.json")
                .is_file()
        );
        assert!(
            summary
                .output_dir
                .join("cases/compare-version/old_skill.json")
                .is_file()
        );
        assert_eq!(
            summary
                .benchmark
                .comparison_mode_counts
                .get("new_old_skill")
                .copied(),
            Some(1)
        );
    }
}
