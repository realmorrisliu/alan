use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use alan_os_host::installation::InstallationPaths;

fn command(home: &Path, data: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_alan"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ALAN_") {
            command.env_remove(key);
        }
    }
    command.env("HOME", home).env("XDG_DATA_HOME", data);
    command.args([
        "legacy-state",
        "migrate-installation",
        "--from",
        "dev",
        "--json",
    ]);
    command
}

struct Consumer(Child);
impl Drop for Consumer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "requires no other Alan invocation and working ps/lsof/python3"]
fn real_process_probe_and_source_independent_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    let data: PathBuf = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else {
        temp.path().join("data")
    };
    let paths = InstallationPaths::from_data_dir(&data).unwrap();
    let source = paths
        .system_root()
        .join("dev/services/memory/stores/personal");
    fs::create_dir_all(&source).unwrap();
    let note = source.join("note.md");
    fs::write(&note, "source memory").unwrap();
    let host = paths.host_root().join("dev");
    fs::create_dir_all(host.join("credentials")).unwrap();
    fs::write(host.join("credentials/secrets.toml"), "revoked = ['key']\n").unwrap();
    alan_auth::AuthStorage::new(host.join("auth.json"))
        .unwrap()
        .clear_chatgpt()
        .unwrap();

    let dry = command(&home, &data).arg("--dry-run").output().unwrap();
    assert!(dry.status.success(), "{dry:?}");
    assert!(!paths.journal().exists());
    assert!(!paths.product.join("installation.lock").exists());

    let child = Command::new("python3")
        .args([
            "-c",
            "import sys; f=open(sys.argv[1]); print('ready', flush=True); sys.stdin.read()",
        ])
        .arg(&note)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut consumer = Consumer(child);
    let mut line = String::new();
    BufReader::new(consumer.0.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "ready");
    let blocked = command(&home, &data).output().unwrap();
    assert!(!blocked.status.success());
    assert!(
        String::from_utf8_lossy(&blocked.stderr).contains("active open-file consumer"),
        "{blocked:?}"
    );
    assert!(!paths.journal().exists());
    drop(consumer);

    let applied = command(&home, &data).output().unwrap();
    assert!(applied.status.success(), "{applied:?}");
    assert_eq!(
        fs::read(paths.host_root().join("auth.json")).unwrap(),
        fs::read(host.join("auth.json")).unwrap()
    );
    let retried = command(&home, &data).output().unwrap();
    assert!(retried.status.success(), "{retried:?}");
    assert!(String::from_utf8_lossy(&retried.stdout).contains("already-committed"));
    fs::write(&note, "new legacy work").unwrap();
    let rolled_back = command(&home, &data).arg("--rollback").output().unwrap();
    assert!(rolled_back.status.success(), "{rolled_back:?}");
    assert!(!paths.system_root().join("services").exists());
    assert!(!paths.host_root().join("auth.json").exists());
    assert_eq!(fs::read_to_string(note).unwrap(), "new legacy work");
    assert!(host.join("auth.json").exists());
    assert!(!paths.journal().exists());
}
