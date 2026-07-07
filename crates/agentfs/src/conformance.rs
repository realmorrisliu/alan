//! Agent file-layout conformance checker.
//!
//! The checker speaks only aP against a supplied root file server. That keeps
//! conformance as a filesystem property: any runtime that exports a compatible
//! tree can be tested without a kernel flag or an Alan-internal type check.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, InProcessTransport, OpenMode, Request, Response};

/// One conformance failure found while inspecting a process tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceIssue {
    pub path: String,
    pub message: String,
}

/// The accumulated result of a conformance check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceReport {
    pub target: String,
    pub issues: Vec<ConformanceIssue>,
}

impl ConformanceReport {
    fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            issues: Vec::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(ConformanceIssue {
            path: path.into(),
            message: message.into(),
        });
    }

    pub fn assert_ok(&self) {
        assert!(
            self.is_ok(),
            "agent file-layout conformance failed for {}: {:?}",
            self.target,
            self.issues
        );
    }
}

/// aP-only checker for the agent file-layout contract.
#[derive(Clone)]
pub struct AgentConformanceChecker {
    root: InProcessTransport,
    next_fid: Arc<AtomicU64>,
}

impl AgentConformanceChecker {
    pub fn new(root: InProcessTransport) -> Self {
        Self {
            root,
            next_fid: Arc::new(AtomicU64::new(9_000_000)),
        }
    }

    /// Verify the generic process layout plus the agent superset at `path`.
    pub async fn check_agent_process(&self, path: &str) -> ConformanceReport {
        let mut report = ConformanceReport::new(path);
        self.check_generic_process_into(path, &mut report).await;
        for (rel, kind) in [
            ("events", FileKind::Stream),
            ("machine", FileKind::Dir),
            ("machine/tape", FileKind::Stream),
            ("machine/checkpoints", FileKind::Dir),
            ("machine/checkpoints/current", FileKind::File),
            ("machine/status", FileKind::File),
            ("machine/ctl", FileKind::File),
            ("machine/ui", FileKind::Dir),
            ("machine/ui/activity", FileKind::File),
            ("machine/ui/plan", FileKind::File),
            ("machine/ui/thinking", FileKind::File),
            ("machine/ui/notice", FileKind::File),
            ("machine/ui/events", FileKind::Stream),
            ("requests", FileKind::Dir),
            ("requests/clone", FileKind::Clone),
            ("requests/events", FileKind::Stream),
            ("actions", FileKind::Dir),
            ("actions/clone", FileKind::Clone),
            ("actions/events", FileKind::Stream),
            ("context", FileKind::Dir),
            ("children", FileKind::Dir),
        ] {
            self.expect_kind(&mut report, &join_path(path, rel), kind)
                .await;
        }
        report
    }

    /// Verify only the generic process layout at `path`.
    pub async fn check_generic_process(&self, path: &str) -> ConformanceReport {
        let mut report = ConformanceReport::new(path);
        self.check_generic_process_into(path, &mut report).await;
        report
    }

    /// Verify that dynamic agent containers announce clone-created children on
    /// their observable `events` stream.
    pub async fn check_dynamic_container_events(&self, agent_path: &str) -> ConformanceReport {
        let mut report = ConformanceReport::new(agent_path);
        for container in ["requests", "actions"] {
            self.check_container_events(agent_path, container, &mut report)
                .await;
        }
        report
    }

    /// Verify that `/agent/root` resolves to the same conforming surface as the
    /// supplied current root pid.
    pub async fn check_root_alias(
        &self,
        agent_root_path: &str,
        current_root_pid: &str,
    ) -> ConformanceReport {
        let root_alias = join_path(agent_root_path, "root");
        let pid_path = join_path(agent_root_path, current_root_pid);
        let mut report = ConformanceReport::new(root_alias.clone());

        let alias_report = self.check_agent_process(&root_alias).await;
        report.issues.extend(alias_report.issues);

        let pid_report = self.check_agent_process(&pid_path).await;
        report.issues.extend(pid_report.issues);

        match (
            self.read_dir_entries(&root_alias).await,
            self.read_dir_entries(&pid_path).await,
        ) {
            (Ok(alias_entries), Ok(pid_entries)) if alias_entries == pid_entries => {}
            (Ok(alias_entries), Ok(pid_entries)) => report.push(
                root_alias,
                format!(
                    "root alias listing differs from pid listing: {alias_entries:?} != {pid_entries:?}"
                ),
            ),
            (Err(error), _) => report.push(root_alias, format!("cannot read root alias: {error:?}")),
            (_, Err(error)) => report.push(pid_path, format!("cannot read root pid: {error:?}")),
        }
        report
    }

    async fn check_generic_process_into(&self, path: &str, report: &mut ConformanceReport) {
        for (rel, kind) in [
            ("io", FileKind::Dir),
            ("io/input", FileKind::Stream),
            ("io/output", FileKind::Stream),
            ("io/events", FileKind::Stream),
            ("status", FileKind::File),
            ("ctl", FileKind::File),
        ] {
            self.expect_kind(report, &join_path(path, rel), kind).await;
        }
    }

    async fn check_container_events(
        &self,
        agent_path: &str,
        container: &str,
        report: &mut ConformanceReport,
    ) {
        let events_path = join_path(agent_path, &format!("{container}/events"));
        let clone_path = join_path(agent_path, &format!("{container}/clone"));
        let event_fid = match self.walk_open(&events_path, OpenMode::Read).await {
            Ok((fid, _)) => fid,
            Err(error) => {
                report.push(events_path, format!("cannot open events stream: {error:?}"));
                return;
            }
        };
        let offset = match self.stat(event_fid).await {
            Ok(stat) => stat.length,
            Err(error) => {
                report.push(
                    events_path.clone(),
                    format!("cannot stat events stream: {error:?}"),
                );
                let _ = self.clunk(event_fid).await;
                return;
            }
        };

        let reader = {
            let checker = self.clone();
            tokio::spawn(async move { checker.read(event_fid, offset, 4096).await })
        };
        let mut reader = reader;

        match self.walk_open(&clone_path, OpenMode::ReadWrite).await {
            Ok((clone_fid, _)) => {
                let _ = self.read(clone_fid, 0, 64).await;
                let _ = self.clunk(clone_fid).await;
            }
            Err(error) => {
                reader.abort();
                let _ = reader.await;
                report.push(
                    clone_path,
                    format!("cannot clone container child: {error:?}"),
                );
                let _ = self.clunk(event_fid).await;
                return;
            }
        }

        match tokio::time::timeout(Duration::from_millis(250), &mut reader).await {
            Ok(Ok(Ok(bytes))) if !bytes.is_empty() => {}
            Ok(Ok(Ok(_))) => report.push(events_path, "events stream returned no bytes"),
            Ok(Ok(Err(error))) => {
                report.push(events_path, format!("events stream read failed: {error:?}"))
            }
            Ok(Err(error)) => report.push(events_path, format!("events read task failed: {error}")),
            Err(_) => {
                reader.abort();
                let _ = reader.await;
                report.push(
                    events_path,
                    "events stream did not unblock after clone allocation",
                );
            }
        }
        let _ = self.clunk(event_fid).await;
    }

    async fn expect_kind(&self, report: &mut ConformanceReport, path: &str, expected: FileKind) {
        match self.walk_stat(path).await {
            Ok((fid, stat)) => {
                if stat.qid.kind != expected {
                    report.push(
                        path,
                        format!("expected {expected:?}, got {:?}", stat.qid.kind),
                    );
                }
                let _ = self.clunk(fid).await;
            }
            Err(error) => report.push(path, format!("missing or unreadable: {error:?}")),
        }
    }

    async fn read_dir_entries(&self, path: &str) -> Result<Vec<String>, ErrorCode> {
        let (fid, _) = self.walk_open(path, OpenMode::Read).await?;
        let bytes = self.read(fid, 0, 64 * 1024).await;
        let clunk = self.clunk(fid).await;
        let bytes = bytes?;
        clunk?;
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::Io)?;
        let mut entries = text
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        entries.sort();
        Ok(entries)
    }

    async fn walk_stat(&self, path: &str) -> Result<(Fid, alan_ap::Stat), ErrorCode> {
        let fid = self.walk(path).await?;
        let stat = self.stat(fid).await?;
        Ok((fid, stat))
    }

    async fn walk_open(
        &self,
        path: &str,
        mode: OpenMode,
    ) -> Result<(Fid, alan_ap::Qid), ErrorCode> {
        let fid = self.walk(path).await?;
        match self.open(fid, mode).await {
            Ok(qid) => Ok((fid, qid)),
            Err(error) => {
                let _ = self.clunk(fid).await;
                Err(error)
            }
        }
    }

    async fn walk(&self, path: &str) -> Result<Fid, ErrorCode> {
        let fid = Fid(self.next_fid.fetch_add(1, Ordering::Relaxed));
        self.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: split_path(path),
        })
        .await
        .and_then(|response| match response {
            Response::Walk { .. } => Ok(fid),
            _ => Err(ErrorCode::Io),
        })
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<alan_ap::Qid, ErrorCode> {
        self.call(Request::Open { fid, mode })
            .await
            .and_then(|response| match response {
                Response::Open { qid } => Ok(qid),
                _ => Err(ErrorCode::Io),
            })
    }

    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.call(Request::Read { fid, offset, count })
            .await
            .and_then(|response| match response {
                Response::Read { data } => Ok(data),
                _ => Err(ErrorCode::Io),
            })
    }

    async fn stat(&self, fid: Fid) -> Result<alan_ap::Stat, ErrorCode> {
        self.call(Request::Stat { fid })
            .await
            .and_then(|response| match response {
                Response::Stat { stat } => Ok(stat),
                _ => Err(ErrorCode::Io),
            })
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.call(Request::Clunk { fid })
            .await
            .and_then(|response| match response {
                Response::Clunk => Ok(()),
                _ => Err(ErrorCode::Io),
            })
    }

    async fn call(&self, request: Request) -> Result<Response, ErrorCode> {
        self.root.call(request).await
    }
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect()
}

fn join_path(base: &str, rel: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.is_empty() {
        format!("/{rel}")
    } else {
        format!("{base}/{rel}")
    }
}
