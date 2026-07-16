use super::*;
use tempfile::TempDir;

fn git<I, S>(cwd: &Path, args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_git(args, Some(cwd)).unwrap()
}

fn create_bare_repo_fixture(temp: &TempDir) -> (String, String, PathBuf) {
    let source_repo = temp.path().join("source");
    fs::create_dir_all(&source_repo).unwrap();
    git(&source_repo, ["init"]);
    fs::write(source_repo.join("README.md"), "hello\n").unwrap();
    git(&source_repo, ["add", "README.md"]);
    git(
        &source_repo,
        [
            "-c",
            "user.name=alan Test",
            "-c",
            "user.email=alan@example.com",
            "commit",
            "-m",
            "initial",
        ],
    );
    let base_commit = git(&source_repo, ["rev-parse", "HEAD"]);

    let github_root = temp.path().join("github");
    let bare_repo = github_root.join("owner/repo.git");
    fs::create_dir_all(bare_repo.parent().unwrap()).unwrap();
    run_git(
        vec![
            OsString::from("clone"),
            OsString::from("--bare"),
            source_repo.as_os_str().to_owned(),
            bare_repo.as_os_str().to_owned(),
        ],
        None,
    )
    .unwrap();

    (
        base_commit,
        format!("file://{}", github_root.display()),
        github_root,
    )
}

#[test]
fn read_instance_ids_ignores_comments_and_blank_lines() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("instance_ids.txt");
    fs::write(&path, "\n# comment\nfoo\n  \nbar\n").unwrap();

    let instance_ids = read_instance_ids(&path).unwrap();
    assert_eq!(instance_ids, vec!["foo".to_string(), "bar".to_string()]);
}

#[test]
fn resolve_path_expands_home_prefix_before_resolution() {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME must be set for this test"));
    let resolved_home = fs::canonicalize(&home).unwrap_or_else(|_| normalize_path(&home));

    let resolved =
        resolve_path(Path::new("~/alan-swebench-missing/../workspace_map.json")).unwrap();

    assert_eq!(
        resolved,
        normalize_path(&resolved_home.join("workspace_map.json"))
    );
}

#[test]
fn materialize_subset_writes_case_suite_and_report_files() {
    let temp = TempDir::new().unwrap();
    let instance_ids_file = temp.path().join("instance_ids.txt");
    let dataset_file = temp.path().join("dataset.json");
    let workspace_root = temp.path().join("workspaces");
    let output_dir = temp.path().join("manifests");

    fs::write(&instance_ids_file, "repo__case-1\n").unwrap();
    fs::create_dir_all(workspace_root.join("repo__case-1")).unwrap();
    fs::write(
        &dataset_file,
        r#"[
  {
"instance_id": "repo__case-1",
"repo": "owner/repo",
"problem_statement": "Fix the failing test."
  }
]"#,
    )
    .unwrap();

    let result = materialize_swebench_lite_subset(&MaterializeSwebenchLiteSubsetOptions {
        instance_ids_file,
        dataset_files: vec![dataset_file],
        dataset_name: None,
        split: "test".to_string(),
        workspace_root: Some(workspace_root),
        workspace_map_file: None,
        output_dir: output_dir.clone(),
        suite_name: "pilot".to_string(),
        dataset_label: "SWE-bench Lite".to_string(),
        scoring_dataset_name: "princeton-nlp/SWE-bench_Lite".to_string(),
        max_workers: 4,
        timeout_secs: 1800,
        allow_missing_workspaces: false,
    })
    .unwrap();

    assert_eq!(result.report.instance_count, 1);
    assert!(result.suite_path.is_file());
    assert!(output_dir.join("cases/repo__case-1.json").is_file());
    assert!(
        output_dir
            .join("problem_statements/repo__case-1.txt")
            .is_file()
    );
    assert!(result.report_path.is_file());
}

#[test]
fn prepare_workspaces_clones_local_git_repositories() {
    let temp = TempDir::new().unwrap();
    let instance_ids_file = temp.path().join("instance_ids.txt");
    let dataset_file = temp.path().join("dataset.json");
    let workspace_root = temp.path().join("workspaces");
    let (base_commit, github_root, _) = create_bare_repo_fixture(&temp);

    fs::write(&instance_ids_file, "repo__case-1\n").unwrap();
    fs::write(
        &dataset_file,
        format!(
            r#"[
  {{
"instance_id": "repo__case-1",
"repo": "owner/repo",
"base_commit": "{base_commit}",
"environment_setup_commit": ""
  }}
]"#
        ),
    )
    .unwrap();

    let result = prepare_swebench_lite_workspaces(&PrepareSwebenchLiteWorkspacesOptions {
        instance_ids_file: instance_ids_file.clone(),
        dataset_files: vec![dataset_file.clone()],
        dataset_name: None,
        split: "test".to_string(),
        workspace_root: workspace_root.clone(),
        repo_cache_root: None,
        github_root: github_root.clone(),
        workspace_map_output: None,
        skip_mirror_fetch: false,
        reuse_existing_workspaces: false,
    })
    .unwrap();

    assert_eq!(result.report.failed_count, 0);
    assert!(result.report.recreated_mirrors.is_empty());
    let workspace = workspace_root.join("repo__case-1");
    assert!(workspace.is_dir());
    assert_eq!(git(&workspace, ["rev-parse", "HEAD"]), base_commit);
    assert!(result.workspace_map_path.is_file());
    assert!(result.report_path.is_file());

    let repo_cache_root = PathBuf::from(&result.report.repo_cache_root);
    let mirror_path = repo_cache_root.join(format!("{}.git", slug_repo_name("owner/repo")));
    fs::remove_dir_all(&mirror_path).unwrap();
    fs::create_dir_all(&mirror_path).unwrap();
    fs::write(mirror_path.join("stale.txt"), "partial mirror").unwrap();

    let rerun = prepare_swebench_lite_workspaces(&PrepareSwebenchLiteWorkspacesOptions {
        instance_ids_file,
        dataset_files: vec![dataset_file],
        dataset_name: None,
        split: "test".to_string(),
        workspace_root,
        repo_cache_root: None,
        github_root,
        workspace_map_output: None,
        skip_mirror_fetch: false,
        reuse_existing_workspaces: true,
    })
    .unwrap();

    assert_eq!(rerun.report.recreated_mirrors, vec!["owner/repo"]);
    assert_eq!(rerun.report.reused_count, 1);
}

#[test]
fn prepare_workspaces_recreates_invalid_existing_workspace_when_reuse_is_enabled() {
    let temp = TempDir::new().unwrap();
    let instance_ids_file = temp.path().join("instance_ids.txt");
    let dataset_file = temp.path().join("dataset.json");
    let workspace_root = temp.path().join("workspaces");
    let (base_commit, github_root, _) = create_bare_repo_fixture(&temp);

    fs::write(&instance_ids_file, "repo__case-1\n").unwrap();
    fs::write(
        &dataset_file,
        format!(
            r#"[
  {{
"instance_id": "repo__case-1",
"repo": "owner/repo",
"base_commit": "{base_commit}",
"environment_setup_commit": ""
  }}
]"#
        ),
    )
    .unwrap();

    let stale_workspace = workspace_root.join("repo__case-1");
    fs::create_dir_all(&stale_workspace).unwrap();
    fs::write(stale_workspace.join("stale.txt"), "partial run").unwrap();

    let result = prepare_swebench_lite_workspaces(&PrepareSwebenchLiteWorkspacesOptions {
        instance_ids_file,
        dataset_files: vec![dataset_file],
        dataset_name: None,
        split: "test".to_string(),
        workspace_root: workspace_root.clone(),
        repo_cache_root: None,
        github_root,
        workspace_map_output: None,
        skip_mirror_fetch: false,
        reuse_existing_workspaces: true,
    })
    .unwrap();

    assert_eq!(result.report.failed_count, 0);
    assert_eq!(result.report.reused_count, 0);
    assert_eq!(result.report.recreated_count, 1);
    let workspace = workspace_root.join("repo__case-1");
    assert!(workspace.is_dir());
    assert_eq!(git(&workspace, ["rev-parse", "HEAD"]), base_commit);
    assert!(!workspace.join("stale.txt").exists());
}
