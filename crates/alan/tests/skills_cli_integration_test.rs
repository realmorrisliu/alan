use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

#[cfg(unix)]
struct PackageBinWrapperCase {
    wrapper_name: &'static str,
    wrapper_contents: &'static str,
    path_binary_name: &'static str,
    expected_args: &'static [&'static str],
}

fn write_skill(root: &std::path::Path, name: &str, body: &str) {
    let skill_dir = root.join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), body).unwrap();
}

#[cfg(unix)]
fn write_executable(path: &std::path::Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(path, contents).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn run_wrapper(wrapper_path: &Path, test_path: &str, marker: &Path) -> Output {
    const TEXT_FILE_BUSY_OS_ERROR: i32 = 26;

    let mut last_text_busy_error = None;
    for attempt in 0..5 {
        match Command::new(wrapper_path)
            .arg("input")
            .env("PATH", test_path)
            .env("WRAPPER_MARKER", marker)
            .output()
        {
            Ok(output) => return output,
            Err(error) if error.raw_os_error() == Some(TEXT_FILE_BUSY_OS_ERROR) && attempt < 4 => {
                last_text_busy_error = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("failed to run {}: {error}", wrapper_path.display()),
        }
    }

    let error = last_text_busy_error.expect("text-busy retry loop must capture an error");
    panic!(
        "failed to run {} after retrying text-busy errors: {error}",
        wrapper_path.display()
    );
}

#[cfg(unix)]
#[test]
fn package_bin_wrappers_fall_back_to_path_outside_source_tree() {
    let cases = [
        PackageBinWrapperCase {
            wrapper_name: "aggregate-benchmark",
            wrapper_contents: include_str!(
                "../../agent-engine/skills/skill-creator/bin/aggregate-benchmark"
            ),
            path_binary_name: "alan",
            expected_args: &["skills", "aggregate-benchmark", "input"],
        },
        PackageBinWrapperCase {
            wrapper_name: "generate-review",
            wrapper_contents: include_str!(
                "../../agent-engine/skills/skill-creator/bin/generate-review"
            ),
            path_binary_name: "alan",
            expected_args: &["skills", "generate-review", "input"],
        },
        PackageBinWrapperCase {
            wrapper_name: "swebench-lite-prepare-workspaces",
            wrapper_contents: include_str!(
                "../../agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces"
            ),
            path_binary_name: "alan",
            expected_args: &["skills", "swebench-lite-prepare-workspaces", "input"],
        },
        PackageBinWrapperCase {
            wrapper_name: "swebench-lite-materialize-subset",
            wrapper_contents: include_str!(
                "../../agent-engine/skills/swebench/bin/swebench-lite-materialize-subset"
            ),
            path_binary_name: "alan",
            expected_args: &["skills", "swebench-lite-materialize-subset", "input"],
        },
    ];

    for case in cases {
        let temp = TempDir::new().unwrap();
        let package_bin = temp.path().join("materialized/package/bin");
        let fake_bin = temp.path().join("fake-bin");
        let marker = temp.path().join("invocation.txt");
        std::fs::create_dir_all(&package_bin).unwrap();
        std::fs::create_dir_all(&fake_bin).unwrap();

        let wrapper_path = package_bin.join(case.wrapper_name);
        write_executable(&wrapper_path, case.wrapper_contents);
        write_executable(
            &fake_bin.join(case.path_binary_name),
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$0\" \"$@\" > \"$WRAPPER_MARKER\"\n",
        );

        let original_path = std::env::var_os("PATH").unwrap_or_default();
        let test_path = format!("{}:{}", fake_bin.display(), original_path.to_string_lossy());
        let output = run_wrapper(&wrapper_path, &test_path, &marker);

        assert!(
            output.status.success(),
            "{case_name}: {output:?}",
            case_name = case.wrapper_name
        );
        let invocation = std::fs::read_to_string(&marker).unwrap();
        let mut lines = invocation.lines();
        assert!(
            lines
                .next()
                .is_some_and(|line| line.ends_with(case.path_binary_name)),
            "{case_name}: {invocation}",
            case_name = case.wrapper_name
        );
        let args: Vec<_> = lines.collect();
        assert_eq!(args, case.expected_args, "{}", case.wrapper_name);
    }
}

#[cfg(unix)]
#[test]
fn swebench_bin_wrappers_skip_themselves_when_searching_path_helpers() {
    let cases = [
        PackageBinWrapperCase {
            wrapper_name: "swebench-lite-prepare-workspaces",
            wrapper_contents: include_str!(
                "../../agent-engine/skills/swebench/bin/swebench-lite-prepare-workspaces"
            ),
            path_binary_name: "swebench-lite-prepare-workspaces",
            expected_args: &["input"],
        },
        PackageBinWrapperCase {
            wrapper_name: "swebench-lite-materialize-subset",
            wrapper_contents: include_str!(
                "../../agent-engine/skills/swebench/bin/swebench-lite-materialize-subset"
            ),
            path_binary_name: "swebench-lite-materialize-subset",
            expected_args: &["input"],
        },
    ];

    for case in cases {
        let temp = TempDir::new().unwrap();
        let package_bin = temp.path().join("materialized/package/bin");
        let helper_bin = temp.path().join("helper-bin");
        let marker = temp.path().join("invocation.txt");
        std::fs::create_dir_all(&package_bin).unwrap();
        std::fs::create_dir_all(&helper_bin).unwrap();

        let wrapper_path = package_bin.join(case.wrapper_name);
        write_executable(&wrapper_path, case.wrapper_contents);
        write_executable(
            &helper_bin.join(case.path_binary_name),
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$0\" \"$@\" > \"$WRAPPER_MARKER\"\n",
        );

        let test_path = format!(
            "{}:{}:/bin:/usr/bin",
            package_bin.display(),
            helper_bin.display()
        );
        let output = run_wrapper(&wrapper_path, &test_path, &marker);

        assert!(
            output.status.success(),
            "{case_name}: {output:?}",
            case_name = case.wrapper_name
        );
        let invocation = std::fs::read_to_string(&marker).unwrap();
        let mut lines = invocation.lines();
        assert!(
            lines
                .next()
                .is_some_and(|line| line.ends_with(case.path_binary_name)),
            "{case_name}: {invocation}",
            case_name = case.wrapper_name
        );
        let args: Vec<_> = lines.collect();
        assert_eq!(args, case.expected_args, "{}", case.wrapper_name);
    }
}

#[test]
fn skills_init_inline_scaffolds_and_validates_package() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("doc-review");

    let init = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args([
            "skills",
            "init",
            package_root.to_str().unwrap(),
            "--template",
            "inline",
            "--name",
            "Doc Review",
            "--description",
            "Review documentation when asked.",
            "--short-description",
            "Review documentation",
        ])
        .output()
        .unwrap();

    assert!(init.status.success(), "{init:?}");
    assert!(package_root.join("SKILL.md").is_file());
    assert!(package_root.join("agents/openai.yaml").is_file());

    let validate = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["skills", "validate", package_root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(validate.status.success(), "{validate:?}");
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("status: valid"));
    assert!(stdout.contains("execution: inline(no_child_agent_exports)"));
}

#[test]
fn skills_init_normalizes_runtime_skill_id_from_package_directory() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("repo.review");

    let init = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args([
            "skills",
            "init",
            package_root.to_str().unwrap(),
            "--template",
            "inline",
            "--name",
            "Repo Review",
            "--description",
            "Review repositories when asked.",
            "--short-description",
            "Review repositories",
        ])
        .output()
        .unwrap();

    assert!(init.status.success(), "{init:?}");
    let stdout = String::from_utf8_lossy(&init.stdout);
    assert!(stdout.contains("skill: repo-review"));

    let validate = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["skills", "validate", package_root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(validate.status.success(), "{validate:?}");
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("skill: repo-review"));
    assert!(stdout.contains("status: valid"));
}

#[test]
fn skills_init_delegate_scaffolds_a_delegated_package() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("repo-review");

    let init = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args([
            "skills",
            "init",
            package_root.to_str().unwrap(),
            "--template",
            "delegate",
            "--name",
            "Repo Review",
            "--description",
            "Review repositories when asked.",
            "--short-description",
            "Review repositories",
        ])
        .output()
        .unwrap();

    assert!(init.status.success(), "{init:?}");
    assert!(package_root.join("agents/openai.yaml").is_file());
    assert!(
        package_root
            .join("agents/repo-review/persona/ROLE.md")
            .is_file()
    );

    let validate = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["skills", "validate", package_root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(validate.status.success(), "{validate:?}");
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains(
        "execution: delegate(target=repo-review, source=same_name_skill_and_child_agent)"
    ));
}

#[test]
fn skills_validate_uses_normalized_same_name_delegate_matching() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("repo_review");
    write_skill(
        temp.path(),
        "repo_review",
        r#"---
name: Repo Review
description: Review repositories
---

Body
"#,
    );
    std::fs::create_dir_all(package_root.join("agents/repo_review/persona")).unwrap();
    std::fs::create_dir_all(package_root.join("agents/grader/persona")).unwrap();

    let validate = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["skills", "validate", package_root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(validate.status.success(), "{validate:?}");
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("status: valid"));
    assert!(stdout.contains("skill: repo-review"));
    assert!(stdout.contains(
        "execution: delegate(target=repo_review, source=same_name_skill_and_child_agent)"
    ));
}

#[test]
fn skills_validate_fails_for_ambiguous_delegate_shape() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("skill-creator");
    write_skill(
        temp.path(),
        "skill-creator",
        r#"---
name: Skill Creator
description: Create skills
---

Body
"#,
    );
    std::fs::create_dir_all(package_root.join("agents/creator/persona")).unwrap();
    std::fs::create_dir_all(package_root.join("agents/grader/persona")).unwrap();

    let validate = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["skills", "validate", package_root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!validate.status.success(), "{validate:?}");
    let stdout = String::from_utf8_lossy(&validate.stdout);
    assert!(stdout.contains("status: invalid"));
    assert!(stdout.contains("execution: unresolved(ambiguous_package_shape)"));
}

#[test]
fn skills_eval_runs_package_local_hook() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("lint-check");
    write_skill(
        temp.path(),
        "lint-check",
        r#"---
name: Lint Check
description: Run lint checks
---

Body
"#,
    );
    std::fs::create_dir_all(package_root.join("scripts")).unwrap();
    std::fs::write(
        package_root.join("scripts/eval.sh"),
        "#!/bin/sh\necho \"eval hook ok\"\n",
    )
    .unwrap();

    let eval = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["skills", "eval", package_root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(eval.status.success(), "{eval:?}");
    let stdout = String::from_utf8_lossy(&eval.stdout);
    assert!(stdout.contains("status: passed"));
    assert!(stdout.contains("eval hook ok"));
}

#[test]
fn skills_eval_runs_structured_manifest_and_writes_artifacts() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("skill-creator-eval");
    let output_dir = temp.path().join("eval-output");
    write_skill(
        temp.path(),
        "skill-creator-eval",
        r#"---
name: Skill Creator Eval
description: Evaluate skill packages
---

Body
"#,
    );
    std::fs::create_dir_all(package_root.join("scripts")).unwrap();
    std::fs::create_dir_all(package_root.join("evals")).unwrap();
    std::fs::create_dir_all(package_root.join("eval-viewer")).unwrap();
    std::fs::create_dir_all(package_root.join("agents")).unwrap();
    std::fs::write(
        package_root.join("scripts/candidate.sh"),
        "#!/bin/sh\nprintf '{\"passed\":true,\"variant\":\"candidate\"}'\n",
    )
    .unwrap();
    std::fs::write(
        package_root.join("scripts/baseline.sh"),
        "#!/bin/sh\nprintf '{\"passed\":false,\"variant\":\"baseline\"}'\n",
    )
    .unwrap();
    std::fs::write(
        package_root.join("scripts/grader.sh"),
        "#!/bin/sh\nprintf '{\"passed\":true,\"score\":1}'\n",
    )
    .unwrap();
    std::fs::write(
        package_root.join("scripts/analyzer.sh"),
        "#!/bin/sh\nprintf '{\"passed\":true,\"notes\":[\"candidate kept the skill-specific workflow\"]}'\n",
    )
    .unwrap();
    std::fs::write(
        package_root.join("scripts/comparator.sh"),
        "#!/bin/sh\nprintf '{\"passed\":true,\"delta\":\"candidate is more explicit than baseline\"}'\n",
    )
    .unwrap();
    std::fs::write(package_root.join("agents/grader.md"), "# grader\n").unwrap();
    std::fs::write(package_root.join("agents/analyzer.md"), "# analyzer\n").unwrap();
    std::fs::write(package_root.join("agents/comparator.md"), "# comparator\n").unwrap();
    std::fs::write(
        package_root.join("eval-viewer/viewer.html"),
        "<!doctype html><title>viewer</title>",
    )
    .unwrap();
    std::fs::write(
        package_root.join("evals/evals.json"),
        r#"{
  "version": 1,
  "suite": "skill-creator-eval",
  "review": {"viewer": "eval-viewer"},
  "cases": [
    {
      "id": "trigger-create",
      "type": "trigger",
      "input": "please use $skill-creator-eval to create skill package",
      "expected": true
    },
    {
      "id": "compare-guidance",
      "type": "command",
      "prompt": "Compare candidate and baseline outputs",
      "command": ["sh", "scripts/candidate.sh"],
      "comparison": {
        "mode": "with_without_skill",
        "baseline_command": ["sh", "scripts/baseline.sh"]
      },
      "grading": {
        "command": ["sh", "scripts/grader.sh"],
        "prompt_file": "agents/grader.md"
      },
      "analyzer": {
        "command": ["sh", "scripts/analyzer.sh"],
        "prompt_file": "agents/analyzer.md"
      },
      "comparator": {
        "command": ["sh", "scripts/comparator.sh"],
        "prompt_file": "agents/comparator.md"
      }
    }
  ]
}"#,
    )
    .unwrap();

    let eval = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args([
            "skills",
            "eval",
            package_root.to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(eval.status.success(), "{eval:?}");
    let stdout = String::from_utf8_lossy(&eval.stdout);
    assert!(stdout.contains("status: passed"));
    assert!(stdout.contains("manifest:"));
    assert!(stdout.contains("cases: 2 total, 2 passed, 0 failed"));
    assert!(stdout.contains("compare-guidance [command] [passed]"));
    assert!(output_dir.join("run.json").is_file());
    assert!(output_dir.join("benchmark.json").is_file());
    assert!(output_dir.join("review/index.html").is_file());
    assert!(output_dir.join("review/viewer/viewer.html").is_file());
    assert!(output_dir.join("cases/trigger-create/case.json").is_file());
    assert!(
        output_dir
            .join("cases/compare-guidance/with_skill.json")
            .is_file()
    );
    assert!(
        output_dir
            .join("cases/compare-guidance/without_skill.json")
            .is_file()
    );
    assert!(
        output_dir
            .join("cases/compare-guidance/grading.json")
            .is_file()
    );
    assert!(
        output_dir
            .join("cases/compare-guidance/analyzer.json")
            .is_file()
    );
    assert!(
        output_dir
            .join("cases/compare-guidance/comparator.json")
            .is_file()
    );
}

#[test]
fn skills_aggregate_benchmark_and_generate_review_rebuild_artifacts() {
    let temp = TempDir::new().unwrap();
    let package_root = temp.path().join("skill-eval");
    let output_dir = temp.path().join("eval-output");

    write_skill(
        temp.path(),
        "skill-eval",
        r#"---
name: Skill Eval
description: Evaluate skills when asked
---

Body
"#,
    );
    std::fs::create_dir_all(package_root.join("evals")).unwrap();
    std::fs::write(
        package_root.join("evals/evals.json"),
        r#"{
  "version": 1,
  "cases": [
    {"id": "trigger", "type": "trigger", "input": "please use $skill-eval", "expected": true}
  ]
}"#,
    )
    .unwrap();

    let eval = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args([
            "skills",
            "eval",
            package_root.to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(eval.status.success(), "{eval:?}");

    std::fs::remove_file(output_dir.join("benchmark.json")).unwrap();
    std::fs::remove_file(output_dir.join("review/index.html")).unwrap();

    let aggregate = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args([
            "skills",
            "aggregate-benchmark",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(aggregate.status.success(), "{aggregate:?}");
    assert!(output_dir.join("benchmark.json").is_file());

    let review = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["skills", "generate-review", output_dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(review.status.success(), "{review:?}");
    assert!(output_dir.join("review/index.html").is_file());
}
