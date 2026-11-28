use super::super::agent_files::tape_record_bytes;
use super::*;

#[tokio::test]
async fn engine_tape_writer_holds_generating_lease_and_allows_readers() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let agent_files = environment.agent_files();

    let writer = agent_files.begin_tape_generation().await.unwrap();

    let second_writer = agent_files.begin_tape_generation().await;
    assert!(
        second_writer.is_err(),
        "a second engine writer must not acquire machine/tape while GENERATING lease is held"
    );

    let mut tape_tail = shell.tail("/agent/1/machine/tape").await.unwrap();
    writer
        .append_record("user", "hello", None, &[])
        .await
        .unwrap();
    let streamed = String::from_utf8(tape_tail.read(64 * 1024).await.unwrap()).unwrap();
    assert!(streamed.contains(r#""role":"user""#), "{streamed}");
    assert!(streamed.contains(r#""content":"hello""#), "{streamed}");
    tape_tail.close().await.unwrap();

    writer
        .append_record("assistant", "hi", None, &[])
        .await
        .unwrap();
    writer.finish().await.unwrap();

    let next_writer = agent_files.begin_tape_generation().await.unwrap();
    next_writer
        .append_record("assistant", "after lease", None, &[])
        .await
        .unwrap();
    next_writer.finish().await.unwrap();

    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    assert!(tape.contains(r#""content":"hi""#), "{tape}");
    assert!(tape.contains(r#""content":"after lease""#), "{tape}");
}

#[test]
fn tape_record_shape_is_content_addressable_ready() {
    let record = tape_record_bytes("assistant", "stable text", None, &[], None).unwrap();
    assert_eq!(
        String::from_utf8(record).unwrap(),
        r#"{"version":1,"kind":"message","role":"assistant","content":"stable text"}"#.to_string()
            + "\n",
        "tape records must stay canonical, self-contained newline-delimited units"
    );
}

#[tokio::test]
async fn failed_turn_runtime_closes_writer_before_returning() {
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(AgentFs::new())),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    shell.write("/agent/1/io/input", b"hello").await.unwrap();
    let mut runtime = NamespaceTurnRuntime::new(
        root.clone(),
        NamespaceTurnRuntimeConfig::new("/agent/1", "missing"),
    );
    assert!(runtime.run_next_turn().await.is_err());
    // No yield: an asynchronously scheduled Drop clunk would still hold the lease.
    shell
        .write_exclusive("/agent/1/machine/tape", b"")
        .await
        .unwrap();
}

#[tokio::test]
async fn engine_writers_use_write_exclusive_and_reject_non_exclusive_or_contending_opens() {
    use alan_agent_protocol::{UiActivitySnapshot, UiEvent};
    use alan_ap::{ErrorCode, Fid, OpenMode, Request};

    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
    let agent_files = environment.agent_files();

    // Non-exclusive Write and ReadWrite opens are rejected on all three single-producer streams.
    for path in [
        "/agent/1/machine/tape",
        "/agent/1/io/output",
        "/agent/1/machine/ui/events",
    ] {
        assert_eq!(
            shell.write(path, b"nope\n").await,
            Err(ErrorCode::NoAccess),
            "{path} must reject non-exclusive Write"
        );
        let fid = Fid(700);
        let names: Vec<String> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect();
        root.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names,
        })
        .await
        .unwrap();
        assert_eq!(
            root.call(Request::Open {
                fid,
                mode: OpenMode::ReadWrite,
            })
            .await,
            Err(ErrorCode::NoAccess),
            "{path} must reject ReadWrite"
        );
        root.call(Request::Clunk { fid }).await.unwrap();
    }

    // Legitimate engine writers succeed via WriteExclusive on all three surfaces.
    let writer = agent_files.begin_tape_generation().await.unwrap();
    writer
        .append_record("assistant", "exclusive tape", None, &[])
        .await
        .unwrap();
    writer.finish().await.unwrap();

    agent_files
        .write_assistant_output("exclusive output")
        .await
        .unwrap();
    agent_files
        .append_ui_event(&UiEvent::Activity {
            snapshot: UiActivitySnapshot::idle(),
        })
        .await
        .unwrap();

    // Holding an exclusive lease on io/output or machine/ui/events blocks the engine emitter
    // until clunk releases it cleanly.
    for (path, is_output) in [
        ("/agent/1/io/output", true),
        ("/agent/1/machine/ui/events", false),
    ] {
        let holder = Fid(710);
        let names: Vec<String> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect();
        root.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: holder,
            names,
        })
        .await
        .unwrap();
        root.call(Request::Open {
            fid: holder,
            mode: OpenMode::WriteExclusive,
        })
        .await
        .unwrap();

        let contended = if is_output {
            agent_files.write_assistant_output("contended").await
        } else {
            agent_files
                .append_ui_event(&UiEvent::Activity {
                    snapshot: UiActivitySnapshot::idle(),
                })
                .await
        };
        assert!(
            contended.is_err(),
            "{path} must reject a second WriteExclusive while leased"
        );

        root.call(Request::Clunk { fid: holder }).await.unwrap();

        if is_output {
            agent_files
                .write_assistant_output(" after-clunk")
                .await
                .unwrap();
        } else {
            agent_files
                .append_ui_event(&UiEvent::Activity {
                    snapshot: UiActivitySnapshot::idle(),
                })
                .await
                .unwrap();
        }
    }

    assert_eq!(
        String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap(),
        "exclusive output after-clunk"
    );
    let ui_events =
        String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    assert_eq!(ui_events.lines().count(), 2);
}
