use std::process::Command;

#[test]
fn help_lists_ephemeral_and_cwd_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_alan"))
        .arg("--help")
        .output()
        .expect("run alan --help");
    assert!(output.status.success(), "alan --help failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--ephemeral"),
        "missing --ephemeral:\n{stdout}"
    );
    assert!(stdout.contains("--cwd"), "missing --cwd:\n{stdout}");
}

#[test]
fn ephemeral_rejects_resume_and_subcommands() {
    let conflict = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--ephemeral", "--resume"])
        .output()
        .expect("run alan --ephemeral --resume");
    assert!(
        !conflict.status.success(),
        "--ephemeral --resume should fail"
    );
    let stderr = String::from_utf8_lossy(&conflict.stderr);
    assert!(
        stderr.contains("--ephemeral") && stderr.contains("--resume"),
        "unexpected stderr: {stderr}"
    );

    let subcmd = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--ephemeral", "connection", "list"])
        .output()
        .expect("run alan --ephemeral connection list");
    assert!(
        !subcmd.status.success(),
        "--ephemeral with subcommand should fail"
    );
    let sub_stderr = String::from_utf8_lossy(&subcmd.stderr);
    assert!(
        sub_stderr.contains("--ephemeral"),
        "unexpected stderr: {sub_stderr}"
    );
}

#[test]
fn cwd_rejects_missing_non_directory_and_subcommands() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("does-not-exist");
    let missing_out = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--cwd", missing.to_str().unwrap()])
        .output()
        .expect("run alan --cwd <missing>");
    assert!(!missing_out.status.success(), "--cwd missing should fail");

    let regular_file = tmp.path().join("file.txt");
    std::fs::write(&regular_file, b"hello").expect("write file");
    let file_out = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--cwd", regular_file.to_str().unwrap()])
        .output()
        .expect("run alan --cwd <file>");
    assert!(!file_out.status.success(), "--cwd file should fail");
    let file_stderr = String::from_utf8_lossy(&file_out.stderr);
    assert!(
        file_stderr.contains("not a directory"),
        "unexpected stderr: {file_stderr}"
    );

    let subcmd = Command::new(env!("CARGO_BIN_EXE_alan"))
        .args(["--cwd", tmp.path().to_str().unwrap(), "connection", "list"])
        .output()
        .expect("run alan --cwd <dir> connection list");
    assert!(
        !subcmd.status.success(),
        "--cwd with subcommand should fail"
    );
    let sub_stderr = String::from_utf8_lossy(&subcmd.stderr);
    assert!(
        sub_stderr.contains("--cwd"),
        "unexpected stderr: {sub_stderr}"
    );
}

#[test]
fn valid_ephemeral_and_cwd_pass_cli_validation_before_empty_stdin_check() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let xdg_data = tmp.path().join("xdg");
    let cwd = tmp.path().join("project");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(&xdg_data).expect("xdg");
    std::fs::create_dir_all(&cwd).expect("cwd");

    let out = Command::new(env!("CARGO_BIN_EXE_alan"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data)
        .env("TMPDIR", tmp.path())
        .env_remove("ALAN_INSTALL_CHANNEL")
        .args(["--ephemeral", "--cwd", cwd.to_str().unwrap()])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run alan --ephemeral --cwd");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("--ephemeral") && !stderr.contains("--cwd"),
        "valid --ephemeral --cwd should pass flag validation, got: {stderr}"
    );
}
