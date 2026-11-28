//! Crash-isolation, backpressure, and concurrent reader/writer tests for
//! [`RouteFs`] and [`RouteFsActor`] (ADR-0063, Task 4.2).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc as std_mpsc};
use std::time::Duration;

use alan_ap::{
    ActorExitReason, ActorOutcome, ErrorCode, Fid, FileKind, FileServer, FileServerActor,
    InProcessTransport, Offset, OpenMode, Qid, Stat, spawn_file_actor,
};
use alan_kernel::{Access, MountFs, Namespace};
use alan_routefs::{
    DEAD_LETTER_PORT, MAX_HANDOFF_HOPS, MOUNT_PATH, ROOT_PORT, RouteFs, RouteFsActor,
    RouteHandoffKind, RouteHandoffMessage, RoutedRecord, RuleSpec,
};
use alan_shell::Shell;
use serde_json::json;
use tokio::time::timeout;

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct GateRouteActor {
    inner: RouteFsActor,
    arm_next_stat: Arc<AtomicBool>,
    panic_on_release: bool,
    entered_tx: Option<std_mpsc::Sender<()>>,
    release_rx: Option<std_mpsc::Receiver<()>>,
    stat_count: Arc<AtomicUsize>,
    _drop_flag: Option<DropFlag>,
}

impl FileServerActor for GateRouteActor {
    fn walk(&mut self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.inner.walk(fid, newfid, names)
    }

    fn open(&mut self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode)
    }

    fn read(&mut self, fid: Fid, offset: Offset, count: u32) -> Result<ActorOutcome, ErrorCode> {
        self.inner.read(fid, offset, count)
    }

    fn write(&mut self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data)
    }

    fn stat(&mut self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.stat_count.fetch_add(1, Ordering::SeqCst);
        if self.arm_next_stat.swap(false, Ordering::SeqCst) {
            if let Some(tx) = self.entered_tx.take() {
                let _ = tx.send(());
            }
            if let Some(rx) = self.release_rx.take() {
                let _ = rx.recv();
            }
            if self.panic_on_release {
                panic!("synthetic routefs actor panic");
            }
        }
        self.inner.stat(fid)
    }

    fn create(
        &mut self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind)
    }

    fn remove(&mut self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid)
    }

    fn clunk(&mut self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid)
    }
}

fn path_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

async fn open_stream_fid(fs: &MountFs, path: &str, fid: Fid) -> Result<(), ErrorCode> {
    fs.walk(Fid::ROOT, fid, &path_segments(path)).await?;
    fs.open(fid, OpenMode::Read).await?;
    Ok(())
}

async fn read_exact_jsonl_records(
    fs: Arc<MountFs>,
    path: String,
    fid: Fid,
    expected_records: usize,
) -> Vec<RoutedRecord> {
    open_stream_fid(&fs, &path, fid)
        .await
        .expect("open stream fid");
    let mut offset = 0u64;
    let mut buf = Vec::new();
    let mut records = Vec::with_capacity(expected_records);

    while records.len() < expected_records {
        let chunk = fs
            .read(fid, offset, 8192)
            .await
            .expect("stream read succeeds");
        assert!(!chunk.is_empty(), "stream closed before expected records");
        offset += chunk.len() as u64;
        buf.extend_from_slice(&chunk);

        while let Some(newline_pos) = buf.iter().position(|&byte| byte == b'\n') {
            let line = buf.drain(..=newline_pos).collect::<Vec<_>>();
            let parsed: RoutedRecord =
                serde_json::from_slice(&line).expect("routed stream line is valid JSONL");
            records.push(parsed);
        }
    }

    fs.clunk(fid).await.expect("clunk stream fid");
    records
}

#[tokio::test]
async fn blocked_port_and_log_readers_do_not_stall_send_commits_or_metadata_ops() {
    let routefs = Arc::new(RouteFs::new_with_workstream_handoff_rules());
    let exit_handle = routefs.exit_handle();
    let mut ns = Namespace::new();
    ns.mount(
        MOUNT_PATH,
        InProcessTransport::new(routefs),
        Access::ReadWrite,
    );
    let mount = Arc::new(MountFs::new(ns));
    let shell = Shell::new(InProcessTransport::new(mount.clone()));

    // Park independent subscribers at offset 0 on both /mnt/route/ports/root and /mnt/route/log.
    open_stream_fid(&mount, "/mnt/route/ports/root", Fid(100_001))
        .await
        .unwrap();
    open_stream_fid(&mount, "/mnt/route/log", Fid(100_002))
        .await
        .unwrap();

    let port_reader = {
        let mount = Arc::clone(&mount);
        tokio::spawn(async move { mount.read(Fid(100_001), 0, 4096).await })
    };
    let log_reader = {
        let mount = Arc::clone(&mount);
        tokio::spawn(async move { mount.read(Fid(100_002), 0, 4096).await })
    };

    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!port_reader.is_finished());
    assert!(!log_reader.is_finished());

    // While both stream readers are blocked, metadata queries, rule creation, and send commits
    // must complete immediately without mailbox starvation.
    let first_msg = RouteHandoffMessage::new(
        RouteHandoffKind::RerouteRequest,
        "stream-a",
        "corr-unblocked-1",
        "reroute while readers wait",
    )
    .with_suggested_target("stream-b");

    timeout(Duration::from_secs(1), async {
        let stat = shell.stat("/mnt/route/ports/root").await.unwrap();
        assert_eq!(stat.length, 0);
        assert!(!stat.writable);

        let rules = shell.ls("/mnt/route/rules").await.unwrap();
        assert!(!rules.is_empty());

        shell
            .write("/mnt/route/send", &serde_json::to_vec(&first_msg).unwrap())
            .await
            .unwrap();
    })
    .await
    .expect("blocked stream readers must not starve RouteFs mailbox");

    let port_bytes = timeout(Duration::from_secs(1), port_reader)
        .await
        .expect("port reader wakes immediately")
        .unwrap()
        .unwrap();
    let log_bytes = timeout(Duration::from_secs(1), log_reader)
        .await
        .expect("log reader wakes immediately")
        .unwrap()
        .unwrap();
    assert_eq!(port_bytes, log_bytes);
    let first_record: RoutedRecord = serde_json::from_slice(&port_bytes).unwrap();
    assert_eq!(first_record.port, ROOT_PORT);
    assert_eq!(
        first_record.handoff_message().unwrap().correlation_id,
        "corr-unblocked-1"
    );

    // Park at the live edge and verify a subsequent send wakes the reader cleanly.
    let next_offset = port_bytes.len() as u64;
    let next_port_reader = {
        let mount = Arc::clone(&mount);
        tokio::spawn(async move { mount.read(Fid(100_001), next_offset, 4096).await })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!next_port_reader.is_finished());

    let second_msg = RouteHandoffMessage::new(
        RouteHandoffKind::HandoffResult,
        "stream-b",
        "corr-unblocked-2",
        "second record at live edge",
    );
    shell
        .write("/mnt/route/send", &serde_json::to_vec(&second_msg).unwrap())
        .await
        .unwrap();

    let second_bytes = timeout(Duration::from_secs(1), next_port_reader)
        .await
        .expect("second read wakes")
        .unwrap()
        .unwrap();
    let second_record: RoutedRecord = serde_json::from_slice(&second_bytes).unwrap();
    assert_eq!(
        second_record.handoff_message().unwrap().correlation_id,
        "corr-unblocked-2"
    );

    mount.clunk(Fid(100_001)).await.unwrap();
    mount.clunk(Fid(100_002)).await.unwrap();
    drop(shell);
    drop(mount);

    assert_eq!(
        timeout(Duration::from_secs(1), exit_handle.wait())
            .await
            .expect("actor exits when handles drop"),
        ActorExitReason::Exited
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_readers_and_writers_preserve_routing_dead_letter_and_loop_guard() {
    const PER_CATEGORY: usize = 12;

    let routefs = Arc::new(RouteFs::new_with_workstream_handoff_rules());
    routefs
        .install_rule(
            "10-alerts",
            RuleSpec::for_type("alert", "alerts").with_reason("route alert events"),
        )
        .await
        .unwrap();

    let mut ns = Namespace::new();
    ns.mount(
        MOUNT_PATH,
        InProcessTransport::new(routefs),
        Access::ReadWrite,
    );
    let mount = Arc::new(MountFs::new(ns));

    // Launch concurrent live stream readers before writers start committing.
    let root_reader = tokio::spawn(read_exact_jsonl_records(
        Arc::clone(&mount),
        "/mnt/route/ports/root".to_string(),
        Fid(100_010),
        PER_CATEGORY,
    ));
    let alerts_reader = tokio::spawn(read_exact_jsonl_records(
        Arc::clone(&mount),
        "/mnt/route/ports/alerts".to_string(),
        Fid(100_011),
        PER_CATEGORY,
    ));
    let dead_letter_reader = tokio::spawn(read_exact_jsonl_records(
        Arc::clone(&mount),
        format!("/mnt/route/ports/{DEAD_LETTER_PORT}"),
        Fid(100_012),
        PER_CATEGORY * 2,
    ));
    let log_reader = tokio::spawn(read_exact_jsonl_records(
        Arc::clone(&mount),
        "/mnt/route/log".to_string(),
        Fid(100_013),
        PER_CATEGORY * 4,
    ));

    // Concurrent metadata inspector exercising stat/ls while writes and stream reads run.
    let inspector_shell = Shell::new(InProcessTransport::new(mount.clone()));
    let inspector = tokio::spawn(async move {
        for _ in 0..20 {
            let ports = inspector_shell.ls("/mnt/route/ports").await.unwrap();
            assert!(ports.contains(&ROOT_PORT.to_string()));
            assert!(ports.contains(&DEAD_LETTER_PORT.to_string()));
            let _ = inspector_shell.stat("/mnt/route/log").await.unwrap();
        }
    });

    // Four concurrent writer tasks covering handoff->root, rule->alerts, unmatched->dead-letter,
    // and hop-overflow->loop-guard dead-letter.
    let handoff_writer = {
        let shell = Shell::new(InProcessTransport::new(mount.clone()));
        tokio::spawn(async move {
            for idx in 0..PER_CATEGORY {
                let msg = RouteHandoffMessage::new(
                    RouteHandoffKind::CrossStreamDependency,
                    "stream-a",
                    format!("corr-root-{idx}"),
                    format!("dependency payload {idx}"),
                )
                .with_hop_count((idx % usize::from(MAX_HANDOFF_HOPS + 1)) as u8);
                shell
                    .write("/mnt/route/send", &serde_json::to_vec(&msg).unwrap())
                    .await
                    .unwrap();
            }
        })
    };

    let alerts_writer = {
        let shell = Shell::new(InProcessTransport::new(mount.clone()));
        tokio::spawn(async move {
            for idx in 0..PER_CATEGORY {
                let doc = json!({
                    "version": 1,
                    "type": "alert",
                    "content": format!("alert-{idx}")
                });
                shell
                    .write("/mnt/route/send", &serde_json::to_vec(&doc).unwrap())
                    .await
                    .unwrap();
            }
        })
    };

    let unmatched_writer = {
        let shell = Shell::new(InProcessTransport::new(mount.clone()));
        tokio::spawn(async move {
            for idx in 0..PER_CATEGORY {
                let doc = json!({
                    "version": 1,
                    "type": "unmatched",
                    "content": format!("unmatched-{idx}")
                });
                shell
                    .write("/mnt/route/send", &serde_json::to_vec(&doc).unwrap())
                    .await
                    .unwrap();
            }
        })
    };

    let loop_writer = {
        let shell = Shell::new(InProcessTransport::new(mount.clone()));
        tokio::spawn(async move {
            for idx in 0..PER_CATEGORY {
                let msg = RouteHandoffMessage::new(
                    RouteHandoffKind::RerouteRequest,
                    "stream-loop",
                    format!("corr-loop-{idx}"),
                    format!("loop payload {idx}"),
                )
                .with_hop_count(MAX_HANDOFF_HOPS + 1);
                shell
                    .write("/mnt/route/send", &serde_json::to_vec(&msg).unwrap())
                    .await
                    .unwrap();
            }
        })
    };

    timeout(Duration::from_secs(5), async {
        handoff_writer.await.unwrap();
        alerts_writer.await.unwrap();
        unmatched_writer.await.unwrap();
        loop_writer.await.unwrap();
        inspector.await.unwrap();
    })
    .await
    .expect("concurrent writers and metadata inspector complete");

    let root_records = timeout(Duration::from_secs(5), root_reader)
        .await
        .expect("root reader completes")
        .unwrap();
    let alert_records = timeout(Duration::from_secs(5), alerts_reader)
        .await
        .expect("alerts reader completes")
        .unwrap();
    let dead_records = timeout(Duration::from_secs(5), dead_letter_reader)
        .await
        .expect("dead-letter reader completes")
        .unwrap();
    let log_records = timeout(Duration::from_secs(5), log_reader)
        .await
        .expect("log reader completes")
        .unwrap();

    assert_eq!(root_records.len(), PER_CATEGORY);
    assert!(root_records.iter().all(|rec| rec.port == ROOT_PORT));
    assert_eq!(alert_records.len(), PER_CATEGORY);
    assert!(alert_records.iter().all(|rec| rec.port == "alerts"));
    assert_eq!(dead_records.len(), PER_CATEGORY * 2);
    assert_eq!(
        dead_records
            .iter()
            .filter(|rec| rec.is_loop_escalation())
            .count(),
        PER_CATEGORY
    );
    assert_eq!(
        dead_records
            .iter()
            .filter(|rec| rec.is_dead_letter() && !rec.is_loop_escalation())
            .count(),
        PER_CATEGORY
    );
    assert_eq!(log_records.len(), PER_CATEGORY * 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_mailbox_applies_async_backpressure_without_rejecting_routefs_callers() {
    let (entered_tx, entered_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let arm_next_stat = Arc::new(AtomicBool::new(true));
    let stat_count = Arc::new(AtomicUsize::new(0));

    let actor = GateRouteActor {
        inner: RouteFsActor::new_with_workstream_handoff_rules(),
        arm_next_stat,
        panic_on_release: false,
        entered_tx: Some(entered_tx),
        release_rx: Some(release_rx),
        stat_count: Arc::clone(&stat_count),
        _drop_flag: None,
    };

    // Mailbox capacity of 1: one in-flight call executing + at most one queued.
    let (server, exit_handle) = spawn_file_actor(actor, 1);

    let s1 = Arc::clone(&server);
    let req1 = tokio::spawn(async move { s1.stat(Fid::ROOT).await });
    tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(1)))
        .await
        .unwrap()
        .expect("req1 entered RouteFsActor");

    // Fill the single bounded mailbox slot.
    let s2 = Arc::clone(&server);
    let req2 = tokio::spawn(async move { s2.stat(Fid::ROOT).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!req2.is_finished());

    // Third caller blocks on async backpressure (`send().await`) instead of failing.
    let s3 = Arc::clone(&server);
    let req3 = tokio::spawn(async move { s3.stat(Fid::ROOT).await });
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!req3.is_finished());

    // Aborting the backpressured caller prevents it from entering the actor mailbox.
    req3.abort();
    let _ = req3.await;

    release_tx.send(()).unwrap();
    assert!(req1.await.unwrap().is_ok());
    assert!(req2.await.unwrap().is_ok());
    assert_eq!(stat_count.load(Ordering::SeqCst), 2);

    drop(server);
    assert_eq!(
        timeout(Duration::from_secs(1), exit_handle.wait())
            .await
            .unwrap(),
        ActorExitReason::Exited
    );

    // Production RouteFs with `mailbox_cap = 1`: burst of 16 concurrent sends completes via
    // async backpressure with zero RateLimited or Io rejections.
    let tight_fs = Arc::new(RouteFs::with_mailbox_cap(1));
    tight_fs
        .install_rule("10-burst", RuleSpec::for_type("burst", "burst-port"))
        .await
        .unwrap();
    let mut ns = Namespace::new();
    ns.mount(
        MOUNT_PATH,
        InProcessTransport::new(tight_fs),
        Access::ReadWrite,
    );
    let mount = Arc::new(MountFs::new(ns));

    let mut writers = Vec::new();
    for idx in 0..16 {
        let shell = Shell::new(InProcessTransport::new(mount.clone()));
        writers.push(tokio::spawn(async move {
            let doc = json!({
                "version": 1,
                "type": "burst",
                "content": format!("msg-{idx}")
            });
            shell
                .write("/mnt/route/send", &serde_json::to_vec(&doc).unwrap())
                .await
        }));
    }

    for writer in writers {
        timeout(Duration::from_secs(2), writer)
            .await
            .expect("burst writer completes under backpressure")
            .unwrap()
            .expect("backpressure never rejects callers");
    }

    let shell = Shell::new(InProcessTransport::new(mount));
    let port_body = String::from_utf8(shell.cat("/mnt/route/ports/burst-port").await.unwrap())
        .expect("utf8 port body");
    assert_eq!(port_body.lines().count(), 16);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routefs_panic_isolates_failure_to_io_without_poisoning_kernel_mounts() {
    let (entered_tx, entered_rx) = std_mpsc::channel();
    let (release_tx, release_rx) = std_mpsc::channel();
    let arm_next_stat = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));

    let actor = GateRouteActor {
        inner: RouteFsActor::new_with_workstream_handoff_rules(),
        arm_next_stat: Arc::clone(&arm_next_stat),
        panic_on_release: true,
        entered_tx: Some(entered_tx),
        release_rx: Some(release_rx),
        stat_count: Arc::new(AtomicUsize::new(0)),
        _drop_flag: Some(DropFlag(Arc::clone(&dropped))),
    };

    let (panicking_route, exit_handle) = spawn_file_actor(actor, 4);
    let panicking_route_direct = Arc::clone(&panicking_route);
    let healthy_route = Arc::new(RouteFs::new_with_workstream_handoff_rules());

    let mut ns = Namespace::new();
    ns.mount(
        MOUNT_PATH,
        InProcessTransport::new(panicking_route),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/healthy-route",
        InProcessTransport::new(healthy_route),
        Access::ReadWrite,
    );
    let mount = Arc::new(MountFs::new(ns));

    // Park a blocked stream reader on /mnt/route/ports/root before triggering the panic.
    open_stream_fid(&mount, "/mnt/route/ports/root", Fid(100_021))
        .await
        .unwrap();
    let blocked_reader = {
        let mount = Arc::clone(&mount);
        tokio::spawn(async move { mount.read(Fid(100_021), 0, 4096).await })
    };
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!blocked_reader.is_finished());

    // Walk fids for the panicking request and a queued in-flight request before arming the gate.
    mount
        .walk(Fid::ROOT, Fid(100_022), &path_segments("/mnt/route/send"))
        .await
        .unwrap();
    mount
        .walk(Fid::ROOT, Fid(100_023), &path_segments("/mnt/route/rules"))
        .await
        .unwrap();

    // Arm the panic gate so Fid(100_022)'s stat enters and holds inside the actor loop.
    arm_next_stat.store(true, Ordering::SeqCst);

    let panicking_req = {
        let mount = Arc::clone(&mount);
        tokio::spawn(async move { mount.stat(Fid(100_022)).await })
    };
    tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(1)))
        .await
        .unwrap()
        .expect("panicking request entered RouteFsActor");

    let queued_req = {
        let mount = Arc::clone(&mount);
        tokio::spawn(async move { mount.stat(Fid(100_023)).await })
    };
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert!(!queued_req.is_finished());

    // Trigger the injected panic inside the isolated RouteFs actor task.
    release_tx.send(()).unwrap();

    assert_eq!(
        timeout(Duration::from_secs(1), panicking_req)
            .await
            .expect("panicking req resolves")
            .unwrap(),
        Err(ErrorCode::Io)
    );
    assert_eq!(
        timeout(Duration::from_secs(1), queued_req)
            .await
            .expect("queued req resolves")
            .unwrap(),
        Err(ErrorCode::Io)
    );
    assert_eq!(
        timeout(Duration::from_secs(1), blocked_reader)
            .await
            .expect("blocked stream reader wakes on panic")
            .unwrap(),
        Err(ErrorCode::Io)
    );

    let reason = timeout(Duration::from_secs(1), exit_handle.wait())
        .await
        .expect("exit handle notifies on panic");
    match reason {
        ActorExitReason::Panicked(message) => {
            assert!(
                message.contains("synthetic routefs actor panic"),
                "unexpected panic reason: {message}"
            );
        }
        ActorExitReason::Exited => panic!("expected ActorExitReason::Panicked"),
    }
    assert!(
        dropped.load(Ordering::SeqCst),
        "owned RouteFsActor state must drop cleanly on panic"
    );

    // Every direct FileServer call on the crashed RouteFs actor fails cleanly with ErrorCode::Io.
    assert_eq!(
        panicking_route_direct
            .walk(Fid::ROOT, Fid(900), &["send".to_string()])
            .await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        panicking_route_direct.open(Fid::ROOT, OpenMode::Read).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        panicking_route_direct.read(Fid::ROOT, 0, 64).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        panicking_route_direct.write(Fid::ROOT, 0, b"x").await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        panicking_route_direct.stat(Fid::ROOT).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        panicking_route_direct
            .create(Fid::ROOT, Fid(901), "r", FileKind::File)
            .await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        panicking_route_direct.remove(Fid(900)).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        panicking_route_direct.clunk(Fid(900)).await,
        Err(ErrorCode::Io)
    );

    // Existing MountFs fids targeting the crashed /mnt/route mount also fail with ErrorCode::Io,
    // and new MountFs walks fail cleanly without hanging or poisoning the mount table.
    assert_eq!(mount.stat(Fid(100_022)).await, Err(ErrorCode::Io));
    assert_eq!(
        mount.open(Fid(100_022), OpenMode::Write).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(mount.read(Fid(100_021), 0, 4096).await, Err(ErrorCode::Io));
    assert_eq!(
        mount
            .create(Fid(100_023), Fid(100_025), "99-after", FileKind::File)
            .await,
        Err(ErrorCode::Io)
    );
    assert!(
        mount
            .walk(Fid::ROOT, Fid(100_024), &path_segments("/mnt/route/send"))
            .await
            .is_err()
    );

    // Kernel MountFs and sibling mounts remain healthy and completely unpoisoned.
    let healthy_shell = Shell::new(InProcessTransport::new(mount));
    let handoff = RouteHandoffMessage::new(
        RouteHandoffKind::HandoffResult,
        "stream-ok",
        "corr-healthy-1",
        "sibling mount still healthy",
    );
    healthy_shell
        .write(
            "/mnt/healthy-route/send",
            &serde_json::to_vec(&handoff).unwrap(),
        )
        .await
        .expect("healthy sibling mount remains usable after RouteFs panic");
    let body = String::from_utf8(
        healthy_shell
            .cat("/mnt/healthy-route/ports/root")
            .await
            .unwrap(),
    )
    .unwrap();
    let record: RoutedRecord = serde_json::from_str(body.trim()).unwrap();
    assert_eq!(
        record.handoff_message().unwrap().correlation_id,
        "corr-healthy-1"
    );
}

#[tokio::test]
async fn write_exclusive_and_rule_replacement_behave_consistently_on_actor_routefs() {
    let fs = RouteFs::new();
    let exit_handle = fs.exit_handle();

    // WriteExclusive on read-only nodes is rejected at dial time.
    for (fid, path) in [
        (Fid(1), vec!["ports".to_string(), ROOT_PORT.to_string()]),
        (Fid(2), vec!["log".to_string()]),
        (Fid(3), vec!["rules".to_string()]),
    ] {
        fs.walk(Fid::ROOT, fid, &path).await.unwrap();
        assert_eq!(
            fs.open(fid, OpenMode::WriteExclusive).await,
            Err(ErrorCode::NoAccess)
        );
        fs.clunk(fid).await.unwrap();
    }

    // Install and then replace an existing rule via RouteFs::install_rule.
    fs.install_rule("10-route", RuleSpec::for_type("job", "port-a"))
        .await
        .unwrap();
    fs.install_rule("10-route", RuleSpec::for_type("job", "port-b"))
        .await
        .unwrap();

    // A pending rule create still blocks install_rule with BadRequest.
    fs.walk(Fid::ROOT, Fid(4), &["rules".to_string()])
        .await
        .unwrap();
    fs.create(Fid(4), Fid(5), "20-pending", FileKind::File)
        .await
        .unwrap();
    assert_eq!(
        fs.install_rule("20-pending", RuleSpec::for_type("job", "port-b"))
            .await,
        Err(ErrorCode::BadRequest)
    );
    fs.clunk(Fid(5)).await.unwrap();
    fs.clunk(Fid(4)).await.unwrap();

    // WriteExclusive on send commits to the replaced rule's destination port.
    fs.walk(Fid::ROOT, Fid(6), &["send".to_string()])
        .await
        .unwrap();
    fs.open(Fid(6), OpenMode::WriteExclusive).await.unwrap();
    let doc = serde_json::to_vec(&json!({
        "version": 1,
        "type": "job",
        "content": "exclusive write"
    }))
    .unwrap();
    fs.write(Fid(6), 0, &doc).await.unwrap();
    fs.clunk(Fid(6)).await.unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(7),
        &["ports".to_string(), "port-b".to_string()],
    )
    .await
    .unwrap();
    fs.open(Fid(7), OpenMode::Read).await.unwrap();
    let routed = String::from_utf8(fs.read(Fid(7), 0, 4096).await.unwrap()).unwrap();
    fs.clunk(Fid(7)).await.unwrap();
    assert!(routed.contains(r#""port":"port-b""#), "{routed}");

    drop(fs);
    assert_eq!(
        timeout(Duration::from_secs(1), exit_handle.wait())
            .await
            .unwrap(),
        ActorExitReason::Exited
    );
}
