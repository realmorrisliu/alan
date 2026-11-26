#![cfg(unix)]

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

pub(super) const CHILD_MODE_ENV: &str = "ALAN_PTY_E2E_CHILD";

pub(super) struct PtyState {
    parser: vt100::Parser,
    raw: Vec<u8>,
    eof: bool,
}

#[derive(Clone)]
pub(super) struct PtyScreen {
    shared: Arc<(Mutex<PtyState>, Condvar)>,
}

impl PtyScreen {
    #[allow(dead_code, reason = "used by pty_e2e suites that omit scrollback")]
    pub(super) fn new(rows: u16, cols: u16) -> Self {
        Self::new_with_scrollback(rows, cols, 0)
    }

    #[allow(
        dead_code,
        reason = "scrollback constructor used by multi-stream PTY scrollback tests"
    )]
    pub(super) fn new_with_scrollback(rows: u16, cols: u16, scrollback: usize) -> Self {
        Self {
            shared: Arc::new((
                Mutex::new(PtyState {
                    parser: vt100::Parser::new(
                        NonZeroU16::new(rows).expect("nonzero rows"),
                        NonZeroU16::new(cols).expect("nonzero cols"),
                        scrollback,
                    ),
                    raw: Vec::new(),
                    eof: false,
                }),
                Condvar::new(),
            )),
        }
    }

    fn push_bytes(&self, bytes: &[u8]) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.raw.extend_from_slice(bytes);
        guard.parser.process(bytes);
        self.shared.1.notify_all();
    }

    fn mark_eof(&self) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.eof = true;
        self.shared.1.notify_all();
    }

    pub(super) fn resize_parser(&self, rows: u16, cols: u16) {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        guard.parser.screen_mut().set_size(
            NonZeroU16::new(rows).expect("nonzero rows"),
            NonZeroU16::new(cols).expect("nonzero cols"),
        );
        self.shared.1.notify_all();
    }

    fn cursor_one_based(&self) -> (u16, u16) {
        let guard = self.shared.0.lock().expect("pty state lock");
        let (r, c) = guard.parser.screen().cursor_position();
        (r.saturating_add(1), c.saturating_add(1))
    }

    #[allow(dead_code, reason = "used by pty_e2e suites inspecting active screen")]
    pub(super) fn contents(&self) -> String {
        let guard = self.shared.0.lock().expect("pty state lock");
        guard.parser.screen().contents()
    }

    #[allow(
        dead_code,
        reason = "used by pty_e2e suites checking raw terminal escape sequences"
    )]
    pub(super) fn raw_len(&self) -> usize {
        self.shared.0.lock().expect("pty state lock").raw.len()
    }

    #[allow(
        dead_code,
        reason = "used by pty_e2e suites checking raw terminal escape sequences"
    )]
    pub(super) fn raw_slice_from(&self, offset: usize) -> Vec<u8> {
        let guard = self.shared.0.lock().expect("pty state lock");
        guard.raw.get(offset..).unwrap_or_default().to_vec()
    }

    #[allow(
        dead_code,
        reason = "scrollback+screen helper used by multi-stream PTY scrollback tests"
    )]
    pub(super) fn full_contents(&self) -> String {
        let mut guard = self.shared.0.lock().expect("pty state lock");
        let (rows, cols) = guard.parser.screen().size();
        let rows = rows.get() as usize;
        let cols = cols.get();
        let prev = guard.parser.screen().scrollback();
        guard.parser.screen_mut().set_scrollback(usize::MAX);
        let mut pos = guard.parser.screen().scrollback();
        let mut all_lines = Vec::new();
        while pos > 0 && rows > 0 {
            guard.parser.screen_mut().set_scrollback(pos);
            let actual = guard.parser.screen().scrollback();
            let take = actual.min(rows);
            all_lines.extend(guard.parser.screen().rows(0, cols).take(take));
            pos = actual.saturating_sub(take);
        }
        guard.parser.screen_mut().set_scrollback(0);
        all_lines.extend(guard.parser.screen().rows(0, cols));
        if prev != 0 {
            guard.parser.screen_mut().set_scrollback(prev);
        }
        all_lines.join("\n")
    }

    pub(super) fn wait_for<F>(
        &self,
        label: &str,
        timeout: Duration,
        mut predicate: F,
    ) -> Result<String>
    where
        F: FnMut(&str, bool) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut guard = self.shared.0.lock().expect("pty state lock");
        loop {
            let contents = guard.parser.screen().contents();
            if predicate(&contents, guard.eof) {
                return Ok(contents);
            }
            if guard.eof {
                bail!("PTY reached EOF before `{label}` was satisfied.\nScreen:\n{contents}");
            }
            let now = Instant::now();
            if now >= deadline {
                bail!("timed out waiting for `{label}`.\nScreen:\n{contents}");
            }
            let (next_guard, wait_res) = self
                .shared
                .1
                .wait_timeout(guard, deadline - now)
                .expect("condvar wait");
            guard = next_guard;
            if wait_res.timed_out() {
                let contents = guard.parser.screen().contents();
                if predicate(&contents, guard.eof) {
                    return Ok(contents);
                }
                bail!("timed out waiting for `{label}`.\nScreen:\n{contents}");
            }
        }
    }

    pub(super) fn wait_eof(&self, timeout: Duration) -> Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut guard = self.shared.0.lock().expect("pty state lock");
        while !guard.eof {
            let now = Instant::now();
            if now >= deadline {
                bail!(
                    "timed out waiting for PTY EOF.\nScreen:\n{}",
                    guard.parser.screen().contents()
                );
            }
            let (next_guard, _) = self
                .shared
                .1
                .wait_timeout(guard, deadline - now)
                .expect("condvar wait");
            guard = next_guard;
        }
        Ok(guard.raw.clone())
    }
}

pub(super) struct ChildGuard(Option<Child>);

impl ChildGuard {
    pub(super) fn new(child: Child) -> Self {
        Self(Some(child))
    }

    #[allow(dead_code, reason = "used by pty_e2e suites checking child liveness")]
    pub(super) fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        Ok(self.0.as_mut().expect("child").try_wait()?)
    }

    pub(super) fn wait_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<std::process::ExitStatus> {
        let deadline = Instant::now() + timeout;
        let child = self.0.as_mut().expect("child");
        loop {
            if let Some(status) = child.try_wait()? {
                self.0.take();
                return Ok(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                self.0.take();
                bail!("child process did not exit within {:?}", timeout);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(super) fn open_pty(rows: u16, cols: u16) -> Result<(OwnedFd, OwnedFd)> {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: `master`, `slave`, and `ws` are valid aligned pointers; `name` and `termp` are null.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &ws as *const libc::winsize as *mut libc::winsize,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error()).context("openpty");
    }
    // SAFETY: `openpty` returned 0, so `master` and `slave` are newly created owned file descriptors.
    Ok(unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) })
}

pub(super) fn spawn_in_pty(
    slave: &OwnedFd,
    master_to_close: RawFd,
    test_name: &str,
    mode: &str,
    extra_env: &[(&str, &OsStr)],
) -> Result<Child> {
    let exe = std::env::current_exe().context("locate test executable")?;
    let slave_fd: RawFd = std::os::fd::AsRawFd::as_raw_fd(slave);
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", test_name, "--nocapture"])
        .env(CHILD_MODE_ENV, mode)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    // SAFETY: `pre_exec` invokes only async-signal-safe POSIX syscalls (`setsid`, `ioctl`, `dup2`, `close`).
    unsafe {
        cmd.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            for target in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
                if libc::dup2(slave_fd, target) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            if slave_fd > libc::STDERR_FILENO {
                libc::close(slave_fd);
            }
            if master_to_close > libc::STDERR_FILENO {
                libc::close(master_to_close);
            }
            Ok(())
        });
    }

    cmd.spawn().context("spawn pty child test")
}

pub(super) fn spawn_master_reader(
    master_read: OwnedFd,
    master_write: Arc<Mutex<std::fs::File>>,
    screen: PtyScreen,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = std::fs::File::from(master_read);
        let mut buf = [0u8; 4096];
        let mut tail = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    screen.mark_eof();
                    break;
                }
                Ok(n) => {
                    let chunk = &buf[..n];
                    screen.push_bytes(chunk);
                    tail.extend_from_slice(chunk);
                    if tail.windows(4).any(|w| w == b"\x1b[6n") {
                        let (row, col) = screen.cursor_one_based();
                        let reply = format!("\x1b[{row};{col}R");
                        if let Ok(mut w) = master_write.lock() {
                            let _ = w.write_all(reply.as_bytes());
                            let _ = w.flush();
                        }
                        tail.clear();
                    } else if tail.len() > 8 {
                        let drain = tail.len() - 4;
                        tail.drain(..drain);
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    screen.mark_eof();
                    break;
                }
            }
        }
    })
}

pub(super) fn write_master(master: &Arc<Mutex<std::fs::File>>, data: &[u8]) -> Result<()> {
    let mut guard = master.lock().expect("master write lock");
    guard.write_all(data)?;
    guard.flush()?;
    Ok(())
}

pub(super) fn resize_pty(fd: RawFd, rows: u16, cols: u16) -> Result<()> {
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: `fd` is an open PTY master descriptor and `ws` is a valid `libc::winsize`.
    let rc = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ as _, &ws as *const libc::winsize) };
    if rc == -1 {
        return Err(std::io::Error::last_os_error()).context("TIOCSWINSZ");
    }
    Ok(())
}

pub(super) fn stdin_restored_canon_and_echo() -> Result<bool> {
    // SAFETY: `libc::termios` is a plain C struct initialized immediately by `tcgetattr`.
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    // SAFETY: `STDIN_FILENO` is a valid fd and `termios` points to valid writable storage.
    if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut termios) } != 0 {
        return Err(std::io::Error::last_os_error()).context("tcgetattr stdin");
    }
    let has_canon = (termios.c_lflag & libc::ICANON) != 0;
    let has_echo = (termios.c_lflag & libc::ECHO) != 0;
    Ok(has_canon && has_echo)
}
