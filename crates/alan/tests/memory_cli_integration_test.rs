use std::fs;
use std::path::Path;
use std::process::Command;

fn run_alan(home: &Path, xdg_data: &Path, tmp: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", home)
        .env("XDG_DATA_HOME", xdg_data)
        .env("TMPDIR", tmp)
        .env_remove("ALAN_INSTALL_CHANNEL")
        .env_remove("ALAN_CONFIG_PATH")
        .args(args)
        .output()
        .expect("execute alan CLI")
}

#[test]
fn memory_cli_help_describes_subcommands_and_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["memory", "--help"])
        .output()
        .expect("run alan memory --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for sub in ["list", "show", "edit", "revert"] {
        assert!(stdout.contains(sub), "missing `{sub}` in help:\n{stdout}");
    }

    let edit_help = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["memory", "edit", "--help"])
        .output()
        .expect("run alan memory edit --help");
    assert!(edit_help.status.success());
    let edit_stdout = String::from_utf8_lossy(&edit_help.stdout);
    assert!(edit_stdout.contains("--content"));
    assert!(edit_stdout.contains("--file"));
    assert!(edit_stdout.contains("--domain"));
}

#[test]
fn memory_cli_end_to_end_round_trip_bm25_cjk_scrubbing_and_domain_isolation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let xdg_data = tmp.path().join("xdg");
    fs::create_dir_all(home.join("Library/Application Support")).expect("mac app support");
    fs::create_dir_all(&xdg_data).expect("xdg data");

    // Seed content via --file containing a secret token.
    let seed_file = tmp.path().join("seed.md");
    fs::write(
        &seed_file,
        "# Preferences\n用户偏好：默认使用中文与 Rust。\nKey: sk-ant-secret1234567890\n",
    )
    .expect("write seed file");

    let edit1 = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &[
            "memory",
            "edit",
            "topics/prefs.md",
            "--domain",
            "personal",
            "--file",
            seed_file.to_str().unwrap(),
        ],
    );
    assert!(
        edit1.status.success(),
        "edit1 failed: {}",
        String::from_utf8_lossy(&edit1.stderr)
    );

    // Second edit snapshots v1 into .revisions/ before writing v2.
    let edit2 = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &[
            "memory",
            "edit",
            "topics/prefs.md",
            "--domain",
            "personal",
            "--content",
            "# Preferences\nTemporary override\n",
        ],
    );
    assert!(edit2.status.success());
    let edit2_stdout = String::from_utf8_lossy(&edit2.stdout);
    assert!(edit2_stdout.contains("rev-0001"), "{edit2_stdout}");

    // Show verifies revision metadata and sanitized content.
    let show_v2 = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &["memory", "show", "topics/prefs.md", "--domain", "personal"],
    );
    assert!(show_v2.status.success());
    let show_v2_stdout = String::from_utf8_lossy(&show_v2.stdout);
    assert!(show_v2_stdout.contains("Temporary override"));
    assert!(show_v2_stdout.contains("rev-0001"));

    // Revert restores v1 cleanly while never leaking the scrubbed secret.
    let revert = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &[
            "memory",
            "revert",
            "topics/prefs.md",
            "--domain",
            "personal",
        ],
    );
    assert!(revert.status.success());

    let show_v1 = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &["memory", "show", "topics/prefs.md", "--domain", "personal"],
    );
    assert!(show_v1.status.success());
    let show_v1_stdout = String::from_utf8_lossy(&show_v1.stdout);
    assert!(show_v1_stdout.contains("默认使用中文与 Rust"));
    assert!(!show_v1_stdout.contains("sk-ant-secret1234567890"));
    assert!(show_v1_stdout.contains("[REDACTED"));

    // Write isolated project memory and verify cross-domain isolation + mixed CJK/English BM25 recall.
    let edit_proj = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &[
            "memory",
            "edit",
            "cargo.md",
            "--domain",
            "project:alan",
            "--content",
            "处理 Rust 项目里的 Cargo 报错时检查 feature 标志。\n",
        ],
    );
    assert!(edit_proj.status.success());

    let list_personal = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &[
            "memory",
            "list",
            "--domain",
            "personal",
            "--query",
            "Rust 项目里的 Cargo 报错",
            "--json",
        ],
    );
    assert!(list_personal.status.success());
    let personal_json = String::from_utf8_lossy(&list_personal.stdout);
    assert!(!personal_json.contains("cargo.md"), "{personal_json}");

    let list_proj = run_alan(
        &home,
        &xdg_data,
        tmp.path(),
        &[
            "memory",
            "list",
            "--domain",
            "project:alan",
            "--query",
            "Rust 项目里的 Cargo 报错",
            "--json",
        ],
    );
    assert!(list_proj.status.success());
    let proj_json = String::from_utf8_lossy(&list_proj.stdout);
    assert!(proj_json.contains("cargo.md"), "{proj_json}");
}
