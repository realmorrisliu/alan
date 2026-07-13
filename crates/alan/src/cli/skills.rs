use crate::cli::skill_authoring::{
    SkillTemplateKind, eval_skill_package, init_skill_package, validate_skill_package,
};
use alan_swebench_tooling::{
    MaterializeSwebenchLiteSubsetOptions, PrepareSwebenchLiteWorkspacesOptions,
    materialize_swebench_lite_subset, prepare_swebench_lite_workspaces,
};
use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn run_init_skill_package(
    path: PathBuf,
    template: SkillTemplateKind,
    name: Option<&str>,
    description: Option<&str>,
    short_description: Option<&str>,
    force: bool,
) -> Result<()> {
    let result = init_skill_package(&path, template, name, description, short_description, force)?;
    print!("{}", result.render_text());
    Ok(())
}

pub fn run_validate_skill_package(path: Option<PathBuf>, json: bool, strict: bool) -> Result<bool> {
    let package_root = canonicalize_package_input(path)?;
    let report = validate_skill_package(&package_root);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.render_text());
    }
    Ok(report.passes(strict))
}

pub fn run_eval_skill_package(
    path: Option<PathBuf>,
    manifest: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    require_hook: bool,
) -> Result<bool> {
    let package_root = canonicalize_package_input(path)?;
    let result = eval_skill_package(
        &package_root,
        manifest.as_deref(),
        output_dir.as_deref(),
        require_hook,
    )?;
    print!("{}", result.render_text());
    Ok(result.passed(require_hook))
}

pub fn run_prepare_swebench_lite_workspaces(
    options: PrepareSwebenchLiteWorkspacesOptions,
) -> Result<bool> {
    let result = prepare_swebench_lite_workspaces(&options)?;
    println!("workspace_root\t{}", result.workspace_root.display());
    println!("workspace_map\t{}", result.workspace_map_path.display());
    println!("report\t{}", result.report_path.display());
    println!("prepared_count\t{}", result.report.prepared_count);
    println!("reused_count\t{}", result.report.reused_count);
    println!("recreated_count\t{}", result.report.recreated_count);
    println!("failed_count\t{}", result.report.failed_count);
    for (repo, count) in &result.report.repos {
        println!("repo_count\t{repo}\t{count}");
    }
    if result.report.failed_count > 0 {
        for failure in &result.report.failures {
            eprintln!("failure\t{}\t{}", failure.instance_id, failure.reason);
        }
        return Ok(false);
    }
    Ok(true)
}

pub fn run_materialize_swebench_lite_subset(
    options: MaterializeSwebenchLiteSubsetOptions,
) -> Result<()> {
    let result = materialize_swebench_lite_subset(&options)?;
    println!("suite_json\t{}", result.suite_path.display());
    println!("instance_count\t{}", result.report.instance_count);
    for (repo, count) in &result.report.repos {
        println!("repo_count\t{repo}\t{count}");
    }
    if !result.report.missing_workspace_dirs.is_empty() {
        eprintln!(
            "warning\tmissing_workspaces\t{}",
            result.report.missing_workspace_dirs.join(",")
        );
    }
    Ok(())
}

fn canonicalize_package_input(path: Option<PathBuf>) -> Result<PathBuf> {
    let path = path.unwrap_or(
        std::env::current_dir()
            .context("Cannot determine current directory for skill package operation")?,
    );
    std::fs::canonicalize(&path)
        .with_context(|| format!("Cannot resolve skill package path: {}", path.display()))
}
