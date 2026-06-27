use alan_swebench_tooling::{
    MaterializeSwebenchLiteSubsetOptions, materialize_swebench_lite_subset,
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
    workspace_root: Option<PathBuf>,
    #[arg(long = "workspace-map-file")]
    workspace_map_file: Option<PathBuf>,
    #[arg(long = "output-dir")]
    output_dir: PathBuf,
    #[arg(long = "suite-name", default_value = "swebench_lite_pilot_v1")]
    suite_name: String,
    #[arg(long = "dataset-label", default_value = "SWE-bench Lite")]
    dataset_label: String,
    #[arg(
        long = "scoring-dataset-name",
        default_value = "princeton-nlp/SWE-bench_Lite"
    )]
    scoring_dataset_name: String,
    #[arg(long = "max-workers", default_value_t = 4)]
    max_workers: usize,
    #[arg(long = "timeout-secs", default_value_t = 1800)]
    timeout_secs: u64,
    #[arg(long = "allow-missing-workspaces", default_value_t = false)]
    allow_missing_workspaces: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let result = materialize_swebench_lite_subset(&MaterializeSwebenchLiteSubsetOptions {
        instance_ids_file: cli.instance_ids_file,
        dataset_files: cli.dataset_files,
        dataset_name: cli.dataset_name,
        split: cli.split,
        workspace_root: cli.workspace_root,
        workspace_map_file: cli.workspace_map_file,
        output_dir: cli.output_dir,
        suite_name: cli.suite_name,
        dataset_label: cli.dataset_label,
        scoring_dataset_name: cli.scoring_dataset_name,
        max_workers: cli.max_workers,
        timeout_secs: cli.timeout_secs,
        allow_missing_workspaces: cli.allow_missing_workspaces,
    })?;
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
