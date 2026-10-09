//! Old binaries do not honor the installation guard; retain their native locks too.

use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct SourceLocks {
    _locks: Vec<File>,
}

impl SourceLocks {
    pub(super) fn acquire(system: &Path, host: &Path) -> Result<Self> {
        let mut locks = Vec::new();
        for path in [
            system.join("services/connections/connections.toml.lock"),
            system.join("services/packages/store.lock"),
            host.join("credentials/secrets.toml.lock"),
            host.join("auth.refresh.lock"),
            host.join("auth.json.lock"),
        ] {
            match fs::symlink_metadata(&path) {
                Ok(metadata) => ensure!(
                    metadata.is_file() && !metadata.file_type().is_symlink(),
                    "source lock is not a regular file"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            }
            let file = File::open(path)?;
            file.try_lock().context(
                "a legacy store writer is active; stop all Alan processes before adoption",
            )?;
            locks.push(file);
        }
        Ok(Self { _locks: locks })
    }

    pub(super) fn check_processes(&self, roots: &[PathBuf]) -> Result<()> {
        check_processes(roots)
    }
}

fn is_alan_executable(executable: &str) -> bool {
    // Linux comm truncates alan-os-host-dev to 15 bytes.
    matches!(
        Path::new(executable)
            .file_name()
            .and_then(|name| name.to_str()),
        Some(
            "alan"
                | "alan-dev"
                | "alan-os-host"
                | "alan-os-host-dev"
                | "alan-os-host-de"
                | "Alan"
                | "Alan Dev"
        )
    )
}

fn check_processes(roots: &[PathBuf]) -> Result<()> {
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .context("inspect current user")?;
    ensure!(
        uid.status.success() && uid.stderr.is_empty(),
        "cannot establish migration user identity"
    );
    let uid = String::from_utf8(uid.stdout)?;
    let processes = Command::new("ps")
        .args(["-U", uid.trim(), "-o", "pid=,comm="])
        .output()
        .context("inspect legacy Alan processes")?;
    ensure!(
        processes.status.success() && processes.stderr.is_empty(),
        "cannot establish legacy process state"
    );
    for line in String::from_utf8(processes.stdout)?.lines() {
        let Some((pid, executable)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        if pid.parse::<u32>()? == std::process::id() {
            continue;
        }
        ensure!(
            !is_alan_executable(executable.trim()),
            "another Alan process is running; stop all Alan invocations before adoption"
        );
    }
    let files = Command::new("lsof")
        .args([
            "-nP",
            "-a",
            "-u",
            uid.trim(),
            "-p",
            &format!("^{}", std::process::id()),
            "-F",
            "n",
        ])
        .output()
        .context("inspect source open-file consumers (lsof is required)")?;
    ensure!(
        files.status.success() && files.stderr.is_empty(),
        "cannot establish source open-file state"
    );
    for line in String::from_utf8(files.stdout)?.lines() {
        if let Some(name) = line.strip_prefix("n/") {
            let path = PathBuf::from(format!("/{name}"));
            let path = fs::canonicalize(&path).unwrap_or(path);
            ensure!(
                !roots.iter().any(|root| path.starts_with(root)),
                "source has an active open-file consumer; stop it before adoption"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_cli_and_retired_desktop_process_names_are_quiescence_boundaries() {
        for name in [
            "alan",
            "alan-dev",
            "alan-os-host",
            "alan-os-host-dev",
            "alan-os-host-de",
            "/Applications/Alan.app/Contents/MacOS/Alan",
            "/Applications/Alan Dev.app/Contents/MacOS/Alan Dev",
        ] {
            assert!(is_alan_executable(name), "{name}");
        }
        assert!(!is_alan_executable("installation_migration_process_test"));
        assert!(!is_alan_executable("python3"));
    }

    #[test]
    fn retains_existing_native_lock_and_never_creates_missing_lock_files() {
        let temp = tempfile::tempdir().unwrap();
        let system = temp.path().join("system");
        let host = temp.path().join("host");
        let locks = SourceLocks::acquire(&system, &host).unwrap();
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        drop(locks);
        fs::create_dir_all(host.join("credentials")).unwrap();
        let path = host.join("credentials/secrets.toml.lock");
        fs::write(&path, "unchanged").unwrap();
        let locks = SourceLocks::acquire(&system, &host).unwrap();
        assert!(File::open(&path).unwrap().try_lock().is_err());
        assert!(SourceLocks::acquire(&system, &host).is_err());
        drop(locks);
        let probe = File::open(&path).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        // Parallel subprocess tests may briefly inherit the descriptor before exec.
        loop {
            match probe.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => panic!("source lock was not released: {error}"),
            }
        }
        assert_eq!(fs::read_to_string(path).unwrap(), "unchanged");
    }
}
