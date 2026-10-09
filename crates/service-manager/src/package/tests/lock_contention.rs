use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[test]
fn store_lock_holder_worker() {
    let Some(root) = std::env::var_os("ALAN_OWNED_STORE_LOCK_FIXTURE") else {
        return;
    };
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(PathBuf::from(root).join("store.lock"))
        .unwrap();
    // SAFETY: this owned fixture descriptor remains live until normal stdin release.
    assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) }, 0);
    println!("OWNED_LOCK_READY");
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    std::io::stdin().read_exact(&mut byte).unwrap();
    drop(file);
}

// All observations are collected before assertions. Never kill the holder or
// contender: even the old blocking implementation is released through stdin.
pub(super) fn contend<T: Send + 'static>(
    root: &Path,
    hold: Duration,
    operation: impl FnOnce() -> T + Send + 'static,
) -> (bool, T, Duration, bool) {
    let mut holder = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "package::tests::lock_contention::store_lock_holder_worker",
            "--nocapture",
        ])
        .env("ALAN_OWNED_STORE_LOCK_FIXTURE", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = holder.stdout.take().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (reader_done_tx, reader_done_rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.ok().as_deref() == Some("OWNED_LOCK_READY") {
                let _ = ready_tx.send(());
            }
        }
        let _ = reader_done_tx.send(());
    });
    let ready = ready_rx.recv_timeout(Duration::from_secs(3));
    let start = Instant::now();
    let (tx, rx) = mpsc::channel();
    let (started_tx, started_rx) = mpsc::channel();
    let contender = std::thread::spawn(move || {
        let _ = started_tx.send(());
        let _ = tx.send(operation());
    });
    let started = started_rx.recv_timeout(Duration::from_secs(3));
    let observed = rx.recv_timeout(hold).ok();
    let elapsed = start.elapsed();
    let alive = holder.try_wait().ok().flatten().is_none();
    let probe = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("store.lock"));
    let owning = probe.and_then(|file| {
        // SAFETY: probe owns a live descriptor throughout the nonblocking call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(false);
        }
        let error = std::io::Error::last_os_error();
        if error
            .raw_os_error()
            .is_some_and(|code| code == libc::EWOULDBLOCK || code == libc::EAGAIN)
        {
            Ok(true)
        } else {
            Err(error)
        }
    });
    let release = holder.stdin.take().unwrap().write_all(b"R");
    let wait_start = Instant::now();
    let exit = loop {
        match holder.try_wait() {
            Ok(Some(status)) => break Some(status),
            Err(_) => break None,
            _ if wait_start.elapsed() >= Duration::from_secs(3) => break None,
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let after = if observed.is_none() {
        rx.recv_timeout(Duration::from_secs(3)).ok()
    } else {
        None
    };
    // Join only after bounded evidence of completion; no hidden blocking wait.
    if exit.is_some() && reader_done_rx.recv_timeout(Duration::from_secs(2)).is_ok() {
        reader.join().unwrap();
    } else {
        panic!("owned holder stdout did not complete within cleanup bound");
    }
    if observed.is_some() || after.is_some() {
        contender.join().unwrap();
    }
    assert!(
        ready.is_ok(),
        "owned holder readiness failed after normal release"
    );
    assert!(
        release.is_ok() && exit.is_some_and(|status| status.success()),
        "holder did not exit normally after release"
    );
    assert!(started.is_ok(), "contender did not start");
    let owning = owning.expect("ownership probe failed with unrelated IO/errno");
    println!("owned holder normal exit; observation={elapsed:?}, alive={alive}, owning={owning}");
    match (observed, after) {
        (Some(value), _) => (true, value, elapsed, alive && owning),
        (None, Some(value)) => (false, value, elapsed, alive && owning),
        _ => panic!("contender did not complete after normal release"),
    }
}

#[test]
fn bounded_store_lock_open() {
    check_long_contention(true);
}

#[test]
fn bounded_store_lock_transaction() {
    check_long_contention(false);
}

fn check_long_contention(opening: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("packages");
    let service = PackageService::open(root.clone()).unwrap();
    let contender_root = root.clone();
    let (bounded, result, elapsed, owned) = contend(&root, Duration::from_millis(900), move || {
        if opening {
            PackageService::open(contender_root).map(|_| ())
        } else {
            service.store.transaction().map(|_| ())
        }
    });
    assert!(
        owned,
        "holder must remain alive and own lock at observation"
    );
    assert!(
        bounded && result.is_err(),
        "open={opening}: old blocking lock exceeded bounded wait ({elapsed:?})"
    );
    assert!(result.unwrap_err().to_string().contains("busy"));
    assert!(elapsed >= Duration::from_millis(450) && elapsed < Duration::from_millis(900));
    assert!(PackageService::open(root).is_ok());
}

#[test]
fn short_contention_serializes_open_and_transaction() {
    for opening in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("packages");
        let service = PackageService::open(root.clone()).unwrap();
        let contender_root = root.clone();
        let (early, result, elapsed, owned) =
            contend(&root, Duration::from_millis(100), move || {
                if opening {
                    PackageService::open(contender_root).map(|_| ())
                } else {
                    service.store.transaction().map(|_| ())
                }
            });
        assert!(
            owned && !early,
            "contender must remain waiting while holder owns lock"
        );
        assert!(
            result.is_ok(),
            "short contention must succeed after normal release: {result:?}"
        );
        assert!(elapsed >= Duration::from_millis(100) && elapsed < Duration::from_millis(450));
    }
}
