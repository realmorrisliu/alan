mod dataset;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use dataset::{
    build_row_index, load_dataset_rows, normalize_preparation_row, read_instance_ids,
    required_row_string,
};

#[derive(Debug, Clone)]
pub struct PrepareSwebenchLiteWorkspacesOptions {
    pub instance_ids_file: PathBuf,
    pub dataset_files: Vec<PathBuf>,
    pub dataset_name: Option<String>,
    pub split: String,
    pub workspace_root: PathBuf,
    pub repo_cache_root: Option<PathBuf>,
    pub github_root: String,
    pub workspace_map_output: Option<PathBuf>,
    pub skip_mirror_fetch: bool,
    pub reuse_existing_workspaces: bool,
}

#[derive(Debug, Clone)]
pub struct MaterializeSwebenchLiteSubsetOptions {
    pub instance_ids_file: PathBuf,
    pub dataset_files: Vec<PathBuf>,
    pub dataset_name: Option<String>,
    pub split: String,
    pub workspace_root: Option<PathBuf>,
    pub workspace_map_file: Option<PathBuf>,
    pub output_dir: PathBuf,
    pub suite_name: String,
    pub dataset_label: String,
    pub scoring_dataset_name: String,
    pub max_workers: usize,
    pub timeout_secs: u64,
    pub allow_missing_workspaces: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwebenchLitePreparationEntry {
    pub instance_id: String,
    pub repo: String,
    pub base_commit: String,
    pub environment_setup_commit: String,
    pub workspace_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwebenchLitePreparationFailure {
    pub instance_id: String,
    pub repo: String,
    pub base_commit: String,
    pub environment_setup_commit: String,
    pub workspace_dir: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwebenchLiteWorkspacePreparationReport {
    pub instance_ids_file: String,
    pub instance_count: usize,
    pub dataset_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_source: Option<String>,
    pub workspace_root: String,
    pub repo_cache_root: String,
    pub workspace_map_file: String,
    pub github_root: String,
    pub reuse_existing_workspaces: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recreated_mirrors: Vec<String>,
    pub repos: BTreeMap<String, usize>,
    pub prepared_count: usize,
    pub reused_count: usize,
    pub recreated_count: usize,
    pub failed_count: usize,
    pub prepared: Vec<SwebenchLitePreparationEntry>,
    pub reused: Vec<SwebenchLitePreparationEntry>,
    pub recreated: Vec<SwebenchLitePreparationEntry>,
    pub failures: Vec<SwebenchLitePreparationFailure>,
}

#[derive(Debug, Clone)]
pub struct PrepareSwebenchLiteWorkspacesResult {
    pub workspace_root: PathBuf,
    pub workspace_map_path: PathBuf,
    pub report_path: PathBuf,
    pub workspace_map: BTreeMap<String, String>,
    pub report: SwebenchLiteWorkspacePreparationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwebenchLiteSubsetMaterializationReport {
    pub suite: String,
    pub dataset_name: String,
    pub instance_ids_file: String,
    pub instance_count: usize,
    pub repos: BTreeMap<String, usize>,
    pub dataset_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_map_file: Option<String>,
    pub allow_missing_workspaces: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_workspace_dirs: Vec<String>,
    pub suite_json: String,
}

#[derive(Debug, Clone)]
pub struct MaterializeSwebenchLiteSubsetResult {
    pub suite_path: PathBuf,
    pub report_path: PathBuf,
    pub report: SwebenchLiteSubsetMaterializationReport,
}

pub fn prepare_swebench_lite_workspaces(
    options: &PrepareSwebenchLiteWorkspacesOptions,
) -> Result<PrepareSwebenchLiteWorkspacesResult> {
    if options.dataset_files.is_empty() && options.dataset_name.is_none() {
        bail!("Provide at least one --dataset-file or --dataset-name.");
    }

    let instance_ids_file = resolve_path(&options.instance_ids_file)?;
    let workspace_root = resolve_path(&options.workspace_root)?;
    let repo_cache_root = match options.repo_cache_root.as_ref() {
        Some(path) => resolve_path(path)?,
        None => workspace_root.join(".repo-cache"),
    };
    let workspace_map_path = match options.workspace_map_output.as_ref() {
        Some(path) => resolve_path(path)?,
        None => workspace_root.join("workspace_map.json"),
    };
    let report_path = workspace_root.join("preparation_report.json");

    let instance_ids = read_instance_ids(&instance_ids_file)?;
    let dataset_files = options
        .dataset_files
        .iter()
        .map(|path| resolve_path(path))
        .collect::<Result<Vec<_>>>()?;
    let dataset_rows = load_dataset_rows(
        &dataset_files,
        options.dataset_name.as_deref(),
        &options.split,
    )?;
    let row_index = build_row_index(dataset_rows);

    let missing_rows: Vec<_> = instance_ids
        .iter()
        .filter(|instance_id| !row_index.contains_key(*instance_id))
        .cloned()
        .collect();
    if !missing_rows.is_empty() {
        bail!(
            "Missing instance rows in dataset input: {}",
            missing_rows.join(", ")
        );
    }

    fs::create_dir_all(&workspace_root)?;
    fs::create_dir_all(&repo_cache_root)?;

    let mut repo_counts = BTreeMap::new();
    let mut rows_by_instance = BTreeMap::new();
    for instance_id in &instance_ids {
        let row = normalize_preparation_row(
            row_index
                .get(instance_id)
                .expect("missing row already checked above"),
            instance_id,
        )?;
        *repo_counts.entry(row.repo.clone()).or_insert(0) += 1;
        rows_by_instance.insert(instance_id.clone(), row);
    }

    let mut mirror_errors = BTreeMap::new();
    let mut mirror_by_repo = BTreeMap::new();
    let mut recreated_mirrors = Vec::new();
    for repo in repo_counts.keys() {
        let mirror_path = repo_cache_root.join(format!("{}.git", slug_repo_name(repo)));
        mirror_by_repo.insert(repo.clone(), mirror_path.clone());
        match ensure_repo_mirror(
            repo,
            &mirror_path,
            &repo_cache_root,
            &options.github_root,
            options.skip_mirror_fetch,
        ) {
            Ok(recreated) => {
                if recreated {
                    recreated_mirrors.push(repo.clone());
                }
            }
            Err(err) => {
                mirror_errors.insert(repo.clone(), err.to_string());
            }
        }
    }

    let reserved_workspace_paths = vec![
        ("repo cache root".to_string(), repo_cache_root.clone()),
        (
            "workspace map output".to_string(),
            workspace_map_path.clone(),
        ),
        ("preparation report".to_string(), report_path.clone()),
    ];

    let mut workspace_map = BTreeMap::new();
    let mut prepared = Vec::new();
    let mut reused = Vec::new();
    let mut recreated = Vec::new();
    let mut failures = Vec::new();

    for instance_id in &instance_ids {
        let row = rows_by_instance
            .get(instance_id)
            .expect("instance row already normalized");
        let workspace_dir_candidate = workspace_root.join(instance_id);

        let workspace_dir = match resolve_owned_workspace_dir(
            instance_id,
            &workspace_root,
            &reserved_workspace_paths,
        ) {
            Ok(path) => path,
            Err(err) => {
                failures.push(SwebenchLitePreparationFailure {
                    instance_id: instance_id.clone(),
                    repo: row.repo.clone(),
                    base_commit: row.base_commit.clone(),
                    environment_setup_commit: row.environment_setup_commit.clone(),
                    workspace_dir: workspace_dir_candidate.display().to_string(),
                    reason: err.to_string(),
                });
                continue;
            }
        };

        workspace_map.insert(instance_id.clone(), workspace_dir.display().to_string());

        let mirror_path = match mirror_by_repo.get(&row.repo) {
            Some(path) => path,
            None => {
                failures.push(SwebenchLitePreparationFailure {
                    instance_id: instance_id.clone(),
                    repo: row.repo.clone(),
                    base_commit: row.base_commit.clone(),
                    environment_setup_commit: row.environment_setup_commit.clone(),
                    workspace_dir: workspace_dir.display().to_string(),
                    reason: format!("missing mirror mapping for repo {:?}", row.repo),
                });
                continue;
            }
        };

        if let Some(mirror_error) = mirror_errors.get(&row.repo) {
            failures.push(SwebenchLitePreparationFailure {
                instance_id: instance_id.clone(),
                repo: row.repo.clone(),
                base_commit: row.base_commit.clone(),
                environment_setup_commit: row.environment_setup_commit.clone(),
                workspace_dir: workspace_dir.display().to_string(),
                reason: format!("mirror_error: {mirror_error}"),
            });
            continue;
        }

        let entry = SwebenchLitePreparationEntry {
            instance_id: instance_id.clone(),
            repo: row.repo.clone(),
            base_commit: row.base_commit.clone(),
            environment_setup_commit: row.environment_setup_commit.clone(),
            workspace_dir: workspace_dir.display().to_string(),
        };

        match prepare_workspace_instance(
            &workspace_root,
            &workspace_dir,
            &row.repo,
            &row.base_commit,
            &options.github_root,
            mirror_path,
            options.reuse_existing_workspaces,
        ) {
            Ok(WorkspacePreparationOutcome::Prepared) => prepared.push(entry),
            Ok(WorkspacePreparationOutcome::Reused) => reused.push(entry),
            Ok(WorkspacePreparationOutcome::RecreatedAndPrepared) => {
                recreated.push(entry.clone());
                prepared.push(entry);
            }
            Err(err) => failures.push(SwebenchLitePreparationFailure {
                instance_id: instance_id.clone(),
                repo: row.repo.clone(),
                base_commit: row.base_commit.clone(),
                environment_setup_commit: row.environment_setup_commit.clone(),
                workspace_dir: workspace_dir.display().to_string(),
                reason: err.to_string(),
            }),
        }
    }

    write_json(&workspace_map_path, &workspace_map)?;

    let report = SwebenchLiteWorkspacePreparationReport {
        instance_ids_file: instance_ids_file.display().to_string(),
        instance_count: instance_ids.len(),
        dataset_files: dataset_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        dataset_source: options.dataset_name.clone(),
        workspace_root: workspace_root.display().to_string(),
        repo_cache_root: repo_cache_root.display().to_string(),
        workspace_map_file: workspace_map_path.display().to_string(),
        github_root: options.github_root.clone(),
        reuse_existing_workspaces: options.reuse_existing_workspaces,
        recreated_mirrors,
        repos: repo_counts,
        prepared_count: prepared.len(),
        reused_count: reused.len(),
        recreated_count: recreated.len(),
        failed_count: failures.len(),
        prepared,
        reused,
        recreated,
        failures,
    };
    write_json(&report_path, &report)?;

    Ok(PrepareSwebenchLiteWorkspacesResult {
        workspace_root,
        workspace_map_path,
        report_path,
        workspace_map,
        report,
    })
}

pub fn materialize_swebench_lite_subset(
    options: &MaterializeSwebenchLiteSubsetOptions,
) -> Result<MaterializeSwebenchLiteSubsetResult> {
    if options.dataset_files.is_empty() && options.dataset_name.is_none() {
        bail!("Provide at least one --dataset-file or --dataset-name.");
    }

    let instance_ids_file = resolve_path(&options.instance_ids_file)?;
    let output_dir = resolve_path(&options.output_dir)?;
    let instance_ids = read_instance_ids(&instance_ids_file)?;
    let dataset_files = options
        .dataset_files
        .iter()
        .map(|path| resolve_path(path))
        .collect::<Result<Vec<_>>>()?;
    let dataset_rows = load_dataset_rows(
        &dataset_files,
        options.dataset_name.as_deref(),
        &options.split,
    )?;
    let row_index = build_row_index(dataset_rows);

    let missing_rows: Vec<_> = instance_ids
        .iter()
        .filter(|instance_id| !row_index.contains_key(*instance_id))
        .cloned()
        .collect();
    if !missing_rows.is_empty() {
        bail!(
            "Missing instance rows in dataset input: {}",
            missing_rows.join(", ")
        );
    }

    let (workspace_map, missing_workspace_mappings) = load_workspace_map(options, &instance_ids)?;
    if !missing_workspace_mappings.is_empty() {
        bail!(
            "Missing workspace mapping for instances: {}",
            missing_workspace_mappings.join(", ")
        );
    }

    fs::create_dir_all(&output_dir)?;
    let cases_dir = output_dir.join("cases");
    let statements_dir = output_dir.join("problem_statements");
    fs::create_dir_all(&cases_dir)?;
    fs::create_dir_all(&statements_dir)?;

    let mut missing_workspace_dirs = Vec::new();
    let mut repo_counts = BTreeMap::new();
    let mut case_files = Vec::new();

    for instance_id in &instance_ids {
        let row = row_index
            .get(instance_id)
            .expect("missing row already checked above");
        let workspace_dir = workspace_map
            .get(instance_id)
            .expect("missing workspace already checked above");
        if !workspace_dir.exists() {
            missing_workspace_dirs.push(instance_id.clone());
            if !options.allow_missing_workspaces {
                bail!(
                    "Workspace directory does not exist for {}: {}",
                    instance_id,
                    workspace_dir.display()
                );
            }
        }

        let repo = row
            .get("repo")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        *repo_counts.entry(repo).or_insert(0) += 1;

        let problem_statement = required_row_string(row, "problem_statement", instance_id)?;
        let problem_statement_path = resolve_materialized_output_path(
            instance_id,
            &statements_dir,
            ".txt",
            "problem statement path",
        )?;
        let case_path = resolve_materialized_output_path(
            instance_id,
            &cases_dir,
            ".json",
            "case manifest path",
        )?;

        if let Some(parent) = problem_statement_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&problem_statement_path, format!("{problem_statement}\n"))?;

        let case_payload = serde_json::json!({
            "instance_id": instance_id,
            "dataset": options.dataset_label,
            "workspace_dir": workspace_dir,
            "problem_statement_file": problem_statement_path,
            "timeout_secs": options.timeout_secs,
        });
        write_json(&case_path, &case_payload)?;
        case_files.push(
            case_path
                .strip_prefix(&output_dir)
                .unwrap_or(&case_path)
                .display()
                .to_string(),
        );
    }

    let suite_path = output_dir.join("suite.json");
    let suite_payload = serde_json::json!({
        "suite": options.suite_name,
        "dataset": options.dataset_label,
        "dataset_name": options.scoring_dataset_name,
        "max_workers": options.max_workers,
        "cases": case_files,
    });
    write_json(&suite_path, &suite_payload)?;

    let report_path = output_dir.join("materialization_report.json");
    let report = SwebenchLiteSubsetMaterializationReport {
        suite: options.suite_name.clone(),
        dataset_name: options.scoring_dataset_name.clone(),
        instance_ids_file: instance_ids_file.display().to_string(),
        instance_count: instance_ids.len(),
        repos: repo_counts,
        dataset_files: dataset_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        dataset_source: options.dataset_name.clone(),
        workspace_root: options
            .workspace_root
            .as_ref()
            .map(|path| resolve_path(path))
            .transpose()?
            .map(|path| path.display().to_string()),
        workspace_map_file: options
            .workspace_map_file
            .as_ref()
            .map(|path| resolve_path(path))
            .transpose()?
            .map(|path| path.display().to_string()),
        allow_missing_workspaces: options.allow_missing_workspaces,
        missing_workspace_dirs,
        suite_json: suite_path.display().to_string(),
    };
    write_json(&report_path, &report)?;

    Ok(MaterializeSwebenchLiteSubsetResult {
        suite_path,
        report_path,
        report,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspacePreparationOutcome {
    Prepared,
    Reused,
    RecreatedAndPrepared,
}

fn prepare_workspace_instance(
    workspace_root: &Path,
    workspace_dir: &Path,
    repo: &str,
    base_commit: &str,
    github_root: &str,
    mirror_path: &Path,
    reuse_existing_workspaces: bool,
) -> Result<WorkspacePreparationOutcome> {
    let mut recreated = false;
    if workspace_dir.exists() {
        if reuse_existing_workspaces
            && let Ok(true) = existing_workspace_matches(workspace_dir, base_commit)
        {
            ensure_origin_url(workspace_dir, &repo_clone_url(repo, github_root))?;
            return Ok(WorkspacePreparationOutcome::Reused);
        }
        reset_owned_directory(workspace_dir, workspace_root, "workspace directory")?;
        recreated = true;
    }

    prepare_workspace(workspace_dir, mirror_path, repo, base_commit, github_root)?;
    Ok(if recreated {
        WorkspacePreparationOutcome::RecreatedAndPrepared
    } else {
        WorkspacePreparationOutcome::Prepared
    })
}

fn resolve_materialized_output_path(
    instance_id: &str,
    output_root: &Path,
    suffix: &str,
    label: &str,
) -> Result<PathBuf> {
    ensure_owned_child_path(
        &output_root.join(format!("{instance_id}{suffix}")),
        output_root,
        &format!("{label} for instance_id {instance_id:?}"),
    )
}

fn load_workspace_map(
    options: &MaterializeSwebenchLiteSubsetOptions,
    instance_ids: &[String],
) -> Result<(BTreeMap<String, PathBuf>, Vec<String>)> {
    let mut mapping = BTreeMap::new();

    if let Some(workspace_map_file) = options.workspace_map_file.as_ref() {
        let workspace_map_file = resolve_path(workspace_map_file)?;
        let payload: Value =
            serde_json::from_str(&fs::read_to_string(&workspace_map_file).with_context(|| {
                format!(
                    "Failed to read workspace map file {}",
                    workspace_map_file.display()
                )
            })?)
            .with_context(|| {
                format!(
                    "Failed to parse workspace map file {}",
                    workspace_map_file.display()
                )
            })?;
        let payload = payload
            .as_object()
            .ok_or_else(|| anyhow!("--workspace-map-file must contain a JSON object"))?;
        for (instance_id, raw_path) in payload {
            let path = raw_path
                .as_str()
                .ok_or_else(|| anyhow!("workspace map entry for {instance_id} must be a string"))?;
            mapping.insert(instance_id.clone(), resolve_path(Path::new(path))?);
        }
    }

    if let Some(workspace_root) = options.workspace_root.as_ref() {
        let workspace_root = resolve_path(workspace_root)?;
        for instance_id in instance_ids {
            if mapping.contains_key(instance_id) {
                continue;
            }
            mapping.insert(
                instance_id.clone(),
                ensure_owned_child_path(
                    &workspace_root.join(instance_id),
                    &workspace_root,
                    &format!("workspace directory for instance_id {instance_id:?}"),
                )?,
            );
        }
    }

    if mapping.is_empty() {
        bail!("Provide --workspace-root or --workspace-map-file");
    }

    let missing = instance_ids
        .iter()
        .filter(|instance_id| !mapping.contains_key(*instance_id))
        .cloned()
        .collect();
    Ok((mapping, missing))
}

fn repo_clone_url(repo: &str, github_root: &str) -> String {
    format!("{}/{repo}.git", github_root.trim_end_matches('/'))
}

fn ensure_origin_url(repo_path: &Path, origin_url: &str) -> Result<()> {
    if run_git(["remote", "set-url", "origin", origin_url], Some(repo_path)).is_err() {
        run_git(["remote", "add", "origin", origin_url], Some(repo_path))?;
    }
    Ok(())
}

fn resolve_owned_workspace_dir(
    instance_id: &str,
    workspace_root: &Path,
    reserved_paths: &[(String, PathBuf)],
) -> Result<PathBuf> {
    let workspace_dir = ensure_owned_child_path(
        &workspace_root.join(instance_id),
        workspace_root,
        &format!("workspace directory for instance_id {instance_id:?}"),
    )?;
    for (reserved_label, reserved_path) in reserved_paths {
        if paths_overlap(&workspace_dir, reserved_path)? {
            bail!(
                "workspace directory for instance_id {instance_id:?} overlaps {reserved_label}: {}",
                reserved_path.display()
            );
        }
    }
    Ok(workspace_dir)
}

fn reset_owned_directory(path: &Path, owner_root: &Path, label: &str) -> Result<()> {
    ensure_owned_child_path(path, owner_root, label)?;
    if path.is_symlink() {
        bail!("{label} must not be a symlink: {}", path.display());
    }
    if !path.exists() {
        return Ok(());
    }
    if !path.is_dir() {
        bail!("{label} exists but is not a directory: {}", path.display());
    }
    fs::remove_dir_all(path)
        .with_context(|| format!("Failed to remove {label} {}", path.display()))?;
    Ok(())
}

fn is_bare_git_repository(path: &Path) -> bool {
    matches!(
        run_git(["rev-parse", "--is-bare-repository"], Some(path)),
        Ok(value) if value == "true"
    )
}

fn ensure_repo_mirror(
    repo: &str,
    mirror_path: &Path,
    repo_cache_root: &Path,
    github_root: &str,
    skip_fetch: bool,
) -> Result<bool> {
    let clone_url = repo_clone_url(repo, github_root);
    let mut recreated = false;

    if mirror_path.exists() {
        if mirror_path.is_symlink() {
            bail!(
                "mirror path must not be a symlink: {}",
                mirror_path.display()
            );
        }
        if !is_bare_git_repository(mirror_path) {
            reset_owned_directory(mirror_path, repo_cache_root, "mirror path")?;
            recreated = true;
        } else {
            ensure_origin_url(mirror_path, &clone_url)?;
            if !skip_fetch {
                run_git(["remote", "update", "--prune"], Some(mirror_path))?;
            }
            return Ok(false);
        }
    }

    if let Some(parent) = mirror_path.parent() {
        fs::create_dir_all(parent)?;
    }
    run_git(
        vec![
            OsString::from("clone"),
            OsString::from("--mirror"),
            mirror_path_arg(&clone_url),
            mirror_path.as_os_str().to_owned(),
        ],
        None,
    )?;
    Ok(recreated)
}

fn existing_workspace_matches(workspace_dir: &Path, base_commit: &str) -> Result<bool> {
    if !workspace_dir.exists() {
        return Ok(false);
    }
    if workspace_dir.is_symlink() {
        bail!(
            "Workspace path must not be a symlink: {}",
            workspace_dir.display()
        );
    }
    if !workspace_dir.is_dir() {
        bail!(
            "Workspace path exists but is not a directory: {}",
            workspace_dir.display()
        );
    }

    let top_level = resolve_path(Path::new(&run_git(
        ["rev-parse", "--show-toplevel"],
        Some(workspace_dir),
    )?))?;
    let head_commit = run_git(["rev-parse", "HEAD"], Some(workspace_dir))?;
    let status = run_git(
        ["status", "--short", "--untracked-files=all"],
        Some(workspace_dir),
    )?;
    Ok(
        top_level == resolve_path(workspace_dir)?
            && head_commit == base_commit
            && status.is_empty(),
    )
}

fn prepare_workspace(
    workspace_dir: &Path,
    mirror_path: &Path,
    repo: &str,
    base_commit: &str,
    github_root: &str,
) -> Result<()> {
    if let Some(parent) = workspace_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    run_git(
        vec![
            OsString::from("clone"),
            mirror_path.as_os_str().to_owned(),
            workspace_dir.as_os_str().to_owned(),
        ],
        None,
    )?;
    ensure_origin_url(workspace_dir, &repo_clone_url(repo, github_root))?;
    run_git(["checkout", "--detach", base_commit], Some(workspace_dir))?;
    let status = run_git(
        ["status", "--short", "--untracked-files=all"],
        Some(workspace_dir),
    )?;
    if !status.is_empty() {
        bail!(
            "Prepared workspace is not clean: {}",
            workspace_dir.display()
        );
    }
    Ok(())
}

fn run_git<I, S>(args: I, cwd: Option<&Path>) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().context("Failed to spawn git command")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        bail!("git command failed: {detail}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn ensure_owned_child_path(path: &Path, owner_root: &Path, label: &str) -> Result<PathBuf> {
    let resolved_path = resolve_path(path)?;
    let resolved_root = resolve_path(owner_root)?;
    if resolved_path == resolved_root || !resolved_path.starts_with(&resolved_root) {
        bail!(
            "{label} must stay under {}: {}",
            resolved_root.display(),
            resolved_path.display()
        );
    }
    Ok(resolved_path)
}

fn paths_overlap(path: &Path, other: &Path) -> Result<bool> {
    let resolved_path = resolve_path(path)?;
    let resolved_other = resolve_path(other)?;
    Ok(resolved_path == resolved_other
        || resolved_path.starts_with(&resolved_other)
        || resolved_other.starts_with(&resolved_path))
}

fn resolve_path(path: &Path) -> Result<PathBuf> {
    let expanded_path = expand_home_path(path)?;
    let path = expanded_path.as_path();
    if let Ok(canonical) = fs::canonicalize(path) {
        return Ok(canonical);
    }
    let absolute = normalize_path(&if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("Failed to resolve current directory")?
            .join(path)
    });

    let mut existing_prefix = absolute.as_path();
    let mut suffix = Vec::new();
    while !existing_prefix.exists() {
        let file_name = existing_prefix
            .file_name()
            .ok_or_else(|| anyhow!("Failed to resolve path {}", absolute.display()))?;
        suffix.push(file_name.to_os_string());
        existing_prefix = existing_prefix
            .parent()
            .ok_or_else(|| anyhow!("Failed to resolve path {}", absolute.display()))?;
    }

    let mut resolved = fs::canonicalize(existing_prefix).with_context(|| {
        format!(
            "Failed to canonicalize existing prefix {} while resolving {}",
            existing_prefix.display(),
            absolute.display()
        )
    })?;
    for segment in suffix.iter().rev() {
        resolved.push(segment);
    }
    Ok(normalize_path(&resolved))
}

fn expand_home_path(path: &Path) -> Result<PathBuf> {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(segment)) if segment == OsStr::new("~") => {
            let mut expanded = home_dir_from_env()
                .context("Cannot determine home directory while resolving ~ path")?;
            for component in components {
                expanded.push(component.as_os_str());
            }
            Ok(expanded)
        }
        _ => Ok(path.to_path_buf()),
    }
}

fn home_dir_from_env() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    normalized
}

fn slug_repo_name(repo: &str) -> String {
    urlencoding::encode(repo).into_owned()
}

fn mirror_path_arg(clone_url: &str) -> OsString {
    OsString::from(clone_url)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{content}\n"))
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
#[path = "swebench_lite_tests.rs"]
mod tests;
