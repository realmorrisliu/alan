// ProcFs dial-time NoAccess enforcement on read-only nodes and serialized
// per-PID cross-stream IO ordering (ADR-0063 / Tasks 2.2, 2.3, 2.4).

#[derive(Default)]
struct YieldingProcessEventSink {
    events: Mutex<Vec<ProcessEvent>>,
}

#[async_trait::async_trait]
impl ProcessEventSink for YieldingProcessEventSink {
    async fn process_event(&self, _pid: &str, event: ProcessEvent) {
        // Yield before recording to amplify any concurrent cross-stream race.
        tokio::task::yield_now().await;
        self.events.lock().unwrap().push(event);
    }
}

#[derive(Default)]
struct YieldingIoEventSink {
    events: Mutex<Vec<(alan_ap::ProcessIoEventKind, u32)>>,
}

#[async_trait::async_trait]
impl alan_ap::ProcessIoEventSink for YieldingIoEventSink {
    async fn io_appended(&self, _pid: &str, kind: alan_ap::ProcessIoEventKind, count: u32) {
        tokio::task::yield_now().await;
        self.events.lock().unwrap().push((kind, count));
    }
}

fn parse_framed_input_lengths(bytes: &[u8]) -> Vec<u32> {
    let mut lengths = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let nl = bytes[cursor..]
            .iter()
            .position(|&b| b == b'\n')
            .expect("framed input header newline");
        let len_text = std::str::from_utf8(&bytes[cursor..cursor + nl]).unwrap();
        let len: usize = len_text.parse().unwrap();
        cursor += nl + 1;
        assert!(cursor + len <= bytes.len(), "truncated framed input");
        lengths.push(len as u32);
        cursor += len;
    }
    lengths
}

fn parse_io_events(bytes: &[u8]) -> Vec<(alan_ap::ProcessIoEventKind, u32)> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(|line| {
            let (kind, count) = line.split_once(':').expect("io/events record");
            let kind = match kind {
                "input" => alan_ap::ProcessIoEventKind::Input,
                "output" => alan_ap::ProcessIoEventKind::Output,
                other => panic!("unexpected io/events kind: {other}"),
            };
            (kind, count.parse::<u32>().unwrap())
        })
        .collect()
}

#[tokio::test]
async fn procfs_open_rejects_write_modes_on_read_only_nodes_at_dial_time() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    // Root fid is read-only.
    for mode in [
        OpenMode::Write,
        OpenMode::WriteExclusive,
        OpenMode::ReadWrite,
    ] {
        assert_eq!(
            fs.open(Fid::ROOT, mode).await,
            Err(ErrorCode::NoAccess),
            "/proc root must reject {mode:?} at open"
        );
    }
    fs.open(Fid::ROOT, OpenMode::Read).await.unwrap();

    // Every read-only per-process node (including `io/events`) rejects write modes at dial time.
    let read_only_paths: Vec<Vec<String>> = vec![
        vec![pid.clone(), "io".into(), "events".into()],
        vec![pid.clone(), "status".into()],
        vec![pid.clone(), "exit".into()],
        vec![pid.clone(), "parent".into()],
        vec![pid.clone(), "credentials".into()],
        vec![pid.clone(), "namespace".into()],
        vec![pid.clone(), "descriptors".into()],
        vec![pid.clone(), "io".into()],
        vec![pid.clone()],
    ];

    let mut next_fid = 100u64;
    for path in &read_only_paths {
        for mode in [
            OpenMode::Write,
            OpenMode::WriteExclusive,
            OpenMode::ReadWrite,
        ] {
            let fid = Fid(next_fid);
            next_fid += 1;
            fs.walk(Fid::ROOT, fid, path).await.unwrap();
            assert_eq!(
                fs.open(fid, mode).await,
                Err(ErrorCode::NoAccess),
                "path {path:?} must reject {mode:?} at dial time"
            );
            fs.clunk(fid).await.unwrap();
        }

        let read_fid = Fid(next_fid);
        next_fid += 1;
        fs.walk(Fid::ROOT, read_fid, path).await.unwrap();
        fs.open(read_fid, OpenMode::Read)
            .await
            .expect("read-only /proc node must allow OpenMode::Read");
        fs.clunk(read_fid).await.unwrap();
    }

    // Writable nodes (such as io/output) still accept OpenMode::WriteExclusive and write bytes.
    let out_fid = Fid(next_fid);
    fs.walk(
        Fid::ROOT,
        out_fid,
        &[pid.clone(), "io".into(), "output".into()],
    )
    .await
    .unwrap();
    fs.open(out_fid, OpenMode::WriteExclusive).await.unwrap();
    assert_eq!(fs.write(out_fid, 0, b"exclusive-out").await.unwrap(), 13);
    fs.clunk(out_fid).await.unwrap();
    assert_eq!(
        read_at(&fs, &[&pid, "io", "output"], Fid(next_fid + 1))
            .await
            .unwrap(),
        b"exclusive-out"
    );
}

#[tokio::test]
async fn concurrent_procfs_io_input_clunks_preserve_exact_cross_stream_order() {
    use alan_ap::ProcessIoEventSource;

    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    let process_sink = Arc::new(YieldingProcessEventSink::default());
    let io_sink = Arc::new(YieldingIoEventSink::default());
    fs.subscribe_process_events(&pid, process_sink.clone())
        .await
        .unwrap();
    fs.subscribe_process_io(&pid, io_sink.clone())
        .await
        .unwrap();

    let writer_count = 16usize;
    let barrier = Arc::new(tokio::sync::Barrier::new(writer_count));
    let mut handles = Vec::with_capacity(writer_count);

    for index in 0..writer_count {
        let fs = fs.clone();
        let pid = pid.clone();
        let barrier = barrier.clone();
        let fid = Fid(100 + index as u64);
        let payload = vec![b'a' + (index % 26) as u8; index + 1];
        handles.push(tokio::spawn(async move {
            fs.walk(Fid::ROOT, fid, &[pid, "io".into(), "input".into()])
                .await
                .unwrap();
            fs.open(fid, OpenMode::Write).await.unwrap();
            fs.write(fid, 0, &payload).await.unwrap();
            barrier.wait().await;
            fs.clunk(fid).await.unwrap();
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let input_bytes = read_at(&fs, &[&pid, "io", "input"], Fid(500))
        .await
        .unwrap();
    let io_events_bytes = read_at(&fs, &[&pid, "io", "events"], Fid(501))
        .await
        .unwrap();

    let input_lengths = parse_framed_input_lengths(&input_bytes);
    let io_events = parse_io_events(&io_events_bytes);
    let io_event_lengths: Vec<u32> = io_events
        .iter()
        .map(|(kind, count)| {
            assert_eq!(*kind, alan_ap::ProcessIoEventKind::Input);
            *count
        })
        .collect();
    let bridged_event_lengths: Vec<u32> = process_sink
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            ProcessEvent::Input { count } => Some(*count),
            _ => None,
        })
        .collect();
    let bridged_io_lengths: Vec<u32> = io_sink
        .events
        .lock()
        .unwrap()
        .iter()
        .map(|(kind, count)| {
            assert_eq!(*kind, alan_ap::ProcessIoEventKind::Input);
            *count
        })
        .collect();

    assert_eq!(input_lengths.len(), writer_count);
    assert_eq!(
        io_event_lengths, input_lengths,
        "io/events order must match io/input framed commit order"
    );
    assert_eq!(
        bridged_event_lengths, input_lengths,
        "bridged ProcessEventSink order must match io/input framed commit order"
    );
    assert_eq!(
        bridged_io_lengths, input_lengths,
        "bridged ProcessIoEventSink order must match io/input framed commit order"
    );
}

#[tokio::test]
async fn concurrent_procfs_input_and_output_preserve_exact_cross_stream_order() {
    use alan_ap::ProcessIoEventSource;

    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    let process_sink = Arc::new(YieldingProcessEventSink::default());
    let io_sink = Arc::new(YieldingIoEventSink::default());
    fs.subscribe_process_events(&pid, process_sink.clone())
        .await
        .unwrap();
    fs.subscribe_process_io(&pid, io_sink.clone())
        .await
        .unwrap();

    let per_direction = 8usize;
    let barrier = Arc::new(tokio::sync::Barrier::new(per_direction * 2));
    let mut handles = Vec::with_capacity(per_direction * 2);

    for index in 0..per_direction {
        // Input writer with distinct length 101..=108.
        let fs_in = fs.clone();
        let pid_in = pid.clone();
        let barrier_in = barrier.clone();
        let fid_in = Fid(200 + index as u64);
        let payload_in = vec![b'i'; 101 + index];
        handles.push(tokio::spawn(async move {
            fs_in
                .walk(Fid::ROOT, fid_in, &[pid_in, "io".into(), "input".into()])
                .await
                .unwrap();
            fs_in.open(fid_in, OpenMode::Write).await.unwrap();
            fs_in.write(fid_in, 0, &payload_in).await.unwrap();
            barrier_in.wait().await;
            fs_in.clunk(fid_in).await.unwrap();
        }));

        // Output writer with distinct length 201..=208 and distinct fill byte.
        let fs_out = fs.clone();
        let pid_out = pid.clone();
        let barrier_out = barrier.clone();
        let fid_out = Fid(300 + index as u64);
        let marker = b'A' + index as u8;
        let payload_out = vec![marker; 201 + index];
        let mode = if index % 2 == 0 {
            OpenMode::WriteExclusive
        } else {
            OpenMode::Write
        };
        handles.push(tokio::spawn(async move {
            fs_out
                .walk(Fid::ROOT, fid_out, &[pid_out, "io".into(), "output".into()])
                .await
                .unwrap();
            fs_out.open(fid_out, mode).await.unwrap();
            barrier_out.wait().await;
            fs_out.write(fid_out, 0, &payload_out).await.unwrap();
            fs_out.clunk(fid_out).await.unwrap();
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let input_bytes = read_at(&fs, &[&pid, "io", "input"], Fid(600))
        .await
        .unwrap();
    let output_bytes = read_at(&fs, &[&pid, "io", "output"], Fid(601))
        .await
        .unwrap();
    let io_events_bytes = read_at(&fs, &[&pid, "io", "events"], Fid(602))
        .await
        .unwrap();

    let io_events = parse_io_events(&io_events_bytes);
    let bridged_io: Vec<_> = io_sink.events.lock().unwrap().clone();
    let bridged_process_io: Vec<_> = process_sink
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            ProcessEvent::Input { count } => Some((alan_ap::ProcessIoEventKind::Input, *count)),
            ProcessEvent::Output { count } => Some((alan_ap::ProcessIoEventKind::Output, *count)),
            ProcessEvent::Status { .. } => None,
        })
        .collect();

    assert_eq!(io_events.len(), per_direction * 2);
    assert_eq!(
        bridged_io, io_events,
        "ProcessIoEventSink must match io/events across interleaved input/output"
    );
    assert_eq!(
        bridged_process_io, io_events,
        "ProcessEventSink must match io/events across interleaved input/output"
    );

    let input_lengths = parse_framed_input_lengths(&input_bytes);
    let io_event_input_lengths: Vec<u32> = io_events
        .iter()
        .filter_map(|(kind, count)| (*kind == alan_ap::ProcessIoEventKind::Input).then_some(*count))
        .collect();
    assert_eq!(io_event_input_lengths, input_lengths);

    // Each output chunk has a unique run-length and fill byte, so run-length decoding
    // verifies that io/output byte order matches io/events output order exactly.
    let mut output_chunk_lengths = Vec::new();
    let mut cursor = 0usize;
    while cursor < output_bytes.len() {
        let byte = output_bytes[cursor];
        let start = cursor;
        while cursor < output_bytes.len() && output_bytes[cursor] == byte {
            cursor += 1;
        }
        output_chunk_lengths.push((cursor - start) as u32);
    }
    let io_event_output_lengths: Vec<u32> = io_events
        .iter()
        .filter_map(|(kind, count)| {
            (*kind == alan_ap::ProcessIoEventKind::Output).then_some(*count)
        })
        .collect();
    assert_eq!(io_event_output_lengths, output_chunk_lengths);
}
