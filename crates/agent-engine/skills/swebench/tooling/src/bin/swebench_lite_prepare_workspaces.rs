use alan_swebench_tooling::{
    PrepareSwebenchLiteWorkspacesOptions, prepare_swebench_lite_workspaces,
};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[arg(long = "instance-ids-file")]
    instance_ids_file: PathBuf,
    #[arg(long = "dataset-file")]
    dataset_files: Vec<PathBuf>,
    #[arg(long = "dataset-name")]
    dataset_name: Option<String>,
    #[arg(long, default_value = "test")]
    split: String,
    #[arg(long = "workspace-root")]
    workspace_root: PathBuf,
    #[arg(long = "repo-cache-root")]
    repo_cache_root: Option<PathBuf>,
    #[arg(long = "github-root", default_value = "https://github.com")]
    github_root: String,
    #[arg(long = "workspace-map-output")]
    workspace_map_output: Option<PathBuf>,
    #[arg(long = "skip-mirror-fetch", default_value_t = false)]
    skip_mirror_fetch: bool,
    #[arg(long = "reuse-existing-workspaces", default_value_t = false)]
    reuse_existing_workspaces: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let result = prepare_swebench_lite_workspaces(&PrepareSwebenchLiteWorkspacesOptions {
        instance_ids_file: cli.instance_ids_file,
        dataset_files: cli.dataset_files,
        dataset_name: cli.dataset_name,
        split: cli.split,
        workspace_root: cli.workspace_root,
        repo_cache_root: cli.repo_cache_root,
        github_root: cli.github_root,
        workspace_map_output: cli.workspace_map_output,
        skip_mirror_fetch: cli.skip_mirror_fetch,
        reuse_existing_workspaces: cli.reuse_existing_workspaces,
    })?;
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
        std::process::exit(1);
    }
    Ok(())
}
