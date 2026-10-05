use super::*;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Default)]
struct OwnedWorkers(Vec<Child>);

impl OwnedWorkers {
    fn complete(&mut self, timeout: Duration) -> std::io::Result<()> {
        let deadline = Instant::now() + timeout;
        let mut first_error = None;
        loop {
            let mut running = false;
            for child in &mut self.0 {
                match child.try_wait() {
                    Ok(Some(status)) if !status.success() => {
                        let mut stderr = String::new();
                        if let Some(stderr_pipe) = child.stderr.take() {
                            stderr_pipe.take(4096).read_to_string(&mut stderr)?;
                        }
                        first_error.get_or_insert_with(|| {
                            std::io::Error::other(format!(
                                "history worker {} failed: {status}; stderr: {stderr}",
                                child.id()
                            ))
                        });
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => running = true,
                    Err(error) => {
                        return Err(error);
                    }
                }
            }
            if !running {
                return first_error.map_or(Ok(()), Err);
            }
            if Instant::now() >= deadline {
                return Err(first_error.unwrap_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "history workers did not complete before fixture deadline",
                    )
                }));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for OwnedWorkers {
    fn drop(&mut self) {
        // Runs on spawn/start/worker/parse/assert failures too. Kill every
        // still-owned process before bounded reaping; never replace the error
        // that caused unwinding with a cleanup panic.
        for child in &mut self.0 {
            if !matches!(child.try_wait(), Ok(Some(_)))
                && let Err(error) = child.kill()
            {
                eprintln!("history worker {} cleanup kill: {error}", child.id());
            }
        }
        if let Err(error) = self.complete(Duration::from_secs(5)) {
            eprintln!("history worker cleanup: {error}");
        }
    }
}

fn entry(worker: usize, record: usize) -> HistoryEntry {
    HistoryEntry {
        body: format!(
            "worker={worker};record={record}; λ\n\"\\{}",
            "body".repeat(2048)
        ),
        intent: if record.is_multiple_of(2) {
            InputIntent::Command
        } else {
            InputIntent::Agent
        },
    }
}

#[test]
fn history_os_worker() {
    let Ok(path) = std::env::var("ALAN_HISTORY_TEST_PATH") else {
        return;
    };
    let worker: usize = std::env::var("ALAN_HISTORY_TEST_WORKER")
        .unwrap()
        .parse()
        .unwrap();
    let mut start = String::new();
    std::io::stdin().read_line(&mut start).unwrap();
    let mut persisted = Vec::new();
    for record in 0..400 {
        let result = append_history_line(std::path::Path::new(&path), &entry(worker, record));
        persisted.push(match result {
            Ok(repaired) => {
                assert!(
                    !repaired,
                    "a complete stress record must never require repair"
                );
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => false,
            Err(error) => panic!(
                "worker={worker}; record={record}; kind={:?}: {error}",
                error.kind()
            ),
        });
    }
    std::fs::write(
        std::path::Path::new(&path).with_extension(format!("worker-{worker}")),
        serde_json::to_vec(&persisted).unwrap(),
    )
    .unwrap();
}

#[test]
fn history_worker_deadline_and_cleanup_reap_blocked_child() {
    let dir = tempfile::tempdir().unwrap();
    let mut workers = OwnedWorkers::default();
    workers.0.push(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "composer::process_tests::history_os_worker",
                "--nocapture",
            ])
            .env("ALAN_HISTORY_TEST_PATH", dir.path().join("history"))
            .env("ALAN_HISTORY_TEST_WORKER", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let error = workers.complete(Duration::from_millis(20)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    workers.0[0].kill().unwrap();
    let cleanup_error = workers.complete(Duration::from_secs(5)).unwrap_err();
    assert_ne!(cleanup_error.kind(), std::io::ErrorKind::TimedOut);
    assert!(workers.0[0].try_wait().unwrap().is_some());
}

#[test]
fn history_cross_process_complete_records_and_loader_equivalence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut children = OwnedWorkers::default();
    for worker in 0..8 {
        children.0.push(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "composer::process_tests::history_os_worker",
                    "--nocapture",
                ])
                .env("ALAN_HISTORY_TEST_PATH", &path)
                .env("ALAN_HISTORY_TEST_WORKER", worker.to_string())
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in &mut children.0 {
        child.stdin.take().unwrap().write_all(b"start\n").unwrap();
    }
    // Join all owned workers before inspecting/asserting record contents.
    children.complete(Duration::from_secs(60)).unwrap();
    let raw = std::fs::read_to_string(history_records_path(&path, 2)).unwrap();
    let parsed: Vec<HistoryEntry> = raw
        .lines()
        .map(|line| serde_json::from_str(line).expect("every raw record must parse"))
        .collect();
    let loaded = load_history(&path, 3200);
    assert_eq!(loaded, parsed);
    let mut actual: Vec<_> = parsed
        .iter()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect();
    let mut expected = Vec::new();
    let mut successful_writers = 0;
    let mut timed_out = 0;
    for worker in 0..8 {
        let attempts: Vec<bool> = serde_json::from_slice(
            &std::fs::read(path.with_extension(format!("worker-{worker}"))).unwrap(),
        )
        .unwrap();
        assert_eq!(attempts.len(), 400, "every one-shot attempt has an outcome");
        successful_writers += usize::from(attempts.iter().any(|persisted| *persisted));
        for (record, persisted) in attempts.into_iter().enumerate() {
            if persisted {
                expected.push(serde_json::to_string(&entry(worker, record)).unwrap());
            } else {
                timed_out += 1;
            }
        }
    }
    assert!(
        successful_writers >= 2,
        "must exercise cross-process successful writes"
    );
    assert_eq!(expected.len() + timed_out, 3200, "exact attempt partition");
    eprintln!(
        "history attempts=3200 persisted={} timed_out={timed_out} successful_writers={successful_writers}",
        expected.len()
    );
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "exact successful body/intent multiset, without loss, extras or duplicates"
    );
}

#[test]
fn held_history_lock_keeps_session_recall_and_reports_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history");
    let mut composer = Composer::from_history_path(path.clone());
    assert!(composer.history_notice().is_none());
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(history_records_path(&path, 2))
        .unwrap();
    let lock = HistoryLock::acquire(&file, false).unwrap();
    let submitted = "exact ! body\n界\ttext";
    composer.remember_input(submitted, InputIntent::Command);
    assert!(composer.history_notice().is_some());
    assert_eq!(
        file.metadata().unwrap().len(),
        0,
        "timed-out append writes no record"
    );
    composer.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(composer.text(), submitted);
    assert_eq!(composer.recalled_intent(), Some(InputIntent::Command));
    drop(lock);
    assert!(load_history(&path, 1000).is_empty());
}
